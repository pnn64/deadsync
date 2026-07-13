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
    "deadsync-theme-",
    "deadsync_theme_",
    "deadsync-shell",
    "deadsync_shell",
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
    "deadsync-theme-",
    "deadsync_theme_",
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
        "PrewarmReplayGain(Vec<PathBuf>)",
        "pub enum PlatformRequest",
        "pub enum RevealPathKind",
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
        "deadlib_platform",
        "BackendType",
        "PresentModePolicy",
        "app_config::DisplayMode",
    ] {
        assert!(
            !contract.contains(runtime_type),
            "generic runtime-request contract exposes runtime type {runtime_type}"
        );
    }
    for wrapper in [
        "Audio(AudioRequest)",
        "Media(SimplyLoveMediaRequest)",
        "Profile(SimplyLoveProfileRequest)",
        "Online(SimplyLoveOnlineRequest)",
        "Graphics(GraphicsRequest)",
        "Platform(PlatformRequest)",
        "Sync(SimplyLoveSyncRequest)",
        "Config(SimplyLoveConfigRequest)",
        "Hardware(SimplyLoveHardwareRequest)",
        "Debug(SimplyLoveDebugRequest)",
        "Updater(SimplyLoveUpdaterRequest)",
    ] {
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
    for category in [
        "Audio", "Media", "Profile", "Online", "Graphics", "Platform", "Sync", "Config",
        "Hardware", "Debug", "Updater",
    ] {
        assert!(
            shell.contains(&format!("SimplyLoveRuntimeRequest::{category}(")),
            "shell runtime executor is missing the {category} request category"
        );
    }
    assert!(
        shell.contains("SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path))")
            && shell.contains("deadsync_audio_stream::play_sfx(&path)")
            && shell.contains(
                "SimplyLoveRuntimeRequest::Audio(AudioRequest::PrewarmReplayGain(paths))"
            )
            && shell.contains("deadsync_audio_replaygain::prewarm_paths(")
            && shell.contains("SimplyLoveRuntimeRequest::Graphics(request)")
            && shell.contains("self.handle_graphics_change(request, event_loop)")
            && shell.contains("SimplyLoveRuntimeRequest::Platform(request)")
            && shell.contains("execute_platform_request(request)")
            && shell.contains("SimplyLoveRuntimeRequest::Updater(request)")
            && shell.contains("updater::execute(request)"),
        "shell must execute generic and grouped runtime requests"
    );
}

#[test]
fn replaygain_prewarm_execution_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let select_music = fs::read_to_string(theme.join("src/screens/select_music.rs"))
        .expect("SelectMusic source should be readable");
    let theme_manifest =
        fs::read_to_string(theme.join("Cargo.toml")).expect("theme manifest should be readable");
    let shell_manifest = fs::read_to_string(root.join("crates/deadsync-shell/Cargo.toml"))
        .expect("shell manifest should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");

    assert!(
        !select_music.contains("deadsync_audio_replaygain"),
        "SelectMusic must emit a neutral prewarm request instead of executing ReplayGain"
    );
    assert!(
        !theme_manifest.contains("deadsync-audio-replaygain ="),
        "the concrete theme must not depend on the ReplayGain runtime service"
    );
    assert!(
        shell_manifest.contains("deadsync-audio-replaygain =")
            && shell.contains("AudioRequest::PrewarmReplayGain(paths)")
            && shell.contains("deadsync_audio_replaygain::Priority::Background"),
        "shell must own background ReplayGain prewarm execution"
    );
}

#[test]
fn select_music_sync_analysis_execution_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let theme_manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    let theme_sync = theme.join("src/screens/components/select_music/sync_analysis.rs");
    let pack_sync = fs::read_to_string(theme.join("src/screens/pack_sync.rs"))
        .expect("Pack Sync UI should be readable");
    let effects = fs::read_to_string(theme.join("src/effects.rs"))
        .expect("Simply Love runtime requests should be readable");
    let shell_manifest = fs::read_to_string(root.join("crates/deadsync-shell/Cargo.toml"))
        .expect("shell manifest should be readable");
    let shell_sync = fs::read_to_string(root.join("crates/deadsync-shell/src/sync_analysis.rs"))
        .expect("shell sync-analysis service should be readable");

    assert!(
        !theme_manifest.contains("deadsync-audio-decode"),
        "Simply Love must not depend on audio decode; sync analysis is shell-owned"
    );
    assert!(
        !theme_manifest.contains("null-or-die")
            && !effects.contains("null_or_die::")
            && !effects.contains("BiasStreamEvent")
            && !effects.contains("BiasEstimateWithPlot"),
        "Simply Love sync events must expose plain theme-owned DTOs"
    );
    for dto in [
        "pub enum SimplyLoveSyncStreamEvent",
        "pub struct SimplyLoveSyncSongResult",
        "pub struct SimplyLoveSyncPlotView",
    ] {
        assert!(
            effects.contains(dto),
            "Simply Love sync boundary is missing neutral DTO {dto}"
        );
    }
    assert!(
        !theme_sync.exists(),
        "the retired theme-side sync-analysis executor must be deleted"
    );
    assert!(
        !pack_sync.contains("std::thread::spawn")
            && !pack_sync.contains("deadsync_audio_decode")
            && !pack_sync.contains("analyze_song_chart_stream"),
        "Pack Sync must retain UI state without owning analysis workers or decoding"
    );
    assert!(
        effects.contains("StartAnalysis") && effects.contains("CancelAnalysis"),
        "Simply Love must express sync-analysis start and cancel as runtime intent"
    );
    assert!(
        shell_manifest.contains("deadsync-audio-decode")
            && shell_manifest.contains("null-or-die")
            && shell_sync.contains("use deadsync_audio_decode as decode")
            && shell_sync.contains("use null_or_die::")
            && shell_sync.contains("fn sync_stream_event")
            && shell_sync.contains("fn sync_song_result")
            && shell_sync.contains("fn analyze_song_chart_stream")
            && shell_sync.contains("std::thread::spawn")
            && shell_sync.contains("pub(crate) struct Service"),
        "shell must own sync-analysis decoding, execution, workers, and polling"
    );
}

#[test]
fn simply_love_options_graphics_uses_theme_graphics_contract() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let options = root.join("crates/deadsync-theme-simply-love/src/screens/options");
    let mut source = String::new();
    for file in rust_files(&options) {
        source.push_str(&fs::read_to_string(file).expect("Options source should be readable"));
    }
    for forbidden in [
        "BackendType",
        "PresentModePolicy",
        "deadlib_platform::display",
    ] {
        assert!(
            !source.contains(forbidden),
            "Simply Love Options still exposes runtime graphics type {forbidden}"
        );
    }
    for contract in [
        "GraphicsOptionsView",
        "RendererChoice",
        "DisplayModeChoice",
        "PresentPolicyChoice",
        "GraphicsRequest",
    ] {
        assert!(
            source.contains(contract),
            "Simply Love Options is missing semantic graphics contract {contract}"
        );
    }

    let views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/graphics.rs"))
        .expect("shell graphics adapter should be readable");
    assert!(
        views.contains("pub struct GraphicsOptionsView")
            && views.contains("pub struct GraphicsMonitorView")
            && shell.contains("fn runtime_backend_type")
            && shell.contains("fn runtime_present_mode_policy")
            && shell.contains("fn theme_renderer_choice")
            && shell.contains("fn theme_present_policy")
            && shell.contains("pub(super) fn options_graphics_view()"),
        "shell must map semantic graphics choices to and from runtime types"
    );
}

