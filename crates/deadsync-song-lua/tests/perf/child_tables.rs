use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "child_tables_baseline.rs"]
mod baseline;

fn direct(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<Vec<Table>> {
    if old {
        baseline::actor_direct_children(lua, actor)
    } else {
        actor_direct_children(lua, actor)
    }
}

fn named(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<Table> {
    if old {
        baseline::actor_named_children(lua, actor)
    } else {
        actor_named_children(lua, actor)
    }
}

fn pointers(tables: &[Table]) -> Vec<usize> {
    tables.iter().map(|t| t.to_pointer() as usize).collect()
}

fn entries(table: &Table) -> Vec<(String, String)> {
    let mut out: Vec<_> = table
        .pairs::<Value, Value>()
        .map(|pair| {
            let (key, value) = pair.unwrap();
            let key = match key {
                Value::String(s) => format!("{:?}", s.as_bytes().as_ref()),
                v => format!("{v:?}"),
            };
            let value = match value {
                Value::Table(t) if actor_is_child_group(&t).unwrap() => format!(
                    "group:{:?}",
                    pointers(
                        &t.sequence_values::<Table>()
                            .collect::<mlua::Result<Vec<_>>>()
                            .unwrap()
                    )
                ),
                v => format!("{v:?}"),
            };
            (key, value)
        })
        .collect();
    out.sort();
    out
}

fn fixture(lua: &Lua, count: usize, kind: &str) -> Table {
    let actor = lua.create_table().unwrap();
    let named = actor_children(lua, &actor).unwrap();
    let children: Vec<_> = (0..count)
        .map(|index| {
            let child = lua.create_table().unwrap();
            child.raw_set("Name", format!("child_{index}")).unwrap();
            child
        })
        .collect();
    for (index, child) in children.iter().enumerate() {
        if kind == "sequence" || kind == "mixed" && index % 2 == 0 {
            actor.raw_set(actor.raw_len() + 1, child).unwrap();
        }
        if kind != "sequence" {
            let value = match kind {
                "ignored" => Value::Integer(index as i64),
                "duplicates" => Value::Table(children[index % count.min(4)].clone()),
                "groups" => {
                    let group = create_actor_child_group(lua).unwrap();
                    group.raw_set(1, child).unwrap();
                    group.raw_set(2, child).unwrap();
                    Value::Table(group)
                }
                _ => Value::Table(child.clone()),
            };
            named.raw_set(format!("child_{index}"), value).unwrap();
        }
    }
    actor
}

#[test]
fn lua_traversal_children_preserve_order_groups_aliases_and_mixed_keys() {
    for count in [0, 1, 8, 32, 33, 128] {
        for kind in [
            "named",
            "sequence",
            "mixed",
            "groups",
            "duplicates",
            "ignored",
        ] {
            let lua = Lua::new();
            let actor = fixture(&lua, count, kind);
            let source = actor_children(&lua, &actor).unwrap();
            let extra = lua.create_table().unwrap();
            source
                .raw_set(lua.create_string([0xff, 0]).unwrap(), extra.clone())
                .unwrap();
            source.raw_set(true, extra.clone()).unwrap();
            source.raw_set(-3, 42).unwrap();
            source.raw_set(extra.clone(), "ignored").unwrap();
            let before = entries(&source);
            assert_eq!(
                pointers(&direct(&lua, &actor, true).unwrap()),
                pointers(&direct(&lua, &actor, false).unwrap())
            );
            let old = named(&lua, &actor, true).unwrap();
            let new = named(&lua, &actor, false).unwrap();
            assert_eq!(entries(&old), entries(&new));
            assert_ne!(old.to_pointer(), source.to_pointer());
            assert_ne!(new.to_pointer(), source.to_pointer());
            assert_eq!(
                new.raw_get::<Table>(true).unwrap().to_pointer(),
                extra.to_pointer()
            );
            assert_eq!(entries(&source), before);
        }
    }
}

#[test]
fn lua_traversal_children_keep_shared_group_append_and_missing_registry_behavior() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.raw_set(1, 42).unwrap();
        assert!(direct(&lua, &actor, old).unwrap().is_empty());
        let source = actor.raw_get::<Table>("__songlua_children").unwrap();
        let child = lua.create_table().unwrap();
        child.raw_set("Name", "shared").unwrap();
        actor.raw_set(2, child.clone()).unwrap();
        let group = create_actor_child_group(&lua).unwrap();
        group.raw_set(1, child.clone()).unwrap();
        source.raw_set("shared", group.clone()).unwrap();
        let out = named(&lua, &actor, old).unwrap();
        assert_eq!(
            out.raw_get::<Table>("shared").unwrap().to_pointer(),
            group.to_pointer()
        );
        assert_eq!(group.raw_len(), 2);
        assert_eq!(
            pointers(&direct(&lua, &actor, old).unwrap()),
            pointers(&[child])
        );
    }
}

