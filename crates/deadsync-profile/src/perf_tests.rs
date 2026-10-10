use super::*;
use std::hint::black_box;

#[path = "perf_original.rs"]
#[allow(dead_code)]
mod original;
use original::*;

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

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
        display_name: "Performance fixture".into(),
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

fn assert_profile_matches(actual: &Profile, expected: &Profile) {
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    // Debug redacts the password; compare the fixture value separately.
    assert_eq!(actual.groovestats_password, expected.groovestats_password);
}

#[test]
fn moved_options_preserve_every_field_and_saved_style() {
    for options in [PlayerOptionsData::default(), populated_options()] {
        for style in STYLES {
            let mut old = populated_profile(3);
            *old.player_options_mut(style) = options.clone();
            let mut current = old.clone();
            old.original_apply_player_options_for_style(style);
            current.apply_player_options_for_style(style);
            assert_profile_matches(&current, &old);
            assert_eq!(current.player_options(style), &options);
            assert_eq!(current.current_player_options(), options);
            // Covers changed values, unchanged values, and returning to defaults.
            for next in [
                populated_options(),
                populated_options(),
                PlayerOptionsData::default(),
            ] {
                let expected_changed = old.original_set_current_player_options(next.clone());
                assert_eq!(current.set_current_player_options(next), expected_changed);
                assert_profile_matches(&current, &old);
            }
        }
    }
    // NaNs must still be assigned (PartialEq returns false), with their bits intact.
    let mut options = populated_options();
    options.long_error_bar_intensity = f32::from_bits(0x7fc01234);
    let mut old = Profile::default();
    let mut current = old.clone();
    assert_eq!(
        current.set_current_player_options(options.clone()),
        old.original_set_current_player_options(options)
    );
    assert_eq!(
        current.long_error_bar_intensity.to_bits(),
        old.long_error_bar_intensity.to_bits()
    );
}

#[test]
fn borrowed_ini_matches_owned_parser_and_typed_defaults() {
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
        let base = populated_options();
        assert_eq!(
            machine_player_defaults_from_ini(text, &base),
            original_machine_player_defaults_from_ini(text, &base)
        );
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

fn save(
    kind: usize,
    original: bool,
    root: &Path,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    match (kind, original) {
        (0, false) => runtime_save_profile_ini_for_side(root, PlayerSide::P1, duplicate),
        (0, true) => original_runtime_save_profile_ini_for_side(root, PlayerSide::P1, duplicate),
        (1, false) => {
            runtime_save_groovestats_credentials_for_side(root, PlayerSide::P1, duplicate)
        }
        (1, true) => {
            original_runtime_save_groovestats_credentials_for_side(root, PlayerSide::P1, duplicate)
        }
        (2, false) => runtime_save_arrowcloud_api_key_for_side(root, PlayerSide::P1, duplicate),
        (2, true) => {
            original_runtime_save_arrowcloud_api_key_for_side(root, PlayerSide::P1, duplicate)
        }
        _ => unreachable!(),
    }
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
        let mut expected: Option<(Vec<u8>, String, allocations::Churn)> = None;
        for original in [true, false] {
            *RUNTIME_PROFILE_DIR_CACHE.lock().unwrap() = None;
            runtime_lock_profiles()[0] = profile.clone();
            runtime_lock_profiles()[0].mini_percent = 21;
            let mut calls = 0;
            let (error, churn) = allocations::measure(|| {
                save(kind, original, root, |guid, _, _, kept| {
                    calls += 1;
                    assert_eq!(guid, GUID);
                    assert_eq!(kept, root.join("a"));
                    // Acquiring the profile mutex also proves it was released before
                    // the externally supplied callback. Mutations must not leak into
                    // the already prepared sidecar snapshot.
                    let mut profiles = RUNTIME_PROFILES
                        .try_lock()
                        .expect("profile lock held across callback");
                    if kind == 0 {
                        assert_eq!(
                            profiles[0]
                                .player_options(runtime_session_play_style())
                                .mini_percent,
                            21
                        );
                    }
                    profiles[0].display_name = "callback change".into();
                    profiles[0].groovestats_api_key = "callback gs".into();
                    profiles[0].arrowcloud_api_key = "callback ac".into();
                })
            });
            assert!(error.is_none());
            assert_eq!(calls, 1);
            let bytes = fs::read(&paths[kind]).unwrap();
            let state = format!("{:?}", runtime_lock_profiles()[0]);
            if let Some((expected_bytes, expected_state, before)) = &expected {
                assert_eq!(&bytes, expected_bytes);
                assert_eq!(&state, expected_state);
                assert!(churn.allocs < before.allocs);
                assert!(churn.allocated_bytes < before.allocated_bytes);
            } else {
                expected = Some((bytes, state, churn));
            }
        }
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
        let old = save(kind, true, root, |_, _, _, _| unreachable!()).unwrap();
        let current = save(kind, false, root, |_, _, _, _| unreachable!()).unwrap();
        assert_eq!(current.path, old.path);
        assert_eq!(current.error.kind(), old.error.kind());
    }
    runtime_lock_session().active_profiles[0] = ActiveProfile::Guest;
    for kind in 0..3 {
        assert!(save(kind, false, root, |_, _, _, _| unreachable!()).is_none());
    }
}

