use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const GAME_UPWARD_DEP_BASELINE: &[(&str, &str, usize)] = &[];

const LOGICAL_INPUT_SYMBOLS: &[&str] = &[
    "ALL_VIRTUAL_ACTIONS",
    "GamepadCodeBinding",
    "InputEdge",
    "InputEvent",
    "InputSource",
    "Lane",
    "PadCode",
    "PadDir",
    "PadEvent",
    "PadId",
    "VirtualAction",
    "action_from_ini_key_lower",
    "action_to_ini_key",
    "clamp_input_debounce_seconds",
    "emit_normalized_actions",
    "gamepad_code_binding_to_token",
    "lane_from_action",
    "lane_from_column",
    "pad_dir_from_action",
    "parse_gamepad_code_binding",
    "parse_pad_dir",
];

const LOGICAL_INPUT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const NATIVE_INPUT_LAUNCH_SYMBOLS: &[&str] = &[
    "run_pad_backend",
    "run_linux_backend",
    "run_freebsd_backend",
    "run_macos_backend",
    "run_windows_backend",
    "set_raw_keyboard_window_focused",
    "set_raw_keyboard_capture_enabled",
    "unix_raw_keyboard_backend_active",
];

const NATIVE_INPUT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const INPUT_FSR_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const ENGINE_VIDEO_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/assets",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const ENGINE_GFX_RENDER_SYMBOLS: &[&str] = &[
    "BackendType",
    "BlendMode",
    "ClockDomainTrace",
    "DrawStats",
    "FastU64Map",
    "INVALID_TEXTURE_HANDLE",
    "INVALID_TMESH_CACHE_KEY",
    "MeshVertex",
    "ObjectType",
    "PresentModePolicy",
    "PresentModeTrace",
    "PresentStats",
    "RenderList",
    "RenderObject",
    "SamplerDesc",
    "SamplerFilter",
    "SamplerWrap",
    "SpriteInstanceRaw",
    "TMeshCacheKey",
    "TextureHandle",
    "TextureHandleMap",
    "TexturedMeshInstanceRaw",
    "TexturedMeshVertex",
    "TexturedMeshVertices",
    "draw_prep",
];

const ENGINE_GFX_RENDER_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/assets",
    "src/config",
    "src/engine/present",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const RENDER_BACKEND_IMPORT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/assets",
    "src/config",
    "src/engine/present",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
    "crates/deadlib-present/src",
];

const RENDER_BACKEND_IMPORTS: &[&str] = &[
    "deadlib_render_backend_gl",
    "deadlib_render_backend_software",
    "deadlib_render_backend_vulkan",
    "deadlib_render_backend_wgpu",
];

const LIGHTS_IMPORT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const SMX_IMPORT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/engine",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const ENGINE_PRESENT_EXTRACTED_FILES: &[&str] = &[
    "src/engine/present/mod.rs",
    "src/engine/present/actors.rs",
    "src/engine/present/anim.rs",
    "src/engine/present/cache.rs",
    "src/engine/present/color.rs",
    "src/engine/present/compose.rs",
    "src/engine/present/density.rs",
    "src/engine/present/dsl.rs",
    "src/engine/present/font.rs",
    "src/engine/present/runtime.rs",
    "src/engine/present/texture.rs",
    "src/engine/space.rs",
];

const PRESENT_SPACE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const VERSION_IMPORT_SCAN_DIRS: &[&str] =
    &["src", "crates", "tests", "tests/compose", "tests/draw"];

const UPDATER_CORE_FILES: &[&str] = &[
    "action.rs",
    "apply_journal.rs",
    "apply_unix.rs",
    "apply_windows.rs",
    "cli.rs",
    "download.rs",
    "state.rs",
];

const UPDATER_CORE_TOKENS: &[&str] = &[
    "action",
    "cli",
    "download",
    "state",
    "apply_journal",
    "apply_unix",
    "apply_windows",
    "ReleaseAsset",
    "ReleaseInfo",
    "UpdateState",
    "FetchOutcome",
    "UpdaterError",
    "ActionPhase",
    "ActionErrorKind",
    "apply_supported_for_host",
    "check_agent",
    "classify_check_result",
    "classify_error",
    "current",
    "dismiss",
    "download_agent",
    "downloads_dir",
    "fetch_latest_release",
    "request_apply",
    "request_cancel",
    "request_check_now",
    "request_download",
];

const ENGINE_PLATFORM_FACADE_MODULES: &[&str] = &[
    "display",
    "host_time",
    "idle_inhibit",
    "logging",
    "open_path",
    "windows_rt",
];

const CONFIG_PLATFORM_FACADE_MODULES: &[&str] = &["dirs"];

const AUDIO_CORE_FORBIDDEN_TOKENS: &[&str] = &[
    "crate::engine",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "deadlib_platform",
    "deadsync_audio_decode",
    "std::fs",
    "std::path",
    "std::sync::mpsc",
    "Mutex",
    "log::",
];

const AUDIO_DECODE_FORBIDDEN_TOKENS: &[&str] = &[
    "crate::engine",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "deadlib_platform",
    "deadlib_present",
    "deadlib_render",
    "std::sync::mpsc",
    "log::",
];

const AUDIO_ANALYSIS_FORBIDDEN_TOKENS: &[&str] = &[
    "crate::engine",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "deadlib_platform",
    "deadlib_present",
    "deadlib_render",
    "std::sync::mpsc",
    "Mutex",
    "log::",
];

const AUDIO_STREAM_FORBIDDEN_TOKENS: &[&str] = &[
    "crate::engine",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "deadlib_present",
    "deadlib_render",
];

const ENGINE_PLATFORM_SCAN_DIRS: &[&str] = &[
    "src/engine",
    "crates/deadsync-shell/src/app",
    "src/assets",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_RULE_FACADE_MODULES: &[&str] = &["judgment", "note", "scroll", "timing"];

const GAME_RULE_FACADE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_PROFILE_RULE_SYMBOLS: &[&str] = &["GUEST_SCROLL_SPEED", "ScrollSpeedSetting"];

const GAME_PROFILE_DATA_SYMBOLS: &[&str] = &[
    "AccelEffectsMask",
    "ActiveProfile",
    "AppearanceEffectsMask",
    "AttackMode",
    "AVERAGE_ERROR_BAR_INTENSITY_DEFAULT",
    "AVERAGE_ERROR_BAR_INTENSITY_MAX",
    "AVERAGE_ERROR_BAR_INTENSITY_MIN",
    "AVERAGE_ERROR_BAR_INTENSITY_STEP",
    "AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT",
    "AVERAGE_ERROR_BAR_INTERVAL_MS_MAX",
    "AVERAGE_ERROR_BAR_INTERVAL_MS_MIN",
    "AVERAGE_ERROR_BAR_INTERVAL_MS_STEP",
    "BackgroundFilter",
    "ComboColors",
    "ComboFont",
    "ComboMode",
    "CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS",
    "CUSTOM_FANTASTIC_WINDOW_MAX_MS",
    "CUSTOM_FANTASTIC_WINDOW_MIN_MS",
    "DataVisualizations",
    "ErrorBarMask",
    "ErrorBarStyle",
    "ErrorBarTrim",
    "GameplayHudPlayerSnapshot",
    "GameplayHudSnapshot",
    "HeldMissGraphic",
    "HideLightType",
    "HoldJudgmentGraphic",
    "HoldsMask",
    "InsertMask",
    "JudgmentGraphic",
    "LastPlayed",
    "LastPlayedCourse",
    "LifeMeterType",
    "LocalProfileSummary",
    "LiveTimingStatsMask",
    "LONG_ERROR_BAR_BUFFER_CAP_DEFAULT",
    "LONG_ERROR_BAR_BUFFER_CAP_MAX",
    "LONG_ERROR_BAR_BUFFER_CAP_MIN",
    "LONG_ERROR_BAR_INTENSITY_DEFAULT",
    "LONG_ERROR_BAR_INTENSITY_MAX",
    "LONG_ERROR_BAR_INTENSITY_MIN",
    "LONG_ERROR_BAR_INTENSITY_STEP",
    "LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT",
    "LONG_ERROR_BAR_MIN_SAMPLES_MAX",
    "LONG_ERROR_BAR_MIN_SAMPLES_MIN",
    "LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT",
    "LONG_ERROR_BAR_THRESHOLD_MS_MAX",
    "LONG_ERROR_BAR_THRESHOLD_MS_MIN",
    "MeasureCounter",
    "MeasureLines",
    "MiniIndicator",
    "MiniIndicatorColor",
    "MiniIndicatorScoreType",
    "MiniIndicatorSize",
    "NoteSkin",
    "NOTE_FIELD_OFFSET_X_MAX",
    "NOTE_FIELD_OFFSET_X_MIN",
    "NOTE_FIELD_OFFSET_Y_MAX",
    "NOTE_FIELD_OFFSET_Y_MIN",
    "DEFAULT_BIRTH_YEAR",
    "DEFAULT_WEIGHT_POUNDS",
    "HUD_OFFSET_MAX",
    "HUD_OFFSET_MIN",
    "MINI_PERCENT_MAX",
    "MINI_PERCENT_MIN",
    "PLAYER_SLOTS",
    "PLAYER_INITIALS_MAX_LEN",
    "Perspective",
    "PlayMode",
    "PlayStyle",
    "Profile",
    "ProfileStats",
    "ProfileStatsDecodeError",
    "PlayerOptionsData",
    "PlayerSide",
    "RemoveMask",
    "ScatterplotMaxWindow",
    "ScrollOption",
    "SESSION_JOINED_MASK_P1",
    "SESSION_JOINED_MASK_P2",
    "TAP_EXPLOSION_MASK_VERSION",
    "SPACING_PERCENT_MAX",
    "SPACING_PERCENT_MIN",
    "TapExplosionMask",
    "TargetScoreSetting",
    "TILT_MAX_THRESHOLD_DEFAULT_MS",
    "TILT_MIN_THRESHOLD_DEFAULT_MS",
    "TILT_THRESHOLD_MAX_MS",
    "TILT_THRESHOLD_MIN_MS",
    "TimingTickMode",
    "TimingWindowsOption",
    "TurnOption",
    "VISUAL_DELAY_MS_MAX",
    "VISUAL_DELAY_MS_MIN",
    "VisualEffectsMask",
    "active_profile_is_guest",
    "active_profile_local_id",
    "age_years_for_birth_year",
    "append_last_played_course_section",
    "append_last_played_section",
    "clamp_average_error_bar_intensity",
    "clamp_average_error_bar_interval_ms",
    "clamp_custom_fantastic_window_ms",
    "clamp_long_error_bar_buffer_cap",
    "clamp_long_error_bar_intensity",
    "clamp_long_error_bar_min_samples",
    "clamp_long_error_bar_threshold_ms",
    "clamp_tilt_threshold_ms",
    "clamp_weight_pounds",
    "cmp_profile_ids_case_insensitive",
    "decode_profile_stats",
    "encode_profile_stats",
    "error_bar_mask_from_style",
    "error_bar_style_from_mask",
    "error_bar_text_from_mask",
    "initials_from_name",
    "is_local_profile_id",
    "joined_player_mask",
    "normalize_tap_explosion_mask",
    "parse_groovestats_is_pad_player",
    "parse_last_played_value",
    "parse_profile_bool",
    "player_options_section",
    "player_side_index",
    "player_side_is_joined",
    "player_side_joined_mask",
    "resolve_noteskin_choice",
    "resolve_tap_explosion_skin",
    "resolved_weight_pounds",
    "rewrite_profile_display_name_content",
    "sanitize_player_initials",
    "tap_explosion_mask_enabled",
    "tap_explosion_mask_for_window",
    "tap_explosion_skin_hidden",
];

const GAME_CHART_FACADE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_PARSING_NOTES_FACADE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_SONG_DATA_SYMBOLS: &[&str] = &[
    "SongBackgroundChange",
    "SongBackgroundChangeTarget",
    "SongBackgroundLuaChange",
    "SongData",
    "SongForegroundChange",
    "SongForegroundLuaChange",
    "SongPack",
    "SyncPref",
];

const GAME_SONG_DATA_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_SCORE_DATA_SYMBOLS: &[&str] = &[
    "ArrowCloudLeaderboard",
    "ArrowCloudPaneKind",
    "ArrowCloudScore",
    "ArrowCloudScores",
    "ArrowCloudServerGrade",
    "ArrowCloudSubmitUiStatus",
    "ArrowCloudUserContext",
    "CachedPlayerLeaderboardData",
    "CachedItlScore",
    "CachedScore",
    "Grade",
    "GameplayScoreboxProfileSnapshot",
    "GrooveStatsEvalState",
    "GrooveStatsSubmitRecordBanner",
    "GrooveStatsSubmitUiStatus",
    "GROOVESTATS_REASON_COUNT",
    "GS_INVALID_HOLDS_MASK",
    "GS_INVALID_INSERT_MASK",
    "GS_INVALID_REMOVE_MASK",
    "GsCommentCounts",
    "GsExEvidence",
    "GsLampChartStats",
    "GsScoreEntry",
    "ItlEvalState",
    "ItlEventProgress",
    "ItlOverlayPage",
    "LOCAL_SCORE_VERSION",
    "LOCAL_SCORE_INDEX_VERSION",
    "LeaderboardEntry",
    "LeaderboardPane",
    "LocalReplayEdge",
    "LocalScalarScore",
    "LocalScoreBestScalar",
    "LocalScoreEntry",
    "LocalScoreHeader",
    "LocalScoreIndex",
    "MachineLeaderboardPlay",
    "MachineReplayEntry",
    "MachineReplayPlay",
    "PlayerLeaderboardCacheKey",
    "PlayerLeaderboardData",
    "RejectReason",
    "ReplayEdge",
    "ScoreBulkImportSummary",
    "ScoreImportEndpoint",
    "ScoreImportProgress",
    "SUBMIT_RETRY_MAX_ATTEMPTS",
    "arrowcloud_empty_hard_ex_leaderboard_pane",
    "arrowcloud_entry_flags",
    "arrowcloud_hard_ex_leaderboard_pane",
    "arrowcloud_leaderboard_entry",
    "arrowcloud_pane_kind_from_type",
    "arrowcloud_score_from_retrieve_fields",
    "arrowcloud_score_from_submit_percent",
    "arrowcloud_target_user_ids",
    "arrowcloud_user_id",
    "cached_failed_gs_score",
    "cached_gs_score_from_chart_stats",
    "cached_gs_score_from_lamp",
    "cached_missing_gs_score",
    "cached_score",
    "cached_score_10000",
    "cached_score_from_gs_entry",
    "cached_score_from_local_header",
    "compute_local_lamp",
    "decode_gs_score_entry",
    "decode_local_score_entry",
    "decode_local_score_header",
    "decode_local_score_index",
    "duration_to_ceil_secs",
    "encode_gs_score_entry",
    "encode_local_score_entry",
    "encode_local_score_index",
    "failed_score_override",
    "fix_gs_cached_score",
    "fix_local_ex_grade",
    "gameplay_run_failed",
    "gameplay_run_passed",
    "grade_from_code",
    "grade_to_code",
    "groovestats_reason_lines",
    "groovestats_submit_record_banner",
    "gs_score_entry_from_cached",
    "gs_ex_scoreboard_is_quint",
    "gs_lamp_index_from_chart_stats",
    "gs_lamp_judge_count",
    "is_better_itg",
    "is_better_scalar_score",
    "leaderboard_nonzero_rank",
    "leaderboard_pane",
    "leaderboard_rank_for_score",
    "leaderboard_score_10000",
    "leaderboard_username_matches",
    "lua_chart_submit_allowed",
    "lua_submit_allowed",
    "machine_leaderboard_entries",
    "machine_replay_entries",
    "merge_arrowcloud_score_slot",
    "parse_score_file_name",
    "parse_gs_comment_counts",
    "parse_gs_comment_ex_percent",
    "player_leaderboard_cache_key",
    "promote_quint_grade",
    "replaces_stale_quint",
    "same_score_10000",
    "score_file_shard",
    "score_import_entry_matches_profile",
    "score_to_grade",
    "scorebox_snapshot",
    "set_arrowcloud_score_for_leaderboard",
    "submit_retry_delay_secs",
    "update_local_score_index",
];

const GAME_SCORE_DATA_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAMEPLAY_LIMIT_SYMBOLS: &[&str] = &["MAX_COLS", "MAX_PLAYERS"];

const GAMEPLAY_LIMIT_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const CORE_NOTE_SYMBOLS: &[&str] = &["NoteType"];

const CORE_NOTE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-chart",
    "crates/deadsync-simfile",
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const ARROWCLOUD_PROTOCOL_SYMBOLS: &[&str] = &[
    "ARROWCLOUD_BULK_MAX_HASHES",
    "ConnectionError",
    "ConnectionProbeError",
    "ConnectionStatus",
    "DeviceLoginPollReq",
    "DeviceLoginPollResp",
    "DeviceLoginEvent",
    "DeviceLoginStartReq",
    "DeviceLoginStartResp",
    "DeviceLoginStatus",
    "ArrowCloudJudgmentCounts",
    "ArrowCloudLeaderboardEntry",
    "ArrowCloudLeaderboardPane",
    "ArrowCloudLeaderboardsApiResponse",
    "ArrowCloudLifePoint",
    "ArrowCloudModifiers",
    "ArrowCloudNpsInfo",
    "ArrowCloudNpsPoint",
    "ArrowCloudPayload",
    "ArrowCloudRadar",
    "ArrowCloudRetrieveScoreEntry",
    "ArrowCloudRetrieveScoresRequest",
    "ArrowCloudRetrieveScoresResponse",
    "ArrowCloudSpeed",
    "ArrowCloudSubmitRequestError",
    "ArrowCloudSubmitRequestSuccess",
    "ArrowCloudTimingDatum",
    "ArrowCloudTimingOffset",
    "ArrowCloudUserApiResponse",
    "ArrowCloudUserApiUser",
    "api_base_url",
    "check_connection",
    "classify_connection_error",
    "connection_error_from_network_error",
    "device_login_poll",
    "device_login_start",
    "fetch_leaderboards",
    "fetch_player_leaderboards",
    "fetch_user",
    "leaderboards_url",
    "legacy_leaderboards_url",
    "player_leaderboards_url",
    "probe_connection",
    "retrieve_scores_url",
    "retrieve_scores",
    "run_device_login_session",
    "submit_score_request",
    "submit_url",
    "user_url",
];

const ARROWCLOUD_PROTOCOL_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GROOVESTATS_PROTOCOL_SYMBOLS: &[&str] = &[
    "ConnectionError",
    "ConnectionProbeError",
    "ConnectionStatus",
    "GROOVESTATS_CHART_HASH_VERSION",
    "GROOVESTATS_COMMENT_PREFIX",
    "GROOVESTATS_SUBMIT_MAX_ENTRIES",
    "GrooveStatsJudgmentCounts",
    "GrooveStatsRescoreCounts",
    "GrooveStatsSubmitApiAchievement",
    "GrooveStatsSubmitApiAchievementReward",
    "GrooveStatsSubmitApiEvent",
    "GrooveStatsSubmitApiPlayer",
    "GrooveStatsSubmitApiProgress",
    "GrooveStatsSubmitApiQuest",
    "GrooveStatsSubmitApiQuestReward",
    "GrooveStatsSubmitApiResponse",
    "GrooveStatsSubmitApiStatImprovement",
    "GrooveStatsSubmitRequestError",
    "GrooveStatsSubmitRequestSuccess",
    "GrooveStatsQrLoginEvent",
    "GrooveStatsQrLoginWsEffect",
    "GrooveStatsSubmitPlayerPayload",
    "GROOVESTATS_QR_LOGIN_WS_READ_TIMEOUT_MS",
    "LeaderboardApiEntry",
    "LeaderboardApiPlayer",
    "LeaderboardEventData",
    "LeaderboardsApiResponse",
    "NewSessionResponse",
    "NewSessionServices",
    "Service",
    "Services",
    "api_base_url",
    "boogiestats_api_base_url",
    "compact_f32_text",
    "classify_qr_login_ws_message",
    "check_connection",
    "connection_error_from_network_error",
    "connection_status_from_new_session",
    "generate_qr_login_uuid",
    "manual_qr_url",
    "new_session_url",
    "fetch_player_leaderboards",
    "player_leaderboards_url",
    "probe_connection",
    "primary_api_base_url",
    "qr_base_url",
    "qr_login_url",
    "qr_login_uuid_message",
    "qr_login_ws_url",
    "run_qr_login_session",
    "score_submit_url",
    "service_name",
    "services_from_new_session",
    "submit_score_request",
];

const GROOVESTATS_PROTOCOL_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const LOBBY_DATA_SYMBOLS: &[&str] = &[
    "ConnectionState",
    "EVENT_CLIENT_DISCONNECTED",
    "EVENT_CREATE_LOBBY",
    "EVENT_JOIN_LOBBY",
    "EVENT_LEAVE_LOBBY",
    "EVENT_LOBBY_LEFT",
    "EVENT_LOBBY_SEARCHED",
    "EVENT_LOBBY_STATE",
    "EVENT_RESPONSE_STATUS",
    "EVENT_SEARCH_LOBBY",
    "EVENT_SELECT_SONG",
    "EVENT_UPDATE_MACHINE",
    "InboundEnvelope",
    "JoinedLobby",
    "LobbyInboundEffect",
    "LobbyInboundParseError",
    "LobbyJudgments",
    "LobbyLeftData",
    "LobbyMachinePlayer",
    "LobbyMachineState",
    "LobbyPlayer",
    "LobbySocket",
    "LobbySocketError",
    "LobbySearchedData",
    "LobbySongInfo",
    "LobbyStateData",
    "LobbyStatePlayerData",
    "LOBBY_PASSWORD_MAX_LEN",
    "LOBBY_SERVICE_URL",
    "MachinePlayerStats",
    "OutboundEnvelope",
    "PublicLobby",
    "PublicLobbyData",
    "ResponseStatus",
    "ResponseStatusData",
    "Snapshot",
    "close_lobby_socket",
    "connect_lobby_socket",
    "create_lobby_text",
    "flush_lobby_socket",
    "is_transient_lobby_socket_error",
    "joined_lobby_from_state",
    "join_lobby_text",
    "leave_lobby_text",
    "lobby_left_clears_joined",
    "lobby_machine_player",
    "lobby_machine_state_value",
    "lobby_profile_name",
    "normalize_lobby_password",
    "outbound_event_text",
    "parse_inbound_text",
    "public_lobbies_from_search",
    "read_lobby_text",
    "response_status_clears_joined",
    "response_status_from_data",
    "search_lobby_text",
    "select_song_text",
    "send_lobby_ping",
    "send_lobby_text",
    "update_machine_text",
];

const LOBBY_DATA_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const DOWNLOAD_PROTOCOL_SYMBOLS: &[&str] = &[
    "DownloadSnapshot",
    "DownloadZipError",
    "UnlockCache",
    "UnlockCacheFile",
    "cache_has_destination",
    "download_zip_to_path",
    "itl_unlock_pack_ini_content",
    "mime_token",
    "sanitize_pack_name",
];

const DOWNLOAD_PROTOCOL_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const NET_TRANSPORT_ERROR_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const NET_RESPONSE_BODY_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-online",
    "crates/deadsync-shell/src/app",
    "src/config",
    "src/game",
    "crates/deadsync-theme-simply-love/src/screens",
    "tests",
];

