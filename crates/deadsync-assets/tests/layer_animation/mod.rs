//! Compare production layer preparation with the frozen faef9065f implementation.
use super::super::{NUM_QUANTIZATIONS, animate_layer_groups};
use super::*;
use deadsync_noteskin::NoteskinSlot;
use std::hint::black_box;

mod baseline;

type Animate =
    fn(&mut [Arc<[SpriteSlot]>], usize, NotePartAnimation, NotePartTextureTranslate, bool);

fn fixture(kind: &str, shared: bool) -> Vec<Arc<[SpriteSlot]>> {
    let mut groups = Vec::new();
    for col in 0..4 {
        for quant in 0..NUM_QUANTIZATIONS {
            if shared && quant > 0 {
                groups.push(Arc::clone(groups.last().unwrap()));
                continue;
            }
            let layers: Arc<[SpriteSlot]> = (0..3)
                .map(|layer| {
                    let mut slot = test_model_slot();
                    slot.set_rotation_deg(col * 90);
                    slot.model_draw.tint = [quant as f32 / 9.0, layer as f32 / 3.0, 0.5, 0.75];
                    if kind != "model" && !(kind == "mixed" && layer == 1) {
                        slot.model = None;
                    }
                    let key = if kind == "static" {
                        "layer-perf.png"
                    } else {
                        "layer-perf 8x4.png"
                    };
                    if let SpriteSource::Atlas {
                        texture_key,
                        tex_dims,
                        ..
                    } = Arc::get_mut(&mut slot.source).unwrap()
                    {
                        *texture_key = Arc::from(key);
                        *tex_dims = (512, 256);
                    }
                    slot.def.size = [64; 2];
                    slot.source_size = [64; 2];
                    if kind == "animated" || (kind == "mixed" && layer == 2) {
                        baseline::apply(
                            &mut slot,
                            NotePartAnimation::default(),
                            NotePartTextureTranslate::default(),
                            true,
                        );
                    }
                    slot
                })
                .collect();
            groups.push(layers);
        }
    }
    groups
}

fn snapshot(slot: &SpriteSlot) -> String {
    // Clone assigns a fresh cache identity. Compare every other stored field.
    let mut snapshot = format!("{slot:?}").replacen(
        &format!("stable_id: {}", slot.stable_id()),
        "stable_id: <identity>",
        1,
    );
    // Sequential frames have equivalent explicit and implicit representations.
    // Playback and UV checks below still compare their observable results.
    if let SpriteSource::Animated {
        frame_count,
        frame_indices,
        ..
    } = slot.source.as_ref()
        && frame_indices
            .as_deref()
            .is_none_or(|indices| indices.is_empty() || indices.iter().copied().eq(0..*frame_count))
    {
        snapshot = snapshot.replacen(
            &format!("frame_indices: {frame_indices:?}"),
            "frame_indices: <sequential>",
            1,
        );
    }
    snapshot
}