fn save_preparation(profile: &mut Profile, kind: usize, original: bool) -> String {
    if kind == 0 {
        profile.store_current_player_options(PlayStyle::Single);
    }
    // Mirrors the original save's snapshot lifetime through serialization.
    let snapshot;
    let profile = if original {
        snapshot = profile.clone();
        &snapshot
    } else {
        &*profile
    };
    match kind {
        0 => render_profile_ini_content(GUID, profile),
        1 => render_groovestats_ini_content(
            &profile.groovestats_api_key,
            profile.groovestats_is_pad_player,
            &profile.groovestats_username,
            profile.groovestats_password.expose(),
        ),
        2 => render_arrowcloud_ini_content(&profile.arrowcloud_api_key),
        _ => unreachable!(),
    }
}

#[test]
fn ownership_changes_reduce_allocation_churn() {
    let mut profile = populated_profile(10_000);
    for kind in 0..3 {
        let (old, before) = allocations::measure(|| save_preparation(&mut profile, kind, true));
        let (current, after) = allocations::measure(|| save_preparation(&mut profile, kind, false));
        assert_eq!(current, old);
        assert!(before.allocs > after.allocs + 11_000);
        println!("save {kind}: original {before:?}, current {after:?}");
    }
    let text = render_machine_player_defaults_template(&populated_options());
    let base = PlayerOptionsData::default();
    let (old, before) =
        allocations::measure(|| original_machine_player_defaults_from_ini(&text, &base));
    let (current, after) = allocations::measure(|| machine_player_defaults_from_ini(&text, &base));
    assert_eq!(current, old);
    assert!(after.allocs < before.allocs / 2);
    println!("INI defaults: original {before:?}, current {after:?}");
    let (_, before) =
        allocations::measure(|| profile.original_apply_player_options_for_style(PlayStyle::Single));
    let (_, after) =
        allocations::measure(|| profile.apply_player_options_for_style(PlayStyle::Single));
    assert!(after.allocs < before.allocs);
    println!("style apply: original {before:?}, current {after:?}");
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture --test-threads=1"]
fn benchmark_profile_sidecar_preparation() {
    for count in [0, 1_000, 10_000] {
        for (kind, name) in ["profile", "groovestats", "arrowcloud"]
            .into_iter()
            .enumerate()
        {
            let mut profile = populated_profile(count);
            paired::compare(
                &format!("{name} save preparation/{count} favorites"),
                100,
                |current| {
                    black_box(save_preparation(black_box(&mut profile), kind, !current));
                },
            );
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture --test-threads=1"]
fn benchmark_profile_ini_loading() {
    let base = populated_options();
    for (name, text) in [
        ("empty", String::new()),
        ("template", render_machine_player_defaults_template(&base)),
        (
            "four styles",
            render_profile_ini_content(GUID, &populated_profile(0)),
        ),
    ] {
        paired::compare(&format!("INI parse/{name}"), 1_000, |current| {
            if current {
                black_box(ProfileIni::parse(black_box(&text)));
            } else {
                black_box(OriginalProfileIni::parse(black_box(&text)));
            }
        });
        paired::compare(&format!("typed defaults/{name}"), 1_000, |current| {
            black_box(if current {
                machine_player_defaults_from_ini(black_box(&text), &base)
            } else {
                original_machine_player_defaults_from_ini(black_box(&text), &base)
            });
        });
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture --test-threads=1"]
fn benchmark_owned_player_options() {
    for (name, options) in [
        ("defaults", PlayerOptionsData::default()),
        ("custom", populated_options()),
    ] {
        let mut profile = Profile::default();
        profile.player_options_singles = options.clone();
        paired::compare(&format!("apply style/{name}"), 10_000, |current| {
            if current {
                black_box(&mut profile).apply_player_options_for_style(PlayStyle::Single);
            } else {
                black_box(&mut profile).original_apply_player_options_for_style(PlayStyle::Single);
            }
            black_box(&profile);
        });
        paired::compare_prepared(
            &format!("set changed options/{name}"),
            10_000,
            || options.clone(),
            |options, current| {
                // Force a real change without allocating or cloning a whole profile.
                profile.mini_percent = options.mini_percent.wrapping_add(1);
                black_box(if current {
                    profile.set_current_player_options(options)
                } else {
                    profile.original_set_current_player_options(options)
                });
                black_box(&profile);
            },
        );
    }
}
