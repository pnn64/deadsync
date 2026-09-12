use super::*;
use std::hint::black_box;

#[path = "children_clear_baseline.rs"]
mod baseline;

fn clear(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::remove_all_actor_children(lua, actor)
    } else {
        remove_all_actor_children(lua, actor)
    }
}

#[test]
fn lua_state_children_clear_preserves_aliases_metatables_and_actor_fields() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let registry = actor_children(&lua, &actor).unwrap();
        let child = lua.create_table().unwrap();
        let group = create_actor_child_group(&lua).unwrap();
        group.raw_set(1, &child).unwrap();
        child.raw_set("Name", "shared").unwrap();
        actor.raw_set(1, &child).unwrap();
        actor.raw_set(2, &child).unwrap();
        actor.raw_set("Name", "parent").unwrap();
        actor.raw_set(-1, "preserved").unwrap();
        registry.raw_set("shared", &group).unwrap();
        registry.raw_set(true, &child).unwrap();
        registry.raw_set(&child, &child).unwrap();
        registry
            .raw_set(lua.create_string([255, 0]).unwrap(), false)
            .unwrap();
        registry.raw_set(1, &child).unwrap();
        registry.raw_set(9, 42).unwrap();
        let mt: Table = lua
            .load(
                r#"return {
            __pairs=function() error('pairs') end,
            __len=function() error('len') end,
            __newindex=function() error('newindex') end,
        }"#,
            )
            .eval()
            .unwrap();
        registry.set_metatable(Some(mt.clone())).unwrap();
        clear(&lua, &actor, old).unwrap();
        assert!(registry.is_empty());
        assert_eq!(
            actor_children(&lua, &actor).unwrap().to_pointer(),
            registry.to_pointer()
        );
        assert_eq!(registry.metatable().unwrap().to_pointer(), mt.to_pointer());
        assert_eq!(actor.raw_len(), 0);
        assert_eq!(actor.get::<String>("Name").unwrap(), "parent");
        assert_eq!(actor.raw_get::<String>(-1).unwrap(), "preserved");
        assert_eq!(child.get::<String>("Name").unwrap(), "shared");
        assert_eq!(group.raw_len(), 1);
        registry.raw_set("reused", &child).unwrap();
        clear(&lua, &actor, old).unwrap();
        assert!(registry.is_empty());
    }
}

#[test]
fn lua_state_children_clear_preserves_missing_invalid_and_self_registry_behavior() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        actor.raw_set(1, 42).unwrap();
        clear(&lua, &actor, old).unwrap();
        assert!(
            actor
                .raw_get::<Table>("__songlua_children")
                .unwrap()
                .is_empty()
        );
        actor.raw_set(1, 42).unwrap();
        actor.raw_set("__songlua_children", false).unwrap();
        assert!(clear(&lua, &actor, old).is_err());
        // Array removal still happens before registry conversion can fail.
        assert_eq!(actor.raw_len(), 0);
        actor.raw_set(1, 42).unwrap();
        actor.raw_set("__songlua_children", &actor).unwrap();
        actor.raw_set("Name", "self registry").unwrap();
        clear(&lua, &actor, old).unwrap();
        assert!(actor.is_empty());
    }
}

#[test]
fn lua_state_children_clear_preserves_sparse_array_boundary() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    // Use the very same table and raw length: sparse-array boundaries are a Lua detail.
    let mut outcomes = Vec::new();
    for old in [true, false] {
        actor.clear().unwrap();
        actor.raw_set(1, 1).unwrap();
        actor.raw_set(3, 3).unwrap();
        actor.raw_set(99, 99).unwrap();
        let boundary = actor.raw_len();
        clear(&lua, &actor, old).unwrap();
        outcomes.push((
            boundary,
            [1, 3, 99].map(|i| actor.raw_get::<Option<i32>>(i).unwrap()),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn lua_state_children_clear_warm_named_registry_has_no_churn() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let registry = actor_children(&lua, &actor).unwrap();
    for i in 0..256 {
        registry.raw_set(format!("child_{i}"), i).unwrap();
    }
    crate::perf::assert_no_churn(|| clear(&lua, &actor, false).unwrap());
    assert!(registry.is_empty());
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_state_bench_children_clear() {
    for (count, kind) in [
        (0, "named"),
        (1, "named"),
        (8, "named"),
        (64, "named"),
        (512, "named"),
        (64, "mixed"),
        (64, "sequence"),
    ] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let registry = actor_children(&lua, &actor).unwrap();
        let child = lua.create_table().unwrap();
        let keys: Vec<_> = (0..count)
            .map(|i| lua.create_string(format!("child_{i}")).unwrap())
            .collect();
        let refill = || {
            for (i, key) in keys.iter().enumerate() {
                if kind != "sequence" {
                    registry.raw_set(key, &child).unwrap();
                }
                if kind != "named" {
                    actor.raw_set(i + 1, &child).unwrap();
                }
            }
        };
        for old in [true, false] {
            refill();
            clear(&lua, &actor, old).unwrap();
            assert!(registry.is_empty());
            assert_eq!(actor.raw_len(), 0);
        }
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("clear_{kind}_{count}/{}", if old { "old" } else { "new" }),
                if count <= 8 { 512 } else { 64 },
                count.max(1),
                || {
                    refill();
                    clear(&lua, black_box(&actor), black_box(old)).unwrap();
                },
            );
        }
    }
}
