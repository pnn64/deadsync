use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const GAME_UPWARD_DEP_BASELINE: &[(&str, &str, usize)] = &[
    ("src/game/course.rs", "config", 1),
    ("src/game/gameplay.rs", "assets", 1),
    ("src/game/gameplay.rs", "config", 8),
    ("src/game/gameplay.rs", "engine", 3),
    ("src/game/gameplay.rs", "screens", 3),
    ("src/game/gameplay/attacks.rs", "config", 2),
    ("src/game/gameplay/attacks.rs", "engine", 1),
    ("src/game/gameplay/clock.rs", "engine", 1),
    ("src/game/gameplay/input.rs", "config", 1),
    ("src/game/gameplay/input.rs", "engine", 1),
    ("src/game/online/arrowcloud.rs", "config", 1),
    ("src/game/online/downloads.rs", "config", 2),
    ("src/game/online/groovestats.rs", "config", 2),
    ("src/game/parsing/noteskin/compile.rs", "config", 1),
    ("src/game/parsing/noteskin/mod.rs", "assets", 1),
    ("src/game/parsing/noteskin/mod.rs", "config", 1),
    ("src/game/parsing/simfile.rs", "config", 3),
    ("src/game/parsing/simfile.rs", "engine", 1),
    ("src/game/parsing/simfile/cache.rs", "config", 1),
    ("src/game/parsing/simfile/scan.rs", "config", 3),
    ("src/game/parsing/song_lua/actor_host.rs", "assets", 3),
    ("src/game/profile.rs", "config", 1),
    ("src/game/random_movies.rs", "config", 1),
    ("src/game/scores.rs", "config", 4),
    ("src/game/scores/arrowcloud.rs", "config", 2),
    ("src/game/scores/groovestats.rs", "config", 6),
    ("src/game/scores/itl.rs", "config", 2),
    ("src/game/song.rs", "config", 9),
];

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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const INPUT_FSR_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const ENGINE_VIDEO_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/assets",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/assets",
    "src/config",
    "src/engine/present",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const RENDER_BACKEND_IMPORT_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/assets",
    "src/config",
    "src/engine/present",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
    "crates/deadsync-present/src",
];

const RENDER_BACKEND_IMPORTS: &[&str] = &[
    "deadsync_render_backend_gl",
    "deadsync_render_backend_software",
    "deadsync_render_backend_vulkan",
    "deadsync_render_backend_wgpu",
];

const LIGHTS_IMPORT_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const SMX_IMPORT_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/engine",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const ENGINE_PRESENT_EXTRACTED_FILES: &[&str] = &[
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "deadsync_platform",
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
    "deadsync_platform",
    "deadsync_present",
    "deadsync_render",
    "std::sync::mpsc",
    "Mutex",
    "log::",
];

const AUDIO_ANALYSIS_FORBIDDEN_TOKENS: &[&str] = &[
    "crate::engine",
    "crate::assets",
    "crate::config",
    "crate::game",
    "crate::screens",
    "deadsync_platform",
    "deadsync_present",
    "deadsync_render",
    "std::sync::mpsc",
    "Mutex",
    "log::",
];

