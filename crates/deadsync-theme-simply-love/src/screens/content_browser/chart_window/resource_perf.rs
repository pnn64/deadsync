use super::*;


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
    skin.hold_columns.clear();
    skin.roll_columns.clear();
    for visuals in [&mut skin.hold, &mut skin.roll] {
        (visuals.head_inactive, visuals.head_active) = (None, None);
        (visuals.head_inactive_layers, visuals.head_active_layers) = (None, None);
        (visuals.body_inactive, visuals.body_active) = (None, None);
        (visuals.bottomcap_inactive, visuals.bottomcap_active) = (None, None);
    }
    skin
}

#[test]
fn resource_textures_empty_and_one_slot() {
    let mut empty = empty_skin();
    assert!(preview_skin_textures(&empty).is_empty());
    empty.mine_layers = vec![Arc::from([skin("default").notes[0].clone()])];
    assert_eq!(preview_skin_textures(&empty).len(), 1);
}
