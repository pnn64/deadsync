use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_theme_simply_love::views::{
    SelectMusicHistorySideView, SelectMusicHistoryView, SelectMusicInitView,
    SelectMusicInteractionPolicyView, SelectMusicLastPlayedView, SelectMusicMediaPolicyView,
    SelectMusicPlaylistView, SelectMusicPolicyView, SelectMusicPresentationPolicyView,
    SelectMusicProfileView, SelectMusicSessionView, SelectMusicWheelPolicyView,
};
use log::warn;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[inline(always)]
fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    key
}

fn find_child_dir(root: &Path, name: &str) -> Option<PathBuf> {
    let exact = root.join(name);
    if exact.is_dir() {
        return Some(exact);
    }
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    std::fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|found| found.eq_ignore_ascii_case(name)))
        .then_some(path)
    })
}

fn playlist_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        })
        .collect();
    files.sort_by_cached_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| path.to_string_lossy().to_ascii_lowercase())
    });
    files
}

fn playlist_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn read_playlist(path: PathBuf, owner: Option<String>) -> Option<SelectMusicPlaylistView> {
    let name = playlist_name(&path)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(SelectMusicPlaylistView {
            id: path_key(&path),
            owner,
            name,
            text,
        }),
        Err(error) => {
            warn!("Failed to read playlist '{}': {error}", path.display());
            None
        }
    }
}

fn machine_playlists() -> Vec<SelectMusicPlaylistView> {
    let dirs = deadlib_platform::dirs::app_dirs();
    let mut roots = Vec::with_capacity(2);
    if let Some(root) = find_child_dir(&dirs.data_dir, "playlists") {
        roots.push(root);
    }
    if !dirs.portable
        && let Some(root) = find_child_dir(&dirs.exe_dir, "playlists")
        && !roots.iter().any(|known| path_key(known) == path_key(&root))
    {
        roots.push(root);
    }

    let mut seen = HashSet::new();
    roots
        .into_iter()
        .flat_map(|root| playlist_files(&root))
        .filter_map(|path| {
            let name = playlist_name(&path)?;
            seen.insert(name.to_ascii_lowercase())
                .then(|| read_playlist(path, None))?
        })
        .collect()
}

fn profile_playlists() -> Vec<SelectMusicPlaylistView> {
    let mut seen_profiles = HashSet::new();
    let mut playlists = Vec::new();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        if !seen_profiles.insert(profile_id.clone()) {
            continue;
        }
        let Some(root) =
            find_child_dir(&profile::local_profile_dir_for_id(&profile_id), "playlists")
        else {
            continue;
        };
        let display_name = profile::get_for_side(side).display_name;
        let owner = if display_name.trim().is_empty() {
            profile_id
        } else {
            display_name
        };
        playlists.extend(
            playlist_files(&root)
                .into_iter()
                .filter_map(|path| read_playlist(path, Some(owner.clone()))),
        );
    }
    playlists
}

pub(crate) fn init_view() -> SelectMusicInitView {
    let dirs = deadlib_platform::dirs::app_dirs();
    let songs_root = dirs.songs_dir();
    let mut playlists = machine_playlists();
    playlists.extend(profile_playlists());
    let cfg = config::get();
    let session = session_view();
    SelectMusicInitView {
        song_scan_roots: deadsync_simfile::app_runtime::collect_song_scan_roots(&songs_root),
        song_packs: deadsync_simfile::runtime_cache::get_song_cache().clone(),
        songs_root,
        courses_root: dirs.courses_dir(),
        playlists,
        history: Default::default(),
        policy: policy_view(&cfg),
        profiles: profile_view(),
        profile_picker: crate::local_profiles::picker_view(),
        last_played: last_played_view(session),
        favorites: deadsync_profile::runtime_favorite_snapshot(),
        known_packs: deadsync_profile::runtime_known_pack_snapshot(),
        session,
    }
}

pub(crate) fn session_view() -> SelectMusicSessionView {
    let session = profile::get_session_snapshot();
    SelectMusicSessionView {
        play_style: session.play_style,
        player_side: session.player_side,
        joined: std::array::from_fn(|idx| {
            session.side_joined(deadsync_profile::player_side_for_index(idx))
        }),
        guest: std::array::from_fn(|idx| {
            profile::is_session_side_guest(deadsync_profile::player_side_for_index(idx))
        }),
        music_rate: session.music_rate,
    }
}

pub(crate) fn profile_view() -> SelectMusicProfileView {
    let players = deadsync_profile::runtime_session_players_view();
    SelectMusicProfileView {
        display_names: players.display_names,
        avatar_texture_keys: std::array::from_fn(|idx| {
            profile::get_for_side(deadsync_profile::player_side_for_index(idx))
                .avatar_texture_key
                .clone()
        }),
        local_profile_ids: std::array::from_fn(|idx| {
            profile::active_local_profile_id_for_side(deadsync_profile::player_side_for_index(idx))
        }),
        pad_profile_ids: std::array::from_fn(|idx| {
            profile::active_local_profile_id_for_pad(idx == 1)
        }),
    }
}

