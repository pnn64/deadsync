use super::*;
use crate::{
    SongLuaOverlayRuntimeUpdateSample as Sample, SongLuaOverlayRuntimeUpdateTrack as Track,
    SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value,
};

type Slot = deadsync_assets::noteskin::SpriteSlot;
type Overlay = SongLuaOverlayActor<Slot>;

fn actor(
    name: &str,
    kind: SongLuaOverlayKind<Slot>,
    parent_index: Option<usize>,
    initial_state: SongLuaOverlayState,
) -> Overlay {
    Overlay {
        name: Some(name.into()),
        kind,
        parent_index,
        initial_state,
        message_commands: Vec::new(),
    }
}

fn track(overlay_index: usize, target: Target, samples: &[(f32, Value)]) -> Track {
    Track {
        overlay_index,
        target,
        samples: samples
            .iter()
            .map(|(second, value)| Sample {
                second: *second,
                value: value.clone(),
            })
            .collect(),
    }
}

fn sample_states(
    mut overlays: Vec<Overlay>,
    mut tracks: Vec<Track>,
    times: &[f32],
    mut check: impl FnMut(f32, &[SongLuaOverlayState], &[SongLuaOverlayState]),
) {
    tracks.sort_by_key(|track| track.overlay_index);
    overlays.push(actor(
        "CapturedUpdates",
        SongLuaOverlayKind::UpdateTracks {
            tracks,
            draw_frames: Arc::from([]),
        },
        None,
        SongLuaOverlayState::default(),
    ));
    let ranges = vec![0..0; overlays.len()];
    let events = vec![Vec::new(); overlays.len()];
    let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
    let mut caches = Vec::new();
    let mut local = Vec::new();
    let mut composed = Vec::new();
    for &now in times {
        song_lua_overlay_state_sets_from_into::<Slot>(
            now,
            &overlays,
            &events,
            &[],
            &ranges,
            640.0,
            480.0,
            &mut order,
            &mut caches,
            &mut local,
            &mut composed,
            [0.0; 2],
        );
        check(now, &local, &composed);
    }
}

#[test]
fn first_visible_multitap_frame_has_its_position_and_arrow_rotation() {
    let reveal = 10.0_f32;
    let prior = reveal - 1.0 / 60.0;
    for target in [Target::Visible, Target::Hibernating] {
        let hiding = target == Target::Hibernating;
        let hidden = SongLuaOverlayState {
            visible: hiding,
            hibernating: hiding,
            ..Default::default()
        };
        let overlays = vec![
            actor(
                "MultitapFrameP1",
                SongLuaOverlayKind::ActorFrame,
                None,
                SongLuaOverlayState {
                    x: 160.0,
                    y: 240.0,
                    ..Default::default()
                },
            ),
            actor(
                "MultitapP1_1",
                SongLuaOverlayKind::ActorFrame,
                Some(0),
                hidden,
            ),
            actor(
                "MultitapArrowP1_1",
                SongLuaOverlayKind::Sprite {
                    texture_path: PathBuf::from("Down Tap Note.png"),
                    texture_key: Arc::from("down-arrow"),
                    states: Arc::from([]),
                    textures: Arc::from([]),
                },
                Some(1),
                SongLuaOverlayState::default(),
            ),
        ];
        // The factory writes visibility while dormant. Position and lane
        // rotation first enter capture on the reveal frame, without an invented
        // sample at the actor's default local origin.
        let tracks = vec![
            track(
                1,
                target,
                &[
                    (0.0, Value::Bool(hiding)),
                    (prior, Value::Bool(hiding)),
                    (reveal, Value::Bool(!hiding)),
                ],
            ),
            track(1, Target::X, &[(reveal, Value::F32(-96.0))]),
            track(1, Target::Y, &[(reveal, Value::F32(220.0))]),
            track(
                2,
                Target::BaseRotation,
                &[(reveal, Value::Vec3([0.0, 0.0, 90.0]))],
            ),
        ];
        // Revisit the boundary after a backward seek using the same caches.
        sample_states(
            overlays,
            tracks,
            &[
                prior,
                prior.midpoint(reveal),
                reveal.next_down(),
                reveal,
                reveal + 1.0 / 60.0,
                prior,
                reveal,
            ],
            |now, local, composed| {
                if now < reveal {
                    assert!(
                        !composed[2].draw_visible(),
                        "{target:?} revealed a default arrow at {now}"
                    );
                    assert_eq!([local[1].x, local[1].y], [0.0, 0.0]);
                    assert_eq!(local[2].base_rotation, [0.0; 3]);
                } else {
                    assert!(
                        composed[2].draw_visible(),
                        "{target:?} stayed hidden at {now}"
                    );
                    assert_eq!([local[1].x, local[1].y], [-96.0, 220.0]);
                    assert_eq!(local[2].base_rotation, [0.0, 0.0, 90.0]);
                    assert_eq!([composed[2].x, composed[2].y], [64.0, 460.0]);
                    assert_eq!(composed[2].rot_z_deg, 90.0);
                }
            },
        );
    }
}