#[test]
fn noteskin_discovery_is_shell_prepared_for_themes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love/src");
    let generic_views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let shell_manifest = fs::read_to_string(root.join("crates/deadsync-shell/Cargo.toml"))
        .expect("shell manifest should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    let options = fs::read_to_string(theme.join("screens/options/state.rs"))
        .expect("Simply Love options state should be readable");
    let player_options = fs::read_to_string(theme.join("screens/player_options/mod.rs"))
        .expect("Simply Love player options should be readable");

    let mut failures = Vec::new();
    for file in rust_files(&theme) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        for token in ["noteskin_roots()", "itg::discover_skins"] {
            if source.contains(token) {
                failures.push(format!(
                    "{} performs shell-owned noteskin discovery via {token}",
                    rel_path(&root, &file)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "concrete themes must consume prepared noteskin names:\n{}",
        failures.join("\n")
    );
    assert!(
        generic_views.contains("pub struct NoteskinCatalogView")
            && generic_views.contains("pub names: Vec<String>"),
        "the generic theme contract must expose a renderer-neutral noteskin catalog"
    );
    assert!(
        shell_manifest.contains("deadsync-noteskin =")
            && shell.contains("fn noteskin_catalog_view() -> NoteskinCatalogView")
            && shell.contains("deadsync_noteskin::itg::discover_skins"),
        "shell must discover installed noteskins and prepare the theme view"
    );
    assert!(
        options.contains("noteskin_catalog: NoteskinCatalogView")
            && player_options.contains("noteskin_catalog: NoteskinCatalogView"),
        "Simply Love options screens must receive the prepared noteskin catalog"
    );
}

#[test]
fn simply_love_smx_assignment_uses_shell_prepared_hardware_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme_screen = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/smx_assign.rs"),
    )
    .expect("Simply Love SMX assignment screen should be readable");
    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");
    let generic_views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let shell_smx = fs::read_to_string(root.join("crates/deadsync-shell/src/smx_config.rs"))
        .expect("shell SMX service should be readable");
    let shell_app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");

    for forbidden in [
        "deadsync_smx",
        "smx::manager",
        "smx::get_info",
        "smx::set_player_lights",
        "smx::reenable_auto_lights",
        "update_smx_pad_assignment",
    ] {
        assert!(
            !theme_screen.contains(forbidden),
            "SMX assignment theme screen still executes hardware token {forbidden}"
        );
    }
    assert!(
        generic_views.contains("pub struct SmxAssignmentPadView")
            && generic_views.contains("pub struct SmxAssignmentView"),
        "generic theme views must expose prepared SMX assignment state"
    );
    for request in [
        "AssignSmxPads",
        "SetSmxPlayerLights",
        "ReenableSmxAutoLights",
    ] {
        assert!(
            effects.contains(request),
            "Simply Love hardware requests are missing {request}"
        );
        assert!(
            shell_app.contains(&format!("SimplyLoveHardwareRequest::{request}")),
            "shell runtime executor is missing {request}"
        );
    }
    assert!(
        shell_smx.contains("pub fn smx_assignment_view() -> SmxAssignmentView")
            && shell_smx.contains("deadsync_smx::manager()")
            && shell_smx.contains("deadsync_smx::get_info(slot)"),
        "shell must prepare the SMX assignment hardware snapshot"
    );
}

#[test]
fn simply_love_options_smx_assignment_uses_prepared_hardware_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let options = root.join("crates/deadsync-theme-simply-love/src/screens/options");
    let mut options_source = String::new();
    for path in [
        "input.rs",
        "mod.rs",
        "state.rs",
        "update.rs",
        "visibility.rs",
        "submenus/input_dev.rs",
    ] {
        options_source
            .push_str(&fs::read_to_string(options.join(path)).unwrap_or_else(|_| {
                panic!("Simply Love Options source {path} should be readable")
            }));
    }
    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");
    let shell_app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");

    for forbidden in [
        "deadsync_smx::get_info",
        "deadsync_smx::connected_serials",
        "deadsync_smx::conflict_warning_active",
        "update_smx_pad_assignment",
        "swap_smx_pad_assignment",
    ] {
        assert!(
            !options_source.contains(forbidden),
            "Simply Love Options still executes SMX assignment token {forbidden}"
        );
    }
    assert!(
        options_source.contains("smx_assignment: SmxAssignmentView")
            && options_source.contains("smx_assignment: &SmxAssignmentView"),
        "Simply Love Options must store and refresh shell-prepared SMX assignment state"
    );
    for request in ["AssignSmxPads", "SwapSmxPads"] {
        assert!(
            effects.contains(request) && options_source.contains(request),
            "Simply Love Options is missing hardware request {request}"
        );
        assert!(
            shell_app.contains(&format!("SimplyLoveHardwareRequest::{request}")),
            "shell runtime executor is missing Options hardware request {request}"
        );
    }
}

#[test]
fn select_music_smx_pad_profile_hardware_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let select_music = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/select_music.rs"),
    )
    .expect("Simply Love Select Music source should be readable");
    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");
    let generic_views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let shell_smx = fs::read_to_string(root.join("crates/deadsync-shell/src/smx_config.rs"))
        .expect("shell SMX service should be readable");
    let shell_app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");

    assert!(
        !select_music.contains("deadsync_smx"),
        "Simply Love Select Music must not read, capture, or write SMX hardware directly"
    );
    assert!(
        generic_views.contains("pub backend_id: String")
            && generic_views.contains("pub pad_type: Option<String>"),
        "the prepared SMX pad view must include backend-neutral profile identity"
    );
    assert!(
        select_music.contains("smx_pads: [SmxAssignmentPadView; 2]")
            && select_music.contains("smx_pad_profile_events: Vec<SmxPadProfileEvent>"),
        "Select Music must consume prepared pad identity and shell result events"
    );
    for request in [
        "ApplySmxPadPreset",
        "ApplySmxPadConfig",
        "CaptureSmxPadConfig",
    ] {
        assert!(
            effects.contains(request) && select_music.contains(request),
            "Simply Love is missing SMX pad-profile request {request}"
        );
        assert!(
            shell_app.contains(&format!("SimplyLoveHardwareRequest::{request}")),
            "shell runtime executor is missing SMX pad-profile request {request}"
        );
    }
    for owner in [
        "pub fn apply_smx_pad_preset",
        "pub fn apply_smx_saved_pad_config",
        "pub fn capture_smx_pad_config",
    ] {
        assert!(
            shell_smx.contains(owner),
            "shell SMX service is missing pad-profile owner {owner}"
        );
    }
}

#[test]
fn simply_love_has_no_direct_smx_backend_dependency() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    let generic_views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let input_contract = fs::read_to_string(root.join("crates/deadsync-input/src/lib.rs"))
        .expect("input contract should be readable");
    let input_backend = fs::read_to_string(root.join("crates/deadsync-shell/src/input_backend.rs"))
        .expect("shell input backend should be readable");
    let shell_smx = fs::read_to_string(root.join("crates/deadsync-shell/src/smx_config.rs"))
        .expect("shell SMX service should be readable");

    assert!(
        !manifest.contains("deadsync-smx"),
        "Simply Love must not depend directly on the SMX backend"
    );
    let mut failures = Vec::new();
    for file in rust_files(&theme.join("src")) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        if source.contains("deadsync_smx") {
            failures.push(rel_path(&root, &file));
        }
    }
    assert!(
        failures.is_empty(),
        "Simply Love still imports the SMX backend:\n{}",
        failures.join("\n")
    );
    assert!(
        generic_views.contains("pub struct SmxGifCatalogView")
            && generic_views.contains("pub background_packs: Vec<String>")
            && generic_views.contains("pub judgment_packs: Vec<String>"),
        "generic theme views must expose the shell-prepared SMX GIF catalog"
    );
    assert!(
        shell_smx.contains("pub fn smx_gif_catalog_view() -> SmxGifCatalogView")
            && shell_smx.contains("deadsync_smx::gifs::discover_packs"),
        "shell must discover SMX GIF packs"
    );
    assert!(
        input_contract.contains("pub fn raw_button_label")
            && input_backend
                .contains("deadsync_input::set_button_labeler(deadsync_smx::trigger_label)",),
        "shell must register SMX labels behind the backend-neutral input contract"
    );
}

#[test]
fn smx_hardware_side_effects_are_shell_owned_not_config_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_update =
        fs::read_to_string(root.join("crates/deadsync-config/src/runtime_update.rs"))
            .expect("config runtime updates should be readable");
    let shell_smx = fs::read_to_string(root.join("crates/deadsync-shell/src/smx_config.rs"))
        .expect("shell SMX service should be readable");
    let shell_app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell runtime executor should be readable");
    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");

    for forbidden in [
        "deadsync_smx::set_platform_lights_grb",
        "deadsync_smx::set_platform_lights_solid",
        "deadsync_smx::set_player_assignment",
        "deadsync_smx::connected_serials",
        "deadsync_smx::get_info",
    ] {
        assert!(
            !config_update.contains(forbidden),
            "config runtime update still executes SMX hardware token {forbidden}"
        );
        assert!(
            shell_smx.contains(forbidden),
            "shell SMX service is missing hardware token {forbidden}"
        );
    }
    for request in ["SetSmxUnderglowTheme", "SetSmxUnderglowGrb"] {
        assert!(
            effects.contains(request),
            "Simply Love hardware requests are missing {request}"
        );
        assert!(
            shell_app.contains(&format!("SimplyLoveHardwareRequest::{request}")),
            "shell runtime executor is missing {request}"
        );
    }
    for owner in [
        "pub fn apply_smx_underglow",
        "pub fn set_smx_assignment",
        "pub fn swap_smx_assignment",
    ] {
        assert!(
            shell_smx.contains(owner),
            "shell SMX service is missing runtime owner {owner}"
        );
    }
}

