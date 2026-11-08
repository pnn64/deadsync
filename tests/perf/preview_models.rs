use super::*;
use crate::preview_dataflows_support::compare;
use deadsync_noteskin::NoteskinSlot;
use std::hint::black_box;

mod original {
    include!("preview_models_original.rs");
}

fn skin(name: &str) -> Arc<Noteskin> {
    crate::tests::init_paths();
    deadsync_assets::noteskin::load_itg_skin_cached(
        &deadsync_noteskin::Style {
            num_cols: 4,
            num_players: 1,
        },
        name,
    )
    .unwrap()
}

fn shared_models() -> Noteskin {
    let mut result = (*skin("cyber")).clone();
    let layers = result
        .note_layers
        .iter()
        .find(|layers| layers.iter().any(|slot| slot.model.is_some()))
        .unwrap()
        .clone();
    for slot in &mut result.note_layers {
        *slot = Arc::clone(&layers);
    }
    result.lift_note_layers = result.note_layers.clone();
    result
}

fn check_geometry(skin: &Noteskin) {
    let mut before = original::preview_skin_models(skin).into_cache();
    let mut after = preview_skin_models(skin).into_cache();
    assert_eq!(before.stats(), after.stats());
    assert_eq!(after.stats(), Default::default());
    before.begin_hit_stats(true);
    after.begin_hit_stats(true);
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot| {
        if slot.model.is_none() {
            return;
        }
        let a = before.model_geometry(slot).unwrap();
        let b = after.model_geometry(slot).unwrap();
        assert_eq!(a.0, b.0);
        assert_eq!(format!("{:?}", a.1), format!("{:?}", b.1));
        let again = after.model_geometry(slot).unwrap();
        assert!(Arc::ptr_eq(&b.1, &again.1), "geometry remains retained");
        for (time, beat) in [(0.0, 0.0), (0.4, 0.8), (8.2, 16.4)] {
            assert_eq!(
                format!("{:?}", before.draw_at(slot, time, beat)),
                format!("{:?}", after.draw_at(slot, time, beat))
            );
        }
    });
    assert_eq!(after.stats().misses, 0);
    assert_eq!(after.stats().saturated_misses, 0);
    assert_eq!(after.frame_stats().unregistered_misses, 0);
}

#[test]
fn preview_models_preserve_geometry_and_animation_for_native_skins() {
    for name in ["default", "cyber"] {
        check_geometry(&skin(name));
    }
}

#[test]
fn preview_models_preserve_shared_slot_identity_and_sealing() {
    let skin = shared_models();
    check_geometry(&skin);
    let mut cache = preview_skin_models(&skin).into_cache();
    let unknown = skin.note_layers[0]
        .iter()
        .find(|slot| slot.model.is_some())
        .unwrap()
        .clone();
    assert!(
        !cache.prewarm_slot(&unknown),
        "new IDs must not grow a sealed cache"
    );
    assert!(cache.stats().saturated_misses > 0);
}

#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_preview_dataflows_models() {
    let shared = shared_models();
    for (label, skin) in [
        ("sprite", skin("default")),
        ("cyber", skin("cyber")),
        ("shared", Arc::new(shared)),
    ] {
        compare(
            &format!("models/{label}"),
            || {
                black_box(original::preview_skin_models(black_box(&skin)));
            },
            || {
                black_box(preview_skin_models(black_box(&skin)));
            },
        );
    }
}
