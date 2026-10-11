use super::*;
use std::hint::black_box;

pub fn original_picker_view() -> ProfilePickerView {
    let config = deadsync_config::runtime::get();
    let default_options = profile::new_profile_player_options();
    let guest_options = profile::guest_player_options();
    let default_speed_mod = default_options.scroll_speed.to_string();
    let default_scroll_option = default_options.scroll_option;
    let player_options_section =
        deadsync_profile::player_options_section(profile::get_session_play_style());
    let profiles = profile::scan_local_profiles()
        .into_iter()
        .map(|summary| {
            let mut speed_mod = default_speed_mod.clone();
            let mut scroll_option = default_scroll_option;
            let mut mini_indicator = default_options.mini_indicator;
            let mut noteskin = default_options.noteskin.clone();
            let mut judgment = default_options.judgment_graphic.clone();
            let ini_path = profile::local_profile_dir_for_id(&summary.id).join("profile.ini");
            let mut ini = deadsync_config::ini::SimpleIni::new();
            if ini.load(&ini_path).is_ok() {
                let get_player_option = |key: &str| ini.get(player_options_section, key);
                if let Some(raw) = get_player_option("ScrollSpeed") {
                    let trimmed = raw.trim();
                    speed_mod = ScrollSpeedSetting::from_str(trimmed)
                        .map(|setting| format!("{setting}"))
                        .unwrap_or_else(|_| trimmed.to_owned());
                }
                scroll_option = get_player_option("Scroll")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        let reverse = get_player_option("ReverseScroll")
                            .and_then(|value| value.parse::<u8>().ok())
                            .is_some_and(|value| value != 0);
                        if reverse {
                            deadsync_profile::ScrollOption::Reverse
                        } else {
                            default_scroll_option
                        }
                    });
                mini_indicator = get_player_option("MiniIndicator")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        let subtractive = get_player_option("SubtractiveScoring")
                            .and_then(parse_ini_bool)
                            .unwrap_or(false);
                        let pacemaker = get_player_option("Pacemaker")
                            .and_then(parse_ini_bool)
                            .unwrap_or(false);
                        if subtractive {
                            deadsync_profile::MiniIndicator::SubtractiveScoring
                        } else if pacemaker {
                            deadsync_profile::MiniIndicator::Pacemaker
                        } else {
                            default_options.mini_indicator
                        }
                    });
                noteskin = get_player_option("NoteSkin")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| default_options.noteskin.clone());
                judgment = get_player_option("JudgmentGraphic")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| default_options.judgment_graphic.clone());
            }
            ProfilePickerEntryView {
                id: summary.id.clone(),
                display_name: summary.display_name,
                speed_mod,
                avatar_key: summary
                    .avatar_path
                    .map(|path| path.to_string_lossy().into_owned()),
                total_songs_played: deadsync_online::score_compat::total_songs_played_for_profile(
                    &summary.id,
                ),
                scroll_option,
                mini_indicator,
                noteskin,
                judgment,
            }
        })
        .collect();
    ProfilePickerView {
        game: config.game_flag,
        guest: ProfilePickerEntryView {
            id: String::new(),
            display_name: String::new(),
            speed_mod: guest_options.scroll_speed.to_string(),
            avatar_key: None,
            total_songs_played: 0,
            scroll_option: guest_options.scroll_option,
            mini_indicator: guest_options.mini_indicator,
            noteskin: guest_options.noteskin,
            judgment: guest_options.judgment_graphic,
        },
        profiles,
        default_profiles: [
            profile::get_default_profile_for_side(PlayerSide::P1),
            profile::get_default_profile_for_side(PlayerSide::P2),
        ],
        three_key_navigation: config.three_key_navigation,
    }
}

fn picker_fixture(count: usize, extra: usize, settings: &str) -> Vec<(String, std::path::PathBuf)> {
    crate::tests::init_paths();
    let root = deadsync_profile::app_runtime::profiles_root();
    assert!(
        root.starts_with(std::env::temp_dir()),
        "test profile root must be temporary"
    );
    (0..count)
        .map(|i| {
            let id = profile::create_local_profile(&format!("Dataflow fixture {i}")).unwrap();
            let dir = profile::local_profile_dir_for_id(&id);
            assert!(dir.starts_with(&root) && dir != root);
            let mut text = format!("[userprofile]\nGuid={id}\nDisplayName=Fixture {i}\n");
            let section =
                deadsync_profile::player_options_section(profile::get_session_play_style());
            text.push_str(&settings.replace("[PlayerOptions]", &format!("[{section}]")));
            text.push_str("\n[Unrelated]\n");
            for field in 0..extra {
                use std::fmt::Write;
                writeln!(text, "Unused{field}=retained source text {field}").unwrap();
            }
            std::fs::write(dir.join("profile.ini"), text).unwrap();
            (id, dir)
        })
        .collect()
}

fn remove_picker_fixture(fixture: Vec<(String, std::path::PathBuf)>) {
    for (id, _) in fixture {
        profile::delete_local_profile(&id).unwrap();
    }
}

#[test]
fn picker_preserves_complete_catalog_for_legacy_and_malformed_values() {
    let cases = [
        "",
        "[PlayerOptions]\nScrollSpeed= c400 \nScroll=Reverse\nNoteSkin=cel\nJudgmentGraphic=ITG2\n",
        "[PlayerOptions]\nScrollSpeed= odd speed \nReverseScroll=1\nSubtractiveScoring=YES\nPacemaker=On\nNoteSkin=雪\nJudgmentGraphic=\n",
        "[PlayerOptions]\nScrollSpeed=X2\nScrollSpeed=m600\nScroll=bad\nReverseScroll=255\nMiniIndicator=bad\nSubtractiveScoring=off\nPacemaker=true\n[PlayerOptions]\nScrollSpeed=\nNoteSkin=\n",
    ];
    for settings in cases {
        let fixture = picker_fixture(2, 64, settings);
        let actual = picker_view();
        let expected = original_picker_view();
        assert_eq!(actual, expected, "{settings:?}");
        assert!(actual.profiles.iter().any(|p| p.id == fixture[0].0));
        remove_picker_fixture(fixture);
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_profile_dataflow_picker() {
    let section = deadsync_profile::player_options_section(profile::get_session_play_style());
    for (label, count, extra, settings) in [
        ("picker/sparse", 1, 0, String::new()),
        (
            "picker/typical",
            8,
            80,
            format!(
                "[{section}]\nScrollSpeed=C600\nScroll=Reverse\nMiniIndicator=Pacemaker\nNoteSkin=cel\nJudgmentGraphic=ITG2\n"
            ),
        ),
        (
            "picker/large",
            16,
            400,
            format!("[{section}]\nScrollSpeed=C600\nNoteSkin=cel\n"),
        ),
    ] {
        let fixture = picker_fixture(count, extra, &settings);
        assert_eq!(original_picker_view(), picker_view());
        crate::dataflow_perf::compare(
            label,
            || {
                black_box(original_picker_view());
            },
            || {
                black_box(picker_view());
            },
        );
        remove_picker_fixture(fixture);
    }
}