#[test]
fn concrete_theme_does_not_execute_updater_or_native_dialog_services() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    for dependency in [
        "deadsync-updater",
        "deadlib-video",
        "null-or-die",
        "rfd =",
        "semver =",
    ] {
        assert!(
            !manifest.contains(dependency),
            "Simply Love still owns runtime dependency {dependency}"
        );
    }

    let mut failures = Vec::new();
    for file in rust_files(&theme.join("src/screens")) {
        let source = fs::read_to_string(&file).expect("theme screen should be readable");
        for token in [
            "deadsync_updater",
            "deadlib_video",
            "rfd::FileDialog",
            "open_path::reveal",
            "std::fs::create_dir_all",
        ] {
            if source.contains(token) {
                failures.push(format!(
                    "{} executes runtime token {token}",
                    rel_path(&root, &file)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Simply Love screens must emit shell requests instead of executing services:\n{}",
        failures.join("\n")
    );

    let shell_manifest = fs::read_to_string(root.join("crates/deadsync-shell/Cargo.toml"))
        .expect("shell manifest should be readable");
    for dependency in ["deadsync-updater", "deadlib-video", "rfd ="] {
        assert!(
            shell_manifest.contains(dependency),
            "shell is missing runtime dependency {dependency}"
        );
    }
}

#[test]
fn simply_love_audio_flow_slices_use_ordered_theme_effects() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let screens = theme.join("src/screens");
    for path in [
        "arrowcloud_login.rs",
        "components/shared/profile_boxes.rs",
        "evaluation.rs",
        "groovestats_login.rs",
        "initials.rs",
        "manage_local_profiles.rs",
        "menu.rs",
        "pack_sync.rs",
        "select_color.rs",
    ] {
        let source =
            fs::read_to_string(screens.join(path)).expect("theme screen should be readable");
        assert!(
            !source.contains("deadsync_audio_stream"),
            "{path} still executes audio directly"
        );
        assert!(
            source.contains("crate::effects::sfx"),
            "{path} must emit a typed audio effect"
        );
    }

    let manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    assert!(
        !manifest.contains("deadsync-audio-stream"),
        "Simply Love must not depend on the audio execution runtime"
    );
    assert!(
        !manifest.contains("deadsync-audio ="),
        "Simply Love must consume shell-prepared audio views, not audio runtime types"
    );
    let mut direct_audio = Vec::new();
    for file in rust_files(&theme.join("src")) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        for token in [
            "deadsync_audio",
            "deadsync_audio_stream",
            "audio::play_sfx(",
            "audio::play_music(",
            "audio::stop_music(",
            "audio::set_music_rate(",
        ] {
            if source.contains(token) {
                direct_audio.push(format!("{}: {token}", rel_path(&root, &file)));
            }
        }
    }
    assert!(
        direct_audio.is_empty(),
        "Simply Love must emit audio requests instead of executing playback:\n{}",
        direct_audio.join("\n")
    );

    let generic_audio = fs::read_to_string(root.join("crates/deadsync-theme/src/runtime.rs"))
        .expect("generic theme audio requests should be readable");
    for contract in [
        "pub struct AudioCut",
        "PlaySfx(String)",
        "PlayMusic {",
        "StopMusic",
        "SetMusicRate(f32)",
        "PrewarmReplayGain(Vec<PathBuf>)",
    ] {
        assert!(
            generic_audio.contains(contract),
            "generic theme audio contract is missing {contract}"
        );
    }
    let generic_views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    for view in [
        "pub struct AudioPlaybackView",
        "pub struct AudioOutputDeviceView",
        "pub struct AudioOptionsView",
        "pub struct AudioTimingView",
    ] {
        assert!(
            generic_views.contains(view),
            "generic theme view contract is missing {view}"
        );
    }

    let select_music = fs::read_to_string(screens.join("select_music.rs"))
        .expect("SelectMusic should be readable");
    assert!(select_music.contains("pending_audio: Vec<AudioRequest>"));
    assert!(select_music.contains("AudioRequest::PlayMusic"));
    assert!(select_music.contains("AudioRequest::StopMusic"));
    assert!(select_music.contains("AudioPlaybackView"));
    let options = fs::read_to_string(screens.join("options/state.rs"))
        .expect("Options state should be readable");
    assert!(options.contains("audio_options: AudioOptionsView"));
    let practice =
        fs::read_to_string(screens.join("practice.rs")).expect("Practice should be readable");
    assert!(practice.contains("GameplayAudioCommand::SetMusicRate"));

    let gameplay_runtime =
        fs::read_to_string(root.join("crates/deadsync-shell/src/gameplay_runtime.rs"))
            .expect("shell gameplay runtime bridge should be readable");
    for execution in [
        "deadsync_audio_stream::get_music_stream_clock_snapshot()",
        "GameplayAudioCommand::PlayMusic",
        "GameplayAudioCommand::SetMusicRate(rate)",
        "deadsync_audio_stream::snap_music_start_sec",
    ] {
        assert!(
            gameplay_runtime.contains(execution),
            "shell gameplay runtime is missing {execution}"
        );
    }

    let select_color = fs::read_to_string(screens.join("select_color.rs"))
        .expect("SelectColor should be readable");
    assert!(!select_color.contains("config::update_simply_love_color"));
    assert!(select_color.contains("SimplyLoveConfigRequest::PersistColor"));

    let profile_boxes = fs::read_to_string(screens.join("components/shared/profile_boxes.rs"))
        .expect("profile boxes should be readable");
    assert!(profile_boxes.contains("p1_joined: state.p1_joined"));
    assert!(profile_boxes.contains("p2_joined: state.p2_joined"));
    assert!(profile_boxes.contains("fast_switch: state.fast_switch"));
    assert!(!profile_boxes.contains("fast_profile_switch_from_select_music"));
    for direct_session_write in [
        "set_session_player_side",
        "set_session_joined",
        "set_session_play_style",
    ] {
        assert!(
            !profile_boxes.contains(direct_session_write),
            "profile boxes still execute session mutation {direct_session_write}"
        );
    }

    let generic = fs::read_to_string(root.join("crates/deadsync-theme/src/effect.rs"))
        .expect("generic theme effects should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(generic.contains("Batch(Vec<Self>)"));
    assert!(shell.contains("ThemeEffectExecution::Batch(effects)"));
    assert!(shell.contains("execute_effect_batch(effects"));
    for execution in [
        "AudioRequest::PlaySfx(path)",
        "AudioRequest::PlayMusic {",
        "AudioRequest::StopMusic",
        "AudioRequest::SetMusicRate(rate)",
        "AudioRequest::PrewarmReplayGain(paths)",
    ] {
        assert!(
            shell.contains(execution),
            "shell audio executor is missing {execution}"
        );
    }
    assert!(shell.contains("profile_selection_session_plan("));
    assert!(shell.contains("profile::set_session_player_side(session.active_side)"));
    assert!(shell.contains("profile::set_session_joined(session.p1_joined, session.p2_joined)"));
    assert!(shell.contains("profile::set_session_play_style(session.play_style)"));
    assert!(!shell.contains("take_fast_profile_switch_from_select_music"));

    let profile_requests =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
            .expect("Simply Love requests should be readable");
    assert!(profile_requests.contains("fast_switch: bool"));

    assert!(
        select_music
            .matches("set_fast_switch(&mut overlay, true)")
            .count()
            >= 2
    );
    assert!(!select_music.contains("set_fast_profile_switch_from_select_music"));

    let profile = fs::read_to_string(root.join("crates/deadsync-profile/src/lib.rs"))
        .expect("profile crate should be readable");
    assert!(!profile.contains("fast_profile_switch_from_select_music"));
}

#[test]
fn simply_love_crossover_parity_stays_behind_simfile() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    let gameplay = fs::read_to_string(theme.join("src/screens/gameplay.rs"))
        .expect("Simply Love gameplay should be readable");
    let timing = fs::read_to_string(root.join("crates/deadsync-simfile/src/timing.rs"))
        .expect("simfile timing adapter should be readable");

    assert!(!manifest.contains("\nrssp ="));
    assert!(!gameplay.contains("rssp::"));
    assert!(gameplay.contains("deadsync_simfile::timing::crossover_annotations::<4>"));
    assert!(gameplay.contains("deadsync_simfile::timing::crossover_annotations::<8>"));
    assert!(timing.contains("pub fn crossover_annotations<const LANES: usize>"));
    assert!(timing.contains("rssp::step_parity::annotate_timing_rows"));
    assert!(!timing.contains("pub fn rssp_timing_segments_from_deadsync"));
}

#[test]
fn concrete_theme_uses_the_input_key_contract_instead_of_winit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let manifest =
        fs::read_to_string(theme.join("Cargo.toml")).expect("theme manifest should be readable");
    assert!(
        !manifest.contains("winit ="),
        "Simply Love should consume keyboard codes through deadsync-input"
    );
    assert!(
        !manifest.contains("deadsync-input-native"),
        "Simply Love should consume shell-prepared native input views"
    );

    let mut failures = Vec::new();
    for file in rust_files(&theme.join("src")) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        for token in ["winit::", "deadsync_input_native"] {
            if source.contains(token) {
                failures.push(format!("{}: {token}", rel_path(&root, &file)));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Simply Love still imports native input runtime types:\n{}",
        failures.join("\n")
    );

    let input = fs::read_to_string(root.join("crates/deadsync-input/src/lib.rs"))
        .expect("input contract should be readable");
    assert!(
        (input.contains("pub use") && input.contains("KeyCode"))
            || input.contains("pub enum KeyCode")
            || input.contains("pub struct KeyCode")
            || input.contains("pub type KeyCode"),
        "deadsync-input must expose its keyboard-code contract"
    );
    let views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("theme views should be readable");
    assert!(views.contains("pub enum GamepadSystemView"));
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/input.rs"))
        .expect("shell input owner should be readable");
    assert!(shell.contains("pub fn gamepad_system_view"));
}

#[test]
fn simply_love_test_lights_uses_shell_prepared_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love");
    let manifest = fs::read_to_string(theme.join("Cargo.toml"))
        .expect("Simply Love manifest should be readable");
    assert!(!manifest.contains("deadsync-lights"));

    let mut failures = Vec::new();
    for file in rust_files(&theme.join("src")) {
        let source = fs::read_to_string(&file).expect("theme source should be readable");
        if source.contains("deadsync_lights") {
            failures.push(rel_path(&root, &file));
        }
    }
    assert!(
        failures.is_empty(),
        "Simply Love still imports the lights runtime:\n{}",
        failures.join("\n")
    );

    let views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("theme views should be readable");
    assert!(views.contains("pub struct LightsTestView"));
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/lighting.rs"))
        .expect("shell lighting owner should be readable");
    assert!(shell.contains("pub fn lights_test_view"));
    let screen = fs::read_to_string(theme.join("src/screens/test_lights.rs"))
        .expect("test-lights screen should be readable");
    assert!(screen.contains("lights: LightsTestView"));
}