fn last_played_view(session: SelectMusicSessionView) -> SelectMusicLastPlayedView {
    let profile = profile::get();
    let last_played = profile.last_played(session.play_style);
    SelectMusicLastPlayedView {
        song_music_path: last_played.song_music_path.clone(),
        chart_hash: last_played.chart_hash.clone(),
        difficulty_index: last_played.difficulty_index,
    }
}

pub(crate) fn policy_view(config: &config::Config) -> SelectMusicPolicyView {
    SelectMusicPolicyView {
        machine_font: config.machine_font,
        dedicated_menu_only: config.only_dedicated_menu_buttons,
        three_key_navigation: config.three_key_navigation,
        fsr_profiles: config.use_fsrs,
        replays: config.machine_enable_replays,
        profile_switch: config.allow_switch_profile_in_menu,
        keyboard_features: config.keyboard_features,
        allow_song_deletion: config.allow_song_deletion,
        media: SelectMusicMediaPolicyView {
            show_banners: config.show_select_music_banners,
            show_cdtitles: config.show_select_music_cdtitles,
            show_folder_stats: config.show_select_music_folder_stats,
            show_previews: config.show_select_music_previews,
            preview_loop: config.select_music_preview_loop,
            preview_starts_immediately: config.select_music_preview_starts_immediately,
            show_preview_marker: config.show_select_music_preview_marker,
            replay_gain: config.enable_replaygain,
            song_select_bg_mode: config.select_music_song_select_bg_mode,
        },
        wheel: SelectMusicWheelPolicyView {
            show_grades: config.show_music_wheel_grades,
            show_lamps: config.show_music_wheel_lamps,
            itl_rank_mode: config.select_music_itl_rank_mode,
            itl_score_mode: config.select_music_itl_wheel_mode,
        },
        interaction: SelectMusicInteractionPolicyView {
            wheel_switch_speed: config.music_wheel_switch_speed,
            wheel_style: config.select_music_wheel_style,
            sort_by_series: config.sort_music_wheel_by_series,
            new_pack_mode: config.select_music_new_pack_mode,
            show_srpg_shop: config.show_srpg_shop,
            srpg10_visuals: config.visual_style.is_srpg()
                && matches!(config.srpg_variant, config::SrpgVariant::Srpg10),
            practice_shortcut: config.music_select_shortcut_practice,
            song_search_shortcut: config.music_select_shortcut_song_search,
            reload_shortcut: config.music_select_shortcut_load_songs,
            test_input_shortcut: config.music_select_shortcut_test_input,
        },
        presentation: SelectMusicPresentationPolicyView {
            show_scorebox: config.show_select_music_scorebox,
            scorebox_cycle_enabled: config.select_music_scorebox_cycle_itg
                || config.select_music_scorebox_cycle_ex
                || config.select_music_scorebox_cycle_hard_ex
                || config.select_music_scorebox_cycle_tournaments,
            scorebox_in_step_pane: config.select_music_scorebox_placement
                == config::SelectMusicScoreboxPlacement::StepPane,
            show_stage_display: config.show_select_music_stage_display,
            show_gameplay_timer: config.show_select_music_gameplay_timer,
            step_artist_expanded: config
                .select_music_step_artist_box_mode
                .is_expanded(config.theme_flag),
            breakdown_style: config.select_music_breakdown_style,
            pattern_info_mode: config.select_music_pattern_info_mode,
            chart_info_peak_nps: config.select_music_chart_info_peak_nps,
            chart_info_effective_bpm: config.select_music_chart_info_effective_bpm,
            chart_info_matrix_rating: config.select_music_chart_info_matrix_rating,
            show_breakdown: config.show_select_music_breakdown,
            pack_ini_offsets: config.machine_pack_ini_offsets,
            default_sync_offset: config.machine_default_sync_offset,
        },
    }
}

pub(crate) fn history_view() -> SelectMusicHistoryView {
    let profile_ids: [Option<String>; 2] = std::array::from_fn(|side_idx| {
        let side = [PlayerSide::P1, PlayerSide::P2][side_idx];
        profile::active_local_profile_id_for_side(side)
    });
    for profile_id in profile_ids.iter().flatten() {
        profile::ensure_score_caches_loaded_for_id(profile_id);
    }
    let machine_played_chart_counts = scores::played_chart_counts_for_machine();
    let machine_recent_chart_hashes = scores::recent_played_chart_hashes_for_machine();
    let mut sides = std::array::from_fn(|side_idx| {
        let Some(profile_id) = profile_ids[side_idx].as_deref() else {
            return SelectMusicHistorySideView::default();
        };
        SelectMusicHistorySideView {
            available: true,
            played_chart_counts: scores::played_chart_counts_for_profile(profile_id),
            recent_chart_hashes: scores::recent_played_chart_hashes_for_profile(profile_id),
            cached_scores: Vec::new(),
        }
    });
    let score_caches = scores::lock_score_caches();
    for (side, profile_id) in sides.iter_mut().zip(profile_ids.iter()) {
        if let Some(profile_id) = profile_id {
            side.cached_scores = score_caches.merged_profile_scores(profile_id);
        }
    }
    SelectMusicHistoryView {
        machine_played_chart_counts,
        machine_recent_chart_hashes,
        sides,
    }
}

