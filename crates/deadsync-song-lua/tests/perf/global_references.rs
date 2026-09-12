use super::*;
use std::hint::black_box;

#[path = "global_references_baseline.rs"]
mod baseline;

fn references(lua: &Lua, old: bool) -> Result<HashSet<usize>, String> {
    if old {
        baseline::global_actor_references(lua)
    } else {
        global_actor_references(lua)
    }
}

fn fixture(lua: &Lua, count: usize, fields: usize, nested: usize, referenced: bool) -> Vec<Table> {
    let globals = lua.create_table().unwrap();
    lua.set_globals(globals.clone()).unwrap();
    let registry = song_lua_actor_registry(lua).unwrap();
    let actors: Vec<_> = (0..count)
        .map(|index| {
            let actor = lua.create_table().unwrap();
            registry.raw_set(index + 1, actor.clone()).unwrap();
            actor
        })
        .collect();
    for index in 0..fields {
        let value = if referenced && !actors.is_empty() && index % 3 == 0 {
            Value::Table(actors[index % count].clone())
        } else {
            Value::Integer(index as i64)
        };
        globals.raw_set(format!("global_{index}"), value).unwrap();
    }
    for table_index in 0..nested {
        let table = lua.create_table().unwrap();
        for index in 0..fields {
            let value = if referenced && !actors.is_empty() && index % 3 == 0 {
                Value::Table(actors[(table_index + index) % count].clone())
            } else {
                Value::Integer(index as i64)
            };
            table.raw_set(format!("field_{index}"), value).unwrap();
        }
        globals
            .raw_set(format!("nested_{table_index}"), table)
            .unwrap();
    }
    actors
}

#[test]
fn lua_traversal_references_preserve_two_level_scan_identity_and_exclusions() {
    let lua = Lua::new();
    let actors = fixture(&lua, 8, 0, 0, false);
    let globals = lua.globals();
    let registry = song_lua_actor_registry(&lua).unwrap();
    globals.raw_set("direct", actors[0].clone()).unwrap();
    globals.raw_set("direct_alias", actors[0].clone()).unwrap();
    actors[0]
        .raw_set("not_traversed", actors[1].clone())
        .unwrap();
    let nested = lua.create_table().unwrap();
    nested
        .raw_set(lua.create_string([0xff, 0]).unwrap(), actors[2].clone())
        .unwrap();
    nested.raw_set(false, actors[2].clone()).unwrap();
    let third = lua.create_table().unwrap();
    third.raw_set("too_deep", actors[3].clone()).unwrap();
    nested.raw_set("third", third).unwrap();
    nested.raw_set(actors[4].clone(), 42).unwrap(); // keys are not references
    globals.raw_set("nested", nested.clone()).unwrap();
    globals.raw_set("nested_alias", nested.clone()).unwrap();
    globals.raw_set("self", globals.clone()).unwrap();
    globals.raw_set("registry_alias", registry.clone()).unwrap();
    registry
        .raw_set("outside_sequence", actors[5].clone())
        .unwrap();
    let unrelated = lua.create_table().unwrap();
    globals.raw_set(unrelated, "ignored").unwrap();
    let mt = lua.create_table().unwrap();
    for key in ["__pairs", "__index"] {
        mt.raw_set(
            key,
            lua.create_function(|_, (): ()| -> mlua::Result<()> {
                Err(mlua::Error::runtime("unexpected lookup"))
            })
            .unwrap(),
        )
        .unwrap();
    }
    globals.set_metatable(Some(mt.clone())).unwrap();
    nested.set_metatable(Some(mt)).unwrap();
    let expected: HashSet<_> = [
        actors[0].to_pointer() as usize,
        actors[2].to_pointer() as usize,
    ]
    .into_iter()
    .collect();
    assert_eq!(references(&lua, true).unwrap(), expected);
    assert_eq!(references(&lua, false).unwrap(), expected);
    // Repeated registry entries still describe the same actor identity.
    registry.raw_set(9, actors[0].clone()).unwrap();
    assert_eq!(
        references(&lua, false).unwrap(),
        references(&lua, true).unwrap()
    );
}

#[test]
fn lua_traversal_references_preserve_registry_creation_holes_and_errors() {
    for old in [true, false] {
        let lua = Lua::new();
        lua.set_globals(lua.create_table().unwrap()).unwrap();
        assert!(references(&lua, old).unwrap().is_empty());
        assert!(
            lua.globals()
                .raw_get::<Option<Table>>("__songlua_actor_registry")
                .unwrap()
                .is_some()
        );
    }
    let lua = Lua::new();
    let actors = fixture(&lua, 4, 32, 2, true);
    let registry = song_lua_actor_registry(&lua).unwrap();
    registry.raw_set(2, Value::Nil).unwrap();
    assert_eq!(
        references(&lua, true).unwrap(),
        references(&lua, false).unwrap()
    );
    assert!(
        references(&lua, false)
            .unwrap()
            .iter()
            .all(|p| *p == actors[0].to_pointer() as usize)
    );
    registry.raw_set(2, "invalid actor").unwrap();
    assert_eq!(
        references(&lua, true).unwrap_err(),
        references(&lua, false).unwrap_err()
    );
    lua.globals()
        .raw_set("__songlua_actor_registry", false)
        .unwrap();
    assert_eq!(
        references(&lua, true).unwrap_err(),
        references(&lua, false).unwrap_err()
    );
    for old in [true, false] {
        let lua = Lua::new();
        let globals = lua.create_table().unwrap();
        let mt = lua.create_table().unwrap();
        mt.raw_set(
            "__newindex",
            lua.create_function(|_, (): ()| -> mlua::Result<()> {
                Err(mlua::Error::runtime("registry creation failed"))
            })
            .unwrap(),
        )
        .unwrap();
        globals.set_metatable(Some(mt)).unwrap();
        lua.set_globals(globals).unwrap();
        assert!(
            references(&lua, old)
                .unwrap_err()
                .contains("registry creation failed")
        );
    }
}

#[test]
fn lua_traversal_references_match_dense_sparse_and_empty_scans_without_churn() {
    for count in [0, 1, 33, 128] {
        for nested in [0, 4] {
            for referenced in [false, true] {
                let lua = Lua::new();
                fixture(&lua, count, 128, nested, referenced);
                assert_eq!(
                    references(&lua, true).unwrap(),
                    references(&lua, false).unwrap()
                );
                if count == 0 {
                    crate::perf::assert_no_churn(|| {
                        assert!(references(&lua, false).unwrap().is_empty())
                    });
                }
            }
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_traversal_bench_global_references() {
    for (count, fields, nested, referenced) in [
        (0, 0, 0, false),
        (0, 512, 0, false),
        (16, 64, 0, true),
        (64, 512, 0, true),
        (64, 64, 8, true),
        (0, 64, 8, false),
        (64, 64, 8, false),
        (1024, 64, 8, true),
    ] {
        let lua = Lua::new();
        fixture(&lua, count, fields, nested, referenced);
        assert_eq!(
            references(&lua, true).unwrap(),
            references(&lua, false).unwrap()
        );
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let units = (fields * (1 + nested) + count).max(1);
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "references_{count}_{fields}_{nested}_{referenced}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                units,
                || drop(black_box(references(&lua, black_box(old)).unwrap())),
            );
        }
    }
}