#[test]
fn select_music_unlock_availability_is_shell_prepared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let screen = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/select_music.rs"),
    )
    .expect("Select Music source should be readable");
    assert!(!screen.contains("deadsync_online::runtime::unlock_downloads_available"));
    assert!(!screen.contains("deadsync_online::runtime::take_ready_song_reload_request"));
    assert!(screen.contains("pub fn sync_runtime_view"));
    assert!(screen.contains("state.unlock_downloads_available"));

    let views = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/views.rs"))
        .expect("Simply Love views should be readable");
    assert!(views.contains("pub struct SelectMusicRuntimeView"));
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(shell.contains("fn sync_select_music_runtime_view"));
    assert!(shell.contains("deadsync_online::runtime::unlock_downloads_available()"));
    assert!(shell.contains("deadsync_online::runtime::take_ready_song_reload_request()"));
}

#[test]
fn select_music_arrow_offset_is_shell_prepared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let screen = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/select_music.rs"),
    )
    .expect("Select Music source should be readable");
    let views = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/views.rs"))
        .expect("Simply Love views should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");

    assert!(!screen.contains("crate::config::get().global_offset_seconds"));
    assert!(screen.contains("state.arrow_bounce_offset"));
    assert!(views.contains("pub arrow_bounce_offset: f32"));
    assert!(shell.contains("-10.0 * config.global_offset_seconds"));
    assert!(shell.contains("arrow_bounce_offset,"));
}

#[test]
fn select_music_feature_policy_is_shell_prepared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let screen = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/select_music.rs"),
    )
    .expect("Select Music source should be readable");
    let views = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/views.rs"))
        .expect("Simply Love views should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");

    for direct_read in [
        "config::get().only_dedicated_menu_buttons",
        "config::get().use_fsrs",
        "config::get().machine_enable_replays",
        "config::get().allow_switch_profile_in_menu",
        "config::get().keyboard_features",
    ] {
        assert!(!screen.contains(direct_read));
    }
    assert!(views.contains("pub struct SelectMusicPolicyView"));
    for field in [
        "pub dedicated_menu_only: bool",
        "pub fsr_profiles: bool",
        "pub replays: bool",
        "pub profile_switch: bool",
        "pub keyboard_features: bool",
    ] {
        assert!(views.contains(field));
    }
    for field in [
        "config.only_dedicated_menu_buttons",
        "config.use_fsrs",
        "config.machine_enable_replays",
        "config.allow_switch_profile_in_menu",
        "config.keyboard_features",
    ] {
        assert!(shell.contains(field));
    }
}

#[test]
fn options_online_reinitialization_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/options/input.rs"),
    )
    .expect("Simply Love Options input should be readable");
    assert!(!input.contains("deadsync_online::runtime::init"));
    assert_eq!(
        input
            .matches("action = Some(online_reinitialize_effect())")
            .count(),
        3
    );

    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");
    assert!(effects.contains("pub enum SimplyLoveOnlineRequest"));
    assert!(effects.contains("Reinitialize,"));
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(shell.contains("SimplyLoveOnlineRequest::Reinitialize"));
    assert!(shell.contains("deadsync_online::runtime::init()"));
}

#[test]
fn options_folder_paths_are_shell_prepared() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("crates/deadsync-theme-simply-love/src/screens/options");
    let folders = fs::read_to_string(theme.join("submenus/folders.rs"))
        .expect("Options folders source should be readable");
    let state =
        fs::read_to_string(theme.join("state.rs")).expect("Options state should be readable");
    let views = fs::read_to_string(root.join("crates/deadsync-theme/src/views.rs"))
        .expect("generic theme views should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    let reload = fs::read_to_string(theme.join("reload.rs"))
        .expect("Options reload source should be readable");

    for direct_read in ["deadlib_platform::dirs", "app_dirs()", "std::env::var_os"] {
        assert!(!folders.contains(direct_read));
    }
    assert!(folders.contains("HelpEntry::AppPath(AppPathKind::Data)"));
    assert!(folders.contains("folder_reveal_request("));
    assert!(state.contains("pub(super) app_paths: AppPathsView"));
    assert!(!reload.contains("deadlib_platform::dirs"));
    assert!(reload.contains("state.app_paths.songs.path.clone()"));
    assert!(reload.contains("state.app_paths.courses.path.clone()"));
    assert!(views.contains("pub struct AppPathView"));
    assert!(views.contains("pub struct AppPathsView"));
    assert!(shell.contains("fn app_paths_view() -> AppPathsView"));
    assert!(shell.contains("deadlib_platform::dirs::app_dirs()"));
    assert!(shell.contains("deadlib_platform::dirs::path_shorthand(&path)"));
}

#[test]
fn select_music_uses_shell_prepared_paths_and_playlists() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let select_music = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/select_music.rs"),
    )
    .expect("Select Music source should be readable");
    for forbidden in [
        "deadlib_platform::dirs",
        "std::fs",
        "fs::read_dir",
        "fs::read_to_string",
        "config::get().null_or_die_confidence_percent",
        "config::get().null_or_die_sync_graph",
    ] {
        assert!(
            !select_music.contains(forbidden),
            "Select Music still performs shell-owned filesystem work via {forbidden}"
        );
    }
    assert!(
        select_music.contains("pub fn init(init_view: SelectMusicInitView)")
            && select_music.contains("init_view.songs_root")
            && select_music.contains("init_view.courses_root")
            && select_music.contains("init_view.playlists"),
        "Select Music must initialize from a shell-prepared path and playlist view"
    );

    let views = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/views.rs"))
        .expect("Simply Love views should be readable");
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/select_music.rs"))
        .expect("shell Select Music adapter should be readable");
    let app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(
        views.contains("pub struct SelectMusicInitView")
            && views.contains("pub struct SelectMusicPlaylistView")
            && views.contains("pub sync_graph_mode:")
            && views.contains("pub sync_confidence_percent: u8")
            && shell.contains("deadlib_platform::dirs::app_dirs()")
            && shell.contains("std::fs::read_dir")
            && shell.contains("std::fs::read_to_string")
            && shell.contains("pub(crate) fn init_view() -> SelectMusicInitView")
            && app.contains("config.null_or_die_sync_graph")
            && app.contains("config.null_or_die_confidence_percent")
            && app.contains("select_music::init(crate::select_music::init_view())"),
        "shell must resolve Select Music paths and load playlist files"
    );
}

#[test]
fn arrowcloud_status_refresh_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let qr = fs::read_to_string(
        root.join("crates/deadsync-theme-simply-love/src/screens/options/qr_login.rs"),
    )
    .expect("QR login source should be readable");
    assert!(!qr.contains("deadsync_online::runtime::refresh_arrowcloud_status"));
    assert!(qr.contains("poll_qr_login_ui(ui: &mut QrLoginUiState) -> bool"));

    let effects = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/effects.rs"))
        .expect("Simply Love effects should be readable");
    assert!(effects.contains("RefreshArrowCloudStatus"));
    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(shell.contains("SimplyLoveOnlineRequest::RefreshArrowCloudStatus"));
    assert!(shell.contains("deadsync_online::runtime::refresh_arrowcloud_status()"));
}

#[test]
fn simply_love_main_menu_uses_prepared_runtime_view() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let views = fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/views.rs"))
        .expect("Simply Love views should be readable");
    for field in [
        "pub struct MainMenuRuntimeView",
        "pub allow_shutdown_host: bool",
        "pub song_count: usize",
        "pub pack_count: usize",
        "pub course_count: usize",
        "pub groovestats: MainMenuGrooveStatus",
        "pub arrowcloud: MainMenuArrowCloudStatus",
        "pub smx_conflict: Option<MainMenuSmxConflictView>",
    ] {
        assert!(views.contains(field), "main-menu view is missing {field}");
    }

    let menu =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/screens/menu.rs"))
            .expect("Simply Love menu should be readable");
    assert!(menu.contains("runtime_view: MainMenuRuntimeView"));
    assert!(menu.contains("pub fn sync_runtime_view"));
    for runtime_read in [
        "deadsync_config",
        "deadsync_simfile",
        "deadsync_online",
        "deadsync_smx",
        "get_song_cache(",
        "get_course_cache(",
        "runtime_get_status(",
    ] {
        assert!(
            !menu.contains(runtime_read),
            "Simply Love menu still reads runtime service {runtime_read}"
        );
    }

    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/main_menu.rs"))
        .expect("shell main-menu bridge should be readable");
    for runtime_read in [
        "runtime_view() -> MainMenuRuntimeView",
        "deadsync_config::prelude::get()",
        "deadsync_simfile::runtime_cache::get_song_cache()",
        "deadsync_simfile::runtime_cache::get_course_cache()",
        "deadsync_online::groovestats::runtime_get_status()",
        "deadsync_online::arrowcloud::runtime_get_status()",
        "deadsync_smx::conflict_warning_active()",
    ] {
        assert!(
            shell.contains(runtime_read),
            "shell main-menu bridge is missing {runtime_read}"
        );
    }

    let app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    assert!(app.contains("crate::main_menu::runtime_view()"));
    assert!(app.contains("menu::sync_runtime_view"));
}