const GAME_TRANSPORT_CRATES: &[&str] = &["deadsync_net", "tungstenite", "ureq::"];

const NOTESKIN_CRATE_FORBIDDEN_TOKENS: &[&str] = &[
    "deadlib_platform",
    "deadlib_present",
    "deadlib_render",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "TextureKeyHandle",
    "INVALID_TEXTURE_HANDLE",
];

const NOTEFIELD_CRATE_FORBIDDEN_TOKENS: &[&str] = &[
    "deadsync-assets",
    "deadsync_assets",
    "deadsync_assets::noteskin",
    "deadsync::game::parsing::noteskin",
    "deadsync-theme-simply-love",
    "deadsync_theme_simply_love",
    "deadsync-shell",
    "deadsync_shell",
    "deadsync-screens",
    "deadsync_screens",
    "deadsync-config",
    "deadsync_config",
    "deadsync-profile",
    "deadsync_profile",
    "deadsync-online",
    "deadsync_online",
    "deadsync-simfile",
    "deadsync_simfile",
    "deadlib-renderer",
    "deadlib_renderer",
    "deadlib-render-backend-",
    "deadlib_render_backend_",
    "deadlib-video",
    "deadlib_video",
    "deadsync-input-fsr",
    "deadsync_input_fsr",
    "deadsync-input-native",
    "deadsync_input_native",
    "deadsync-input =",
    "deadsync_input::",
    "deadsync-audio",
    "deadsync_audio",
    "deadsync-audio-stream",
    "deadsync_audio_stream",
    "deadlib-platform",
    "deadlib_platform",
    "deadsync-smx",
    "deadsync_smx",
    "deadsync-lights",
    "deadsync_lights",
    "deadsync-updater",
    "deadsync_updater",
    "rfd",
    "winit",
    "std::fs",
    "std::net",
    "std::process",
    "Instant::now",
];

const CONTRACT_CRATE_FORBIDDEN_TOKENS: &[&str] = &[
    "deadsync-assets",
    "deadsync_assets",
    "deadsync-audio",
    "deadsync_audio",
    "deadsync-config",
    "deadsync_config",
    "deadsync-gameplay",
    "deadsync_gameplay",
    "deadsync-notefield",
    "deadsync_notefield",
    "deadsync-online",
    "deadsync_online",
    "deadsync-profile",
    "deadsync_profile",
    "deadsync-simfile",
    "deadsync_simfile",
    "deadsync-theme-simply-love",
    "deadsync_theme_simply_love",
    "deadsync-shell",
    "deadsync_shell",
    "deadlib-renderer",
    "deadlib_renderer",
    "deadlib-render-backend-",
    "deadlib_render_backend_",
    "deadlib-video",
    "deadlib_video",
    "deadsync-input-native",
    "deadsync_input_native",
    "deadsync-audio-stream",
    "deadsync_audio_stream",
    "deadlib-platform",
    "deadlib_platform",
    "deadsync-smx",
    "deadsync_smx",
    "deadsync-lights",
    "deadsync_lights",
    "deadsync-updater",
    "deadsync_updater",
    "rfd",
    "winit",
    "std::fs",
    "std::net",
    "std::process",
];

const THEME_SOURCE_DIRS: &[&str] = &[
    "crates/deadsync-theme/src",
    "crates/deadsync-theme-simply-love/src",
];

