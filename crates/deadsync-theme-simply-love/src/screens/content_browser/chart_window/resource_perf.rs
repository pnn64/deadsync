// Frozen originals from 3b0f61b73348db057f8d2a189ac86d46da9b6513; paired benchmarks exercise the production functions.
use super::*;
use crate::resource_perf_support::compare;
use std::hint::black_box;
fn original(skin: &Noteskin) -> Vec<(Arc<str>, bool)> {
    let mut textures: Vec<(Arc<str>, bool)> = Vec::new();
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot: &SpriteSlot| {
        for texture_slot in std::iter::once(slot).chain(slot.model_additive.as_deref()) {
            for key in std::iter::once(texture_slot.texture_key_shared())
                .chain(texture_slot.model_texture_keys.iter().cloned())
            {
                match textures.iter_mut().find(|(known, _)| *known == key) {
                    Some((_, model)) => *model |= slot.model.is_some(),
                    None => textures.push((key, slot.model.is_some())),
                }
            }
        }
    });
    textures
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

fn empty_skin() -> Noteskin {
    let mut skin = (*skin("default")).clone();
    skin.notes.clear();
    skin.note_layers.clear();
    skin.lift_note_layers.clear();
    skin.mine_layers.clear();
    skin.receptor_off.clear();
    skin.receptor_glow.clear();
    skin.receptor_idle_glow_layers.clear();
    skin.receptor_overlays.clear();
    skin.tap_explosions = Default::default();
    skin.tap_explosions_by_col.clear();
    skin
}

#[test]
fn resource_textures_empty_and_one_slot_match_original() {
    let mut empty = empty_skin();
    assert!(original(&empty).is_empty());
    assert!(preview_skin_textures(&empty).is_empty());
    empty.mine_layers = vec![Arc::from([skin("default").notes[0].clone()])];
    assert_eq!(original(&empty), preview_skin_textures(&empty));
    assert_eq!(preview_skin_textures(&empty).len(), 1);
}

#[test]
fn resource_textures_match_original_order_flags_and_ownership() {
    for name in ["default", "cel", "cyber"] {
        let skin = skin(name);
        let before = original(&skin);
        let after = preview_skin_textures(&skin);
        assert_eq!(before, after, "{name}");
        for ((a, _), (b, _)) in before.iter().zip(&after) {
            assert!(Arc::ptr_eq(a, b), "retained texture identity: {name}");
        }
    }
}

#[test]
fn resource_textures_merge_additive_materials_and_model_flags() {
    let mut skin = (*skin("cyber")).clone();
    let mut model = skin
        .notes
        .iter()
        .find(|s| s.model.is_some())
        .unwrap()
        .clone();
    let mut sprite = model.clone();
    sprite.model = None;
    sprite.model_texture_keys = Arc::from([Arc::<str>::from("same-key"), Arc::from("sprite-only")]);
    let mut additive = sprite.clone();
    additive.model_texture_keys =
        Arc::from([Arc::<str>::from("additive-only"), Arc::from("same-key")]);
    model.model_additive = Some(Arc::new(additive));
    model.model_texture_keys = Arc::from([Arc::<str>::from("same-key"), Arc::from("model-only")]);
    let layers: Arc<[SpriteSlot]> = Arc::from([sprite, model]);
    skin.note_layers = vec![Arc::clone(&layers); SKIN_COLS * 9];
    skin.lift_note_layers = skin.note_layers.clone();
    let before = original(&skin);
    let after = preview_skin_textures(&skin);
    assert_eq!(before, after);
    for key in ["same-key", "model-only", "additive-only"] {
        assert!(after.iter().find(|(name, _)| &**name == key).unwrap().1);
    }
    assert_eq!(
        after.iter().filter(|(key, _)| &**key == "same-key").count(),
        1
    );
    assert!(
        !after
            .iter()
            .find(|(name, _)| &**name == "sprite-only")
            .unwrap()
            .1
    );
    drop(skin);
    assert_eq!(before, after, "result owns the retained handles");
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_resources_textures() {
    let mut empty = empty_skin();
    for label in ["empty", "one"] {
        if label == "one" {
            empty.mine_layers = vec![Arc::from([skin("default").notes[0].clone()])];
        }
        compare(
            &format!("textures/{label}"),
            || {
                black_box(original(black_box(&empty)));
            },
            || {
                black_box(preview_skin_textures(black_box(&empty)));
            },
        );
    }
    for name in ["default", "cel", "cyber"] {
        let skin = skin(name);
        assert_eq!(original(&skin), preview_skin_textures(&skin));
        compare(
            &format!("textures/{name}"),
            || {
                black_box(original(black_box(&skin)));
            },
            || {
                black_box(preview_skin_textures(black_box(&skin)));
            },
        );
    }
}