#[test]
fn simply_love_gameplay_smx_execution_is_shell_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        fs::read_to_string(root.join("crates/deadsync-theme-simply-love/src/screens/gameplay.rs"))
            .expect("Simply Love gameplay should be readable");
    for contract in [
        "pub struct SmxSensorPanelView",
        "pub struct SmxSensorPadView",
        "pub fn smx_sensor_pad_plan",
        "pub fn smx_sensor_refresh_due",
        "pub fn smx_sensor_pad_view",
        "pub fn set_smx_sensor_pad_view",
    ] {
        assert!(
            theme.contains(contract),
            "Simply Love gameplay is missing sensor view contract {contract}"
        );
    }
    for runtime_type in [
        "deadsync_smx",
        "SensorTestData",
        "SmxConfig",
        "SensorTestMode",
        "get_test_data(",
        "get_config(",
        "set_test_mode(",
    ] {
        assert!(
            !theme.contains(runtime_type),
            "Simply Love gameplay still executes SMX runtime operation {runtime_type}"
        );
    }

    let shell = fs::read_to_string(root.join("crates/deadsync-shell/src/gameplay_runtime.rs"))
        .expect("shell gameplay runtime bridge should be readable");
    for execution in [
        "fn enter_smx_sensors",
        "fn refresh_smx_sensors",
        "deadsync_smx::set_test_mode",
        "SensorTestMode::CalibratedValues",
        "SensorTestMode::Off",
        "deadsync_smx::get_config",
        "deadsync_smx::get_test_data",
        "gameplay::set_smx_sensor_pad_view",
    ] {
        assert!(
            shell.contains(execution),
            "shell gameplay runtime is missing SMX execution {execution}"
        );
    }

    let app = fs::read_to_string(root.join("crates/deadsync-shell/src/app/mod.rs"))
        .expect("shell app should be readable");
    let navigation = fs::read_to_string(root.join("crates/deadsync-shell/src/app/screen_nav.rs"))
        .expect("shell navigation should be readable");
    assert!(app.contains("crate::gameplay_runtime::exit(gs)"));
    assert!(navigation.contains("crate::gameplay_runtime::exit(gs)"));
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
        "pub type GameplayCoreState",
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
    let display_mods =
        fs::read_to_string(root.join(
            "crates/deadsync-theme-simply-love/src/screens/components/gameplay/display_mods.rs",
        ))
        .expect("Simply Love DisplayMods component should be readable");
    let field_frame = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");
    let hud_frame = fs::read_to_string(root.join("crates/deadsync-notefield/src/frame_hud.rs"))
        .expect("canonical HUD-frame composer should be readable");
    let gameplay_rows = fs::read_to_string(root.join("crates/deadsync-gameplay/src/rows.rs"))
        .expect("gameplay row views should be readable");
    let gameplay_runtime =
        fs::read_to_string(root.join("crates/deadsync-gameplay/src/runtime_update.rs"))
            .expect("gameplay runtime accessors should be readable");

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
    assert!(source.contains("pub(crate) fn compose_frame("));
    assert!(
        !source.contains("act!("),
        "Simply Love's notefield adapter must assemble canonical DTOs, not actors"
    );
    assert!(display_mods.contains("pub(super) fn compose("));
    assert!(display_mods.contains("actors.push(act!("));

    assert!(
        source.contains("NotefieldComposeRequest {")
            && source.contains("prepare_notefield(&request)"),
        "Simply Love must enter the canonical notefield request boundary"
    );
    for (view, composer) in [
        ("NotefieldFieldFrameView {", "compose_notefield_field("),
        ("NotefieldHudFrameView {", "compose_notefield_hud("),
    ] {
        assert!(
            source.contains(view),
            "Simply Love must prepare the canonical frame view {view}"
        );
        assert_eq!(
            source.match_indices(composer).count(),
            1,
            "Simply Love must cross the canonical frame boundary exactly once through {composer}"
        );
    }
    for low_level in [
        "MeasureComposeRequest",
        "compose_measure_lines(",
        "HoldEntryPlanRequest",
        "hold_entry_plan(",
        "HoldBodyCapRequest",
        "compose_hold_body_caps(",
        "NoteLayerRequest",
        "compose_note_layer(",
        "MineLayerRequest",
        "compose_mine_layers(",
        "compose_notefield_feedback(",
        "ComboFeedbackRequest",
        "compose_combo_feedback(",
        "ErrorBarComposeRequest",
        "compose_error_bar(",
        "CounterHudRequest",
        "compose_counter_hud(",
        "MiniIndicatorRequest",
        "compose_mini_indicator(",
        "JudgmentFeedbackRequest",
        "compose_judgment_feedback(",
    ] {
        assert!(
            !source.contains(low_level),
            "Simply Love must not bypass a canonical frame composer through {low_level}"
        );
    }

    for definition in [
        "pub struct NotefieldFieldFrameView",
        "pub struct NotefieldFieldResult",
        "pub fn compose_notefield_field",
    ] {
        assert!(
            field_frame.contains(definition),
            "canonical field-frame owner is missing {definition}"
        );
    }
    for definition in [
        "pub struct NotefieldHudFrameView",
        "pub struct NotefieldHudComposeResult",
        "pub fn compose_notefield_hud",
    ] {
        assert!(
            hud_frame.contains(definition),
            "canonical HUD-frame owner is missing {definition}"
        );
    }

    for (path, low_level) in [
        (
            "crates/deadsync-notefield/src/measure_lines.rs",
            "pub struct MeasureComposeRequest",
        ),
        (
            "crates/deadsync-notefield/src/measure_lines.rs",
            "pub fn compose_measure_lines",
        ),
        (
            "crates/deadsync-notefield/src/holds.rs",
            "pub struct HoldBodyCapRequest",
        ),
        (
            "crates/deadsync-notefield/src/holds.rs",
            "pub fn compose_hold_body_caps",
        ),
        (
            "crates/deadsync-notefield/src/notes.rs",
            "pub struct NoteLayerRequest",
        ),
        (
            "crates/deadsync-notefield/src/notes.rs",
            "pub struct MineLayerRequest",
        ),
        (
            "crates/deadsync-notefield/src/notes.rs",
            "pub fn compose_note_layer",
        ),
        (
            "crates/deadsync-notefield/src/notes.rs",
            "pub fn compose_mine_layers",
        ),
        (
            "crates/deadsync-notefield/src/frame_feedback.rs",
            "pub fn compose_notefield_feedback",
        ),
        (
            "crates/deadsync-notefield/src/combo_feedback.rs",
            "pub struct ComboFeedbackRequest",
        ),
        (
            "crates/deadsync-notefield/src/combo_feedback.rs",
            "pub fn compose_combo_feedback",
        ),
        (
            "crates/deadsync-notefield/src/error_bar.rs",
            "pub struct ErrorBarComposeRequest",
        ),
        (
            "crates/deadsync-notefield/src/error_bar.rs",
            "pub fn compose_error_bar",
        ),
        (
            "crates/deadsync-notefield/src/hud.rs",
            "pub struct CounterHudRequest",
        ),
        (
            "crates/deadsync-notefield/src/hud.rs",
            "pub fn compose_counter_hud",
        ),
        (
            "crates/deadsync-notefield/src/hud.rs",
            "pub struct MiniIndicatorRequest",
        ),
        (
            "crates/deadsync-notefield/src/hud.rs",
            "pub fn compose_mini_indicator",
        ),
        (
            "crates/deadsync-notefield/src/judgment_feedback.rs",
            "pub struct JudgmentFeedbackRequest",
        ),
        (
            "crates/deadsync-notefield/src/judgment_feedback.rs",
            "pub fn compose_judgment_feedback",
        ),
    ] {
        let owner = fs::read_to_string(root.join(path))
            .unwrap_or_else(|_| panic!("canonical notefield source should be readable: {path}"));
        assert!(
            !owner.contains(low_level),
            "canonical low-level API must stay internal: {low_level} in {path}"
        );
    }

    let field_contents_start = field_frame
        .find("fn compose_field_contents")
        .expect("canonical field frame should define its content pass");
    let field_contents_end = field_frame[field_contents_start..]
        .find("fn compose_visible_notes")
        .map(|offset| field_contents_start + offset)
        .expect("canonical field frame should define its visible-note pass");
    let field_contents = &field_frame[field_contents_start..field_contents_end];
    let mut previous = 0;
    for marker in [
        "compose_measure_lines(",
        "compose_notefield_feedback(",
        "compose_hold_body_caps(",
        "compose_visible_notes(",
    ] {
        let position = field_contents[previous..]
            .find(marker)
            .map(|offset| previous + offset)
            .unwrap_or_else(|| panic!("canonical field sequence is missing {marker}"));
        previous = position + marker.len();
    }

    let field_entry_start = field_frame
        .find("pub fn compose_notefield_field")
        .expect("canonical field frame should expose its entry point");
    let field_entry_end = field_frame[field_entry_start..]
        .find("fn compose_field_contents")
        .map(|offset| field_entry_start + offset)
        .expect("canonical field entry point should precede its content helper");
    let field_entry = &field_frame[field_entry_start..field_entry_end];
    let mut previous = 0;
    for marker in [
        "compose_field_contents(",
        "wrap_field_camera(",
        "share_actor_range(",
    ] {
        let position = field_entry[previous..]
            .find(marker)
            .map(|offset| previous + offset)
            .unwrap_or_else(|| panic!("canonical field finalization is missing {marker}"));
        previous = position + marker.len();
    }

    let hud_entry_start = hud_frame
        .find("pub fn compose_notefield_hud")
        .expect("canonical HUD frame should expose its entry point");
    let hud_entry_end = hud_frame[hud_entry_start..]
        .find("fn compose_combo")
        .map(|offset| hud_entry_start + offset)
        .expect("canonical HUD entry point should precede its helpers");
    let hud_entry = &hud_frame[hud_entry_start..hud_entry_end];
    let mut previous = 0;
    for marker in [
        "compose_combo(",
        "share_actor_range(actors, combo_capture_start)",
        "compose_error(",
        "compose_counter_hud(",
        "compose_mini_indicator(",
        "compose_judgment(",
        "share_actor_range(actors, judgment_capture_start)",
    ] {
        let position = hud_entry[previous..]
            .find(marker)
            .map(|offset| previous + offset)
            .unwrap_or_else(|| panic!("canonical HUD sequence is missing {marker}"));
        previous = position + marker.len();
    }

    assert!(gameplay_rows.contains("pub struct CompletedRowVisibility"));
    assert!(gameplay_rows.contains("pub const fn new("));
    assert!(gameplay_rows.contains("pub fn hides_note("));
    assert!(gameplay_runtime.contains("pub fn completed_row_visibility("));
    assert!(field_frame.contains("pub completed_rows: CompletedRowVisibility"));
    assert!(field_frame.contains("completed_rows.hides_note(note.row_index)"));
    assert!(source.contains("state.completed_row_visibility(player_idx)"));
    assert!(
        !source.contains("row_hides_completed_note("),
        "Simply Love must consume the borrowed completed-row view instead of querying runtime rows"
    );

    let field_call = source
        .find("let field_result = compose_notefield_field(")
        .expect("Simply Love must compose the canonical field pass");
    let display_mods_call = source[field_call..]
        .find("display_mods::compose(")
        .map(|offset| field_call + offset)
        .expect("Simply Love must retain its concrete DisplayMods insertion point");
    let hud_call = source[display_mods_call..]
        .find("let hud_result = compose_notefield_hud(")
        .map(|offset| display_mods_call + offset)
        .expect("Simply Love must compose the canonical post-chrome HUD pass");
    assert!(field_call < display_mods_call && display_mods_call < hud_call);

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
fn simply_love_note_layers_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/notes.rs"))
        .expect("canonical note composer should be readable");
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");

    for definition in ["struct NoteLayerRequest", "fn compose_note_layer"] {
        assert!(
            canonical.contains(definition),
            "canonical note-layer owner is missing {definition}"
        );
    }
    for delegation in ["NoteLayerRequest {", "compose_note_layer("] {
        assert!(
            field.contains(delegation),
            "canonical field frame must delegate note-layer composition through {delegation}"
        );
        assert!(
            !theme.contains(delegation),
            "Simply Love must not import low-level note-layer API {delegation}"
        );
    }
    for retired_public_seam in ["pub struct NoteGlowRequest", "pub fn compose_note_glow"] {
        assert!(
            !canonical.contains(retired_public_seam),
            "canonical notefield still exports transitional seam {retired_public_seam}"
        );
    }
    for old_definition in [
        "struct NoteGlowDraw",
        "fn push_note_glow_actor",
        "compose_note_glow(",
        "noteskin_model_actor_from_draw_cached",
    ] {
        assert!(
            !theme.contains(old_definition),
            "Simply Love reintroduced canonical note-layer emission {old_definition}"
        );
    }
    for concrete in ["deadsync_assets", "TextureKeyHandle", "texture_key_handle"] {
        assert!(
            !canonical.contains(concrete),
            "canonical note-layer composition imports concrete asset token {concrete}"
        );
    }
}