#[test]
fn theme_sources_do_not_import_shell() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut failures = Vec::new();

    for dir in THEME_SOURCE_DIRS {
        files.extend(rust_files(&root.join(dir)));
    }
    for crate_name in ["deadsync-theme", "deadsync-theme-simply-love"] {
        files.push(root.join("crates").join(crate_name).join("Cargo.toml"));
    }

    for file in files {
        let text = fs::read_to_string(&file).expect("theme crate file should be readable");
        for token in ["deadsync-shell", "deadsync_shell"] {
            if text.contains(token) {
                failures.push(format!("{}: {token}", rel_path(&root, &file)));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "theme sources must consume lower-layer contracts, not deadsync-shell:\n{}",
        failures.join("\n")
    );
}

#[test]
fn game_upward_dependencies_do_not_grow() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let game_dir = root.join("src/game");
    let baseline = baseline_map();
    let mut failures = Vec::new();

    for file in rust_files(&game_dir) {
        let text = fs::read_to_string(&file).expect("source file should be readable");
        let rel = rel_path(&root, &file);

        for target in ["assets", "config", "engine", "screens", "app"] {
            let count = count_game_upward_refs(&text, target);
            let allowed = baseline
                .get(&(rel.clone(), target.to_owned()))
                .copied()
                .unwrap_or(0);

            if count > allowed {
                failures.push(format!(
                    "{rel} references crate::{target} {count} times, baseline is {allowed}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "game layer gained upward dependencies:\n{}",
        failures.join("\n")
    );
}

#[test]
fn game_layer_does_not_import_transport_crates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let game_dir = root.join("src/game");
    let mut failures = Vec::new();

    for file in rust_files(&game_dir) {
        let text = fs::read_to_string(&file).expect("source file should be readable");
        let rel = rel_path(&root, &file);
        for token in GAME_TRANSPORT_CRATES {
            let count = text.match_indices(token).count();
            if count != 0 {
                failures.push(format!(
                    "{rel} references transport crate token {token} {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "game layer should depend on deadsync-online DTO/client APIs, not raw transport crates:\n{}",
        failures.join("\n")
    );
}

#[test]
fn noteskin_crate_stays_renderer_and_app_independent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = rust_files(&root.join("crates/deadsync-noteskin/src"));
    files.push(root.join("crates/deadsync-noteskin/Cargo.toml"));
    let mut failures = Vec::new();

    for file in files {
        let text = fs::read_to_string(&file).expect("noteskin crate file should be readable");
        let rel = rel_path(&root, &file);
        for token in NOTESKIN_CRATE_FORBIDDEN_TOKENS {
            let count = text.match_indices(token).count();
            if count != 0 {
                failures.push(format!(
                    "{rel} references forbidden token {token} {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "deadsync-noteskin must stay renderer/app independent:\n{}",
        failures.join("\n")
    );
}

#[test]
fn notefield_crate_stays_independent_of_themes_and_runtime() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = root.join("crates/deadsync-notefield");
    let mut files = rust_files(&crate_dir.join("src"));
    files.push(crate_dir.join("Cargo.toml"));
    let mut failures = Vec::new();

    for file in files {
        let text = fs::read_to_string(&file).expect("notefield crate file should be readable");
        let rel = rel_path(&root, &file);
        for token in NOTEFIELD_CRATE_FORBIDDEN_TOKENS {
            let count = text.match_indices(token).count();
            if count != 0 {
                failures.push(format!(
                    "{rel} references forbidden token {token} {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "deadsync-notefield must consume lower-layer data and generic theme contracts only:\n{}",
        failures.join("\n")
    );
}

#[test]
fn generic_theme_contract_stays_runtime_independent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = root.join("crates/deadsync-theme");
    let mut files = rust_files(&crate_dir.join("src"));
    files.push(crate_dir.join("Cargo.toml"));
    let mut failures = Vec::new();

    for file in files {
        let text = fs::read_to_string(&file).expect("contract crate file should be readable");
        let rel = rel_path(&root, &file);
        for token in CONTRACT_CRATE_FORBIDDEN_TOKENS {
            let count = text.match_indices(token).count();
            if count != 0 {
                failures.push(format!(
                    "{rel} references runtime or concrete-theme token {token} {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "the generic theme contract must not depend on concrete themes or runtime backends:\n{}",
        failures.join("\n")
    );
}

#[test]
fn generic_runtime_requests_stay_backend_neutral() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract = fs::read_to_string(root.join("crates/deadsync-theme/src/runtime.rs"))
        .expect("generic runtime-request contract should be readable");
    let concrete =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
            .expect("Simply Love runtime requests should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");

    for definition in [
        "pub enum AudioRequest",
        "pub struct GraphicsRequest",
        "pub enum RendererChoice",
        "pub enum DisplayModeChoice",
        "pub enum FullscreenChoice",
        "pub enum PresentPolicyChoice",
    ] {
        assert!(
            contract.contains(definition),
            "generic runtime-request contract is missing {definition}"
        );
    }
    for runtime_type in [
        "deadlib_render",
        "deadsync_config",
        "deadsync_audio_stream",
        "BackendType",
        "PresentModePolicy",
        "app_config::DisplayMode",
    ] {
        assert!(
            !contract.contains(runtime_type),
            "generic runtime-request contract exposes runtime type {runtime_type}"
        );
    }
    for wrapper in ["Audio(AudioRequest)", "Graphics(GraphicsRequest)"] {
        assert!(
            concrete.contains(wrapper),
            "Simply Love runtime request is missing generic wrapper {wrapper}"
        );
    }
    for concrete_type in [
        "BackendType",
        "PresentModePolicy",
        "app_config::DisplayMode",
    ] {
        assert!(
            !concrete.contains(concrete_type),
            "Simply Love runtime request still exposes concrete graphics type {concrete_type}"
        );
    }
    assert!(
        shell.contains("SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path))")
            && shell.contains("deadsync_audio_stream::play_sfx(&path)")
            && shell.contains("SimplyLoveRuntimeRequest::Graphics(request)")
            && shell.contains("self.handle_graphics_change(request, event_loop)"),
        "shell must execute generic audio and graphics requests"
    );
}

#[test]
fn presentation_crates_do_not_import_runtime_renderers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for crate_name in [
        "deadsync-notefield",
        "deadsync-theme",
        "deadsync-theme-simply-love",
    ] {
        let crate_dir = root.join("crates").join(crate_name);
        let mut files = rust_files(&crate_dir.join("src"));
        files.push(crate_dir.join("Cargo.toml"));
        for file in files {
            let text = fs::read_to_string(&file).expect("presentation file should be readable");
            for token in [
                "deadlib-renderer",
                "deadlib_renderer",
                "deadlib-render-backend-",
                "deadlib_render_backend_",
            ] {
                if text.contains(token) {
                    failures.push(format!("{}: {token}", rel_path(&root, &file)));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "presentation crates must consume render contracts, not runtime renderers:\n{}",
        failures.join("\n")
    );
}

#[test]
fn notefield_theme_dependency_points_toward_contracts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = |name: &str| {
        fs::read_to_string(root.join("crates").join(name).join("Cargo.toml"))
            .expect("manifest should be readable")
    };
    let theme = manifest("deadsync-theme");
    let notefield = manifest("deadsync-notefield");
    let simply_love = manifest("deadsync-theme-simply-love");

    assert!(notefield.contains("deadsync-theme ="));
    assert!(!notefield.contains("deadsync-theme-simply-love"));
    assert!(!theme.contains("deadsync-notefield ="));
    assert!(!theme.contains("deadsync-theme-simply-love"));
    assert!(simply_love.contains("deadsync-theme ="));
    assert!(simply_love.contains("deadsync-notefield ="));
}

#[test]
fn deterministic_gameplay_crate_stays_runtime_independent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = root.join("crates/deadsync-gameplay");
    let mut failures = Vec::new();

    if !crate_dir.exists() {
        failures.push("crates/deadsync-gameplay is missing".to_string());
    }

    if let Ok(manifest) = fs::read_to_string(crate_dir.join("Cargo.toml")) {
        for token in [
            "deadsync-audio",
            "deadsync-audio-backend-",
            "deadsync-audio-stream",
            "deadlib-platform",
            "deadsync-config",
            "deadsync-input-fsr",
            "deadsync-input-native",
            "deadsync-lights",
            "deadsync-notefield",
            "deadsync-noteskin",
            "deadsync-online",
            "deadlib-present",
            "deadsync-profile",
            "deadsync-simfile",
            "deadsync-shell",
            "deadsync-smx",
            "deadsync-song-lua",
            "deadsync-theme",
            "deadsync-updater",
            "deadlib-render",
            "deadlib-render-backend-",
            "deadlib-renderer",
            "deadsync-score",
            "deadlib-video",
            "rssp",
            "rfd",
            "winit",
        ] {
            let count = manifest.match_indices(token).count();
            if count != 0 {
                failures.push(format!(
                    "crates/deadsync-gameplay/Cargo.toml references runtime dependency {token} {count} times"
                ));
            }
        }
    }

    let src_dir = crate_dir.join("src");
    if src_dir.exists() {
        for file in rust_files(&src_dir) {
            let rel = rel_path(&root, &file);
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for token in [
                "crate::app",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadsync_audio",
                "deadsync_audio_backend_",
                "deadsync_audio_stream",
                "deadlib_platform",
                "deadsync_config",
                "deadsync_input_fsr",
                "deadsync_input_native",
                "deadsync_lights",
                "deadsync_notefield",
                "deadsync_noteskin",
                "deadsync_online",
                "deadlib_present",
                "deadsync_profile",
                "deadsync_simfile",
                "deadsync_shell",
                "deadsync_smx",
                "deadsync_song_lua",
                "deadsync_theme",
                "deadsync_updater",
                "deadlib_render",
                "deadlib_render_backend_",
                "deadlib_renderer",
                "deadsync_score",
                "deadlib_video",
                "rssp::",
                "rfd",
                "std::fs",
                "std::io",
                "std::net",
                "std::process",
                "winit",
            ] {
                let count = text.match_indices(token).count();
                if count != 0 {
                    failures.push(format!(
                        "{rel} references runtime dependency token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "deterministic gameplay crate should stay free of runtime dependencies:\n{}",
        failures.join("\n")
    );
}

#[test]
fn audio_core_lives_in_audio_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-audio/src/lib.rs"),
        root.join("crates/deadsync-audio/src/mixer.rs"),
        root.join("crates/deadsync-audio/src/output.rs"),
        root.join("crates/deadsync-audio/src/position.rs"),
        root.join("crates/deadsync-audio/src/render.rs"),
        root.join("crates/deadsync-audio/src/ring.rs"),
        root.join("crates/deadsync-audio/src/telemetry.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    for file in [
        root.join("crates/deadsync-audio-backend-native/Cargo.toml"),
        root.join("crates/deadsync-audio-backend-native/build.rs"),
        root.join("crates/deadsync-audio-backend-native/src/lib.rs"),
        root.join("crates/deadsync-audio-backend-native/src/freebsd_pcm.rs"),
        root.join("crates/deadsync-audio-backend-native/src/launch.rs"),
        root.join("crates/deadsync-audio-backend-native/src/linux_alsa.rs"),
        root.join("crates/deadsync-audio-backend-native/src/linux_jack.rs"),
        root.join("crates/deadsync-audio-backend-native/src/linux_pipewire.rs"),
        root.join("crates/deadsync-audio-backend-native/src/linux_pulse.rs"),
        root.join("crates/deadsync-audio-backend-native/src/macos_coreaudio.rs"),
        root.join("crates/deadsync-audio-backend-native/src/telemetry.rs"),
        root.join("crates/deadsync-audio-backend-native/src/windows_wasapi.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    if root.join("src/engine/audio/backends/telemetry.rs").exists() {
        failures.push(
            "src/engine/audio/backends/telemetry.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/freebsd_pcm.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/freebsd_pcm.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/macos_coreaudio.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/macos_coreaudio.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/linux_pulse.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/linux_pulse.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/linux_jack.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/linux_jack.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/linux_pipewire.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/linux_pipewire.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/linux_alsa.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/linux_alsa.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }
    if root
        .join("src/engine/audio/backends/windows_wasapi.rs")
        .exists()
    {
        failures.push(
            "src/engine/audio/backends/windows_wasapi.rs should live in deadsync-audio-backend-native"
                .to_string(),
        );
    }

    let engine_audio = root.join("src/engine/audio/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_audio) {
        for token in [
            "struct SpscRingI16",
            "struct SpscRingMusicSeg",
            "pub enum OutputTelemetryBackend",
            "pub enum OutputTelemetryClock",
            "pub enum OutputTimingQuality",
            "pub enum StutterDiagAudioEventKind",
            "pub struct StutterDiagAudioEvent",
            "struct AudioDiagEventSlot",
            "struct AudioTelemetryState",
            "AUDIO_STUTTER_DIAG_EVENT_COUNT",
            "static AUDIO_STUTTER_DIAG_",
            "static OUTPUT_TIMING_",
            "fn record_stutter_diag_event",
            "fn stutter_diag_callback_gap_threshold_ns",
            "pub struct Cut",
            "pub struct MusicStreamClockSnapshot",
            "enum CallbackClockSource",
            "struct CallbackClockWindow",
            "static MUSIC_TOTAL_FRAMES",
            "static MUSIC_TRACK_START_FRAME",
            "static MUSIC_TRACK_HAS_STARTED",
            "static MUSIC_TRACK_ACTIVE",
            "static MUSIC_CLOCK_",
            "static MUSIC_TRACK_ID",
            "static MUSIC_TARGET_GAIN_BITS",
            "static MUSIC_GAIN_SNAP_GEN",
            "static MUSIC_MAP_GEN",
            "static CALLBACK_CLOCK_",
            "static LAST_CALLBACK_",
            "static PREV_CALLBACK_",
            "static AUDIO_TIMING_DIAG_LAST_",
            "fn seed_music_stream_clock",
            "fn clear_music_stream_clock_seed",
            "fn seeded_music_position",
            "fn reset_music_stream_clock_state",
            "fn bump_music_map_generation",
            "fn music_map_generation",
            "fn stream_position_frames_from_callback",
            "fn stream_position_frames_from_anchor_pair",
            "fn begin_callback_clock_write",
            "fn end_callback_clock_write",
            "enum SfxLane",
            "struct QueuedSfx",
            "struct ActiveSfx",
            "type ActiveSfx",
            "static SCREEN_SFX_STOP_GEN",
            "static ASSIST_SFX_GEN",
            "fn sfx_stop_generation",
            "fn sfx_is_stale",
            "fn bump_screen_sfx_generation",
            "fn bump_assist_sfx_generation",
            "fn push_queued_sfx",
            "fn mix_active_sfx",
            "pub struct InitConfig",
            "pub struct AudioMixLevels",
            "static AUDIO_MIX_LEVELS_PACKED",
            "fn set_audio_mix_levels",
            "fn audio_mix_levels",
            "fn audio_mix_level_gains",
            "fn mix_level_gains",
            "const fn pack_audio_mix_levels",
            "const fn unpack_audio_mix_levels",
            "pub enum AudioOutputMode",
            "pub enum LinuxAudioBackend",
            "pub struct OutputDeviceInfo",
            "struct OutputBackendReady",
            "pub struct OutputTimingSnapshot",
            "const fn output_mode_bits",
            "const fn output_mode_from_bits",
            "enum ScheduledOnset",
            "fn scheduled_onset_decision",
            "MAX_SCHEDULE_AHEAD_FRAMES",
            "fn f32_to_i16",
            "fn i16_to_f32",
            "struct PlaybackPosMap",
            "impl PlaybackPosMap",
            "MUSIC_POS_MAP_BACKLOG_FRAMES",
            "static QUEUED_MUSIC_MAP_SEGS",
            "static PLAYED_MUSIC_MAP_SEGS",
            "static PLAYBACK_POS_MAP",
            "VecDeque<MusicMapSeg>",
            "fn audio_render_maps",
            "fn clear_music_pos_map",
            "fn lookup_music_position",
            "fn music_nanos_from_seconds",
            "fn normalized_music_rate",
            "fn fallback_music_position",
            "fn music_clock_seed_enabled",
            "NANOS_PER_SECOND",
            "std::cell::UnsafeCell",
            "struct RenderState",
            "impl RenderState",
            "pub struct AudioRenderMaps",
            "pub struct AudioRenderCallbackResult",
            "const MUSIC_GAIN_RAMP_FRAMES",
            "fn commit_played_music_map",
            "fn render_i16_host_nanos",
            "fn render_f32_host_nanos",
            "fn render_i16_qpc",
            "fn render_f32_qpc",
            "fn report_audio_render_callback",
            "fn publish_output_timing(",
            "fn publish_output_timing_quality",
            "fn note_output_underrun",
            "fn note_output_clock_fallback",
            "fn note_output_timing_sanity_failure",
            "struct WasapiBackendHint",
            "struct AlsaBackendHint",
            "struct JackBackendHint",
            "struct PipeWireBackendHint",
            "struct PulseBackendHint",
            "struct CoreAudioBackendHint",
            "struct FreeBsdPcmBackendHint",
            "enum OutputBackend",
            "struct AudioThreadLaunch",
            "struct NativeBackendLaunch",
            "struct OutputDeviceProbe",
            "fn build_audio_launch",
            "fn linux_default_output_device",
            "fn start_linux_alsa_backend",
            "fn start_linux_jack_backend",
            "fn start_linux_pipewire_backend",
            "fn start_linux_pulse_backend",
            "fn start_freebsd_pcm_backend",
            "fn start_macos_coreaudio_backend",
            "fn start_output_backend",
            "WasapiAccessMode",
            "static ASSIST_TICK_SFX",
            "const ASSIST_TICK_SFX_PATH",
            "HashMap<String, Arc<[i16]>>",
            "fn play_cached_sfx_on_lane",
            "fn cache_assist_tick",
            "load_and_resample_sfx",
            "enum AudioCommand",
            "MusicDecodeContext",
            "Option<MusicStream",
            "spawn_music_decoder_thread",
            "ring_new",
            "ring_clear",
            "activate_music_track",
            "stop_music_track",
            "AtomicU32",
            "Ordering",
            "pub(crate) use deadsync_audio::ring",
            "fn callback_nanos_at",
            "fn current_callback_clock_nanos",
            "fn load_callback_clock_snapshot_now",
            "fn stream_position_frames_from_window",
            "fn music_stream_clock_snapshot_at_nanos",
            "deadlib_platform::host_time::instant_nanos",
            "current_qpc_nanos",
            "fallback_stream_position_frames",
            "stream_position_frames_from_window as audio_stream_position_frames_from_window",
            "backends::windows_wasapi",
            "backends::freebsd_pcm",
            "backends::macos_coreaudio",
            "backends::linux_pulse",
            "backends::linux_jack",
            "backends::linux_pipewire",
            "backends::linux_alsa",
            "mod backends;",
            "windows_wasapi::prepare",
            "windows_wasapi::start",
            "fn start_wasapi_backend",
        ] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still defines realtime ring token {token} {count} times",
                    rel_path(&root, &engine_audio)
                ));
            }
        }
    }

    let backend_dir = root.join("src/engine/audio/backends");
    if backend_dir.join("mod.rs").exists() {
        failures.push(
            "src/engine/audio/backends/mod.rs should be deleted after native backend extraction"
                .to_string(),
        );
    }
    if backend_dir.exists() {
        for file in rust_files(&backend_dir) {
            let rel = rel_path(&root, &file);
            if rel.ends_with("telemetry.rs") {
                continue;
            }
            let text = fs::read_to_string(&file).expect("backend source file should be readable");
            for token in [
                "super::super",
                "crate::engine::audio::internal",
                "super::telemetry",
            ] {
                if text.contains(token) {
                    failures.push(format!(
                        "{rel} should import backend contracts directly instead of {token}"
                    ));
                }
            }
        }
    }

    let config_audio = root.join("src/config/audio.rs");
    if let Ok(text) = fs::read_to_string(&config_audio) {
        if !text.contains(
            "pub use deadsync_audio::{AudioMixLevels, AudioOutputMode, LinuxAudioBackend};",
        ) {
            failures.push(format!(
                "{} should re-export audio mix/config contracts from deadsync-audio",
                rel_path(&root, &config_audio)
            ));
        }
        for token in [
            "pub enum AudioOutputMode",
            "pub enum LinuxAudioBackend",
            "pub struct AudioMixLevels",
            "fn pack_audio_mix_levels",
            "fn unpack_audio_mix_levels",
            "pub(crate) use deadsync_audio::{pack_audio_mix_levels, unpack_audio_mix_levels};",
        ] {
            if text.contains(token) {
                failures.push(format!(
                    "{} still defines audio contract token {token}",
                    rel_path(&root, &config_audio)
                ));
            }
        }
    }

    let config_runtime = root.join("src/config/runtime.rs");
    if let Ok(text) = fs::read_to_string(&config_runtime) {
        for required in [
            "deadsync_audio::set_audio_mix_levels",
            "deadsync_audio::audio_mix_levels",
        ] {
            if !text.contains(required) {
                failures.push(format!(
                    "{} should delegate live audio mix-level state through {required}",
                    rel_path(&root, &config_runtime)
                ));
            }
        }
        for token in [
            "AUDIO_MIX_LEVELS_PACKED",
            "pack_audio_mix_levels",
            "unpack_audio_mix_levels",
        ] {
            if text.contains(token) {
                failures.push(format!(
                    "{} still owns audio mix-level state token {token}",
                    rel_path(&root, &config_runtime)
                ));
            }
        }
    }

    let audio_src = root.join("crates/deadsync-audio/src");
    if audio_src.exists() {
        for file in rust_files(&audio_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in AUDIO_CORE_FORBIDDEN_TOKENS {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden audio-core token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "audio core primitives should live in deadsync-audio:\n{}",
        failures.join("\n")
    );
}

#[test]
fn audio_decode_helpers_live_in_decode_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-audio-decode/src/lib.rs"),
        root.join("crates/deadsync-audio-decode/src/folder.rs"),
        root.join("crates/deadsync-audio-decode/src/resample.rs"),
        root.join("crates/deadsync-audio-stream/Cargo.toml"),
        root.join("crates/deadsync-audio-stream/src/clock.rs"),
        root.join("crates/deadsync-audio-stream/src/lib.rs"),
        root.join("crates/deadsync-audio-stream/src/sfx_cache.rs"),
        root.join("crates/deadsync-audio-stream/src/stream_runtime.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    let engine_resample = root.join("src/engine/audio/resample.rs");
    if engine_resample.exists() {
        failures.push(format!(
            "{} still exists; decoder stream runtime should live in deadsync-audio-stream",
            rel_path(&root, &engine_resample)
        ));
    }

    let engine_audio = root.join("src/engine/audio/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_audio) {
        for token in [
            "deadsync_audio_decode as decode",
            "snap_start_forward_to_packet",
            "MAX_PACKET_START_SNAP_SEC",
        ] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still references decode stream token {token} {count} times",
                    rel_path(&root, &engine_audio)
                ));
            }
        }
    }

    let stream_runtime = root.join("crates/deadsync-audio-stream/src/lib.rs");
    if let Ok(text) = fs::read_to_string(&stream_runtime) {
        for token in ["ENGINE", "crate::engine::audio"] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still references root audio runtime token {token} {count} times",
                    rel_path(&root, &stream_runtime)
                ));
            }
        }
    }

    let engine_folder = root.join("src/engine/audio/folder.rs");
    if engine_folder.exists() {
        failures.push(format!(
            "{} still exists; asset-path audio folder helpers should live in crates/deadsync-assets/src/audio_folder.rs",
            rel_path(&root, &engine_folder)
        ));
    }

    let assets_folder = root.join("crates/deadsync-assets/src/audio_folder.rs");
    if !assets_folder.exists() {
        failures.push(format!("{} is missing", rel_path(&root, &assets_folder)));
    }
    if let Ok(text) = fs::read_to_string(&assets_folder) {
        for token in [
            "fn is_ogg",
            "fn is_skipped_stem",
            "std::fs::read_dir",
            "path.is_file() && is_ogg",
            "dir.join(format!(\"{index}.ogg\"))",
        ] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still defines decode folder token {token} {count} times",
                    rel_path(&root, &assets_folder)
                ));
            }
        }
    }

    let decode_src = root.join("crates/deadsync-audio-decode/src");
    if decode_src.exists() {
        for file in rust_files(&decode_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in AUDIO_DECODE_FORBIDDEN_TOKENS {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden audio-decode token {token} {count} times"
                    ));
                }
            }
        }
    }

    let stream_src = root.join("crates/deadsync-audio-stream/src");
    if stream_src.exists() {
        for file in rust_files(&stream_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in AUDIO_STREAM_FORBIDDEN_TOKENS {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden audio-stream token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "audio decode helpers and stream runtime should live in audio crates:\n{}",
        failures.join("\n")
    );
}

#[test]
fn audio_analysis_cache_lives_in_analysis_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-audio-analysis/src/lib.rs"),
        root.join("crates/deadsync-audio-analysis/src/cache.rs"),
        root.join("crates/deadsync-audio-replaygain/Cargo.toml"),
        root.join("crates/deadsync-audio-replaygain/src/lib.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    let engine_replaygain = root.join("src/engine/audio/replaygain.rs");
    if engine_replaygain.exists() {
        failures.push(format!(
            "{} still exists; ReplayGain worker runtime should live in deadsync-audio-replaygain",
            rel_path(&root, &engine_replaygain)
        ));
    }

    let replaygain_runtime = root.join("crates/deadsync-audio-replaygain/src/lib.rs");
    if let Ok(text) = fs::read_to_string(&replaygain_runtime) {
        for token in [
            "struct PersistedEntry",
            "struct PersistedCacheV1",
            "fn encode_cache_file",
            "fn decode_cache_file",
            "fn path_hash",
            "CACHE_MAGIC",
            "CACHE_VERSION",
            "use bincode",
            "XxHash64",
            "fn source_mtime_unix_nanos",
            "ReplayGainCacheEntry::new",
            "create_dir_all",
            "write_all",
            "with_extension(\"bin.tmp\")",
        ] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still defines ReplayGain cache token {token} {count} times",
                    rel_path(&root, &replaygain_runtime)
                ));
            }
        }
    }

    let analysis_src = root.join("crates/deadsync-audio-analysis/src");
    if analysis_src.exists() {
        for file in rust_files(&analysis_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in AUDIO_ANALYSIS_FORBIDDEN_TOKENS {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden audio-analysis token {token} {count} times"
                    ));
                }
            }
        }
    }

    let replaygain_src = root.join("crates/deadsync-audio-replaygain/src");
    if replaygain_src.exists() {
        for file in rust_files(&replaygain_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadlib_platform",
                "deadlib_present",
                "deadlib_render",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden audio-replaygain token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "ReplayGain cache data and worker runtime should live in audio crates:\n{}",
        failures.join("\n")
    );
}

#[test]
fn logical_input_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in LOGICAL_INPUT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for symbol in LOGICAL_INPUT_SYMBOLS {
                let count = count_engine_input_symbol_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} imports {symbol} from engine::input {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "logical input should be imported from deadsync_input:\n{}",
        failures.join("\n")
    );
}

