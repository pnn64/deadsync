use deadsync_profile::{
    NoteSkin, PlayerOptionsData, append_player_options_section, load_player_options_section,
    migrate_noteskin_parts,
};
use std::collections::HashMap;

#[test]
fn legacy_overrides_keep_precedence_and_mine_models() {
    let mut options = PlayerOptionsData {
        noteskin: NoteSkin::new(
            "cel-workshop?arrows=red&mines=blue&mine_size=smaller&receptors=white",
        ),
        receptor_noteskin: Some(NoteSkin::new("metal")),
        ..PlayerOptionsData::default()
    };
    migrate_noteskin_parts(&mut options);
    assert_eq!(options.noteskin.as_str(), "cel-workshop");
    assert_eq!(
        options.mine_noteskin.as_ref().unwrap().as_str(),
        "cel-workshop?mines=blue&mine_size=smaller"
    );
    assert_eq!(options.receptor_noteskin, Some(NoteSkin::new("metal")));
    assert_eq!(
        options.arrow_noteskin,
        Some(NoteSkin::new("cel-workshop?arrows=red"))
    );
    let migrated = options.clone();
    migrate_noteskin_parts(&mut options);
    assert_eq!(options, migrated, "migration is idempotent");
}

#[test]
fn components_survive_base_changes_and_profile_roundtrips() {
    let mut options = PlayerOptionsData {
        noteskin: NoteSkin::new("cel-workshop"),
        arrow_noteskin: Some(NoteSkin::new("metal-workshop?arrows=red")),
        mine_noteskin: Some(NoteSkin::new("cel")),
        receptor_noteskin: None,
        hold_active_noteskin: Some(NoteSkin::new("missing-provider?hold_active=blue")),
        hold_inactive_noteskin: Some(NoteSkin::new("metal-workshop")),
        roll_active_noteskin: Some(NoteSkin::new("cel")),
        roll_inactive_noteskin: Some(NoteSkin::new("metal")),
        tap_explosion_noteskin: Some(NoteSkin::none_choice()),
        hold_explosion_noteskin: Some(NoteSkin::new("lambda")),
        lift_noteskin: Some(NoteSkin::new("metal-workshop?lifts=blue")),
        mine_size_percent: 80,
        ..PlayerOptionsData::default()
    };
    options.noteskin = NoteSkin::new("ddr-vivid");
    let mut text = String::new();
    append_player_options_section(&mut text, "PlayerOptionsSingles", &options);
    let values: HashMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    let restored = load_player_options_section(
        true,
        |key| values.get(key).map(|value| value.to_string()),
        &PlayerOptionsData::default(),
    )
    .unwrap();
    assert_eq!(restored, options);
    assert_eq!(
        restored.hold_inactive_noteskin.unwrap().as_str(),
        "metal-workshop",
        "explicit Original retains its provider"
    );
    assert!(
        restored.receptor_noteskin.is_none(),
        "inherited receptors follow the new base"
    );
}