#[test]
fn simply_love_mine_layers_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/notes.rs"))
        .expect("canonical note composer should be readable");
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");
    let contract = fs::read_to_string(root.join("crates/deadsync-noteskin/src/sprite.rs"))
        .expect("noteskin slot contract should be readable");
    let assets = fs::read_to_string(root.join("crates/deadsync-assets/src/noteskin/texture.rs"))
        .expect("asset-backed noteskin slot should be readable");

    for definition in ["struct MineLayerRequest", "fn compose_mine_layers"] {
        assert!(
            canonical.contains(definition),
            "canonical mine-layer owner is missing {definition}"
        );
    }
    for delegation in ["MineLayerRequest {", "compose_mine_layers("] {
        assert!(
            field.contains(delegation),
            "canonical field frame must delegate mine-layer composition through {delegation}"
        );
        assert!(
            !theme.contains(delegation),
            "Simply Love must not import low-level mine-layer API {delegation}"
        );
    }
    assert!(
        !theme.contains(".source.frame_count()"),
        "Simply Love must not inspect concrete mine sprite sources"
    );
    assert!(contract.contains("fn frame_count(&self) -> usize"));
    assert!(assets.contains("self.source.frame_count()"));
    for concrete in ["deadsync_assets", "TextureKeyHandle", "texture_key_handle"] {
        assert!(
            !canonical.contains(concrete),
            "canonical mine-layer composition imports concrete asset token {concrete}"
        );
    }
}

#[test]
fn simply_love_explosion_layers_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/explosions.rs"))
        .expect("canonical explosion composer should be readable");
    let frame = fs::read_to_string(root.join("crates/deadsync-notefield/src/frame_feedback.rs"))
        .expect("canonical feedback-frame composer should be readable");
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");
    let contract = fs::read_to_string(root.join("crates/deadsync-noteskin/src/sprite.rs"))
        .expect("noteskin slot contract should be readable");
    let assets = fs::read_to_string(root.join("crates/deadsync-assets/src/noteskin/texture.rs"))
        .expect("asset-backed noteskin slot should be readable");

    for definition in [
        "pub(crate) enum ExplosionRotation",
        "pub(crate) struct ExplosionComposeRequest",
        "pub(crate) fn compose_explosion_layers",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical explosion owner is missing {definition}"
        );
    }
    for delegation in [
        "ExplosionComposeRequest {",
        "ExplosionRotation::Tap",
        "ExplosionRotation::Mine",
        "compose_explosion_layers(",
    ] {
        assert!(
            frame.contains(delegation),
            "canonical feedback frame must delegate explosion composition through {delegation}"
        );
    }
    assert!(theme.contains("NotefieldFeedbackFrameView {"));
    assert!(theme.contains("NotefieldFieldFrameView {"));
    assert!(theme.contains("compose_notefield_field("));
    assert!(field.contains("compose_notefield_feedback("));
    assert!(!theme.contains("compose_notefield_feedback("));
    for low_level in [
        "ExplosionComposeRequest",
        "ExplosionRotation",
        "compose_explosion_layers(",
    ] {
        assert!(
            !theme.contains(low_level),
            "Simply Love still imports low-level explosion API {low_level}"
        );
    }
    for old_emission in [
        "layer.animation.state_at",
        "glow_strength",
        ".source.is_beat_based()",
    ] {
        assert!(
            !theme.contains(old_emission),
            "Simply Love reintroduced explosion actor logic {old_emission}"
        );
    }
    assert!(contract.contains("fn animation_is_beat_based(&self) -> bool"));
    assert!(assets.contains("self.source.is_beat_based()"));
    for concrete in ["deadsync_assets", "TextureKeyHandle", "texture_key_handle"] {
        assert!(
            !canonical.contains(concrete) && !frame.contains(concrete),
            "canonical explosion composition imports concrete asset token {concrete}"
        );
    }
}

#[test]
fn simply_love_hold_body_caps_use_canonical_notefield_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = fs::read_to_string(root.join(
        "crates/deadsync-theme-simply-love/src/screens/components/gameplay/notefield/mod.rs",
    ))
    .expect("Simply Love notefield adapter should be readable");
    let canonical = fs::read_to_string(root.join("crates/deadsync-notefield/src/holds.rs"))
        .expect("canonical hold composer should be readable");
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");

    for definition in [
        "struct HoldPathSample",
        "struct HoldBodyCapRequest",
        "enum HoldComposeControl",
        "fn compose_hold_body_caps",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical hold body/cap owner is missing {definition}"
        );
    }
    for delegation in [
        "HoldPathSample {",
        "HoldBodyCapRequest {",
        "compose_hold_body_caps(",
        "HoldComposeControl::AbortHold",
    ] {
        assert!(
            field.contains(delegation),
            "canonical field frame must delegate hold body/cap composition through {delegation}"
        );
        assert!(
            !theme.contains(delegation),
            "Simply Love must not import low-level hold body/cap API {delegation}"
        );
    }
    for old_emission in [
        "actors.push(act!(sprite(",
        "actors.push(hold_strip_actor(",
        "actors.push(hold_strip_glow_actor(",
        "clipped_hold_body_bounds(",
        "hold_strip_row_3d(",
        "hold_tail_cap_bounds(",
        "bottom_cap_uv_window(",
    ] {
        assert!(
            !theme.contains(old_emission),
            "Simply Love reintroduced hold actor logic {old_emission}"
        );
    }
    for concrete in ["deadsync_assets", "TextureKeyHandle", "texture_key_handle"] {
        assert!(
            !canonical.contains(concrete),
            "canonical hold composition imports concrete asset token {concrete}"
        );
    }
}

#[test]
fn canonical_notefield_crate_root_facade_is_explicit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("crates/deadsync-notefield/src/lib.rs"))
        .expect("canonical notefield crate root should be readable");
    let glob_exports = source
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            (line.starts_with("pub use ") || line.starts_with("pub(crate) use "))
                && line.contains("::*")
        })
        .collect::<Vec<_>>();

    assert!(
        glob_exports.is_empty(),
        "canonical notefield crate root must list its facade explicitly: {glob_exports:?}"
    );
    for facade in [
        "pub use actor_builder::{",
        "pub use compose::{",
        "pub use field_frame::{",
        "pub use frame_hud::{",
        "pub use measure_lines::MeasureLineMode;",
        "pub use noteskin_model::{",
        "pub use placement::{",
    ] {
        assert!(
            source.contains(facade),
            "canonical notefield crate root is missing explicit facade group {facade}"
        );
    }
    assert!(
        !source.contains("pub mod "),
        "canonical notefield implementation modules must stay private"
    );
}