#[test]
fn native_input_launch_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    let input_dir = root.join("src/engine/input");
    if input_dir.exists() {
        failures.push("src/engine/input still exists; import input crates directly".to_string());
    }

    let backend_dir = root.join("src/engine/input/backends");
    if backend_dir.exists() {
        failures.push(
            "src/engine/input/backends still exists; import deadsync_input_native directly"
                .to_string(),
        );
    }

    let engine_mod_path = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod_path) {
        let count = count_token_refs(&text, "pub mod input");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::input {count} times; import input crates directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
    }

    let input_mod_path = root.join("src/engine/input/mod.rs");
    if let Ok(text) = fs::read_to_string(&input_mod_path) {
        for token in ["mod backends;", "BackendHost", "InputThreadPolicy"] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still references native input backend token {token} {count} times",
                    rel_path(&root, &input_mod_path)
                ));
            }
        }
        for symbol in NATIVE_INPUT_LAUNCH_SYMBOLS {
            let count = count_token_refs(&text, symbol);
            if count != 0 {
                failures.push(format!(
                    "{} still defines native input launch symbol {symbol} {count} times",
                    rel_path(&root, &input_mod_path)
                ));
            }
        }
    }

    for dir in NATIVE_INPUT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in ["use crate::engine::input;", "use deadsync::engine::input;"] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} imports native input facade token {token} {count} times"
                    ));
                }
            }
            for symbol in NATIVE_INPUT_LAUNCH_SYMBOLS {
                let count = count_engine_input_symbol_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} imports native input launch symbol {symbol} from engine::input {count} times"
                    ));
                }
            }
        }
    }

    for dir in INPUT_FSR_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in ["crate::engine::input", "deadsync::engine::input"] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::input facade token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "native input launch should be imported from deadsync_input_native:\n{}",
        failures.join("\n")
    );
}

#[test]
fn fsr_monitor_lives_in_input_fsr_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-input-fsr/Cargo.toml"),
        root.join("crates/deadsync-input-fsr/src/lib.rs"),
        root.join("crates/deadsync-input-fsr/src/fsrio.rs"),
        root.join("crates/deadsync-input-fsr/src/smx.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    let src = root.join("crates/deadsync-input-fsr/src");
    if src.exists() {
        for file in rust_files(&src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::app",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadsync::engine",
                "deadsync::app",
                "deadsync::assets",
                "deadsync::config",
                "deadsync::game",
                "deadsync::screens",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden root token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "FSR hardware monitor should live in deadsync-input-fsr:\n{}",
        failures.join("\n")
    );
}

#[test]
fn lights_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-lights/Cargo.toml"),
        root.join("crates/deadsync-lights/src/lib.rs"),
        root.join("crates/deadsync-lights/src/fusion.rs"),
        root.join("crates/deadsync-lights/src/gpb.rs"),
        root.join("crates/deadsync-lights/src/hid_blue_dot.rs"),
        root.join("crates/deadsync-lights/src/linux_leds.rs"),
        root.join("crates/deadsync-lights/src/litboard.rs"),
        root.join("crates/deadsync-lights/src/minimaid_hid.rs"),
        root.join("crates/deadsync-lights/src/pac_drive.rs"),
        root.join("crates/deadsync-lights/src/snek.rs"),
        root.join("crates/deadsync-lights/src/stac2.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    if root.join("src/engine/lights").exists() {
        failures
            .push("src/engine/lights still exists; import deadsync_lights directly".to_string());
    }
    let engine_mod_path = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod_path) {
        let count = count_token_refs(&text, "pub mod lights");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::lights {count} times; import deadsync_lights directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
    }

    let lights_src = root.join("crates/deadsync-lights/src");
    if lights_src.exists() {
        for file in rust_files(&lights_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::app",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadsync::engine",
                "deadsync::app",
                "deadsync::assets",
                "deadsync::config",
                "deadsync::game",
                "deadsync::screens",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden root token {token} {count} times"
                    ));
                }
            }
        }
    }

    for dir in LIGHTS_IMPORT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let count = count_engine_lights_facade_refs(&text);
            if count != 0 {
                failures.push(format!(
                    "{rel} references engine::lights facade {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "lights should be imported from deadsync_lights:\n{}",
        failures.join("\n")
    );
}

#[test]
fn smx_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in [
        root.join("crates/deadsync-smx/Cargo.toml"),
        root.join("crates/deadsync-smx/src/lib.rs"),
        root.join("crates/deadsync-smx/src/panels.rs"),
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    if root.join("src/engine/smx.rs").exists() {
        failures.push("src/engine/smx.rs still exists; import deadsync_smx directly".to_string());
    }
    if root.join("src/engine/smx_panels.rs").exists() {
        failures.push(
            "src/engine/smx_panels.rs still exists; import deadsync_smx::panels directly"
                .to_string(),
        );
    }

    let engine_mod_path = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod_path) {
        let count = count_token_refs(&text, "pub mod smx");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::smx {count} times; import deadsync_smx directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
        let panel_count = count_token_refs(&text, "pub mod smx_panels");
        if panel_count != 0 {
            failures.push(format!(
                "{} declares engine::smx_panels {panel_count} times; import deadsync_smx::panels directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
    }

    let smx_src = root.join("crates/deadsync-smx/src");
    if smx_src.exists() {
        for file in rust_files(&smx_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::app",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadsync::engine",
                "deadsync::app",
                "deadsync::assets",
                "deadsync::config",
                "deadsync::game",
                "deadsync::screens",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden root token {token} {count} times"
                    ));
                }
            }
        }
    }

    for dir in SMX_IMPORT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let count = count_engine_smx_facade_refs(&text);
            if count != 0 {
                failures.push(format!("{rel} references engine::smx facade {count} times"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "StepManiaX SDK manager should be imported from deadsync_smx:\n{}",
        failures.join("\n")
    );
}

#[test]
fn video_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let facade_path = root.join("src/engine/video");
    let mut failures = Vec::new();

    if facade_path.exists() {
        failures.push(format!(
            "{} still exists; import deadlib_video directly",
            rel_path(&root, &facade_path)
        ));
    }

    for dir in ENGINE_VIDEO_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let count = count_engine_video_facade_refs(&text);
            if count != 0 {
                failures.push(format!(
                    "{rel} references engine::video facade {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "video should be imported from deadlib_video:\n{}",
        failures.join("\n")
    );
}

#[test]
fn render_contract_imports_do_not_use_engine_gfx_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let draw_prep_path = root.join("src/engine/gfx/draw_prep.rs");
    let opengl_backend_path = root.join("src/engine/gfx/backends/opengl.rs");
    let software_backend_path = root.join("src/engine/gfx/backends/software.rs");
    let vulkan_backend_path = root.join("src/engine/gfx/backends/vulkan.rs");
    let wgpu_backend_path = root.join("src/engine/gfx/backends/wgpu_core.rs");
    let gfx_facade_path = root.join("src/engine/gfx/mod.rs");
    let mut failures = Vec::new();

    if draw_prep_path.exists() {
        failures.push(format!(
            "{} still exists; import deadlib_render::draw_prep directly",
            rel_path(&root, &draw_prep_path)
        ));
    }
    if opengl_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadlib-render-backend-gl",
            rel_path(&root, &opengl_backend_path)
        ));
    }
    if software_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadlib-render-backend-software",
            rel_path(&root, &software_backend_path)
        ));
    }
    if vulkan_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadlib-render-backend-vulkan",
            rel_path(&root, &vulkan_backend_path)
        ));
    }
    if wgpu_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadlib-render-backend-wgpu",
            rel_path(&root, &wgpu_backend_path)
        ));
    }
    if gfx_facade_path.exists() {
        failures.push(format!(
            "{} still exists; import deadlib_renderer directly",
            rel_path(&root, &gfx_facade_path)
        ));
    }

    let engine_mod_path = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod_path) {
        let count = count_token_refs(&text, "pub mod gfx");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::gfx {count} times; import deadlib_renderer directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
    }

    let renderer_src = root.join("crates/deadlib-renderer/src");
    if renderer_src.exists() {
        for file in rust_files(&renderer_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::assets",
                "crate::game",
                "crate::screens",
                "deadsync::engine",
                "deadsync::assets",
                "deadsync::game",
                "deadsync::screens",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden root token {token} {count} times"
                    ));
                }
            }
        }
    }

    for dir in RENDER_BACKEND_IMPORT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in RENDER_BACKEND_IMPORTS {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} imports concrete render backend {token} {count} times"
                    ));
                }
            }
        }
    }

    for dir in ENGINE_GFX_RENDER_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for symbol in ENGINE_GFX_RENDER_SYMBOLS {
                let count = count_engine_gfx_render_symbol_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::gfx::{symbol} facade {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "render contract should be imported from deadlib_render:\n{}",
        failures.join("\n")
    );
}