#[test]
fn layer_animation_matches_baseline_and_preserves_other_owners() {
    for kind in ["model", "static", "atlas", "animated", "mixed"] {
        for shared in [false, true] {
            for retained in [false, true] {
                for spacing in [[0.0, 0.0], [0.125, 0.0], [0.0, 0.25], [0.125, 0.25]] {
                    for beat_based in [false, true] {
                        let mut old = fixture(kind, shared);
                        let mut new = fixture(kind, shared);
                        old[NUM_QUANTIZATIONS - 1] = Arc::from([]);
                        new[NUM_QUANTIZATIONS - 1] = Arc::from([]);
                        // A trailing incomplete column must remain untouched.
                        old.push(Arc::from([test_model_slot()]));
                        new.push(Arc::from([test_model_slot()]));
                        let owners = retained.then(|| new.clone());
                        let before: Vec<_> = new
                            .iter()
                            .flat_map(|layers| layers.iter())
                            .map(snapshot)
                            .collect();
                        let animation = NotePartAnimation {
                            length: 2.5,
                            vivid: true,
                        };
                        let translate = NotePartTextureTranslate {
                            note_color_spacing: spacing,
                            ..Default::default()
                        };
                        baseline::animate(
                            &mut old,
                            NUM_QUANTIZATIONS,
                            animation,
                            translate,
                            beat_based,
                        );
                        animate_layer_groups(
                            &mut new,
                            NUM_QUANTIZATIONS,
                            animation,
                            translate,
                            beat_based,
                        );
                        for (old, new) in old.iter().zip(&new) {
                            assert_eq!(old.len(), new.len());
                            for (old, new) in old.iter().zip(new.iter()) {
                                assert_eq!(
                                    snapshot(old),
                                    snapshot(new),
                                    "{kind}, {shared}, {retained}"
                                );
                                for t in [0.0, 0.125, 1.0, 2.5, 12.0] {
                                    let frame = old.frame_index(t, t * 2.0);
                                    assert_eq!(new.frame_index(t, t * 2.0), frame);
                                    assert_eq!(
                                        old.uv_for_frame_at(frame, t),
                                        new.uv_for_frame_at(frame, t)
                                    );
                                    assert_eq!(
                                        format!("{:?}", old.model_draw_at(t, t)),
                                        format!("{:?}", new.model_draw_at(t, t))
                                    );
                                }
                            }
                        }
                        if let Some(owners) = owners {
                            assert_eq!(
                                owners
                                    .iter()
                                    .flat_map(|layers| layers.iter())
                                    .map(snapshot)
                                    .collect::<Vec<_>>(),
                                before
                            );
                        }
                        for a in 0..new.len() {
                            for b in 0..new.len() {
                                assert_eq!(
                                    Arc::ptr_eq(&old[a], &old[b]),
                                    Arc::ptr_eq(&new[a], &new[b])
                                );
                                if a != b
                                    && !new[a].is_empty()
                                    && !new[b].is_empty()
                                    && !Arc::ptr_eq(&new[a], &new[b])
                                {
                                    assert_ne!(new[a][0].stable_id(), new[b][0].stable_id());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark; run alone on a pinned CPU"]
fn benchmark_layer_animation() {
    for kind in ["model", "static", "atlas", "animated"] {
        for shared in [true, false] {
            let mut variants: [(&str, Animate); 2] = [
                ("before", baseline::animate),
                ("after", animate_layer_groups),
            ];
            if std::env::var_os("PERF_REVERSE").is_some() {
                variants.reverse();
            }
            for (variant, animate) in variants {
                let animate = black_box(animate);
                crate::perf::measure_sampled_with_setup(
                    &format!(
                        "layers/{kind}/{}/{variant}",
                        if shared { "shared" } else { "colors" }
                    ),
                    1024,
                    4 * NUM_QUANTIZATIONS * 3,
                    || fixture(kind, shared),
                    |groups| {
                        animate(
                            black_box(groups),
                            NUM_QUANTIZATIONS,
                            NotePartAnimation::default(),
                            NotePartTextureTranslate::default(),
                            true,
                        )
                    },
                );
            }
        }
    }
}

#[test]
fn unchanged_animation_groups_have_no_allocation_churn() {
    for kind in ["model", "static", "animated"] {
        for shared in [false, true] {
            let mut groups = fixture(kind, shared);
            let owners = groups.clone();
            // Measure layer preparation after texture metadata discovery,
            // independently of test order or registry cache misses.
            for slot in groups.iter().flat_map(|layers| layers.iter()) {
                deadlib_assets::sprite_sheet_dims(slot.texture_key());
            }
            crate::perf::assert_no_churn(|| {
                animate_layer_groups(
                    &mut groups,
                    NUM_QUANTIZATIONS,
                    NotePartAnimation::default(),
                    NotePartTextureTranslate::default(),
                    true,
                );
            });
            for (group, owner) in groups.iter().zip(&owners) {
                assert!(Arc::ptr_eq(group, owner));
            }
        }
    }
}