const ENGINE_PLATFORM_SCAN_DIRS: &[&str] = &[
    "src/engine",
    "src/app",
    "src/assets",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const GAME_RULE_FACADE_MODULES: &[&str] = &["judgment", "note", "scroll", "timing"];

const GAME_RULE_FACADE_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "DEFAULT_PROFILE_ID",
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
    "LOCAL_PROFILE_MAX_ID",
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
    "next_local_profile_id",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const GAME_PARSING_NOTES_FACADE_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const GAMEPLAY_LIMIT_SYMBOLS: &[&str] = &["MAX_COLS", "MAX_PLAYERS"];

const GAMEPLAY_LIMIT_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const CORE_NOTE_SYMBOLS: &[&str] = &["NoteType"];

const CORE_NOTE_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-chart",
    "crates/deadsync-simfile",
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
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
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const NET_TRANSPORT_ERROR_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const NET_RESPONSE_BODY_SCAN_DIRS: &[&str] = &[
    "crates/deadsync-online",
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const GAME_TRANSPORT_CRATES: &[&str] = &["deadsync_net", "tungstenite", "ureq::"];

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
            "VecDeque<MusicMapSeg>",
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
            "struct AudioThreadLaunch",
            "struct NativeBackendLaunch",
            "struct OutputDeviceProbe",
            "WasapiAccessMode",
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
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    let engine_resample = root.join("src/engine/audio/resample.rs");
    if let Ok(text) = fs::read_to_string(&engine_resample) {
        for token in [
            "struct PlanarAccum",
            "fn resampler_params",
            "fn write_resampler_output",
            "fn write_channel_mapped_i16",
            "fn drop_front_samples",
            "fn apply_fade_envelope",
            "fn volume_for_frame",
            "fn sat_i64",
            "PLANAR_COMPACT_THRESHOLD_FRAMES",
        ] {
            let count = count_token_refs(&text, token);
            if count != 0 {
                failures.push(format!(
                    "{} still defines decode helper token {token} {count} times",
                    rel_path(&root, &engine_resample)
                ));
            }
        }
    }

    let engine_folder = root.join("src/engine/audio/folder.rs");
    if let Ok(text) = fs::read_to_string(&engine_folder) {
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
                    rel_path(&root, &engine_folder)
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

    assert!(
        failures.is_empty(),
        "audio decode helpers should live in deadsync-audio-decode:\n{}",
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
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    let engine_replaygain = root.join("src/engine/audio/replaygain.rs");
    if let Ok(text) = fs::read_to_string(&engine_replaygain) {
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
                    rel_path(&root, &engine_replaygain)
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

    assert!(
        failures.is_empty(),
        "ReplayGain cache data should live in deadsync-audio-analysis:\n{}",
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
    if !root.join("src/engine/smx_panels.rs").exists() {
        failures.push("src/engine/smx_panels.rs is missing".to_string());
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
    ] {
        if !file.exists() {
            failures.push(format!("{} is missing", rel_path(&root, &file)));
        }
    }

    if root.join("src/engine/smx.rs").exists() {
        failures.push("src/engine/smx.rs still exists; import deadsync_smx directly".to_string());
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
            "{} still exists; import deadsync_video directly",
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
        "video should be imported from deadsync_video:\n{}",
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
            "{} still exists; import deadsync_render::draw_prep directly",
            rel_path(&root, &draw_prep_path)
        ));
    }
    if opengl_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadsync-render-backend-gl",
            rel_path(&root, &opengl_backend_path)
        ));
    }
    if software_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadsync-render-backend-software",
            rel_path(&root, &software_backend_path)
        ));
    }
    if vulkan_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadsync-render-backend-vulkan",
            rel_path(&root, &vulkan_backend_path)
        ));
    }
    if wgpu_backend_path.exists() {
        failures.push(format!(
            "{} still exists; use deadsync-render-backend-wgpu",
            rel_path(&root, &wgpu_backend_path)
        ));
    }
    if gfx_facade_path.exists() {
        failures.push(format!(
            "{} still exists; import deadsync_renderer directly",
            rel_path(&root, &gfx_facade_path)
        ));
    }

    let engine_mod_path = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod_path) {
        let count = count_token_refs(&text, "pub mod gfx");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::gfx {count} times; import deadsync_renderer directly",
                rel_path(&root, &engine_mod_path)
            ));
        }
    }

    let renderer_src = root.join("crates/deadsync-renderer/src");
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
        "render contract should be imported from deadsync_render:\n{}",
        failures.join("\n")
    );
}