#[test]
fn present_model_lives_in_present_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in ENGINE_PRESENT_EXTRACTED_FILES {
        let path = root.join(file);
        if path.exists() {
            failures.push(format!(
                "{} still exists; use deadlib-present",
                rel_path(&root, &path)
            ));
        }
    }

    let engine_mod = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod) {
        let count = count_token_refs(&text, "pub mod space");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::space {count} times; import deadlib_present::space directly",
                rel_path(&root, &engine_mod)
            ));
        }
        let present_count = count_token_refs(&text, "pub mod present");
        if present_count != 0 {
            failures.push(format!(
                "{} declares engine::present {present_count} times; import deadlib_present directly",
                rel_path(&root, &engine_mod)
            ));
        }
    }

    for dir in PRESENT_SPACE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in ["crate::engine::space", "deadsync::engine::space"] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::space facade token {token} {count} times"
                    ));
                }
            }
        }
    }

    let src_assets = root.join("src/assets");
    if src_assets.exists() {
        failures.push(format!(
            "{} still exists; root assets should be re-exported from deadsync-assets",
            rel_path(&root, &src_assets)
        ));
    }

    let assets_lib = root.join("crates/deadsync-assets/src/lib.rs");
    if let Ok(text) = fs::read_to_string(&assets_lib) {
        for token in [
            "pub mod present_dsl",
            "PRESENT_TEXTURE_CONTEXT",
            "pub use manager::",
            "pub use textures::",
        ] {
            if !text.contains(token) {
                failures.push(format!(
                    "{} should expose app asset facade token {token}",
                    rel_path(&root, &assets_lib)
                ));
            }
        }
    } else {
        failures.push(format!("{} is missing", rel_path(&root, &assets_lib)));
    }

    let deadlib_assets_lib = root.join("crates/deadlib-assets/src/lib.rs");
    if let Ok(text) = fs::read_to_string(&deadlib_assets_lib) {
        if !text.contains("ASSET_TEXTURE_CONTEXT")
            || !text.contains("AssetTextureContext")
            || !text.contains("pub use present_dsl::SpriteBuilder")
        {
            failures.push(format!(
                "{} should own reusable asset-backed presentation texture context exports",
                rel_path(&root, &deadlib_assets_lib)
            ));
        }
    }

    let asset_dsl = root.join("crates/deadsync-assets/src/present_dsl.rs");
    if let Ok(text) = fs::read_to_string(&asset_dsl) {
        for token in ["SpriteBuilder", "TextBuilder", "TextureKeyHandle"] {
            if !text.contains(token) {
                failures.push(format!(
                    "{} should re-export asset-backed act! DSL token {token}",
                    rel_path(&root, &asset_dsl)
                ));
            }
        }
    } else {
        failures.push(format!("{} is missing", rel_path(&root, &asset_dsl)));
    }

    let asset_textures = root.join("crates/deadsync-assets/src/textures.rs");
    if let Ok(text) = fs::read_to_string(&asset_textures) {
        if !text.contains("GraphicTextureChoiceCache")
            || !text.contains("load_initial_textures")
            || !text.contains("load_texture_key")
        {
            failures.push(format!(
                "{} should own app texture loading and choice discovery",
                rel_path(&root, &asset_textures)
            ));
        }
    }

    for dir in [
        "crates/deadsync-shell/src/app",
        "src/config",
        "src/game",
        "crates/deadsync-theme-simply-love/src/screens",
        "tests",
    ] {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in [
                "crate::engine::present",
                "deadsync::engine::present",
                "crate::engine::present::compose",
                "deadsync::engine::present::compose",
                "crate::engine::present::dsl",
                "deadsync::engine::present::dsl",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::present facade token {token} {count} times"
                    ));
                }
            }
        }
    }

    let present_crate = root.join("crates/deadlib-present/src/lib.rs");
    if !present_crate.exists() {
        failures.push(format!("{} is missing", rel_path(&root, &present_crate)));
    }

    let present_dsl = root.join("crates/deadlib-present/src/dsl.rs");
    if let Ok(text) = fs::read_to_string(&present_dsl) {
        for macro_name in [
            "macro_rules! __ui_textalign_from_ident",
            "macro_rules! __ui_halign_from_ident",
            "macro_rules! __ui_valign_from_ident",
            "macro_rules! __dsl_apply",
            "macro_rules! __dsl_apply_one",
            "macro_rules! __act_from_builder",
        ] {
            if !text.contains(macro_name) {
                failures.push(format!(
                    "{} should own DSL alignment helper {macro_name}",
                    rel_path(&root, &present_dsl)
                ));
            }
        }
    }

    let present_src = root.join("crates/deadlib-present/src");
    if present_src.exists() {
        for file in rust_files(&present_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::assets",
                "crate::game",
                "crate::screens",
                "winit::",
                "wgpu::",
                "glow::",
                "use ash::",
                "ash::vk",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "presentation model should live in deadlib-present:\n{}",
        failures.join("\n")
    );
}

#[test]
fn version_utils_live_in_version_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    let version_crate = root.join("crates/deadsync-version/src/lib.rs");
    if !version_crate.exists() {
        failures.push(format!("{} is missing", rel_path(&root, &version_crate)));
    }

    let old_version = root.join("src/engine/version.rs");
    if old_version.exists() {
        failures.push(format!(
            "{} still exists; import deadsync_version directly",
            rel_path(&root, &old_version)
        ));
    }

    let engine_mod = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod) {
        if count_token_refs(&text, "pub mod version") != 0 {
            failures.push(format!(
                "{} declares engine::version; import deadsync_version directly",
                rel_path(&root, &engine_mod)
            ));
        }
    }

    for dir in VERSION_IMPORT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in ["crate::engine::version", "deadsync::engine::version"] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::version facade token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "version utilities should be imported from deadsync_version:\n{}",
        failures.join("\n")
    );
}

#[test]
fn updater_core_lives_in_updater_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    let updater_crate = root.join("crates/deadsync-updater/src/lib.rs");
    if !updater_crate.exists() {
        failures.push(format!("{} is missing", rel_path(&root, &updater_crate)));
    }

    for file_name in UPDATER_CORE_FILES {
        let crate_file = root.join("crates/deadsync-updater/src").join(file_name);
        if !crate_file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &crate_file)));
        }
        let root_file = root.join("src/engine/updater").join(file_name);
        if root_file.exists() {
            failures.push(format!(
                "{} still exists; import deadsync_updater directly",
                rel_path(&root, &root_file)
            ));
        }
    }

    let engine_updater = root.join("src/engine/updater/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_updater) {
        for module in [
            "action",
            "state",
            "cli",
            "download",
            "apply_journal",
            "apply_unix",
            "apply_windows",
        ] {
            if text.contains(&format!("pub mod {module}")) {
                failures.push(format!(
                    "{} declares engine::updater::{module}; import deadsync_updater::{module} directly",
                    rel_path(&root, &engine_updater)
                ));
            }
        }
    }

    let engine_mod = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod)
        && count_token_refs(&text, "pub mod updater") != 0
    {
        failures.push(format!(
            "{} declares engine::updater; import deadsync_updater directly",
            rel_path(&root, &engine_mod)
        ));
    }

    for dir in ["src", "tests"] {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for token in UPDATER_CORE_TOKENS {
                for prefix in ["crate::engine::updater", "deadsync::engine::updater"] {
                    let full = format!("{prefix}::{token}");
                    let count = count_token_refs(&text, &full);
                    if count != 0 {
                        failures.push(format!(
                            "{rel} references engine updater core token {full} {count} times"
                        ));
                    }
                }
            }
        }
    }

    let updater_src = root.join("crates/deadsync-updater/src");
    if updater_src.exists() {
        for file in rust_files(&updater_src) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            for token in [
                "crate::engine",
                "crate::app",
                "crate::assets",
                "crate::config",
                "crate::game",
                "crate::screens",
                "deadsync::engine",
                "deadsync::app",
                "deadsync::assets",
                "deadsync::config",
                "deadsync::game",
                "deadsync::screens",
            ] {
                let count = count_token_refs(&text, token);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references forbidden root token {token} {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "updater core should live in deadsync-updater:\n{}",
        failures.join("\n")
    );
}

#[test]
fn platform_imports_do_not_use_engine_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for module in ENGINE_PLATFORM_FACADE_MODULES {
        let file_path = root.join("src").join("engine").join(format!("{module}.rs"));
        let dir_path = root.join("src").join("engine").join(module);
        if file_path.exists() {
            failures.push(format!(
                "{} still exists; import deadlib_platform::{module} directly",
                rel_path(&root, &file_path)
            ));
        }
        if dir_path.exists() {
            failures.push(format!(
                "{} still exists; import deadlib_platform::{module} directly",
                rel_path(&root, &dir_path)
            ));
        }
    }
    for module in CONFIG_PLATFORM_FACADE_MODULES {
        let file_path = root.join("src").join("config").join(format!("{module}.rs"));
        let dir_path = root.join("src").join("config").join(module);
        if file_path.exists() {
            failures.push(format!(
                "{} still exists; import deadlib_platform::{module} directly",
                rel_path(&root, &file_path)
            ));
        }
        if dir_path.exists() {
            failures.push(format!(
                "{} still exists; import deadlib_platform::{module} directly",
                rel_path(&root, &dir_path)
            ));
        }
    }

    for dir in ENGINE_PLATFORM_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            for module in ENGINE_PLATFORM_FACADE_MODULES {
                let count = count_engine_platform_facade_refs(&text, module);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references engine::{module} facade {count} times"
                    ));
                }
            }
            for module in CONFIG_PLATFORM_FACADE_MODULES {
                let count = count_config_platform_facade_refs(&text, module);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references config::{module} facade {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "platform helpers should be imported from deadlib_platform:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rule_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for module in GAME_RULE_FACADE_MODULES {
        let path = root.join("src").join("game").join(format!("{module}.rs"));
        if path.exists() {
            failures.push(format!(
                "{} still exists; import deadsync_rules::{module} directly",
                rel_path(&root, &path)
            ));
        }
    }

    for dir in GAME_RULE_FACADE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for module in GAME_RULE_FACADE_MODULES {
                let count = count_game_rule_facade_refs(&text, module);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references game::{module} facade {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "rules should be imported from deadsync_rules:\n{}",
        failures.join("\n")
    );
}

#[test]
fn profile_rule_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in GAME_RULE_FACADE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in GAME_PROFILE_RULE_SYMBOLS {
                let count = count_game_profile_rule_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references {symbol} through game::profile {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "profile rule types should be imported from deadsync_rules:\n{}",
        failures.join("\n")
    );
}

#[test]
fn game_profile_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in rust_files(&root.join("src/game")) {
        scan_game_profile_data_file(&root, &file, &mut failures);
    }

    assert!(
        failures.is_empty(),
        "game layer profile data should be imported from deadsync_profile:\n{}",
        failures.join("\n")
    );
}

#[test]
fn app_helper_profile_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in rust_files(&root.join("crates/deadsync-shell/src/app")) {
        scan_game_profile_data_file(&root, &file, &mut failures);
    }

    assert!(
        failures.is_empty(),
        "app helper profile data should be imported from deadsync_profile:\n{}",
        failures.join("\n")
    );
}

#[test]
fn screen_profile_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in rust_files(&root.join("crates/deadsync-theme-simply-love/src/screens")) {
        scan_game_profile_data_file(&root, &file, &mut failures);
    }

    assert!(
        failures.is_empty(),
        "screen profile data should be imported from deadsync_profile:\n{}",
        failures.join("\n")
    );
}

