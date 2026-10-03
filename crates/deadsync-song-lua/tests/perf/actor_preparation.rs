use super::*;
use std::hint::black_box;

#[path = "actor_preparation/baseline.rs"]
mod baseline;

type CompileActor = SongLuaOverlayCompileActor<SongLuaOverlayKind<(), (), ()>>;

fn actors(lua: &mlua::Lua, parents: &[Option<usize>], visual: Option<usize>) -> Vec<CompileActor> {
    parents
        .iter()
        .enumerate()
        .map(|(index, &parent_index)| CompileActor {
            table: lua.create_table().unwrap(),
            message_sounds: Vec::new(),
            actor: SongLuaOverlayActor {
                kind: if visual == Some(index) {
                    SongLuaOverlayKind::NoteskinActor {
                        slots: Arc::default(),
                    }
                } else {
                    SongLuaOverlayKind::ActorFrame
                },
                name: None,
                parent_index,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            },
        })
        .collect()
}

#[test]
fn visual_presence_matches_original_for_roots_branches_and_forward_parents() {
    let lua = mlua::Lua::new();
    for parents in [
        vec![None],
        vec![None, Some(0), Some(1), Some(1), None, Some(4)],
        vec![Some(3), Some(3), Some(1), None, Some(99)],
        (0usize..64).map(|index| index.checked_sub(1)).collect(),
    ] {
        for visual in (0..parents.len()).map(Some).chain([None]) {
            let actors = actors(&lua, &parents, visual);
            for root in 0..actors.len() {
                assert_eq!(
                    overlay_actor_tree_has_visual(&actors, root),
                    baseline::overlay_actor_tree_has_visual(&actors, root),
                    "root={root}, visual={visual:?}, parents={parents:?}"
                );
            }
        }
    }
}

#[test]
fn visual_presence_does_not_allocate_for_large_trees() {
    let lua = mlua::Lua::new();
    let parents: Vec<_> = (0usize..512).map(|index| index.checked_sub(1)).collect();
    for visual in [None, Some(1), Some(511)] {
        let actors = actors(&lua, &parents, visual);
        crate::perf::assert_no_churn(|| {
            black_box(overlay_actor_tree_has_visual(black_box(&actors), 0));
        });
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn actor_preparation_benchmark() {
    let lua = mlua::Lua::new();
    for (shape, parents) in [
        (
            "flat",
            (0..512).map(|i| (i != 0).then_some(0)).collect::<Vec<_>>(),
        ),
        ("nested", (0usize..512).map(|i| i.checked_sub(1)).collect()),
    ] {
        for (name, visual) in [("absent", None), ("early", Some(1)), ("late", Some(511))] {
            let actors = actors(&lua, &parents, visual);
            let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
                [false, true]
            } else {
                [true, false]
            };
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "visual/{shape}/{name}/{}",
                        if old { "original" } else { "current" }
                    ),
                    512,
                    1,
                    || {
                        if old {
                            baseline::overlay_actor_tree_has_visual(
                                black_box(&actors),
                                black_box(0),
                            )
                        } else {
                            overlay_actor_tree_has_visual(black_box(&actors), black_box(0))
                        }
                    },
                );
            }
        }
    }
}