pub(crate) fn prepare_init_view(mut view: SelectMusicInitView) -> SelectMusicInitView {
    view.history = history_view();
    view
}

pub(crate) fn prepared_init_view() -> SelectMusicInitView {
    scores::prewarm_select_music_score_caches();
    prepare_init_view(init_view())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_view_maps_media_and_wheel_runtime_flags() {
        let config = config::Config {
            machine_font: config::MachineFont::Mega,
            show_select_music_banners: true,
            show_select_music_previews: true,
            enable_replaygain: true,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: false,
            select_music_itl_rank_mode: config::SelectMusicItlRankMode::Overall,
            select_music_itl_wheel_mode: config::SelectMusicItlWheelMode::PointsAndScore,
            music_wheel_switch_speed: 22,
            select_music_wheel_style: config::SelectMusicWheelStyle::Iidx,
            sort_music_wheel_by_series: true,
            select_music_new_pack_mode: config::NewPackMode::OpenPack,
            show_srpg_shop: false,
            music_select_shortcut_practice: deadsync_input::KeyCode::KeyQ,
            music_select_shortcut_song_search: deadsync_input::KeyCode::KeyW,
            music_select_shortcut_load_songs: deadsync_input::KeyCode::KeyE,
            music_select_shortcut_test_input: deadsync_input::KeyCode::KeyR,
            show_select_music_scorebox: false,
            select_music_scorebox_cycle_itg: false,
            select_music_scorebox_cycle_ex: false,
            select_music_scorebox_cycle_hard_ex: false,
            select_music_scorebox_cycle_tournaments: true,
            select_music_scorebox_placement: config::SelectMusicScoreboxPlacement::StepPane,
            show_select_music_stage_display: false,
            show_select_music_gameplay_timer: false,
            select_music_step_artist_box_mode: config::SelectMusicStepArtistBoxMode::Expanded,
            select_music_breakdown_style: config::BreakdownStyle::Sn,
            select_music_pattern_info_mode: config::SelectMusicPatternInfoMode::Stamina,
            select_music_chart_info_peak_nps: false,
            select_music_chart_info_effective_bpm: true,
            select_music_chart_info_matrix_rating: true,
            show_select_music_breakdown: false,
            machine_pack_ini_offsets: true,
            machine_default_sync_offset: config::DefaultSyncOffset::Itg,
            allow_song_deletion: true,
            ..Default::default()
        };

        let view = policy_view(&config);

        assert_eq!(view.machine_font, config::MachineFont::Mega);
        assert!(view.media.show_banners);
        assert!(view.media.show_previews);
        assert!(view.media.replay_gain);
        assert!(view.allow_song_deletion);
        assert!(view.wheel.show_grades);
        assert!(!view.wheel.show_lamps);
        assert_eq!(
            view.wheel.itl_rank_mode,
            config::SelectMusicItlRankMode::Overall
        );
        assert_eq!(
            view.wheel.itl_score_mode,
            config::SelectMusicItlWheelMode::PointsAndScore
        );
        assert_eq!(view.interaction.wheel_switch_speed, 22);
        assert_eq!(
            view.interaction.wheel_style,
            config::SelectMusicWheelStyle::Iidx
        );
        assert!(view.interaction.sort_by_series);
        assert_eq!(
            view.interaction.new_pack_mode,
            config::NewPackMode::OpenPack
        );
        assert!(!view.interaction.show_srpg_shop);
        assert_eq!(
            view.interaction.song_search_shortcut,
            deadsync_input::KeyCode::KeyW
        );
        assert!(!view.presentation.show_scorebox);
        assert!(view.presentation.scorebox_cycle_enabled);
        assert!(view.presentation.scorebox_in_step_pane);
        assert!(!view.presentation.show_stage_display);
        assert!(!view.presentation.show_gameplay_timer);
        assert!(view.presentation.step_artist_expanded);
        assert_eq!(
            view.presentation.breakdown_style,
            config::BreakdownStyle::Sn
        );
        assert_eq!(
            view.presentation.pattern_info_mode,
            config::SelectMusicPatternInfoMode::Stamina
        );
        assert!(!view.presentation.chart_info_peak_nps);
        assert!(view.presentation.chart_info_effective_bpm);
        assert!(view.presentation.chart_info_matrix_rating);
        assert!(!view.presentation.show_breakdown);
        assert!(view.presentation.pack_ini_offsets);
        assert_eq!(
            view.presentation.default_sync_offset,
            config::DefaultSyncOffset::Itg
        );
    }
}