#[test]
fn chart_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    let facade_path = root.join("src/game/chart.rs");

    if facade_path.exists() {
        failures.push(format!(
            "{} still exists; import deadsync_chart directly",
            rel_path(&root, &facade_path)
        ));
    }

    for dir in GAME_CHART_FACADE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let count = count_game_chart_facade_refs(&text);
            if count != 0 {
                failures.push(format!("{rel} references game::chart facade {count} times"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "chart data should be imported from deadsync_chart:\n{}",
        failures.join("\n")
    );
}

#[test]
fn parsing_notes_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    let facade_path = root.join("src/game/parsing/notes.rs");

    if facade_path.exists() {
        failures.push(format!(
            "{} still exists; import deadsync_chart::notes or deadsync_simfile::notes directly",
            rel_path(&root, &facade_path)
        ));
    }

    for dir in GAME_PARSING_NOTES_FACADE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let count = count_game_parsing_notes_facade_refs(&text);
            if count != 0 {
                failures.push(format!(
                    "{rel} references game::parsing::notes facade {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "parsed notes should come from deadsync_chart and parsing from deadsync_simfile:\n{}",
        failures.join("\n")
    );
}

#[test]
fn song_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in GAME_SONG_DATA_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in GAME_SONG_DATA_SYMBOLS {
                let count = count_game_song_data_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references {symbol} through game::song {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "song data should be imported from deadsync_chart while game::song owns only cache/sync helpers:\n{}",
        failures.join("\n")
    );
}

#[test]
fn score_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in GAME_SCORE_DATA_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in GAME_SCORE_DATA_SYMBOLS {
                let count = count_game_score_data_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references {symbol} through game::scores {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "score data should be imported from deadsync_score while game::scores owns cache and online services:\n{}",
        failures.join("\n")
    );
}

#[test]
fn gameplay_limits_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in GAMEPLAY_LIMIT_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in GAMEPLAY_LIMIT_SYMBOLS {
                let count = count_gameplay_limit_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references {symbol} through game::gameplay {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "gameplay player/column limits should be imported from deadsync_core::input:\n{}",
        failures.join("\n")
    );
}

#[test]
fn core_note_imports_do_not_use_rules_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    let rules_note_path = root.join("crates/deadsync-rules/src/note.rs");

    if rules_note_path.exists() {
        let text = fs::read_to_string(&rules_note_path).expect("source file should be readable");
        if text.contains("pub use deadsync_core::note::NoteType") {
            failures.push(format!(
                "{} re-exports NoteType; import deadsync_core::note::NoteType directly",
                rel_path(&root, &rules_note_path)
            ));
        }
    }

    for dir in CORE_NOTE_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in CORE_NOTE_SYMBOLS {
                let count = count_core_note_rules_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references {symbol} through deadsync_rules::note {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "core note primitives should be imported from deadsync_core::note:\n{}",
        failures.join("\n")
    );
}

#[test]
fn arrowcloud_protocol_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in ARROWCLOUD_PROTOCOL_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in ARROWCLOUD_PROTOCOL_SYMBOLS {
                let count = count_arrowcloud_protocol_game_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references ArrowCloud protocol {symbol} through game::online {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "ArrowCloud protocol DTOs, clients, and URL helpers should be imported from deadsync_online::arrowcloud:\n{}",
        failures.join("\n")
    );
}

#[test]
fn groovestats_protocol_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in GROOVESTATS_PROTOCOL_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in GROOVESTATS_PROTOCOL_SYMBOLS {
                let count = count_groovestats_protocol_game_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references GrooveStats protocol {symbol} through game::online {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "GrooveStats protocol DTOs and URL helpers should be imported from deadsync_online::groovestats:\n{}",
        failures.join("\n")
    );
}

#[test]
fn lobby_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in LOBBY_DATA_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in LOBBY_DATA_SYMBOLS {
                let count = count_lobby_data_game_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references lobby protocol {symbol} through game::online {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "lobby protocol data and helpers should be imported from deadsync_online::lobbies:\n{}",
        failures.join("\n")
    );
}

#[test]
fn download_protocol_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in DOWNLOAD_PROTOCOL_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            for symbol in DOWNLOAD_PROTOCOL_SYMBOLS {
                let count = count_download_protocol_game_facade_refs(&text, symbol);
                if count != 0 {
                    failures.push(format!(
                        "{rel} references download protocol {symbol} through game::online {count} times"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "download protocol data and helpers should be imported from deadsync_online::downloads:\n{}",
        failures.join("\n")
    );
}

#[test]
fn transport_error_mapping_stays_in_net_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in NET_TRANSPORT_ERROR_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let count = text.match_indices("ureq::Error::StatusCode").count();
            if count != 0 {
                failures.push(format!(
                    "{rel} maps ureq status errors directly {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "transport error classification should use deadsync_net helpers:\n{}",
        failures.join("\n")
    );
}

#[test]
fn response_body_decoding_stays_in_net_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for dir in NET_RESPONSE_BODY_SCAN_DIRS {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        for file in rust_files(&path) {
            let rel = rel_path(&root, &file);
            if rel == "tests/architecture_boundaries.rs" {
                continue;
            }
            let text = fs::read_to_string(&file).expect("source file should be readable");
            let count = text.match_indices("into_body().read_json()").count()
                + text.match_indices("into_body().read_to_string()").count();
            if count != 0 {
                failures.push(format!(
                    "{rel} decodes ureq response bodies directly {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "ureq response body decoding should use deadsync_net helpers:\n{}",
        failures.join("\n")
    );
}

fn baseline_map() -> HashMap<(String, String), usize> {
    GAME_UPWARD_DEP_BASELINE
        .iter()
        .map(|(path, target, count)| (((*path).to_owned(), (*target).to_owned()), *count))
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    collect_rust_files(dir, &mut out);
    out.sort();
    out
}

fn files_named(dir: &Path, name: &str) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    collect_files_named(dir, name, &mut out);
    out.sort();
    out
}

fn collect_files_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory should be readable") {
        let path = entry.expect("source entry should be readable").path();
        if path.is_dir() {
            collect_files_named(&path, name, out);
        } else if path.file_name().is_some_and(|file_name| file_name == name) {
            out.push(path);
        }
    }
}

fn count_outside_notefield_forbidden_tokens(text: &str, token: &str) -> usize {
    let start = text
        .find("const NOTEFIELD_CRATE_FORBIDDEN_TOKENS")
        .expect("notefield forbidden-token list should exist");
    let end = start
        + text[start..]
            .find("];")
            .expect("notefield forbidden-token list should end")
        + 2;
    text[..start].match_indices(token).count() + text[end..].match_indices(token).count()
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory should be readable") {
        let path = entry.expect("source entry should be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("source file should be under manifest dir")
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn count_game_upward_refs(text: &str, target: &str) -> usize {
    if target == "config" {
        return count_token_refs(text, "crate::config");
    }
    text.match_indices(&format!("crate::{target}::")).count()
}

fn count_token_refs(text: &str, token: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(token) {
        let after = &rest[index + token.len()..];
        if after
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
        {
            count += 1;
        }
        rest = after;
    }

    count
}

fn count_engine_input_symbol_refs(text: &str, symbol: &str) -> usize {
    let direct = [
        format!("crate::engine::input::{symbol}"),
        format!("deadsync::engine::input::{symbol}"),
    ]
    .iter()
    .map(|token| count_token_refs(text, token))
    .sum::<usize>();

    direct
        + count_grouped_engine_input_uses(text, "use crate::engine::input::{", symbol)
        + count_grouped_engine_input_uses(text, "use deadsync::engine::input::{", symbol)
}

fn count_engine_video_facade_refs(text: &str) -> usize {
    count_token_refs(text, "crate::engine::video")
        + count_token_refs(text, "deadsync::engine::video")
        + count_grouped_game_rule_uses(text, "use crate::engine::{", "video")
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", "video")
}

fn count_engine_lights_facade_refs(text: &str) -> usize {
    count_token_refs(text, "crate::engine::lights")
        + count_token_refs(text, "deadsync::engine::lights")
        + count_grouped_game_rule_uses(text, "use crate::engine::{", "lights")
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", "lights")
}

fn count_engine_smx_facade_refs(text: &str) -> usize {
    count_token_refs(text, "crate::engine::smx")
        + count_token_refs(text, "deadsync::engine::smx")
        + count_token_refs(text, "crate::engine::smx_panels")
        + count_token_refs(text, "deadsync::engine::smx_panels")
        + count_grouped_game_rule_uses(text, "use crate::engine::{", "smx")
        + count_grouped_game_rule_uses(text, "use crate::engine::{", "smx_panels")
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", "smx")
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", "smx_panels")
}

fn count_engine_gfx_render_symbol_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::engine::gfx::{symbol}"))
        + count_token_refs(text, &format!("deadsync::engine::gfx::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::engine::gfx::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::engine::gfx::{", symbol)
}

fn count_engine_platform_facade_refs(text: &str, module: &str) -> usize {
    count_token_refs(text, &format!("crate::engine::{module}"))
        + count_token_refs(text, &format!("deadsync::engine::{module}"))
        + count_grouped_game_rule_uses(text, "use crate::engine::{", module)
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", module)
}

fn count_config_platform_facade_refs(text: &str, module: &str) -> usize {
    count_token_refs(text, &format!("crate::config::{module}"))
        + count_token_refs(text, &format!("deadsync::config::{module}"))
        + count_grouped_game_rule_uses(text, "use crate::config::{", module)
        + count_grouped_game_rule_uses(text, "use deadsync::config::{", module)
}

fn count_grouped_engine_input_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        if statement
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|token| token == symbol)
        {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_game_rule_facade_refs(text: &str, module: &str) -> usize {
    count_token_refs(text, &format!("crate::game::{module}"))
        + count_token_refs(text, &format!("deadsync::game::{module}"))
        + count_grouped_game_rule_uses(text, "use crate::game::{", module)
        + count_grouped_game_rule_uses(text, "use deadsync::game::{", module)
}

fn count_game_profile_rule_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::profile::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::profile::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::profile::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::profile::{", symbol)
        + count_grouped_profile_rule_uses(text, "use crate::game::{", symbol)
        + count_grouped_profile_rule_uses(text, "use deadsync::game::{", symbol)
}

fn scan_game_profile_data_file(root: &Path, file: &Path, failures: &mut Vec<String>) {
    let rel = rel_path(root, file);
    if rel == "tests/architecture_boundaries.rs" {
        return;
    }
    let text = fs::read_to_string(file).expect("source file should be readable");
    for symbol in GAME_PROFILE_DATA_SYMBOLS {
        let count = count_game_profile_data_facade_refs(&text, symbol);
        if count != 0 {
            failures.push(format!(
                "{rel} references profile data {symbol} through game::profile {count} times"
            ));
        }
    }
}

fn count_game_profile_data_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::profile::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::profile::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::profile::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::profile::{", symbol)
        + count_grouped_profile_rule_uses(text, "use crate::game::{", symbol)
        + count_grouped_profile_rule_uses(text, "use deadsync::game::{", symbol)
        + count_game_profile_alias_symbol_refs(text, symbol)
}

fn count_game_profile_alias_symbol_refs(text: &str, symbol: &str) -> usize {
    let mut count = 0;
    if imports_game_profile_alias(text) {
        count += count_ident_prefixed_refs(text, &format!("profile::{symbol}"));
    }
    for alias in game_profile_aliases(text) {
        count += count_ident_prefixed_refs(text, &format!("{alias}::{symbol}"));
    }
    count
}

fn imports_game_profile_alias(text: &str) -> bool {
    count_token_refs(text, "use crate::game::profile;") > 0
        || count_token_refs(text, "use deadsync::game::profile;") > 0
        || grouped_use_contains_token(text, "use crate::game::{", "profile")
        || grouped_use_contains_token(text, "use deadsync::game::{", "profile")
}

fn game_profile_aliases(text: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for marker in [
        "use crate::game::profile as ",
        "use deadsync::game::profile as ",
        "use crate::game::profile::{self as ",
        "use deadsync::game::profile::{self as ",
    ] {
        collect_aliases_after(text, marker, &mut aliases);
    }
    aliases
}

fn collect_aliases_after(text: &str, marker: &str, aliases: &mut Vec<String>) {
    let mut offset = 0;
    while let Some(index) = text[offset..].find(marker) {
        let start = offset + index + marker.len();
        let alias: String = text[start..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        let len = alias.len();
        if !alias.is_empty() && !aliases.contains(&alias) {
            aliases.push(alias);
        }
        offset = start + len;
    }
}

fn count_ident_prefixed_refs(text: &str, token: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;

    while let Some(index) = text[offset..].find(token) {
        let start = offset + index;
        let end = start + token.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        let before_ok = before.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        let after_ok = after.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        if before_ok && after_ok {
            count += 1;
        }
        offset = end;
    }

    count
}

fn count_grouped_profile_rule_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_profile = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_profile |= token == "profile";
            saw_symbol |= token == symbol;
        }
        if saw_profile && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_grouped_game_rule_uses(text: &str, marker: &str, module: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        if statement
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|token| token == module)
        {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_game_chart_facade_refs(text: &str) -> usize {
    count_token_refs(text, "crate::game::chart")
        + count_token_refs(text, "deadsync::game::chart")
        + count_grouped_game_rule_uses(text, "use crate::game::{", "chart")
        + count_grouped_game_rule_uses(text, "use deadsync::game::{", "chart")
}

fn count_game_parsing_notes_facade_refs(text: &str) -> usize {
    count_token_refs(text, "crate::game::parsing::notes")
        + count_token_refs(text, "deadsync::game::parsing::notes")
}

fn count_game_song_data_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::song::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::song::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::song::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::song::{", symbol)
        + count_grouped_game_song_data_uses(text, "use crate::game::{", symbol)
        + count_grouped_game_song_data_uses(text, "use deadsync::game::{", symbol)
}

fn count_grouped_game_song_data_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_song = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_song |= token == "song";
            saw_symbol |= token == symbol;
        }
        if saw_song && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_game_score_data_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::scores::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::scores::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::scores::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::scores::{", symbol)
        + count_grouped_game_score_data_uses(text, "use crate::game::{", symbol)
        + count_grouped_game_score_data_uses(text, "use deadsync::game::{", symbol)
        + count_game_scores_alias_symbol_refs(text, symbol)
}

fn count_grouped_game_score_data_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_scores = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_scores |= token == "scores";
            saw_symbol |= token == symbol;
        }
        if saw_scores && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_game_scores_alias_symbol_refs(text: &str, symbol: &str) -> usize {
    if !imports_game_scores_alias(text) {
        return 0;
    }
    count_token_refs(text, &format!("scores::{symbol}"))
}

fn imports_game_scores_alias(text: &str) -> bool {
    count_token_refs(text, "use crate::game::scores;") > 0
        || count_token_refs(text, "use deadsync::game::scores;") > 0
        || grouped_use_contains_token(text, "use crate::game::{", "scores")
        || grouped_use_contains_token(text, "use deadsync::game::{", "scores")
}

fn grouped_use_contains_token(text: &str, marker: &str, target: &str) -> bool {
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        if statement
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|token| token == target)
        {
            return true;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    false
}

fn count_gameplay_limit_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::gameplay::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::gameplay::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::gameplay::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::gameplay::{", symbol)
        + count_grouped_gameplay_limit_uses(text, "use crate::game::{", symbol)
        + count_grouped_gameplay_limit_uses(text, "use deadsync::game::{", symbol)
}

fn count_grouped_gameplay_limit_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_gameplay = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_gameplay |= token == "gameplay";
            saw_symbol |= token == symbol;
        }
        if saw_gameplay && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_core_note_rules_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("deadsync_rules::note::{symbol}"))
        + count_grouped_game_rule_uses(text, "use deadsync_rules::note::{", symbol)
}

fn count_arrowcloud_protocol_game_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::online::arrowcloud::{symbol}"))
        + count_token_refs(
            text,
            &format!("deadsync::game::online::arrowcloud::{symbol}"),
        )
        + count_token_refs(text, &format!("crate::game::online::arrowcloud_{symbol}"))
        + count_token_refs(
            text,
            &format!("deadsync::game::online::arrowcloud_{symbol}"),
        )
        + count_grouped_game_rule_uses(text, "use crate::game::online::arrowcloud::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::online::arrowcloud::{", symbol)
        + count_grouped_arrowcloud_protocol_online_uses(text, "use crate::game::online::{", symbol)
        + count_grouped_arrowcloud_protocol_online_uses(
            text,
            "use deadsync::game::online::{",
            symbol,
        )
}

fn count_grouped_arrowcloud_protocol_online_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;
    let prefixed_symbol = format!("arrowcloud_{symbol}");

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_arrowcloud = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_arrowcloud |= token == "arrowcloud";
            saw_symbol |= token == symbol || token == prefixed_symbol.as_str();
        }
        if saw_arrowcloud && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_groovestats_protocol_game_facade_refs(text: &str, symbol: &str) -> usize {
    count_token_refs(text, &format!("crate::game::online::groovestats::{symbol}"))
        + count_token_refs(
            text,
            &format!("deadsync::game::online::groovestats::{symbol}"),
        )
        + count_token_refs(text, &format!("crate::game::online::groovestats_{symbol}"))
        + count_token_refs(
            text,
            &format!("deadsync::game::online::groovestats_{symbol}"),
        )
        + count_grouped_game_rule_uses(text, "use crate::game::online::groovestats::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::online::groovestats::{", symbol)
        + count_grouped_groovestats_protocol_online_uses(text, "use crate::game::online::{", symbol)
        + count_grouped_groovestats_protocol_online_uses(
            text,
            "use deadsync::game::online::{",
            symbol,
        )
}

fn count_grouped_groovestats_protocol_online_uses(text: &str, marker: &str, symbol: &str) -> usize {
    let mut count = 0;
    let mut rest = text;
    let prefixed_symbol = format!("groovestats_{symbol}");

    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        let end = after.find(';').unwrap_or(after.len());
        let statement = &after[..end];
        let mut saw_groovestats = false;
        let mut saw_symbol = false;
        for token in statement.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
            saw_groovestats |= token == "groovestats";
            saw_symbol |= token == symbol || token == prefixed_symbol.as_str();
        }
        if saw_groovestats && saw_symbol {
            count += 1;
        }
        rest = &after[end..];
        if end == after.len() {
            break;
        }
    }

    count
}

