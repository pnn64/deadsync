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
    ("src/game/parsing/noteskin/mod.rs", "engine", 14),
    ("src/game/parsing/noteskin/model_cache.rs", "engine", 3),
    ("src/game/parsing/simfile.rs", "config", 3),
    ("src/game/parsing/simfile.rs", "engine", 1),
    ("src/game/parsing/simfile/cache.rs", "config", 1),
    ("src/game/parsing/simfile/scan.rs", "config", 3),
    ("src/game/parsing/song_lua/actor_host.rs", "assets", 3),
    ("src/game/parsing/song_lua/mod.rs", "engine", 2),
    ("src/game/parsing/song_lua/overlay.rs", "engine", 4),
    ("src/game/parsing/song_lua/tests.rs", "engine", 2),
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

const GAME_RULE_FACADE_MODULES: &[&str] = &["judgment", "note", "scroll", "timing"];

const GAME_RULE_FACADE_SCAN_DIRS: &[&str] = &[
    "src/app",
    "src/config",
    "src/game",
    "src/screens",
    "src/test_support",
    "tests",
];

const GAME_PROFILE_RULE_SYMBOLS: &[&str] = &["ScrollSpeedSetting"];

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
    "CachedPlayerLeaderboardData",
    "CachedItlScore",
    "CachedScore",
    "Grade",
    "GrooveStatsEvalState",
    "GrooveStatsSubmitRecordBanner",
    "GrooveStatsSubmitUiStatus",
    "ItlEvalState",
    "ItlEventProgress",
    "ItlOverlayPage",
    "LeaderboardEntry",
    "LeaderboardPane",
    "LocalScalarScore",
    "MachineReplayEntry",
    "PlayerLeaderboardData",
    "RejectReason",
    "ReplayEdge",
    "ScoreBulkImportSummary",
    "ScoreImportEndpoint",
    "ScoreImportProgress",
    "SUBMIT_RETRY_MAX_ATTEMPTS",
    "duration_to_ceil_secs",
    "gameplay_run_failed",
    "gameplay_run_passed",
    "leaderboard_rank_for_score",
    "lua_chart_submit_allowed",
    "lua_submit_allowed",
    "promote_quint_grade",
    "score_to_grade",
    "submit_retry_delay_secs",
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