#[test]
fn canonical_notefield_public_symbols_match_allowlist() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("crates/deadsync-notefield/src/lib.rs"))
        .expect("canonical notefield crate root should be readable");
    let mut statements = Vec::new();
    let mut statement = String::new();

    for line in source.lines().map(str::trim) {
        if statement.is_empty() {
            if !line.starts_with("pub use ") {
                continue;
            }
        } else {
            statement.push(' ');
        }
        statement.push_str(line);
        if line.ends_with(';') {
            statements.push(std::mem::take(&mut statement));
        }
    }
    assert!(statement.is_empty(), "unterminated public use statement");

    let mut actual = Vec::new();
    for statement in statements {
        let body = statement
            .strip_prefix("pub use ")
            .and_then(|body| body.strip_suffix(';'))
            .expect("collected statement should be a public use");
        if let Some(open) = body.find("::{") {
            let items = body[open + 3..]
                .strip_suffix('}')
                .expect("grouped public use should close its item list");
            actual.extend(
                items
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned),
            );
        } else {
            actual.push(
                body.rsplit("::")
                    .next()
                    .expect("public use should contain a symbol")
                    .trim()
                    .to_owned(),
            );
        }
    }

    let mut expected = [
        "BuiltNotefield",
        "clamp_rounded_i16",
        "ComboHudFrame",
        "ComboMilestoneAssets",
        "compose_notefield_field",
        "compose_notefield_hud",
        "CounterHudFrame",
        "DISPLAY_TURN_BLENDER",
        "DISPLAY_TURN_LEFT",
        "DISPLAY_TURN_LR_MIRROR",
        "DISPLAY_TURN_MIRROR",
        "DISPLAY_TURN_RANDOM",
        "DISPLAY_TURN_RIGHT",
        "DISPLAY_TURN_SHUFFLE",
        "DISPLAY_TURN_UD_MIRROR",
        "error_bar_boundaries_s",
        "ErrorBarHudFrame",
        "ErrorBarModes",
        "FieldLayout",
        "FieldPlacement",
        "gameplay_mods_text",
        "GameplayModsAttackMode",
        "GameplayModsTextParams",
        "HudLayoutYs",
        "IndicatorSprite",
        "JudgmentHudFrame",
        "LayoutMiniIndicatorPosition",
        "MeasureCounterOptions",
        "MeasureLineMode",
        "MiniHudFrame",
        "MiniIndicatorColorStyle",
        "MiniIndicatorMode",
        "MiniIndicatorProgress",
        "MiniIndicatorScoreType",
        "MiniIndicatorSize",
        "MiniIndicatorSubtractiveDisplay",
        "mod_percent_key",
        "ModelMeshCache",
        "ModelMeshCacheStats",
        "NotefieldChartView",
        "NotefieldComposeRequest",
        "NotefieldFeedbackFrameView",
        "NotefieldFieldFrameView",
        "NotefieldFieldResult",
        "NotefieldFrameFeatures",
        "NotefieldFramePlan",
        "NotefieldGeometry",
        "NotefieldHudComposeResult",
        "NotefieldHudFrameView",
        "NotefieldLaneFeedback",
        "NotefieldNoteskinView",
        "NotefieldOptions",
        "NotefieldSongLuaView",
        "NotefieldVisualState",
        "noteskin_model_actor",
        "noteskin_model_actor_from_draw",
        "noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry",
        "offset_center",
        "prepare_notefield",
        "PreparedNotefield",
        "PreparedNotefieldNotes",
        "ProxyCaptureRequests",
        "quantize_centi_i32",
        "quantize_centi_u32",
        "ScrollTravel",
        "song_lua_note_model_draw",
        "song_lua_player_skew_x_matrix",
        "song_lua_player_skew_y_matrix",
        "song_lua_player_transform_matrix",
        "song_lua_player_y_fold_actor",
        "SongLuaPlayerTransformRequest",
        "TapJudgmentHudFrame",
        "TapJudgmentSprite",
        "TornadoBounds",
        "ViewOverride",
        "zmod_broken_run_end",
        "zmod_combo_quint_active",
        "zmod_mini_indicator_output",
        "zmod_mini_indicator_zoom",
        "zmod_percent_from_points",
        "zmod_resolved_combo_color",
        "zmod_resolved_mini_indicator_mode",
        "zmod_static_combo_color",
        "zmod_stream_prog_completion_for_beat",
        "ZmodComboColorParams",
        "ZmodComboColorStyle",
        "ZmodLayoutParams",
        "ZmodLayoutYs",
        "ZmodMeasureCounterText",
        "ZmodMiniIndicatorOutput",
        "ZmodMiniIndicatorParams",
        "ZmodMiniIndicatorText",
    ]
    .map(str::to_owned)
    .to_vec();

    let actual_len = actual.len();
    actual.sort_unstable();
    actual.dedup();
    assert_eq!(
        actual.len(),
        actual_len,
        "canonical notefield public facade contains duplicate symbols"
    );
    expected.sort_unstable();
    assert_eq!(
        actual, expected,
        "canonical notefield public facade changed"
    );
}

