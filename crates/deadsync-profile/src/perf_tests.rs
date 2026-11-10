use super::*;


#[path = "../../../tests/support/allocations.rs"]
#[allow(dead_code)]
pub(super) mod allocations;

const GUID: &str = "00000000-0000-4000-8000-000000000001";
const STYLES: [PlayStyle; 6] = [
    PlayStyle::Single,
    PlayStyle::Versus,
    PlayStyle::Double,
    PlayStyle::PumpSingle,
    PlayStyle::PumpVersus,
    PlayStyle::PumpDouble,
];

fn populated_options() -> PlayerOptionsData {
    let skin = Some(NoteSkin::new("workshop?color=blue&arrows=metal"));
    PlayerOptionsData {
        noteskin: NoteSkin::new("custom-primary"),
        arrow_noteskin: skin.clone(),
        hold_active_noteskin: skin.clone(),
        hold_inactive_noteskin: skin.clone(),
        roll_active_noteskin: skin.clone(),
        roll_inactive_noteskin: skin.clone(),
        hold_explosion_noteskin: skin.clone(),
        lift_noteskin: skin.clone(),
        mine_noteskin: skin.clone(),
        receptor_noteskin: skin.clone(),
        tap_explosion_noteskin: skin,
        smx_bg_pack: Some("custom background".into()),
        smx_judge_pack: Some("custom judgment".into()),
        step_stats_extra: StepStatsExtra::gif("custom animation"),
        mini_percent: 35,
        visual_delay_ms: -17,
        judgment_offset_x: 19,
        ..PlayerOptionsData::default()
    }
}

fn populated_profile(favorites: usize) -> Profile {
    let mut profile = Profile {
        display_name: "Profile fixture".into(),
        current_combo: 42,
        groovestats_api_key: "fixture-gs-key".into(),
        groovestats_username: "fixture-user".into(),
        groovestats_password: SecretString::new("fixture-password"),
        groovestats_is_pad_player: true,
        arrowcloud_api_key: "fixture-ac-key".into(),
        favorites: (0..favorites).map(|i| format!("{i:040x}")).collect(),
        known_pack_names: (0..favorites / 10)
            .map(|i| format!("Pack {i:04}"))
            .collect(),
        ..Profile::default()
    };
    profile.set_current_player_options(populated_options());
    profile.store_current_player_options_for_all_styles();
    profile
}

#[test]
fn borrowed_ini_matches_shared_ini_parser() {
    let complete = render_profile_ini_content(GUID, &populated_profile(0));
    let machine = render_machine_player_defaults_template(&populated_options());
    let mut duplicate = machine.clone();
    duplicate.push_str("[GuestPlayerOptions]\nMiniPercent=12\nMiniPercent=24\n[CommonPlayerOptions]\nScrollSpeed=not-a-speed\n[Empty]\n# comment\n");
    let texts = [
        "",
        " ;comment\n Loose = 1 \n[ Mixed ]\nKey= A=B \nKey=final\n[ Mixed ]\nOther=日本語\n[Empty]\n[Empty]\nignored\n=bad\n",
        &complete,
        &machine,
        &duplicate,
    ];
    for text in texts {
        let mut old = deadsync_config::ini::SimpleIni::new();
        old.load_str(text);
        let current = ProfileIni::parse(text);
        for (section, properties) in old.sections() {
            assert_eq!(current.section_has_any(section), !properties.is_empty());
            for (key, value) in properties {
                assert_eq!(current.get(section, key).as_deref(), Some(value.as_str()));
            }
            assert_eq!(current.get(section, "not-present"), None);
        }
        assert!(!current.section_has_any("missing"));
    }
}

// These tests use the same process-wide runtime as the existing profile tests;
// run the profile suite with --test-threads=1. All state is restored on unwind.
struct RuntimeFixture {
    root: PathBuf,
    session: SessionState,
    profiles: [Profile; PLAYER_SLOTS],
    cache: Option<RuntimeProfileDirCache>,
}
impl RuntimeFixture {
    fn new() -> Self {
        Self {
            root: std::env::temp_dir().join(format!(
                "deadsync-profile-ownership-{}",
                uuid::Uuid::new_v4()
            )),
            session: std::mem::take(&mut *runtime_lock_session()),
            profiles: std::mem::replace(
                &mut *runtime_lock_profiles(),
                std::array::from_fn(|_| Profile::default()),
            ),
            cache: RUNTIME_PROFILE_DIR_CACHE.lock().unwrap().take(),
        }
    }
}
impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        *runtime_lock_session() = std::mem::take(&mut self.session);
        std::mem::swap(&mut *runtime_lock_profiles(), &mut self.profiles);
        *RUNTIME_PROFILE_DIR_CACHE.lock().unwrap() = self.cache.take();
        let _ = fs::remove_dir_all(&self.root);
    }
}


