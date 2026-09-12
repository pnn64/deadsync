use super::*;
use std::hint::black_box;

#[path = "child_walk_baseline.rs"]
mod baseline;

fn pointers(values: &[Table]) -> Vec<usize> {
    values.iter().map(|v| v.to_pointer() as usize).collect()
}

struct Fixture {
    lua: Lua,
    actor: Table,
    children: Vec<Table>,
}

impl Fixture {
    fn new(count: usize, repeats: usize, grouped: bool) -> Self {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let children: Vec<_> = (0..count).map(|_| lua.create_table().unwrap()).collect();
        for repeat in 0..repeats {
            for (i, child) in children.iter().enumerate() {
                actor
                    .raw_set(repeat * count + i + 1, child.clone())
                    .unwrap();
            }
        }
        let named = actor_children(&lua, &actor).unwrap();
        if grouped {
            let group = lua.create_table().unwrap();
            let meta = lua.create_table().unwrap();
            meta.set(SONG_LUA_CHILD_GROUP_KEY, true).unwrap();
            group.set_metatable(Some(meta)).unwrap();
            for (i, child) in children.iter().rev().enumerate() {
                group.raw_set(i + 1, child.clone()).unwrap();
            }
            named.raw_set("group", group).unwrap();
        }
        Self {
            lua,
            actor,
            children,
        }
    }
    fn collect(&self, old: bool) -> Vec<Table> {
        if old {
            baseline::actor_direct_children(&self.lua, &self.actor).unwrap()
        } else {
            actor_direct_children(&self.lua, &self.actor).unwrap()
        }
    }
}

#[test]
fn lua_work_children_keep_first_seen_order_across_thresholds_and_named_groups() {
    for count in [0, 1, 4, 31, 32, 33, 64, 128, 1024] {
        for repeats in [1, 3] {
            let fixture = Fixture::new(count, repeats, true);
            let old = pointers(&fixture.collect(true));
            assert_eq!(pointers(&fixture.collect(false)), old);
            assert_eq!(old, pointers(&fixture.children));
            // Group-only children keep Lua's own table visitation order.
            for i in 1..=fixture.actor.raw_len() {
                fixture.actor.raw_set(i, Value::Nil).unwrap();
            }
            let named = actor_children(&fixture.lua, &fixture.actor).unwrap();
            named
                .set("extra", fixture.lua.create_table().unwrap())
                .unwrap();
            named.set("ignored", 4).unwrap();
            assert_eq!(
                pointers(&fixture.collect(false)),
                pointers(&fixture.collect(true))
            );
        }
    }
}

#[test]
fn lua_work_child_membership_preserves_duplicates_after_spilling_and_random_orders() {
    let fixture = Fixture::new(2048, 1, false);
    let mut old_seen = Vec::new();
    let mut seen = ActorChildPointers::new();
    let mut old = Vec::new();
    let mut new = Vec::new();
    let mut seed = 17u64;
    for _ in 0..20_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let index = (seed >> 32) as usize % fixture.children.len();
        baseline::push_unique_actor_child(&mut old, &mut old_seen, fixture.children[index].clone());
        seen.push(&mut new, fixture.children[index].clone());
    }
    assert_eq!(pointers(&new), pointers(&old));
    assert!(matches!(seen, ActorChildPointers::Large(_)));
}

#[test]
fn lua_work_child_walk_preserves_late_group_errors_and_missing_children_creation() {
    let fixture = Fixture::new(40, 2, false);
    let invalid = fixture.lua.create_table().unwrap();
    let meta = fixture.lua.create_table().unwrap();
    let error_meta = fixture.lua.create_table().unwrap();
    error_meta
        .set(
            "__index",
            fixture
                .lua
                .load("return function() error('group lookup failed') end")
                .eval::<Function>()
                .unwrap(),
        )
        .unwrap();
    meta.set_metatable(Some(error_meta)).unwrap();
    invalid.set_metatable(Some(meta)).unwrap();
    actor_children(&fixture.lua, &fixture.actor)
        .unwrap()
        .set("invalid", invalid)
        .unwrap();
    let old = baseline::actor_direct_children(&fixture.lua, &fixture.actor)
        .unwrap_err()
        .to_string();
    let new = actor_direct_children(&fixture.lua, &fixture.actor)
        .unwrap_err()
        .to_string();
    assert_eq!(new, old);
    for old in [true, false] {
        let actor = fixture.lua.create_table().unwrap();
        actor.raw_set(1, 7).unwrap();
        actor.raw_set(2, fixture.children[0].clone()).unwrap();
        let values = if old {
            baseline::actor_direct_children(&fixture.lua, &actor)
        } else {
            actor_direct_children(&fixture.lua, &actor)
        }
        .unwrap();
        assert_eq!(pointers(&values), pointers(&fixture.children[..1]));
        assert!(
            actor
                .get::<Option<Table>>("__songlua_children")
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn lua_work_small_child_membership_has_no_allocation_churn() {
    let fixture = Fixture::new(32, 1, false);
    // Shared Lua handles have already been cloned while constructing the actor.
    let mut out = Vec::with_capacity(32);
    crate::perf::assert_no_churn(|| {
        let mut seen = ActorChildPointers::new();
        for child in fixture.children.iter().cycle().take(96) {
            seen.push(&mut out, child.clone());
        }
    });
    assert_eq!(out.len(), 32);
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_work_bench_children() {
    for (count, repeats, groups) in [
        (0, 1, false),
        (4, 1, false),
        (32, 1, false),
        (33, 1, false),
        (128, 1, false),
        (1024, 1, false),
        (1024, 4, true),
        (4, 128, true),
    ] {
        let fixture = Fixture::new(count, repeats, groups);
        assert_eq!(
            pointers(&fixture.collect(false)),
            pointers(&fixture.collect(true))
        );
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "children_{count}_repeat_{repeats}_groups_{groups}/{}",
                    if old { "old" } else { "new" }
                ),
                64,
                (count * (repeats + usize::from(groups))).max(1),
                || black_box(fixture.collect(old)),
            );
        }
    }
}