#[test]
fn canonical_notefield_keeps_internal_composition_helpers_crate_private() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (file, internal) in [
        ("actor_builder.rs", "struct NotefieldFramePlanRequest"),
        ("actor_builder.rs", "fn notefield_frame_plan"),
        ("actor_builder.rs", "fn actor_with_world_z"),
        (
            "noteskin_model.rs",
            "fn noteskin_model_actor_from_draw_cached",
        ),
        ("placement.rs", "struct FieldLayoutRequest"),
        ("placement.rs", "fn field_layout"),
        ("placement.rs", "fn player_metric_y"),
        ("placement.rs", "fn notefield_view_proj"),
        ("placement.rs", "fn combo_actor_zoom"),
        ("placement.rs", "fn effective_mini_value"),
        ("placement.rs", "fn average_error_bar_mini_scale"),
        ("placement.rs", "fn hud_y"),
        ("placement.rs", "fn zmod_layout_ys"),
        ("placement.rs", "fn default_column_x"),
        ("placement.rs", "trait LaneColumnX"),
        ("placement.rs", "fn fill_lane_col_offsets"),
        ("transforms.rs", "struct NoteAlphaParams"),
        ("transforms.rs", "struct AccelYParams"),
        ("transforms.rs", "struct NoteXParams"),
        ("transforms.rs", "struct VisualEffectParams"),
        ("transforms.rs", "fn sm_scale"),
        ("transforms.rs", "fn quantize_step"),
        ("transforms.rs", "fn beat_factor"),
        ("transforms.rs", "fn mod_divisor"),
        ("transforms.rs", "fn bumpy_angle"),
        ("transforms.rs", "fn apply_accel_y_with_peak"),
        ("transforms.rs", "fn apply_accel_y"),
        ("transforms.rs", "fn itg_actor_rotation_z"),
        ("transforms.rs", "fn visual_hold_body_needs_z_buffer"),
        ("transforms.rs", "fn visual_use_legacy_hold_sprites"),
        ("transforms.rs", "fn visual_tiny_zoom"),
        ("transforms.rs", "fn visual_pulse_active"),
        ("transforms.rs", "fn visual_pulse_inner_zoom"),
        ("transforms.rs", "fn visual_pulse_zoom_for_y"),
        ("transforms.rs", "fn visual_arrow_effect_zoom"),
        ("transforms.rs", "fn visual_dizzy_rotation_deg"),
        ("transforms.rs", "fn visual_note_rotation_z"),
        ("transforms.rs", "fn visual_effect_params_for_col"),
        ("transforms.rs", "fn smoothstep01"),
        ("transforms.rs", "fn compute_invert_distances"),
        ("transforms.rs", "fn compute_tornado_bounds"),
        ("transforms.rs", "fn tipsy_y_extra"),
        ("transforms.rs", "fn beat_x_extra"),
        ("transforms.rs", "fn drunk_x_extra"),
        ("transforms.rs", "fn tornado_x_extra"),
        ("transforms.rs", "fn note_x_extra"),
        ("transforms.rs", "fn note_x_offset"),
        ("transforms.rs", "fn appearance_note_alpha"),
        ("transforms.rs", "fn appearance_note_glow"),
        ("transforms.rs", "fn appearance_note_actor_alpha"),
        ("transforms.rs", "fn appearance_needs_rows"),
        ("transforms.rs", "fn tiny_spacing_scale"),
        ("transforms.rs", "fn note_world_z_for_bumpy"),
        ("transforms.rs", "fn visual_confusion_rotation_deg"),
        ("transforms.rs", "fn gameplay_visual_effect_params"),
        ("transforms.rs", "fn move_col_extra"),
        ("notes.rs", "struct ScrollTravelRequest"),
        ("notes.rs", "fn scroll_travel"),
        ("notes.rs", "const fn mine_hides_after_resolution"),
        ("receptors.rs", "fn hold_indicator_column_x"),
        ("receptors.rs", "struct ReceptorActorsRequest"),
        ("receptors.rs", "struct ReceptorPress"),
        ("receptors.rs", "fn compose_receptor_actors"),
        ("receptors.rs", "fn receptor_row_center"),
        ("feedback.rs", "struct ColumnFeedbackRequest"),
        ("feedback.rs", "struct JudgmentTiltParams"),
        ("feedback.rs", "struct TapJudgmentRowsParams"),
        ("feedback.rs", "fn compose_column_feedback"),
        ("feedback.rs", "fn judgment_tilt_rotation_deg"),
        ("feedback.rs", "fn judgment_actor_zoom"),
        ("feedback.rs", "fn tap_judgment_rows"),
        ("feedback.rs", "fn itg_actor_glow_alpha"),
        ("feedback.rs", "const fn hold_glow_color"),
        ("explosions.rs", "enum ExplosionRotation"),
        ("explosions.rs", "struct ExplosionComposeRequest"),
        ("explosions.rs", "fn compose_explosion_layers"),
        ("measure_actors.rs", "fn append_edit_measure_number"),
        ("measure_actors.rs", "fn append_beat_bar"),
        ("measure_actors.rs", "fn append_cue_bar"),
        ("measure_lines.rs", "struct EditBeatBarInfo"),
        ("measure_lines.rs", "fn edit_beat_bar_info_for_row"),
        ("measure_lines.rs", "fn edit_bar_candidate_step_rows"),
        ("measure_lines.rs", "fn edit_bar_scroll_speed"),
        ("measure_lines.rs", "fn beat_scroll_travel"),
        ("measure_lines.rs", "fn edit_beat_scroll_travel"),
        ("measure_lines.rs", "fn scaled_edit_bar_alpha"),
        ("mini_indicator.rs", "fn stream_segment_index_exclusive_end"),
        ("mini_indicator.rs", "fn stream_segment_index_inclusive_end"),
        ("mini_indicator.rs", "fn zmod_broken_run_segment"),
        ("mini_indicator.rs", "fn zmod_run_timer_index"),
        ("mini_indicator.rs", "fn zmod_measure_counter_text"),
        ("mini_indicator.rs", "fn zmod_broken_run_counter_text"),
        ("mini_indicator.rs", "fn zmod_subtractive_counter_state"),
        ("mini_indicator.rs", "fn zmod_subtractive_points"),
        ("mini_indicator.rs", "fn zmod_rival_color"),
        ("mini_indicator.rs", "fn zmod_pacemaker_color"),
        ("mini_indicator.rs", "fn zmod_stream_prog_color"),
        ("mini_indicator.rs", "fn zmod_combo_glow_color"),
        ("mini_indicator.rs", "fn zmod_combo_glow_pair"),
        ("mini_indicator.rs", "fn zmod_combo_solid_color"),
        ("mini_indicator.rs", "fn zmod_indicator_default_color"),
        ("mini_indicator.rs", "fn zmod_indicator_detailed_color"),
        ("mini_indicator.rs", "fn zmod_combo_rainbow_color"),
        ("holds.rs", "fn scale_effect_size"),
        ("holds.rs", "fn hold_entry_head_beat"),
        ("holds.rs", "fn translated_uv_rect"),
        ("holds.rs", "fn scale_sprite_to_arrow"),
        ("holds.rs", "fn song_time_ns_to_seconds"),
        ("holds.rs", "fn hold_strip_actor"),
        ("holds.rs", "fn bottom_cap_uv_window"),
        ("holds.rs", "fn song_time_ns_delta_seconds"),
        ("error_bar.rs", "fn error_bar_text_scalable_zoom"),
    ] {
        let source = fs::read_to_string(root.join("crates/deadsync-notefield/src").join(file))
            .unwrap_or_else(|_| panic!("canonical notefield source {file} should be readable"));
        assert!(
            source.contains(&format!("pub(crate) {internal}")),
            "canonical helper {internal} in {file} should stay crate-private"
        );
        assert!(
            !source.contains(&format!("pub {internal}")),
            "canonical helper {internal} in {file} leaked into the theme API"
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
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composer should be readable");

    for definition in [
        "struct HoldEntryPlanRequest",
        "struct HoldEntryPlan",
        "fn hold_entry_head_beat",
        "fn hold_entry_plan",
        "fn preferred_hold_visual",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical hold owner is missing {definition}"
        );
    }
    for delegation in [
        "HoldEntryPlanRequest {",
        "hold_entry_head_beat(",
        "hold_entry_plan(",
    ] {
        assert!(
            field.contains(delegation),
            "canonical field frame must delegate hold planning through {delegation}"
        );
        assert!(
            !theme.contains(delegation),
            "Simply Love must not import low-level hold-planning API {delegation}"
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
    assert!(notefield_lib.contains("pub use noteskin_model::{"));
    assert!(notefield_manifest.contains("twox-hash = \"2.1.2\""));
    assert!(!notefield_manifest.contains("deadsync-assets"));

    for definition in [
        "pub struct ModelMeshCacheStats",
        "pub struct ModelMeshCache",
        "pub fn noteskin_model_actor_from_draw",
        "pub fn noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry",
        "pub fn noteskin_model_actor",
    ] {
        assert!(
            canonical.contains(definition),
            "canonical noteskin model owner is missing {definition}"
        );
    }
    assert!(canonical.contains("pub(crate) fn noteskin_model_actor_from_draw_cached"));
    assert!(!canonical.contains("pub fn noteskin_model_actor_from_draw_cached"));
    let note_composer = fs::read_to_string(root.join("crates/deadsync-notefield/src/notes.rs"))
        .expect("canonical note composer should be readable");
    assert!(note_composer.contains("noteskin_model_actor_from_draw_cached"));
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
    let frame = fs::read_to_string(root.join("crates/deadsync-notefield/src/frame_feedback.rs"))
        .expect("canonical feedback-frame composition should be readable");
    let field = fs::read_to_string(root.join("crates/deadsync-notefield/src/field_frame.rs"))
        .expect("canonical field-frame composition should be readable");
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
        "pub(crate) struct ReceptorActorsRequest",
        "pub(crate) struct ReceptorPress",
        "pub(crate) fn compose_receptor_actors",
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
            !canonical.contains(forbidden) && !frame.contains(forbidden),
            "canonical receptor owner imports concrete/runtime token {forbidden}"
        );
    }
    assert!(!manifest.contains("deadsync-theme-simply-love"));
    assert!(song_lua.contains("pub fn song_lua_note_model_draw"));
    assert!(!theme.contains("fn song_lua_note_model_draw"));

    assert!(contract.contains("pub struct ReceptorStyle"));
    assert!(contract.contains("pub struct NotefieldActorStyle"));
    assert!(contract.contains("pub receptor: ReceptorStyle"));
    assert!(contract.contains("pub actors: NotefieldActorStyle"));
    assert!(!contract.contains("SimplyLoveNotefieldStyle"));
    for value in [
        "target_z: 100",
        "press_glow_z: 105",
        "hold_explosion_z: 145",
        "tap_explosion_z: 150",
        "mine_explosion_z: 101",
    ] {
        assert!(
            theme_style.contains(value),
            "Simply Love receptor style lost {value}"
        );
    }

    for token in [
        "pub struct NotefieldFeedbackFrameView",
        "pub struct NotefieldLaneFeedback",
        "fn compose_notefield_feedback",
        "compose_receptor_actors(",
        "ReceptorActorsRequest {",
        "ReceptorPress {",
        "style: request.style.receptor",
    ] {
        assert!(
            frame.contains(token),
            "canonical feedback-frame owner is missing {token}"
        );
    }
    assert!(theme.contains("NotefieldFeedbackFrameView {"));
    assert!(theme.contains("NotefieldFieldFrameView {"));
    assert!(theme.contains("compose_notefield_field("));
    assert!(field.contains("compose_notefield_feedback("));
    assert!(!theme.contains("compose_notefield_feedback("));
    assert!(theme.contains("slot.texture_key_handle().into_sprite_source()"));
    assert!(frame.contains("visual.tiny"));
    let ordered_markers = [
        "compose_column_feedback(",
        "compose_receptor_actors(",
        "for (local_col, active) in frame.tap_explosions",
        "for (local_col, active) in frame.mine_explosions",
    ];
    let mut previous = 0;
    for marker in ordered_markers {
        let position = frame[previous..]
            .find(marker)
            .map(|offset| previous + offset)
            .unwrap_or_else(|| panic!("canonical feedback frame is missing {marker}"));
        previous = position + marker.len();
    }
    for low_level in [
        "compose_receptor_actors(",
        "ReceptorActorsRequest {",
        "ReceptorPress {",
        "compose_column_feedback(",
        "ColumnFeedbackRequest {",
    ] {
        assert!(
            !theme.contains(low_level),
            "Simply Love still imports low-level feedback API {low_level}"
        );
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
fn shell_public_facade_does_not_grow_accidentally() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib = fs::read_to_string(root.join("crates/deadsync-shell/src/lib.rs"))
        .expect("shell facade should be readable");
    let public_lines: Vec<_> = lib
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub "))
        .collect();
    assert_eq!(
        public_lines,
        ["pub mod app;"],
        "only the startup module belongs on the shell's public facade"
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
    for token in [
        "pub enum ThemeEffect",
        "Batch(Vec<Self>)",
        "pub enum ThemeFlowEvent",
    ] {
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
        shell_flow.contains("pub enum ThemeEffectExecution")
            && shell_flow.contains("Batch(Vec<ThemeEffect>)"),
        "deadsync-shell must own ordered ThemeEffect execution"
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
    let mut failures = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("workspace source should be readable");
        let rel = rel_path(&root, &file);
        for token in &removed_tokens {
            let count = text.match_indices(token).count();
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