fn count_lobby_data_game_facade_refs(text: &str, symbol: &str) -> usize {
    let module_alias = text.contains("use crate::game::online::lobbies;")
        || text.contains("use deadsync::game::online::lobbies;")
        || text.contains("use crate::game::online::lobbies as lobbies;")
        || text.contains("use deadsync::game::online::lobbies as lobbies;");

    count_token_refs(text, &format!("crate::game::online::lobbies::{symbol}"))
        + count_token_refs(text, &format!("deadsync::game::online::lobbies::{symbol}"))
        + count_grouped_game_rule_uses(text, "use crate::game::online::lobbies::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::online::lobbies::{", symbol)
        + if module_alias {
            count_token_refs(text, &format!("lobbies::{symbol}"))
        } else {
            0
        }
}

#[test]
fn concrete_theme_resources_live_in_simply_love() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let generic_theme = root.join("crates/deadsync-theme");
    let generic_files = rust_files(&generic_theme.join("src"));
    assert!(
        !generic_files.is_empty(),
        "generic theme must expose contract source files"
    );
    for file in generic_files {
        let generic_source =
            fs::read_to_string(&file).expect("generic theme source should be readable");
        for token in [
            "MachineFont",
            "VisualStyle",
            "SrpgVariant",
            "FONT_ASSETS",
            "SRPG10_",
            "pub mod i18n",
            "scorebox",
            "step_stats",
        ] {
            assert!(
                !generic_source.contains(token),
                "{} still owns concrete resource token {token}",
                rel_path(&root, &file)
            );
        }
    }

    let generic_manifest = fs::read_to_string(generic_theme.join("Cargo.toml"))
        .expect("generic theme manifest should be readable");
    for dependency in ["deadsync-config", "deadsync-profile", "log =", "rand ="] {
        assert!(
            !generic_manifest.contains(dependency),
            "generic theme still depends on concrete resource dependency {dependency}"
        );
    }

    let assets = root.join("crates/deadsync-assets");
    for removed in ["src/i18n.rs", "src/visual_styles.rs"] {
        assert!(
            !assets.join(removed).exists(),
            "deadsync-assets still owns concrete theme module {removed}"
        );
    }
    let assets_source =
        fs::read_to_string(assets.join("src/lib.rs")).expect("asset facade should be readable");
    for token in [
        "pub mod i18n",
        "pub mod visual_styles",
        "FontRole",
        "current_machine_font_key",
    ] {
        assert!(
            !assets_source.contains(token),
            "deadsync-assets still exports concrete theme token {token}"
        );
    }
    let assets_manifest =
        fs::read_to_string(assets.join("Cargo.toml")).expect("asset manifest should be readable");
    assert!(
        !assets_manifest.contains("deadsync-theme-simply-love"),
        "deadsync-assets must not depend on the concrete theme"
    );

    let simply_love = root.join("crates/deadsync-theme-simply-love");
    for owned in [
        "src/fonts.rs",
        "src/i18n.rs",
        "src/i18n_runtime.rs",
        "src/notefield_style.rs",
        "src/resources.rs",
        "src/scorebox.rs",
        "src/step_stats.rs",
        "src/step_stats_gifs.rs",
        "src/visual_styles.rs",
    ] {
        assert!(
            simply_love.join(owned).is_file(),
            "Simply Love resource module is missing: {owned}"
        );
    }
    let simply_love_manifest = fs::read_to_string(simply_love.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    for dependency in ["deadsync-assets", "deadsync-notefield", "deadsync-theme"] {
        assert!(
            simply_love_manifest.contains(dependency),
            "Simply Love must consume {dependency}"
        );
    }
}

#[test]
fn concrete_theme_facade_does_not_reexport_runtime_crates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/lib.rs"))
        .expect("Simply Love facade should be readable");

    for public_facade in [
        "pub use deadsync_profile_gameplay",
        "pub mod config",
        "pub use deadlib_render as render",
    ] {
        assert!(
            !lib.contains(public_facade),
            "Simply Love must not expose runtime compatibility facade {public_facade}"
        );
    }
}

#[test]
fn simply_love_notefield_uses_canonical_composition_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");

    for token in [
        "fn scroll_travel",
        "fn field_layout",
        "fn compose_measure_lines",
        "while iters < 2000",
        "while iterations < 2000",
        "Walk backward from current beat",
        "CUE_SCROLL",
        "fn append_group_lines",
        "fn append_group_cues",
        "fn measure_groups",
        "fn candidate_for_unit",
        "active_column_cue",
        "column_flash_duration",
        "COLUMN_CUE_BASE_ALPHA",
        "Z_COLUMN_CUE",
        "Z_COLUMN_FLASH",
        "column_cue_reverse_top_y",
        "column_flash_reverse_top_y",
        "stream_segment_index_exclusive_end",
        "zmod_measure_counter_text(",
        "zmod_broken_run_counter_text",
        "zmod_broken_run_segment",
        "zmod_run_timer_index",
        "fn visual_effect_params",
        "fn compose_column_feedback",
        "fn compose_receptor_actors",
        "fn compose_counter_hud",
        "fn compose_mini_indicator",
        "fn compose_judgment_feedback",
        "fn compose_combo_feedback",
        "fn compose_note",
        "fn compose_hold",
        "fn visible_note_window",
        "fn hold_mesh",
        "fn measure_line_step",
        "deadlib_platform",
        "host_time",
        "std::time::Instant",
        "Instant::now",
        "fn arrow_effect_game_time_seconds",
        "let player_idx = if state.num_players() == 1",
        "let measure_line_extra",
        "let actor_cap",
        "let hud_cap",
        "let indicator_beat_push",
        "HOLD_JUDGMENT_INITIAL_ZOOM",
        "SPLIT_15_10MS_OVERLAY_ALPHA",
        "HELD_MISS_Y_OFFSET_FROM_CENTER",
        "let linear_index",
        "ComboMilestoneKind::Hundred",
        "COMBO_HUNDRED_MILESTONE_DURATION",
        "COMBO_THOUSAND_MILESTONE_DURATION",
        "SHOW_COMBO_AT",
        "let combo_zoom_mod",
        "let mut styles = [profile_data::ErrorBarStyle",
        "ErrorBarStyle::Monochrome =>",
        "let line_alpha",
        "OFFSET_INDICATOR_DUR_S",
        "ERROR_BAR_LONG_AVG_TICK_RGBA",
        "ERROR_BAR_TEXT_EARLY_RGBA",
        "let mut offset_y = screen_center_y()",
        "if show_error_bar_text && let Some(text)",
    ] {
        assert!(
            !source.contains(token),
            "Simply Love reintroduced canonical notefield algorithm token {token}"
        );
    }
    assert!(
        source.contains("arrow_effect_time_s: f32"),
        "Simply Love notefield adapter must receive arrow-effect time explicitly"
    );

    assert!(
        source.contains("NotefieldComposeRequest {")
            && (source.contains("prepare_notefield(&request)")
                || source.contains("compose_notefield(")),
        "Simply Love must enter the canonical notefield request boundary"
    );

    for (path, definition) in [
        (
            "crates/deadsync-notefield/src/compose.rs",
            "pub struct NotefieldComposeRequest",
        ),
        (
            "crates/deadsync-notefield/src/compose.rs",
            "pub fn prepare_notefield",
        ),
    ] {
        let owner =
            fs::read_to_string(root.join(path)).expect("canonical owner should be readable");
        assert!(
            owner.contains(definition),
            "canonical notefield owner {path} is missing {definition}"
        );
    }

    let compose = fs::read_to_string(root.join("crates/deadsync-notefield/src/compose.rs"))
        .expect("canonical notefield request should be readable");
    for field in [
        "pub placement: FieldPlacement",
        "pub view: ViewOverride",
        "pub geometry: NotefieldGeometry",
        "pub visual: NotefieldVisualState",
        "pub chart: NotefieldChartView",
        "pub notes: &'a [Note]",
        "pub noteskin: NotefieldNoteskinView",
        "pub song_lua: NotefieldSongLuaView",
        "pub options: NotefieldOptions",
        "pub arrow_effect_time_s: f32",
        "pub hold_explosion_enabled: bool",
        "pub error_bar_modes: ErrorBarModes",
        "pub measure_counter: Option<MeasureCounterOptions>",
        "pub target_arrow_pixel_size: f32",
    ] {
        assert!(
            compose.contains(field),
            "canonical notefield request is missing {field}"
        );
    }

    let delegation = source
        .find("let Some(prepared) = prepare_notefield(&request)")
        .or_else(|| source.find("compose_notefield("))
        .expect("Simply Love should cross the canonical composition boundary once");
    let actor_emission = &source[delegation..];
    for profile_field in [
        "profile.hide_targets",
        "profile.hide_combo",
        "profile.hide_combo_explosions",
        "profile.judgment_back",
        "profile.measure_cues",
        "profile.column_cues",
        "profile.crossover_cues",
        "profile.column_flash_on_miss",
        "profile.column_countdown",
        "profile.error_bar_trim",
        "profile.error_bar_multi_tick",
        "profile.short_average_error_bar_enabled",
        "profile.center_tick",
        "profile.error_ms_display",
        "profile.long_error_bar_enabled",
        "profile.long_error_bar_intensity",
        "profile.measure_counter_lookahead",
        "profile.measure_counter_vert",
        "profile.measure_counter_left",
        "profile.broken_run",
        "profile.run_timer",
        "profile.mini_indicator_position",
        "profile.mini_indicator_size",
    ] {
        assert!(
            !actor_emission.contains(profile_field),
            "Simply Love actor emission bypasses NotefieldOptions via {profile_field}"
        );
    }
    assert!(source.contains("target_arrow_pixel_size: TARGET_ARROW_PIXEL_SIZE"));
    for concrete in [
        "deadsync_profile",
        "deadsync_assets",
        "deadlib_platform",
        "Instant::now",
        "winit",
    ] {
        assert!(
            !compose.contains(concrete),
            "canonical notefield request imports concrete/runtime token {concrete}"
        );
    }

    let theme_contract = fs::read_to_string(root.join("crates/deadsync-theme/src/lib.rs"))
        .expect("theme contract should be readable");
    assert!(theme_contract.contains("pub struct ComboFeedbackStyle"));
    let simply_love_style =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/notefield_style.rs"))
            .expect("Simply Love notefield style should be readable");
    assert!(simply_love_style.contains("combo_feedback: ComboFeedbackStyle"));

    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(shell.contains("fn arrow_effect_time_seconds(at: Instant) -> f32"));
    assert!(shell.contains("host_time::instant_nanos(at)"));
}

#[test]
fn simply_love_note_glow_uses_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/notes.rs"))
        .expect("canonical note composer should be readable");

    for definition in ["pub struct NoteGlowRequest", "pub fn compose_note_glow"] {
        assert!(
            canonical.contains(definition),
            "canonical note-glow owner is missing {definition}"
        );
    }
    for delegation in ["NoteGlowRequest {", "compose_note_glow("] {
        assert!(
            theme.contains(delegation),
            "Simply Love must delegate note-glow composition through {delegation}"
        );
    }
    for old_definition in ["struct NoteGlowDraw", "fn push_note_glow_actor"] {
        assert!(
            !theme.contains(old_definition),
            "Simply Love reintroduced canonical note-glow definition {old_definition}"
        );
    }
    for concrete in ["deadsync_assets", "TextureKeyHandle", "texture_key_handle"] {
        assert!(
            !canonical.contains(concrete),
            "canonical note-glow composition imports concrete asset token {concrete}"
        );
    }
}

#[test]
fn simply_love_song_lua_player_transforms_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/screens/gameplay.rs"))
            .expect("Simply Love gameplay bridge should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/song_lua.rs"))
        .expect("canonical Song Lua notefield transforms should be readable");

    for definition in [
        "pub fn song_lua_player_skew_x_matrix",
        "pub fn song_lua_player_skew_y_matrix",
        "fn song_lua_fold_x_around_pivot",
        "pub fn song_lua_player_y_fold_actor",
        "pub fn song_lua_player_transform_matrix",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical Song Lua notefield owner is missing {definition}"
        );
    }

    for forbidden_definition in [
        "fn song_lua_player_skew_x_matrix",
        "fn song_lua_player_skew_y_matrix",
        "fn song_lua_fold_x_around_pivot",
        "fn song_lua_player_y_fold_actor",
        "fn song_lua_player_transform_matrix",
    ] {
        assert!(
            !theme.contains(forbidden_definition),
            "Simply Love reintroduced canonical Song Lua transform {forbidden_definition}"
        );
    }

    for delegation in [
        "song_lua_player_skew_x_matrix(skew_x)",
        "song_lua_player_skew_y_matrix(skew_y)",
        "song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg)",
        "song_lua_player_transform_matrix(SongLuaPlayerTransformRequest",
    ] {
        assert!(
            theme.contains(delegation),
            "Simply Love must delegate Song Lua notefield transforms through {delegation}"
        );
    }

    for hidden_global in ["screen_width()", "screen_height()", "screen_center_y()"] {
        assert!(
            !canonical.contains(hidden_global),
            "canonical Song Lua transforms must receive metrics instead of reading {hidden_global}"
        );
    }
    for forbidden_asset_type in ["SpriteSlot", "ModelMeshCache"] {
        assert!(
            !canonical.contains(forbidden_asset_type),
            "canonical Song Lua transforms must stay independent of {forbidden_asset_type}"
        );
    }
}