#[test]
fn lua_traversal_children_preserve_lookup_mutation_order_and_errors() {
    let lua = Lua::new();
    let actor = fixture(&lua, 8, "named");
    let source = actor_children(&lua, &actor).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    // A group-marker lookup can execute Lua through its metatable. Mutate an
    // existing entry during traversal without changing Lua's hash layout.
    let original: Vec<(String, Table)> = source
        .pairs::<String, Table>()
        .collect::<mlua::Result<_>>()
        .unwrap();
    for (name, child) in &original {
        let mt = lua.create_table().unwrap();
        let index_mt = lua.create_table().unwrap();
        let events = trace.clone();
        let label = name.clone();
        let source = source.clone();
        index_mt
            .raw_set(
                "__index",
                lua.create_function(move |_, (_table, _key): (Table, Value)| {
                    events.borrow_mut().push(label.clone());
                    source.raw_set("child_7", 17)?;
                    Ok(false)
                })
                .unwrap(),
            )
            .unwrap();
        mt.set_metatable(Some(index_mt)).unwrap();
        child.set_metatable(Some(mt)).unwrap();
    }
    let mut outcomes = Vec::new();
    for old in [true, false] {
        for (name, child) in &original {
            source.raw_set(name.as_str(), child).unwrap();
        }
        trace.borrow_mut().clear();
        outcomes.push((
            pointers(&direct(&lua, &actor, old).unwrap()),
            trace.borrow().clone(),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    let bad = lua.create_table().unwrap();
    let mt = lua.create_table().unwrap();
    let error_mt = lua.create_table().unwrap();
    error_mt
        .raw_set(
            "__index",
            lua.create_function(|_, (): ()| -> mlua::Result<()> {
                Err(mlua::Error::runtime("group lookup failed"))
            })
            .unwrap(),
        )
        .unwrap();
    mt.set_metatable(Some(error_mt)).unwrap();
    bad.set_metatable(Some(mt)).unwrap();
    source.raw_set("bad", bad).unwrap();
    let old = direct(&lua, &actor, true).unwrap_err().to_string();
    let new = direct(&lua, &actor, false).unwrap_err().to_string();
    assert_eq!(old, new);
    for old in [true, false] {
        let actor = lua.create_table().unwrap();
        actor.raw_set("__songlua_children", 42).unwrap();
        assert!(direct(&lua, &actor, old).is_err());
        assert!(named(&lua, &actor, old).is_err());
    }
}

#[test]
fn lua_traversal_children_ignored_fields_have_no_allocation_churn() {
    let lua = Lua::new();
    let actor = fixture(&lua, 128, "ignored");
    direct(&lua, &actor, false).unwrap();
    crate::perf::assert_no_churn(|| assert!(direct(&lua, &actor, false).unwrap().is_empty()));
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_traversal_bench_child_tables() {
    for (count, kind) in [
        (0, "named"),
        (8, "named"),
        (64, "named"),
        (512, "named"),
        (64, "mixed"),
        (64, "groups"),
        (128, "ignored"),
        (64, "sequence"),
        (128, "duplicates"),
    ] {
        let lua = Lua::new();
        let actor = fixture(&lua, count, kind);
        assert_eq!(
            pointers(&direct(&lua, &actor, true).unwrap()),
            pointers(&direct(&lua, &actor, false).unwrap())
        );
        assert_eq!(
            entries(&named(&lua, &actor, true).unwrap()),
            entries(&named(&lua, &actor, false).unwrap())
        );
        lua.gc_collect().unwrap();
        lua.gc_stop();
        for named_read in [false, true] {
            let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                [false, true]
            } else {
                [true, false]
            };
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "children_{kind}_{count}_named_{named_read}/{}",
                        if old { "old" } else { "new" }
                    ),
                    64,
                    count.max(1),
                    || {
                        if named_read {
                            drop(black_box(named(&lua, &actor, black_box(old)).unwrap()));
                        } else {
                            drop(black_box(direct(&lua, &actor, black_box(old)).unwrap()));
                        }
                    },
                );
            }
        }
    }
}