#[test]
fn present_model_lives_in_present_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in ENGINE_PRESENT_EXTRACTED_FILES {
        let path = root.join(file);
        let text = fs::read_to_string(&path).ok();
        let is_compose_facade = *file == "src/engine/present/compose.rs"
            && text.as_deref().is_some_and(|text| {
                text.contains("use crate::assets::PRESENT_TEXTURE_CONTEXT;")
                    && text.contains("use deadsync_present::compose as present_compose;")
                    && text.contains("present_compose::build_screen_with_texture_context")
            });
        let is_dsl_facade = *file == "src/engine/present/dsl.rs"
            && text.as_deref().is_some_and(|text| {
                text.contains("use deadsync_present::dsl as present_dsl;")
                    && text.contains("pub struct SpriteBuilder")
                    && text.contains("static_texture_cached")
                    && text.contains("static_texture_cached_with_texture_context")
                    && text.contains("zoomto_with_texture_context")
                    && text.contains("build_with_texture_context")
                    && text.contains("::deadsync_present::__act_from_builder!")
                    && !text.contains("SpriteSource")
                    && !text.contains("fn with_source")
                    && !text.contains("macro_rules! __ui_")
                    && !text.contains("macro_rules! __dsl_apply")
                    && !text.contains("let mut __tw")
                    && !text.contains("__dsl_parse_effect_clock")
            });
        if path.exists() && !is_compose_facade && !is_dsl_facade {
            failures.push(format!(
                "{} still exists; use deadsync-present",
                rel_path(&root, &path)
            ));
        }
    }

    let engine_mod = root.join("src/engine/mod.rs");
    if let Ok(text) = fs::read_to_string(&engine_mod) {
        let count = count_token_refs(&text, "pub mod space");
        if count != 0 {
            failures.push(format!(
                "{} declares engine::space {count} times; import deadsync_present::space directly",
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

    let assets_mod = root.join("src/assets/mod.rs");
    if let Ok(text) = fs::read_to_string(&assets_mod) {
        if !text.contains("impl present_texture::TextureContext for PresentTextureContext")
            || !text.contains("pub(crate) const PRESENT_TEXTURE_CONTEXT")
        {
            failures.push(format!(
                "{} should own the asset-backed presentation texture context",
                rel_path(&root, &assets_mod)
            ));
        }
    }

    let root_lib = root.join("src/lib.rs");
    if let Ok(text) = fs::read_to_string(&root_lib) {
        if !text.contains("pub use deadsync_present::{rgba, rgba_const};") {
            failures.push(format!(
                "{} should re-export presentation color macros from deadsync-present",
                rel_path(&root, &root_lib)
            ));
        }
    }

    let present_mod = root.join("src/engine/present/mod.rs");
    if let Ok(text) = fs::read_to_string(&present_mod) {
        for module in [
            "actors", "anim", "cache", "color", "density", "font", "runtime", "texture",
        ] {
            if !text.contains(module) {
                failures.push(format!(
                    "{} should re-export deadsync-present::{module}",
                    rel_path(&root, &present_mod)
                ));
            }
        }
        if text.contains("macro_rules! rgba") || text.contains("macro_rules! rgba_const") {
            failures.push(format!(
                "{} should not define presentation color macro wrappers",
                rel_path(&root, &present_mod)
            ));
        }
        if text.contains("$crate::engine::present::color::rgba_hex") {
            failures.push(format!(
                "{} still routes root color macros through engine::present::color",
                rel_path(&root, &present_mod)
            ));
        }
    }

    let asset_textures = root.join("src/assets/textures.rs");
    if let Ok(text) = fs::read_to_string(&asset_textures) {
        if !text.contains("present_texture::cached_texture_key_handle") {
            failures.push(format!(
                "{} should delegate TextureChoice handle caching to deadsync-present",
                rel_path(&root, &asset_textures)
            ));
        }
    }

    let present_crate = root.join("crates/deadsync-present/src/lib.rs");
    if !present_crate.exists() {
        failures.push(format!("{} is missing", rel_path(&root, &present_crate)));
    }

    let present_dsl = root.join("crates/deadsync-present/src/dsl.rs");
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

    let present_src = root.join("crates/deadsync-present/src");
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
        "presentation model should live in deadsync-present:\n{}",
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
                "{} still exists; import deadsync_platform::{module} directly",
                rel_path(&root, &file_path)
            ));
        }
        if dir_path.exists() {
            failures.push(format!(
                "{} still exists; import deadsync_platform::{module} directly",
                rel_path(&root, &dir_path)
            ));
        }
    }
    for module in CONFIG_PLATFORM_FACADE_MODULES {
        let file_path = root.join("src").join("config").join(format!("{module}.rs"));
        let dir_path = root.join("src").join("config").join(module);
        if file_path.exists() {
            failures.push(format!(
                "{} still exists; import deadsync_platform::{module} directly",
                rel_path(&root, &file_path)
            ));
        }
        if dir_path.exists() {
            failures.push(format!(
                "{} still exists; import deadsync_platform::{module} directly",
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
        "platform helpers should be imported from deadsync_platform:\n{}",
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

    for file in rust_files(&root.join("src/app")) {
        scan_game_profile_data_file(&root, &file, &mut failures);
    }
    for file in rust_files(&root.join("src/test_support")) {
        scan_game_profile_data_file(&root, &file, &mut failures);
    }

    assert!(
        failures.is_empty(),
        "app helper and test-support profile data should be imported from deadsync_profile:\n{}",
        failures.join("\n")
    );
}

#[test]
fn screen_profile_data_imports_do_not_use_game_facade() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();

    for file in rust_files(&root.join("src/screens")) {
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
    let mut out = Vec::new();
    collect_rust_files(dir, &mut out);
    out.sort();
    out
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
        + count_grouped_game_rule_uses(text, "use crate::engine::{", "smx")
        + count_grouped_game_rule_uses(text, "use deadsync::engine::{", "smx")
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