#[test]
fn hiding_actor_keeps_its_last_visible_pose_until_the_recorded_frame() {
    let start = 1.0_f32;
    let hide = start + 1.0 / 60.0;
    for target in [Target::Visible, Target::Hibernating] {
        let hibernate = target == Target::Hibernating;
        let overlays = vec![actor(
            "HideAndReset",
            SongLuaOverlayKind::Quad,
            None,
            SongLuaOverlayState {
                x: 96.0,
                y: 180.0,
                ..Default::default()
            },
        )];
        let tracks = vec![
            track(
                0,
                target,
                &[
                    (start, Value::Bool(!hibernate)),
                    (hide, Value::Bool(hibernate)),
                ],
            ),
            track(
                0,
                Target::X,
                &[(start, Value::F32(96.0)), (hide, Value::F32(0.0))],
            ),
            track(
                0,
                Target::Y,
                &[(start, Value::F32(180.0)), (hide, Value::F32(0.0))],
            ),
            track(
                0,
                Target::CropBottom,
                &[(start, Value::F32(0.8)), (hide, Value::F32(0.0))],
            ),
        ];
        sample_states(
            overlays,
            tracks,
            &[start, start.midpoint(hide), hide.next_down(), hide],
            |now, local, composed| {
                assert_eq!(
                    composed[0].draw_visible(),
                    now < hide,
                    "{target:?} at {now}"
                );
                let expected = if now < hide {
                    [96.0, 180.0, 0.8]
                } else {
                    [0.0; 3]
                };
                assert_eq!(
                    [local[0].x, local[0].y, local[0].cropbottom],
                    expected,
                    "{target:?} moved toward a hidden pose at {now}"
                );
            },
        );
    }
}

#[test]
fn visibility_edges_do_not_snap_unrelated_actor_geometry() {
    let frame = 1.0_f32 / 60.0;
    for target in [Target::Visible, Target::Hibernating] {
        let hibernating = target == Target::Hibernating;
        let overlays = vec![
            actor(
                "VisibilityOwner",
                SongLuaOverlayKind::Quad,
                None,
                SongLuaOverlayState {
                    visible: hibernating,
                    hibernating,
                    ..Default::default()
                },
            ),
            actor(
                "IndependentMotion",
                SongLuaOverlayKind::Quad,
                None,
                SongLuaOverlayState::default(),
            ),
        ];
        let tracks = vec![
            track(
                0,
                target,
                &[
                    (0.0, Value::Bool(hibernating)),
                    (frame, Value::Bool(!hibernating)),
                    (2.0 * frame, Value::Bool(hibernating)),
                ],
            ),
            track(
                1,
                Target::X,
                &[
                    (0.0, Value::F32(0.0)),
                    (frame, Value::F32(60.0)),
                    (2.0 * frame, Value::F32(120.0)),
                ],
            ),
            track(
                1,
                Target::CropBottom,
                &[
                    (0.0, Value::F32(0.0)),
                    (frame, Value::F32(0.8)),
                    (2.0 * frame, Value::F32(0.0)),
                ],
            ),
        ];
        sample_states(
            overlays,
            tracks,
            &[frame * 0.25, frame, frame * 1.25, frame * 2.0],
            |now, local, composed| {
                let phase = now / frame;
                let expected_crop = 0.8 * if phase <= 1.0 { phase } else { 2.0 - phase };
                assert_eq!(
                    composed[0].draw_visible(),
                    (1.0..2.0).contains(&phase),
                    "{target:?} at {now}"
                );
                assert!(
                    (local[1].x - phase * 60.0).abs() < 0.0001,
                    "{target:?} changed independent motion at {now}: {}",
                    local[1].x
                );
                assert!(
                    (local[1].cropbottom - expected_crop).abs() < 0.0001,
                    "{target:?} changed independent crop at {now}: {}",
                    local[1].cropbottom
                );
            },
        );
    }
}