#[test]
fn canonical_notefield_owns_hold_entry_planning() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/holds.rs"))
        .expect("canonical hold planner should be readable");

    for definition in [
        "pub struct HoldEntryPlanRequest",
        "pub struct HoldEntryPlan",
        "pub fn hold_entry_head_beat",
        "pub fn hold_entry_plan",
        "fn preferred_hold_visual",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical hold owner is missing {definition}"
        );
    }
    for forbidden in [
        "deadsync_assets",
        "SpriteSlot",
        "ModelMeshCache",
        "noteskin_model_actor_from_draw",
    ] {
        assert!(
            !canonical.contains(forbidden),
            "canonical hold planner must not own asset/model adapter token {forbidden}"
        );
    }

    for old_algorithm in [
        "let mut hold_start_y = if lane_reverse",
        "let mut hold_end_y = if lane_reverse",
        "let hold_color_scale = let_go_gray",
        "std::mem::swap(&mut top_cap_slot",
        "let head_layers = if use_active",
        "let head_slot = if head_layers.is_none()",
        "let Some(body_slot) = if use_active",
    ] {
        assert!(
            !theme.contains(old_algorithm),
            "Simply Love reintroduced canonical hold planning token {old_algorithm}"
        );
    }
}

#[test]
fn noteskin_model_cache_and_actors_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let canonical_path = root.join("crates/deadsync-notefield/src/noteskin_model.rs");
    let canonical = fs::read_to_string(&canonical_path)
        .expect("canonical noteskin model owner should be readable");
    let notefield_lib = fs::read_to_string(root.join("crates/deadsync-notefield/src/lib.rs"))
        .expect("notefield exports should be readable");
    let notefield_manifest = fs::read_to_string(root.join("crates/deadsync-notefield/Cargo.toml"))
        .expect("notefield manifest should be readable");
    let old_theme_path = root
        .join("crates/deadsync-theme-simply-love/src/screens/components/shared/noteskin_model.rs");
    let shared_mod = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/components/shared/mod.rs"),
    )
    .expect("Simply Love shared module should be readable");

    assert!(canonical_path.is_file());
    assert!(!old_theme_path.exists());
    assert!(!shared_mod.contains("noteskin_model"));
    assert!(notefield_lib.contains("mod noteskin_model;"));
    assert!(notefield_lib.contains("pub use noteskin_model::*;"));
    assert!(notefield_manifest.contains("twox-hash = \"2.1.2\""));
    assert!(!notefield_manifest.contains("deadsync-assets"));

    for definition in [
        "pub struct ModelMeshCacheStats",
        "pub struct ModelMeshCache",
        "pub fn noteskin_model_actor_from_draw",
        "pub fn noteskin_model_actor_from_draw_cached",
        "pub fn noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry",
        "pub fn noteskin_model_actor",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical noteskin model owner is missing {definition}"
        );
    }
    for forbidden in ["deadsync_assets", "SpriteSlot", "texture_key_handle()"] {
        assert!(
            !canonical.contains(forbidden),
            "canonical noteskin model owner imports concrete asset token {forbidden}"
        );
    }

    let call_sites = [
        (
            "crates/deadsync-theme-simply-love/src/screens/components/evaluation/pane_column.rs",
            "noteskin_model_actor",
        ),
        (
            "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
            "noteskin_model_actor_from_draw_cached",
        ),
        (
            "crates/deadsync-theme-simply-love/src/screens/components/shared/profile_boxes.rs",
            "noteskin_model_actor",
        ),
        (
            "crates/deadsync-theme-simply-love/src/screens/components/shared/technique_bg.rs",
            "noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry",
        ),
        (
            "crates/deadsync-theme-simply-love/src/screens/gameplay.rs",
            "noteskin_model_actor_from_draw",
        ),
        (
            "crates/deadsync-theme-simply-love/src/screens/player_options/mod.rs",
            "noteskin_model_actor",
        ),
    ];
    for (path, symbol) in call_sites {
        let source = fs::read_to_string(root.join(path)).expect("call site should be readable");
        assert!(source.contains("deadsync_notefield"));
        assert!(
            source.contains(symbol),
            "{path} must import canonical model API {symbol}"
        );
    }

    let forbidden_definitions = [
        "struct ModelMeshCacheKey",
        "fn model_draw_transform",
        "fn model_affine_transform",
        "fn sm_rotation_xyz",
        "fn actor_from_vertices",
        "fn noteskin_model_actor",
    ];
    let theme_src = root.join("crates/deadsync-theme-simply-love/src");
    for file in rust_files(&theme_src) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        for definition in forbidden_definitions {
            assert!(
                !source.contains(definition),
                "{} redefines canonical noteskin model token {definition}",
                rel_path(&root, &file)
            );
        }
    }
}

#[test]
fn noteskin_slot_contract_stays_renderer_neutral_and_asset_backed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract = fs::read_to_string(root.join("crates/deadsync-noteskin/src/sprite.rs"))
        .expect("noteskin slot contract should be readable");
    let assets = fs::read_to_string(root.join("crates/deadsync-assets/src/noteskin/texture.rs"))
        .expect("asset-backed noteskin slots should be readable");
    let canonical =
        fs::read_to_string(root.join("crates/deadsync-notefield/src/noteskin_model.rs"))
            .expect("canonical noteskin model owner should be readable");

    for definition in [
        "pub trait NoteskinSlot: Sized",
        "fn sprite_def(&self)",
        "fn source_size(&self)",
        "fn size(&self)",
        "fn logical_size(&self)",
        "fn texture_key_shared(&self)",
        "fn model(&self)",
        "fn frame_index(&self",
        "fn frame_index_from_phase(&self",
        "fn uv_for_frame_at(&self",
        "fn model_draw_at(&self",
        "fn model_glow_with_draw(",
        "fn model_uv_params(&self",
        "pub fn model_vertex_for_sprite",
    ] {
        assert!(
            contract.contains(definition),
            "renderer-neutral noteskin contract is missing {definition}"
        );
    }

    for implementation in [
        "impl NoteskinSlot for SpriteSlot",
        "SpriteSlot::texture_key_shared(self)",
        "SpriteSlot::model_draw_at(self, time, beat)",
        "pub fn texture_key_handle(&self)",
        "model_vertex_for_sprite(&slot.def, vertex)",
    ] {
        assert!(
            assets.contains(implementation),
            "asset-backed noteskin slot lost {implementation}"
        );
    }
    assert!(canonical.contains("S: NoteskinSlot"));
    assert!(canonical.contains("model_vertex_for_sprite(slot.sprite_def(), vertex)"));
}

#[test]
fn receptor_composition_stays_canonical_and_theme_styled() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/receptors.rs"))
        .expect("canonical receptor composition should be readable");
    let song_lua = fs::read_to_string(root.join("crates/deadsync-notefield/src/song_lua.rs"))
        .expect("canonical song Lua presentation should be readable");
    let contract = fs::read_to_string(root.join("crates/deadsync-theme/src/lib.rs"))
        .expect("generic theme contract should be readable");
    let theme_style =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/notefield_style.rs"))
            .expect("Simply Love notefield style should be readable");
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let manifest = fs::read_to_string(root.join("crates/deadsync-notefield/Cargo.toml"))
        .expect("canonical notefield manifest should be readable");

    for token in [
        "pub struct ReceptorActorsRequest",
        "pub struct ReceptorPress",
        "pub fn compose_receptor_actors",
        "S: NoteskinSlot",
        "F: Fn(&S) -> SpriteSource",
        "P: FnOnce() -> Option<ReceptorPress",
        "request.style.target_z",
        "request.style.press_glow_z",
        "request.style.hold_explosion_z",
    ] {
        assert!(
            canonical.contains(token),
            "canonical receptor owner is missing {token}"
        );
    }
    for forbidden in [
        "deadsync_assets",
        "SpriteSlot",
        "texture_key_handle",
        "deadsync_theme_simply_love",
        "deadsync_shell",
    ] {
        assert!(
            !canonical.contains(forbidden),
            "canonical receptor owner imports concrete/runtime token {forbidden}"
        );
    }
    assert!(!manifest.contains("deadsync-theme-simply-love"));
    assert!(song_lua.contains("pub fn song_lua_note_model_draw"));
    assert!(!theme.contains("fn song_lua_note_model_draw"));

    assert!(contract.contains("pub struct ReceptorStyle"));
    assert!(contract.contains("pub receptor: ReceptorStyle"));
    assert!(!contract.contains("SimplyLoveNotefieldStyle"));
    for value in [
        "target_z: 100",
        "press_glow_z: 105",
        "hold_explosion_z: 145",
    ] {
        assert!(
            theme_style.contains(value),
            "Simply Love receptor style lost {value}"
        );
    }

    let start = theme
        .find("// Receptors + glow")
        .expect("theme should retain the receptor adapter boundary");
    let end = theme[start..]
        .find("// Tap explosions")
        .map(|offset| start + offset)
        .expect("theme should retain tap explosion composition after receptors");
    let adapter = &theme[start..end];
    for token in [
        "compose_receptor_actors(",
        "ReceptorActorsRequest {",
        "ReceptorPress {",
        "slot.texture_key_handle().into_sprite_source()",
        "style: style.receptor",
    ] {
        assert!(
            adapter.contains(token),
            "Simply Love receptor adapter is missing {token}"
        );
    }
    assert!(
        !adapter.contains("actors.push"),
        "Simply Love reintroduced canonical receptor actor composition"
    );
}

fn count_download_protocol_game_facade_refs(text: &str, symbol: &str) -> usize {
    let module_alias = text.contains("use crate::game::online::downloads;")
        || text.contains("use deadsync::game::online::downloads;")
        || text.contains("use crate::game::online::downloads as downloads;")
        || text.contains("use deadsync::game::online::downloads as downloads;");

    count_token_refs(text, &format!("crate::game::online::downloads::{symbol}"))
        + count_token_refs(
            text,
            &format!("deadsync::game::online::downloads::{symbol}"),
        )
        + count_grouped_game_rule_uses(text, "use crate::game::online::downloads::{", symbol)
        + count_grouped_game_rule_uses(text, "use deadsync::game::online::downloads::{", symbol)
        + if module_alias {
            count_token_refs(text, &format!("downloads::{symbol}"))
        } else {
            0
        }
}

#[test]
fn shell_app_has_no_move_compatibility_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shell = root.join("crates/deadsync-shell/src");
    let lib = fs::read_to_string(shell.join("lib.rs")).expect("shell facade should be readable");
    for token in [
        "extern crate self as deadsync_shell",
        "pub use deadlib_render as render",
        "pub(crate) use deadsync_profile_gameplay",
        "pub(crate) use deadsync_theme_simply_love",
        "pub(crate) mod config",
        "mod act_macro",
        "use act_macro::act",
    ] {
        assert!(
            !lib.contains(token),
            "deadsync-shell still carries app-move compatibility token {token}"
        );
    }

    let forbidden_app_tokens = [
        "deadsync_shell::",
        "crate::assets::",
        "crate::config::",
        "crate::screens::",
        "use crate::act;",
        "act!",
    ];
    let mut failures = Vec::new();
    for file in rust_files(&shell.join("app")) {
        let text = fs::read_to_string(&file).expect("shell app source should be readable");
        for token in forbidden_app_tokens {
            if text.contains(token) {
                failures.push(format!("{}: {token}", rel_path(&root, &file)));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "shell app must import owning crates and modules directly:\n{}",
        failures.join("\n")
    );
}

#[test]
fn root_screen_tree_is_removed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !root.join("src/screens").exists(),
        "Simply Love screens must remain owned by deadsync-theme-simply-love"
    );
}

#[test]
fn theme_screen_contract_has_explicit_owners() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let generic_screen = fs::read_to_string(root.join("crates/deadsync-theme/src/screen.rs"))
        .expect("generic screen contract should be readable");
    let generic_effect = fs::read_to_string(root.join("crates/deadsync-theme/src/effect.rs"))
        .expect("generic effect contract should be readable");
    let simply_love_flow =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/screens/flow.rs"))
            .expect("Simply Love flow should be readable");
    let shell_flow = fs::read_to_string(root.join("crates/deadsync-shell/src/screen_flow.rs"))
        .expect("shell effect router should be readable");

    for token in ["pub struct ThemeScreenId", "pub trait Theme"] {
        assert!(
            generic_screen.contains(token),
            "deadsync-theme is missing generic screen contract token {token}"
        );
    }
    for token in ["pub enum ThemeEffect", "pub enum ThemeFlowEvent"] {
        assert!(
            generic_effect.contains(token),
            "deadsync-theme is missing generic effect contract token {token}"
        );
    }
    for token in ["pub enum SimplyLoveScreen", "pub fn resolve_navigation"] {
        assert!(
            simply_love_flow.contains(token),
            "Simply Love is missing concrete flow token {token}"
        );
    }
    assert!(
        shell_flow.contains("pub enum ThemeEffectExecution"),
        "deadsync-shell must own ThemeEffect execution"
    );
}

#[test]
fn theme_owned_screen_architecture_has_no_contract_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let removed_crate_name = ["deadsync", "screens"].join("-");
    assert!(
        !root.join("crates").join(&removed_crate_name).exists(),
        "the retired screen contract crate must stay absent; screens and redirects are theme-owned"
    );

    let mut files = rust_files(&root.join("crates"));
    files.extend(files_named(&root.join("crates"), "Cargo.toml"));
    files.extend(rust_files(&root.join("src")));
    files.extend(rust_files(&root.join("tests")));
    files.push(root.join("Cargo.toml"));
    files.push(root.join("Cargo.lock"));
    files.push(root.join("build.rs"));
    files.sort();
    files.dedup();

    let removed_tokens = [
        ["deadsync", "screens"].join("-"),
        ["deadsync", "screens"].join("_"),
    ];
    let boundary_test = "tests/architecture_boundaries.rs";
    let mut failures = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("workspace source should be readable");
        let rel = rel_path(&root, &file);
        for token in &removed_tokens {
            let count = if rel == boundary_test {
                count_outside_notefield_forbidden_tokens(&text, token)
            } else {
                text.match_indices(token).count()
            };
            if count != 0 {
                failures.push(format!(
                    "{rel} references removed crate token {token} {count} times"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "workspace source still references the removed screen contract crate:\n{}",
        failures.join("\n")
    );
}

#[test]
fn root_app_tree_is_removed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !root.join("src/app").exists(),
        "process runtime must remain owned by deadsync-shell"
    );
}

#[test]
fn root_source_is_binary_only() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src");
    let files: Vec<_> = rust_files(&src)
        .into_iter()
        .map(|path| rel_path(&root, &path))
        .collect();
    assert_eq!(
        files,
        ["src/main.rs"],
        "the root package should remain a binary-only entry point"
    );
}