#[test]
fn moved_options_preserve_every_field_and_saved_style() {
    for options in [PlayerOptionsData::default(), populated_options()] {
        for style in STYLES {
            let mut profile = populated_profile(3);
            *profile.player_options_mut(style) = options.clone();
            profile.apply_player_options_for_style(style);
            assert_eq!(profile.player_options(style), &options);
            assert_eq!(profile.current_player_options(), options);
            for next in [populated_options(), populated_options(), PlayerOptionsData::default()] {
                let changed = profile.current_player_options() != next;
                assert_eq!(profile.set_current_player_options(next.clone()), changed);
                assert_eq!(profile.current_player_options(), next);
            }
        }
    }
    let mut options = populated_options();
    options.long_error_bar_intensity = f32::from_bits(0x7fc01234);
    let mut profile = Profile::default();
    assert!(profile.set_current_player_options(options));
    assert_eq!(profile.long_error_bar_intensity.to_bits(), 0x7fc01234);
}

#[test]
fn sidecar_snapshots_preserve_bytes_callbacks_errors_and_load_reports() {
    let fixture = RuntimeFixture::new();
    let root = &fixture.root;
    let profile = populated_profile(64);
    for folder in ["a", "b"] {
        fs::create_dir_all(root.join(folder)).unwrap();
        write_profile_ini_dir(&root.join(folder), GUID, &profile).unwrap();
    }
    runtime_lock_session().active_profiles[0] = ActiveProfile::Local { id: GUID.into() };
    let paths = [
        profile_ini_path(&root.join("a")),
        groovestats_ini_path(&root.join("a")),
        arrowcloud_ini_path(&root.join("a")),
    ];
    for kind in 0..3 {
        *RUNTIME_PROFILE_DIR_CACHE.lock().unwrap() = None;
        let mut snapshot = profile.clone();
        snapshot.mini_percent = 21;
        runtime_lock_profiles()[0] = snapshot.clone();
        if kind == 0 {
            snapshot.store_current_player_options(runtime_session_play_style());
        }
        let expected = match kind {
            0 => render_profile_ini_content(GUID, &snapshot),
            1 => render_groovestats_ini_content(
                &snapshot.groovestats_api_key,
                snapshot.groovestats_is_pad_player,
                &snapshot.groovestats_username,
                snapshot.groovestats_password.expose(),
            ),
            2 => render_arrowcloud_ini_content(&snapshot.arrowcloud_api_key),
            _ => unreachable!(),
        };
        let mut calls = 0;
        let error = save(kind, root, |guid, _, _, kept| {
            calls += 1;
            assert_eq!(guid, GUID);
            assert_eq!(kept, root.join("a"));
            // The callback can acquire the profile mutex. Its mutations must not
            // leak into the snapshot already prepared for serialization.
            let mut profiles = RUNTIME_PROFILES.try_lock().expect("profile lock held across callback");
            if kind == 0 {
                assert_eq!(profiles[0].player_options(runtime_session_play_style()).mini_percent, 21);
            }
            profiles[0].display_name = "callback change".into();
            profiles[0].groovestats_api_key = "callback gs".into();
            profiles[0].arrowcloud_api_key = "callback ac".into();
        });
        assert!(error.is_none());
        assert_eq!(calls, 1);
        assert_eq!(fs::read(&paths[kind]).unwrap(), expected.as_bytes());
    }
    // The new borrowed parser is also exercised through the actual file loader.
    let report = runtime_load_profile_data_for_side(
        PlayerSide::P1,
        &root.join("a"),
        &Profile::default(),
        "2026-10-08",
    );
    assert!(
        report.profile_ini_loaded && report.groovestats_ini_loaded && report.arrowcloud_ini_loaded
    );
    assert_eq!(
        runtime_lock_profiles()[0].display_name,
        profile.display_name
    );
    assert_eq!(
        runtime_lock_profiles()[0].groovestats_password,
        profile.groovestats_password
    );
    fs::write(&paths[1], [0xff, 0xfe]).unwrap();
    fs::remove_file(&paths[2]).unwrap();
    let report = runtime_load_profile_data_for_side(
        PlayerSide::P1,
        &root.join("a"),
        &Profile::default(),
        "2026-10-08",
    );
    assert!(report.profile_ini_loaded);
    assert!(!report.groovestats_ini_loaded && !report.arrowcloud_ini_loaded);
    let blocked = root.join("blocked");
    fs::write(&blocked, b"not a directory").unwrap();
    RUNTIME_PROFILE_DIR_CACHE
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .map
        .insert(GUID.into(), blocked.clone());
    for kind in 0..3 {
        let error = save(kind, root, |_, _, _, _| unreachable!()).unwrap();
        assert_eq!(error.path.parent(), Some(blocked.as_path()));
    }
    runtime_lock_session().active_profiles[0] = ActiveProfile::Guest;
    for kind in 0..3 {
        assert!(save(kind, root, |_, _, _, _| unreachable!()).is_none());
    }
}

fn save(
    kind: usize,
    root: &Path,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    match kind {
        0 => runtime_save_profile_ini_for_side(root, PlayerSide::P1, duplicate),
        1 => runtime_save_groovestats_credentials_for_side(root, PlayerSide::P1, duplicate),
        2 => runtime_save_arrowcloud_api_key_for_side(root, PlayerSide::P1, duplicate),
        _ => unreachable!(),
    }
}
