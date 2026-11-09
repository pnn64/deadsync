#[path = "../support/native_actor_inventory.rs"]
mod native_actor_inventory;
// Original compiler unit fixtures, using the public Rust compiler entry point.
use super::*;
use chrono::{Datelike, Local};
use deadlib_present::actors::{TextAlign, TextAttribute};
use deadlib_present::anim::{EffectClock, EffectMode};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
type TestOverlayKind = SongLuaOverlayKind<(), (), TextAttribute>;
type TestOverlayActor = SongLuaOverlayActor<TestOverlayKind>;
type TestCompiledSongLua = CompiledSongLua<TestOverlayActor>;

fn test_compile_song_lua(
    entry_path: &Path,
    context: &SongLuaCompileContext,
) -> Result<TestCompiledSongLua, String> {
    compile_song_lua_with_default_host(
        entry_path,
        context,
        SongLuaNoteskinResolver::default(),
        test_read_model_slots,
        test_model_layer_from_slot,
        |_, _| None,
    )
}
fn test_compile_song_lua_layers(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
) -> Result<Vec<TestCompiledSongLua>, String> {
    compile_song_lua_layers_with_default_host(
        entry_paths,
        primary_index,
        context,
        SongLuaNoteskinResolver::default(),
        test_read_model_slots,
        test_model_layer_from_slot,
        |_, _| None,
    )
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-song-lua-crate-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn deadsync_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn song_lua_fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = deadsync_root().join("tests/fixtures/song_lua");
    let entry = root.join(name);
    assert!(
        entry.is_file(),
        "missing song Lua fixture: {}",
        entry.display()
    );
    (root, entry)
}

fn write_easing_lib(song_dir: &Path) {
    fs::write(
        song_dir.join("easing.lua"),
        include_str!("../../../../tests/fixtures/song_lua/easing.lua"),
    )
    .expect("write chart owned easing library");
}

fn test_read_model_slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from(Vec::<()>::new().into_boxed_slice()))
}

fn test_model_layer_from_slot(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
    None
}

fn mod_window(start: f32, limit: f32, mods: &str) -> SongLuaModWindow {
    SongLuaModWindow {
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        mods: mods.to_string(),
        player: None,
    }
}

fn ease_window(start: f32, limit: f32) -> SongLuaEaseWindow {
    SongLuaEaseWindow {
        approach_speed: None,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        from: 0.0,
        to: 1.0,
        target: SongLuaEaseTarget::PlayerX,
        easing: None,
        player: None,
        sustain: None,
        opt1: None,
        opt2: None,
    }
}

fn overlay_ease(overlay_index: usize, start: f32, limit: f32) -> SongLuaOverlayEase {
    SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta::default(),
        to: SongLuaOverlayStateDelta::default(),
        easing: None,
        sustain: None,
        opt1: None,
        opt2: None,
    }
}

fn test_song_steps() -> Vec<crate::SongLuaStep> {
    use crate::SongLuaStep;
    [
        SongLuaDifficulty::Beginner,
        SongLuaDifficulty::Easy,
        SongLuaDifficulty::Medium,
        SongLuaDifficulty::Hard,
        SongLuaDifficulty::Challenge,
        SongLuaDifficulty::Edit,
    ]
    .into_iter()
    .map(|difficulty| SongLuaStep {
        steps_type: "StepsType_Dance_Single".into(),
        difficulty,
        description: String::new(),
        chart_name: String::new(),
        credit: String::new(),
        meter: difficulty.meter().max(0) as u32,
        music_file: String::new(),
        possible_grade_points: 0,
    })
    .collect()
}

fn generated_runtime_mod_lua() -> &'static str {
    r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")

mods = {
    {0, 9999, "*1000 no beat, *1000 no drunk, *1000 no tipsy, *1000 no invert, *1000 no flip, *1000 no dizzy", "end"},
}
mod_time = {
    {0.00, 999, "*1 0 Dark1, *1 0 Dark2, *1 0 Dark3, *1 0 Dark4, *1 0 PulseOuter, *1 0 PulseOffset, *1 0 Wave, *1 0 Bumpy3, *1 0 BumpyPeriod, *1 0 Stealth, *1 0 Blind, *1 0 Sudden, *1 0 Tipsy, *1 0 Drunk, *1 0 Dark", "len"},
}
mods_ease = {}

local l = "len"
local function me(...)
    table.insert(mods_ease, {...})
end

me(4, 0.75, 250, 0, "Bumpy1", l, ease.outQuad)
me(4, 0.75, -125, 0, "BumpyPeriod", l, ease.outQuad)
me(4, 0.75, 75, 0, "Wave", l, ease.outElastic)
me(8, 0.75, 250, 0, "Bumpy2", l, ease.outQuad)
me(12, 0.75, 250, 0, "Bumpy3", l, ease.outQuad)
me(16, 0.75, 250, 0, "Bumpy4", l, ease.outQuad)
me(20, 1.5, 50, 1, "hidden", l, ease.outInQuad)
me(24, 0.5, 25, 0, "beat", l, ease.outBounce)

return Def.ActorFrame{}
"#
}

#[test]
fn offline_compile_rejects_input_callbacks_instead_of_dropping_them() {
    for (name, command) in [
        ("immediate-input", "OnCommand=function(self) register() end"),
        (
            "delayed-input",
            "OnCommand=function(self) self:sleep(0.1):queuecommand('Ready') end, ReadyCommand=function(self) register() end",
        ),
    ] {
        let dir = test_dir(name);
        let entry = dir.join("custom.lua");
        std::fs::write(
            &entry,
            format!(
                r#"
local function register()
    local screen=SCREENMAN:GetTopScreen()
    local callback=function(event) return event.button=='Down' end
    screen:AddInputCallback(callback):RemoveInputCallback(callback)
end
return Def.Actor {{ {command} }}
"#
            ),
        )
        .unwrap();
        let mut context = SongLuaCompileContext::new(&dir, name);
        context.music_length_seconds = 0.5;
        let error = test_compile_song_lua(&entry, &context).unwrap_err();
        assert!(error.contains("requires runtime Lua"), "{error}");
    }
}

#[test]
fn compile_song_lua_reads_mod_tables() {
    let song_dir = test_dir("direct");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
mods = {
    {1, 2, "*100 no invert", "len", 2},
}
mod_time = {
    {0, 5, "*100 no dark", "len"},
}
mods_ease = {
    {4, 1, 0, 100, "flip", "len", ease.outQuad, 1},
    {6, 1, 0, 1, function(value) end, "len"},
}
mod_actions = {
    {12, "ShowDDRFail", true},
    {13, function() end},
}
mod_perframes = {
    {16, 20, function() end},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Test Song")).unwrap();
    assert_eq!(compiled.beat_mods.len(), 1);
    assert_eq!(compiled.beat_mods[0].unit, SongLuaTimeUnit::Beat);
    assert_eq!(compiled.beat_mods[0].span_mode, SongLuaSpanMode::Len);
    assert_eq!(compiled.beat_mods[0].player, Some(2));
    assert_eq!(compiled.time_mods.len(), 1);
    assert_eq!(compiled.eases.len(), 1);
    assert_eq!(
        compiled.eases[0].target,
        SongLuaEaseTarget::Mod("flip".to_string())
    );
    assert_eq!(compiled.eases[0].easing.as_deref(), Some("outQuad"));
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ShowDDRFail");
    assert_eq!(compiled.info.unsupported_function_eases, 1);
    assert_eq!(compiled.info.unsupported_function_ease_captures.len(), 1);
    assert!(
        compiled.info.unsupported_function_ease_captures[0]
            .contains("function ease unit=Beat start=6.000")
    );
    assert_eq!(compiled.info.unsupported_function_actions, 1);
    assert_eq!(compiled.info.unsupported_function_action_captures.len(), 1);
    assert!(
        compiled.info.unsupported_function_action_captures[0]
            .contains("function action beat=13.000 persists=false")
    );
    assert_eq!(compiled.info.unsupported_perframes, 1);
    assert_eq!(compiled.info.unsupported_perframe_captures.len(), 1);
    assert!(
        compiled.info.unsupported_perframe_captures[0].contains("perframe start=16.000 end=20.000")
    );
}

#[test]
fn compile_song_lua_reads_local_update_mod_time() {
    let song_dir = test_dir("local-update-mod-time");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local mod_time = {
    {0, 5, "*100 no dark", "len"},
}

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                if mod_time[1] then end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Local Mod Time"),
    )
    .unwrap();
    assert_eq!(compiled.time_mods.len(), 1);
    assert_eq!(compiled.time_mods[0].unit, SongLuaTimeUnit::Second);
    assert_eq!(compiled.time_mods[0].mods, "*100 no dark");
}

#[test]
fn compile_song_lua_samples_player_perframes_into_eases() {
    let song_dir = test_dir("perframe-player");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_perframes = {
    {4, 5, function(beat)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:x(320 + (beat - 4) * 40)
            p:y(240 - (beat - 4) * 30)
            p:z((beat - 4) * -120)
            p:rotationx((beat - 4) * 45)
            p:rotationz((beat - 4) * 90)
            p:skewx((beat - 4) * 0.5)
            p:skewy((beat - 4) * 0.25)
            p:zoom(1 + (beat - 4) * 0.25)
        end
    end},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Perframe Player"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerX) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerY) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerZ) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerRotationX) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerRotationZ) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerSkewX) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerSkewY) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerZoomX) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerZoomY) && window.player == Some(1)
    }));
    assert!(compiled.eases.iter().any(|window| {
        matches!(window.target, SongLuaEaseTarget::PlayerZoomZ) && window.player == Some(1)
    }));
}

#[test]
fn compile_song_lua_exposes_song_time_to_perframes() {
    let song_dir = test_dir("perframe-song-time");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_perframes = {
    {4, 5, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            local beat = math.floor(GAMESTATE:GetSongBeat())
            local seconds = math.floor(GAMESTATE:GetCurMusicSeconds())
            local pos = math.floor(GAMESTATE:GetSongPosition():GetSongBeat())
            local since = math.floor(GetTimeSinceStart())
            p:rotationz(beat + seconds + pos + since)
        end
    end},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Perframe Song Time");
    context.song_display_bpms = [120.0, 120.0];
    context.song_music_rate = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    let windows = compiled
        .eases
        .iter()
        .filter(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerRotationZ) && window.player == Some(1)
        })
        .collect::<Vec<_>>();
    assert!(!windows.is_empty());
    assert!(
        windows
            .iter()
            .all(|window| window.from == 11.0 && window.to == 11.0)
    );
}

#[test]
fn compile_song_lua_exposes_effect_delta_to_perframes() {
    let song_dir = test_dir("perframe-effect-delta");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_perframes = {
    {4, 5, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:effectclock("beat")
            p:x(p:GetEffectDelta() * 100)
            p:effectclock("timer")
            p:y(p:GetEffectDelta() * 1000)
        end
    end},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Perframe Effect Delta");
    context.song_display_bpms = [120.0, 120.0];
    context.song_music_rate = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    let x_windows = compiled
        .eases
        .iter()
        .filter(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerX) && window.player == Some(1)
        })
        .collect::<Vec<_>>();
    let y_windows = compiled
        .eases
        .iter()
        .filter(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerY) && window.player == Some(1)
        })
        .collect::<Vec<_>>();
    assert!(
        x_windows
            .iter()
            .any(|window| window.from > 0.0 || window.to > 0.0)
    );
    assert!(
        y_windows
            .iter()
            .any(|window| window.from > 0.0 || window.to > 0.0)
    );
}

#[test]
fn compile_song_lua_accepts_side_effect_only_perframes() {
    let song_dir = test_dir("perframe-side-effects");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_perframes = {
    {4, 5, function()
        SCREENMAN:SystemMessage("perframe")
        SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen")
    end},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Perframe Side Effects"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    assert!(compiled.eases.is_empty());
    assert!(compiled.overlay_eases.is_empty());
}

#[test]
fn frame_init_follows_child() {
    let song_dir = test_dir("frame-init-order");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local order = 0
return Def.ActorFrame{
    InitCommand=function(self)
        assert(order == 2, 'frame Init must follow both child Init commands')
        self:GetChild('First'):x(10)
    end,
    Def.Quad{
        Name='First',
        InitCommand=function(self) order = order + 1; self:x(order) end
    },
    Def.Quad{
        Name='Second',
        InitCommand=function(self) order = order + 1; self:x(order) end
    }
}
"#,
    )
    .expect("write frame Init control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Frame Init Order"),
    )
    .expect("compile frame Init control");
    for (name, x) in [("First", 10.0), ("Second", 2.0)] {
        let actor = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(name))
            .expect("initialized child");
        assert_eq!(actor.initial_state.x, x, "{name} Init order");
    }
}

#[test]
fn compile_song_lua_runs_actor_init_commands() {
    let song_dir = test_dir("init-command");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals = {
            mods = {
                {2, 1, "*100 no dark", "len", 1},
            },
            ease = {
                {8, 2, 0, 100, "flip", "len", ease.inOutQuad, 2},
            },
            actions = {
                {12, "ShowDDRFail", true},
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Init Command Song"),
    )
    .unwrap();
    assert_eq!(compiled.beat_mods.len(), 1);
    assert_eq!(compiled.beat_mods[0].player, Some(1));
    assert_eq!(compiled.eases.len(), 1);
    assert_eq!(
        compiled.eases[0].target,
        SongLuaEaseTarget::Mod("flip".to_string())
    );
    assert_eq!(compiled.eases[0].player, Some(2));
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ShowDDRFail");
}

#[test]
fn compile_song_lua_names_callable_table_easings() {
    let song_dir = test_dir("callable-table-easings");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
xero = {
    outElastic = setmetatable({}, {
        __call = function(self, t)
            return t
        end,
    }),
}

mods_ease = {
    {1, 1, 0, 100, "tiny", "len", xero.outElastic, 1},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Callable Table Easings"),
    )
    .unwrap();
    assert_eq!(compiled.eases.len(), 1);
    assert_eq!(compiled.eases[0].easing.as_deref(), Some("outElastic"));
}

#[test]
fn compile_song_lua_runs_actor_startup_commands_with_stub_methods() {
    let song_dir = test_dir("startup-command");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
prefix_globals = {}

return Def.ActorFrame{
    OnCommand=function(self)
        prefix_globals.actions = {
            {4, "StartupReady", true},
        }
    end,
    Def.Actor{
        OnCommand=function(self)
            self:sleep(9e9)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Startup Command Song"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "StartupReady");
}

#[test]
fn compile_song_lua_captures_def_actor_message_commands() {
    let song_dir = test_dir("def-actor-message-command");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {2, "WorkerPulse", true},
}

return Def.ActorFrame{
    Def.ActorFrame{
        Name="Worker",
        InitCommand=function(self)
            self:aux(2)
        end,
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
                actor:aux(actor:getaux() + 3)
            end)
        end,
        WorkerPulseMessageCommand=function(self)
            self:x(self:getaux())
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Def Actor Message Command"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "WorkerPulse");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrame
    ));
    assert_eq!(compiled.overlays[0].name.as_deref(), Some("Worker"));
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].message,
        "WorkerPulse"
    );
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0].delta.x,
        Some(5.0)
    );
}

#[test]
fn compile_song_lua_exposes_product_globals() {
    let song_dir = test_dir("product-globals");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local version = ProductVersion()
local product = ProductID()
local family = ProductFamily()

if version ~= "1.2.0" then
    error("unexpected ProductVersion: " .. tostring(version))
end
if product ~= "ITGmania" then
    error("unexpected ProductID: " .. tostring(product))
end
if family ~= "ITGmania" then
    error("unexpected ProductFamily: " .. tostring(family))
end

mod_actions = {
    {4, product .. ":" .. family .. ":" .. version, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Product Globals"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ITGmania:ITGmania:1.2.0");
}

#[test]
fn compile_song_lua_exposes_enabled_player_globals() {
    let song_dir = test_dir("player-globals");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local enabled = GAMESTATE:GetEnabledPlayers()
local human = GAMESTATE:GetHumanPlayers()

if PLAYER_1 ~= "PlayerNumber_P1" then
    error("unexpected PLAYER_1: " .. tostring(PLAYER_1))
end
if PLAYER_2 ~= "PlayerNumber_P2" then
    error("unexpected PLAYER_2: " .. tostring(PLAYER_2))
end
if #enabled ~= 1 or enabled[1] ~= PLAYER_1 then
    error("unexpected enabled players")
end
if #human ~= 1 or human[1] ~= PLAYER_1 then
    error("unexpected human players")
end
if not GAMESTATE:IsHumanPlayer(PLAYER_1) then
    error("PLAYER_1 should be human")
end
if GAMESTATE:IsHumanPlayer(PLAYER_2) then
    error("PLAYER_2 should be disabled")
end

mod_actions = {
    {4, enabled[1], true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Player Globals");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "PlayerNumber_P1");
}

#[test]
fn compile_song_lua_round_trips_noteskins() {
    let song_dir = test_dir("noteskin-round-trip");
    for skin in ["cyber", "default", "cel", "dark", "skew", "turn"] {
        fs::create_dir_all(song_dir.join(skin)).expect("local skin directory");
        fs::write(song_dir.join(skin).join("metrics.ini"), "[Global]\n").expect("local metrics");
    }
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local state = GAMESTATE:GetPlayerState(PLAYER_1)
local po = state:GetPlayerOptions('ModsLevel_Song')
assert(po:NoteSkin() == 'cyber')
assert(THEME:GetMetric('Common', 'DefaultNoteSkinName') == 'cel')
return Def.ActorFrame{OnCommand=function(self)
    po:FromString('50% default, 20% dark')
    assert(po:NoteSkin() == 'default' and math.abs(po:Dark() - 0.2) < 0.000001)
    assert(state:GetPlayerOptionsString('ModsLevel_Song'):find('Default', 1, true))
    po:FromString('25% skew')
    assert(po:NoteSkin() == 'skew' and po:Skew() == 0, 'skin lookup precedes Skew')
    state:SetPlayerOptions('ModsLevel_Song', 'no noteskin')
    assert(po:NoteSkin() == 'cel')
    assert(not state:GetPlayerOptionsString('ModsLevel_Song'):find('Cel', 1, true))
    po:NoteSkin('CYBER')
    state:SetPlayerOptions('ModsLevel_Song', '')
    assert(po:NoteSkin() == 'CYBER', 'assignment without a skin retains prior raw skin')
    po:FromString('CyBeR')
    po:FromString('clearall')
    assert(po:NoteSkin() == 'cel', 'clearall uses theme default')
    self:SetUpdateFunction(function()
        po:FromString('50% default, 20% dark')
        assert(po:NoteSkin() == 'default')
        state:SetPlayerOptions('ModsLevel_Song', 'CyBeR, 40% Dark')
        assert(po:NoteSkin() == 'cyber')
        local before, accepted = po:NoteSkin('CYBER')
        assert(before == 'cyber' and accepted == true)
        before, accepted = po:NoteSkin('missing-skin')
        assert(before == 'CYBER' and accepted == nil and po:NoteSkin() == 'CYBER')
    end)
end}
"#,
    )
    .expect("noteskin fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "noteskin strings");
    context.players[0].noteskin_name = "cyber".into();
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 0.125;
    let compiled = test_compile_song_lua(&entry, &context).expect("source-backed skin semantics");
    #[cfg(feature = "test-support")]
    {
        let writes = &compiled.noteskin_writes;
        assert_eq!(writes[0].previous, "cyber");
        assert_eq!(writes[0].current, "default");
        assert_eq!(writes[1].current, "skew");
        assert_eq!(writes[2].current, "cel");
        assert_eq!(
            writes.iter().filter(|write| write.second == 0.0).count(),
            7 + 4,
            "startup and one genuine zero-time update, without discovery duplicates"
        );
    }
    #[cfg(not(feature = "test-support"))]
    let _ = compiled;
}

#[test]
fn compile_song_lua_exposes_player_noteskin_name() {
    let song_dir = test_dir("player-noteskin");
    fs::create_dir_all(song_dir.join("lambda")).unwrap();
    fs::write(song_dir.join("lambda/metrics.ini"), "[Global]\n").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
if string.lower(po:NoteSkin()) ~= "cyber" then
    error("unexpected NoteSkin getter: " .. tostring(po:NoteSkin()))
end
local was, accepted = po:NoteSkin("lambda")
assert(was == "cyber" and accepted == true)
local before, rejected = po:NoteSkin("missing-skin")
assert(before == "lambda" and rejected == nil)
assert(po:NoteSkin("lambda", true) == po)
if po:NoteSkin() ~= "lambda" then
    error("unexpected NoteSkin setter: " .. tostring(po:NoteSkin()))
end
mod_actions = {
    {4, po:NoteSkin(), true},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Player Noteskin");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            noteskin_name: "cyber".to_string(),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "lambda");
    assert_eq!(
        compiled.startup.noteskins,
        [Some("lambda".to_string()), None]
    );
}

#[test]
fn song_startup_ignores_late_noteskin_and_lead_in_changes() {
    let song_dir = test_dir("startup-skin");
    for skin in ["cel", "metal"] {
        fs::create_dir_all(song_dir.join(skin)).unwrap();
        fs::write(song_dir.join(skin).join("metrics.ini"), "[Global]\n").unwrap();
    }
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
local top = SCREENMAN:GetTopScreen()
top:SetMinSecondsToMusic(6.01)
top:GetChild("In"):visible(false)
return Def.ActorFrame {
    OnCommand = function(self)
        po:NoteSkin("cel")
        self:SetUpdateFunction(function()
            po:NoteSkin("metal")
            top:SetMinSecondsToMusic(20)
        end)
    end,
}
"#,
    )
    .unwrap();
    let context = SongLuaCompileContext::new(&song_dir, "Startup");
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.startup.noteskins[0].as_deref(), Some("cel"));
    assert_eq!(compiled.startup.noteskins[1], None);
    assert_eq!(compiled.startup.min_seconds_to_music, Some(6.01));
    assert!(compiled.startup.hide_in);
}

#[test]
fn compile_song_lua_supports_bitmap_text_ctor() {
    let song_dir = test_dir("bitmap-text");
    let entry = song_dir.join("default.lua");
    fs::write(song_dir.join("_komika axis 42px.ini"), b"placeholder").unwrap();
    fs::write(
        &entry,
        r##"
return Def.ActorFrame{
    Def.BitmapText{
        Name="Countdown",
        Font="_komika axis 42px.ini",
        Text="",
        OnCommand=function(self)
            self:visible(false)
                :z(10)
                :strokecolor(color("#000000"))
                :settext(3)
                :finishtweening()
        end,
    },
}
"##,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BitmapText"))
            .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(!compiled.overlays[0].initial_state.visible);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText {
            ref font_path,
            ref text,
            stroke_color: Some([0.0, 0.0, 0.0, 1.0]),
            ..
        } if font_path.ends_with("_komika axis 42px.ini") && text.as_ref() == "3"
    ));
}

#[test]
fn compile_song_lua_coerces_numeric_actor_visibility() {
    let song_dir = test_dir("numeric-actor-visibility");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Hidden",
        OnCommand=function(self) self:visible(0) end,
    },
    Def.Quad{
        Name="Shown",
        OnCommand=function(self) self:visible(1) end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Numeric Actor Visibility"),
    )
    .unwrap();
    let hidden = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Hidden"))
        .unwrap();
    let shown = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Shown"))
        .unwrap();
    assert!(!hidden.initial_state.visible);
    assert!(shown.initial_state.visible);
}

#[test]
fn actor_lookup_matches_itg() {
    let song_dir = test_dir("native-actor-lookup");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        assert(self:GetNumChildren() == 4)
        assert(self:GetChild("missing") == nil)
        assert(self:GetChild("") == nil)
        assert(self:GetNumChildren() == 4)
        assert(self.GetText == nil)
        assert(self.UnknownMethod == nil)
        assert(self.GetChildAt == nil and ActorFrame.GetChildAt == nil)
        assert(self:GetChild("Child").GetText == nil)
        local label = self:GetChild("Label")
        assert(type(label.GetText) == "function")
        assert(label:GetText() == "Alpha")
        assert(type(self:GetChild("BPM").GetText) == "function")
        assert(type(self:GetChild("Devices").GetText) == "function")
        mod_actions = {{1, "NativeLookupChecked", true}}
    end,
    Def.ActorFrame{Name="Child"},
    Def.BitmapText{Name="Label", Font="Common Normal", Text="Alpha"},
    Def.BPMDisplay{Name="BPM"},
    Def.DeviceList{Name="Devices"}
}
"#,
    )
    .expect("write native actor lookup fixture");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native actor lookup"),
    )
    .expect("native child and GetText lookup assertions");
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "NativeLookupChecked");
}

#[test]
fn compile_song_lua_supports_bitmap_text_get_text() {
    let song_dir = test_dir("bitmap-text-get-text");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="Alpha",
        OnCommand=function(self)
            local before = self:GetText()
            self:settext(3)
            mod_actions = {
                {1, before .. ":" .. self:GetText(), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText GetText"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Alpha:3");
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "3"
    ));
}

#[test]
fn compile_song_lua_supports_bitmap_text_settextf() {
    let song_dir = test_dir("bitmap-text-settextf");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        OnCommand=function(self)
            self:settextf("Stage %02d - %s", 4, "Final")
            mod_actions = {
                {1, self:GetText(), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText SetTextF"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Stage 04 - Final");
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "Stage 04 - Final"
    ));
}

#[test]
fn compile_song_lua_supports_rolling_numbers_shape() {
    let song_dir = test_dir("rolling-numbers-shape");
    let entry = song_dir.join("default.lua");
    fs::write(song_dir.join("_numbers.ini"), b"placeholder").unwrap();
    fs::write(
        &entry,
        r#"
local counts = GetExJudgmentCounts(PLAYER_1)
assert(counts.W0 == 0 and counts.totalHolds == 0)

return Def.ActorFrame{
    Def.RollingNumbers{
        Font="_numbers.ini",
        InitCommand=function(self)
            assert(self:Load("RollingNumbersEvaluationA") == self)
            assert(self:targetnumber(12) == self)
            mod_actions = {{
                1,
                string.format("%s:%d", self:GetText(), self:GetTargetNumber()),
                true,
            }}
        end,
    },
    Def.RollingNumbers{
        Font="_numbers.ini",
        InitCommand=function(self)
            assert(self:Load("RollingNumbersEvaluationB") == self)
            assert(self:SetTargetNumber(7) == self)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Rolling Numbers Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0012:12");
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "0012"
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::BitmapText { ref text, .. } if text.as_ref() == "007"
    ));
}

#[test]
fn compile_song_lua_supports_graph_display_shape() {
    let song_dir = test_dir("graph-display-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
                &entry,
                r#"
return Def.ActorFrame{
    Def.GraphDisplay{
        Name="GraphDisplay",
        InitCommand=function(self)
            self:vertalign(top)
            assert(self:Load("GraphDisplay2") == self)
            assert(self:Set(
                STATSMAN:GetCurStageStats(),
                STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
            ) == self)
            self:SetWidth(120)
            local body = self:GetChild("")
            body[2]:visible(false)
            local line = self:GetChild("Line")
            line:addy(1)
            mod_actions = {{
                1,
                string.format("%d:%s:%.0f:%.0f", #body, tostring(body[2]:GetVisible()), line:GetY(), self:GetWidth()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Graph Display Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "2:false:1:120");
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::GraphDisplay {
        size,
        body_values,
        body_state,
        line_state,
    } = &compiled.overlays[0].kind
    else {
        panic!("expected GraphDisplay overlay");
    };
    assert_eq!(*size, [120.0, 64.0]);
    assert_eq!(body_values.len(), GRAPH_DISPLAY_VALUE_RESOLUTION);
    assert!(
        body_values
            .iter()
            .all(|value| *value == SONG_LUA_INITIAL_LIFE)
    );
    assert!(!body_state.visible);
    assert_eq!(line_state.y, 1.0);
}

#[test]
fn compile_song_lua_uses_single_player_graph_display_width() {
    let song_dir = test_dir("graph-display-single-player-width");
    let entry = song_dir.join("default.lua");
    fs::write(
                &entry,
                r#"
local metric_width = THEME:GetMetricF("GraphDisplay", "BodyWidth")
local metric_height = THEME:GetMetricI("GraphDisplay", "BodyHeight")

return Def.ActorFrame{
    Def.GraphDisplay{
        InitCommand=function(self)
            mod_actions = {{
                1,
                string.format("%.0f:%d:%.0f:%.0f", metric_width, metric_height, self:GetWidth(), self:GetHeight()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Graph Display Single Player");
    context.players[1].enabled = false;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "610:64:610:64");
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::GraphDisplay {
        size, body_values, ..
    } = &compiled.overlays[0].kind
    else {
        panic!("expected GraphDisplay overlay");
    };
    assert_eq!(*size, [610.0, 64.0]);
    assert_eq!(body_values.len(), GRAPH_DISPLAY_VALUE_RESOLUTION);
}

#[test]
fn compile_song_lua_supports_song_meter_display_shape() {
    let song_dir = test_dir("song-meter-display-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
                r#"
return Def.ActorFrame{
    Def.SongMeterDisplay{
        StreamWidth=96,
        Stream=Def.Quad{
            InitCommand=function(self)
                self:zoomy(18):diffuse(GetCurrentColor(true))
            end,
        },
        InitCommand=function(self)
            assert(self:GetStreamWidth() == 96)
            assert(self:SetStreamWidth(144) == self)
            local stream = self:GetChild("Stream")
            stream:visible(false)
            mod_actions = {{
                1,
                string.format("%s:%s:%d", stream:GetName(), tostring(stream:GetVisible()), self:GetStreamWidth()),
                true,
            }}
        end,
    },
}
"#,
            )
            .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Song Meter Display Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Stream:false:144");
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::SongMeterDisplay {
        stream_width,
        stream_state,
        music_length_seconds,
    } = &compiled.overlays[0].kind
    else {
        panic!("expected SongMeterDisplay overlay");
    };
    assert_eq!(*stream_width, 144.0);
    assert!(!stream_state.visible);
    assert_eq!(stream_state.zoom_y, 18.0);
    assert_eq!(*music_length_seconds, 0.0);
}

#[test]
fn compile_song_lua_supports_course_contents_list_shape() {
    let song_dir = test_dir("course-contents-list-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function transform(self, offsetFromCenter, itemIndex, numItems)
    self:y(offsetFromCenter * 23)
end

local function update(ccl, dt)
    if ccl:GetCurrentItem() <= 0 and ccl:GetTweenTimeLeft() == 0 then
        ccl:SetDestinationItem(math.max(0, ccl:GetNumItems() - 1))
    end
end

return Def.ActorFrame{
    Def.CourseContentsList{
        MaxSongs=1000,
        NumItemsToDraw=8,
        InitCommand=function(self)
            self:SetUpdateFunction(update)
        end,
        OnCommand=function(self)
            self:playcommand("Set")
        end,
        SetCommand=function(self)
            assert(self:SetFromGameState() == self)
            assert(self:SetTransformFromHeight(23) == self)
            assert(self:SetTransformFromWidth(100) == self)
            assert(self:SetSecondsPerItem(0.25) == self)
            assert(self:SetNumSubdivisions(2) == self)
            assert(self:ScrollThroughAllItems() == self)
            assert(self:ScrollWithPadding(0, 0) == self)
            assert(self:SetFastCatchup(true) == self)
            assert(self:SetWrap(false) == self)
            assert(self:SetMask(300, 80) == self)
            assert(self:SetNumItemsToDraw(8) == self)
            assert(self:SetCurrentAndDestinationItem(0) == self)
            assert(self:SetTransformFromFunction(transform) == self)
            assert(self:PositionItems() == self)
            assert(self:SetLoop(false) == self)
            assert(self:SetPauseCountdownSeconds(0) == self)
            assert(self:SetSecondsPauseBetweenItems(0.5) == self)

            local display = self:GetChild("Display")
            mod_actions = {{
                1,
                string.format(
                    "%d:%.0f:%.0f:%s:%.1f:%.0f:%.2f:%.2f",
                    self:GetNumItems(),
                    self:GetCurrentItem(),
                    self:GetDestinationItem(),
                    tostring(display ~= nil),
                    self:GetSecondsPauseBetweenItems(),
                    display:GetY(),
                    self:GetSecondsToDestination(),
                    self:GetFullScrollLengthSeconds()
                ),
                true,
            }}
        end,
        Display=Def.ActorFrame{
            Name="Display",
            SetCommand=function(self)
                self:finishtweening()
            end,
            SetSongCommand=function(self, params)
                self:zoom(0.875)
            end,
            Def.BitmapText{
                Font="Common Normal",
                SetSongCommand=function(self, params)
                    self:settext(params.Song:GetDisplayFullTitle() .. ":" .. params.Meter)
                end,
            },
        },
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Course Contents List Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1:0:0:true:0.5:0:0.00:0.25");
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.overlays.len(), 3);
    assert_eq!(compiled.overlays[1].parent_index, Some(0));
    assert_eq!(compiled.overlays[1].initial_state.zoom_x, 0.875);
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            ref text,
            ..
        } if text.as_ref() == "Course Contents List Shape:12"
    ));
}

#[test]
fn compile_song_lua_supports_input_device_list_shapes() {
    let song_dir = test_dir("input-device-list-shapes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.DeviceList{
        Font=THEME:GetPathF("", "Common Normal"),
        InitCommand=function(self)
            self:xy(_screen.cx, _screen.h - 60):zoom(0.8)
            assert(self:GetText() == "No input devices")
        end,
    },
    Def.InputList{
        Font="Common Normal",
        InitCommand=function(self)
            assert(self:GetText() == "No unmapped inputs")
            self:xy(_screen.cx - 250, 50):horizalign(left):vertalign(top):vertspacing(0)
            mod_actions = {{
                1,
                string.format(
                    "%s:%s:%s",
                    tostring(Def.DeviceList ~= nil),
                    self:GetName(),
                    self:GetText()
                ),
                true,
            }}
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Input Device List Shapes"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true::No unmapped inputs");
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            ref text,
            ..
        } if text.as_ref() == "No input devices"
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            ref text,
            ..
        } if text.as_ref() == "No unmapped inputs"
    ));
}

#[test]
fn compile_song_lua_supports_model_base_rotation_shape() {
    let song_dir = test_dir("model-base-rotation-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
        song_dir.join("ring_model.txt"),
        "// MilkShape 3D ASCII\nMeshes: 0\nMaterials: 0\nBones: 0\n",
    )
    .expect("model file for the shape test's slot reader");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Model{
        Meshes="ring_model.txt",
        Materials="ring_model.txt",
        Bones="ring_model.txt",
        InitCommand=function(self)
            self:diffuse(1, 1, 1, 0.8)
                :baserotationx(-60)
                :baserotationy(20)
                :baserotationz(50)
                :SetTextureFiltering(true)
            mod_actions = {{
                1,
                string.format(
                    "%s:%s:%s:%.0f:%.0f:%.0f",
                    self.Meshes,
                    self.Materials,
                    self.Bones,
                    self:GetRotationX(),
                    self:GetRotationY(),
                    self:GetRotationZ()
                ),
                true,
            }}
        end,
        OnCommand=function(self)
            self:zoom(0.75):xy(SCREEN_CENTER_X, SCREEN_CENTER_Y):z(-100)
                :spin():effectmagnitude(0, 0, 20)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Model Base Rotation Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ring_model.txt:ring_model.txt:ring_model.txt:0:0:0"
    );
    assert!(compiled.overlays.is_empty());
}

#[test]
fn native_bitmap_style() {
    let song_dir = test_dir("native-bitmap-style");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="STYLE",
        OnCommand=function(self)
            self:wrapwidthpixels(88)
                :AddAttribute(0, { Length=1, Diffuse=Color.White })
                :ClearAttributes()
                :rainbowscroll(true)
                :jitter(true)
                :distort(0.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native BitmapText Style"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.wrap_width_pixels,
        Some(88)
    );
    assert!(compiled.overlays[0].initial_state.rainbow_scroll);
    assert!(compiled.overlays[0].initial_state.text_jitter);
    assert_eq!(compiled.overlays[0].initial_state.text_distortion, 0.5);
}

#[test]
fn compile_song_lua_exposes_hooks_and_noteskin_variant_helpers() {
    let song_dir = test_dir("hooks-noteskin-variant-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local arch = HOOKS:GetArchName()
assert(type(arch) == "string" and arch ~= "")
assert(HOOKS:GetClipboard() == "")
assert(NOTESKIN:HasVariants("default") == false)
assert(NOTESKIN:IsNoteSkinVariant("default") == false)
assert(#NOTESKIN:GetVariantNamesForNoteSkin("default") == 0)

mod_actions = {
    {1, function()
        assert(HOOKS:SetClipboard("theme helper") == false)
        assert(HOOKS:OpenURL("https://example.invalid") == false)
        assert(HOOKS:OpenFile("Save/ThemePrefs.ini") == false)
        assert(HOOKS:RestartProgram() == false)
    end, true},
    {2, arch:lower():match("windows") and "windows" or "not-windows", true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Hooks Noteskin Variants"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert!(matches!(
        compiled.messages[0].message.as_str(),
        "windows" | "not-windows"
    ));
}

#[test]
fn compile_song_lua_runs_concat_noteskin_sprite_oncommand() {
    let song_dir = test_dir("noteskin-concat-oncommand");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {}

return Def.ActorFrame{
    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")..{
        Name="ConcatNoteskin",
        OnCommand=function(self)
            mod_actions = {
                {4, self:GetName(), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Noteskin Concat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ConcatNoteskin");
}

#[test]
fn compile_song_lua_merges_actor_definitions() {
    let song_dir = test_dir("native-definition-concat");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/definition-concat.lua"))
        .expect("write definition control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native Definition Concat"),
    )
    .expect("compile native definition control");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 42.0);
}

#[test]
fn compile_song_lua_exposes_color_helpers() {
    let song_dir = test_dir("color-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r##"
local c1 = color("#00000080")
local c2 = color("1,0.5,0.25")
local c3 = color(0.25, 0.5, 0.75, 1)
local mix = lerp_color(0.5, c1, c3)

local function approx(a, b)
    return math.abs(a - b) < 0.001
end

if not approx(c1[4], 128 / 255) then
    error("unexpected hex alpha: " .. tostring(c1[4]))
end
if c2[4] ~= 1 then
    error("numeric string alpha default mismatch")
end
if not approx(mix[1], 0.125) or not approx(mix[2], 0.25) or not approx(mix[3], 0.375) then
    error("unexpected lerp color")
end
if Color.White[1] ~= 1 or Color.White[2] ~= 1 or Color.White[3] ~= 1 or Color.White[4] ~= 1 then
    error("unexpected Color.White")
end
if not approx(Color.Blue[3], 239 / 255) or Color.Blue[1] ~= 0 then
    error("unexpected Color.Blue")
end

return Def.ActorFrame{}
"##,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Color Helpers"),
    )
    .unwrap();
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_supports_bitmaptext_skew_methods() {
    let song_dir = test_dir("bitmaptext-overlay-skew");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="SKEW",
        OnCommand=function(self)
            self:skewx(0.15):skewy(-0.35)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Overlay Skew"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert!((state.skew_x - 0.15).abs() <= 0.000_1);
    assert!((state.skew_y + 0.35).abs() <= 0.000_1);
}

#[test]
fn compile_song_lua_captures_bitmap_text_attributes() {
    let song_dir = test_dir("bitmap-text-attributes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="ATTR",
        OnCommand=function(self)
            self:AddAttribute(1, {
                Length=2,
                Diffuse={0.2, 0.4, 0.6, 0.8},
                Glow={0.7, 0.3, 0.9, 0.5},
            })
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="GRAD",
        OnCommand=function(self)
            self:AddAttribute(0, {
                Length=-1,
                Diffuses={
                    {1, 0, 0, 1},
                    {0, 1, 0, 1},
                    {0, 0, 1, 1},
                    {1, 1, 0, 1},
                },
            })
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Attributes"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    let SongLuaOverlayKind::BitmapText { attributes, .. } = &compiled.overlays[0].kind else {
        panic!("expected BitmapText overlay");
    };
    assert_eq!(attributes.len(), 1);
    assert_eq!(attributes[0].start, 1);
    assert_eq!(attributes[0].length, 2);
    assert_eq!(attributes[0].color, [0.2, 0.4, 0.6, 0.8]);
    assert_eq!(attributes[0].glow, Some([0.7, 0.3, 0.9, 0.5]));
    let SongLuaOverlayKind::BitmapText { attributes, .. } = &compiled.overlays[1].kind else {
        panic!("expected BitmapText overlay");
    };
    assert_eq!(attributes.len(), 1);
    assert_eq!(attributes[0].start, 0);
    assert_eq!(attributes[0].length, usize::MAX);
    assert_eq!(attributes[0].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        attributes[0].vertex_colors,
        Some([
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ])
    );
}

#[test]
fn compile_song_lua_exposes_theme_color_helpers() {
    let song_dir = test_dir("theme-color-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
                &entry,
                r##"
local wrap = GetHexColor(13)
local itg = GetHexColor(1, false, "ITG")
local p2 = PlayerColor(PLAYER_2)
local hard = DifficultyColor("Difficulty_Hard")
local edit = DifficultyColor("Difficulty_Edit")
local dark = PlayerDarkColor(PLAYER_2)
local custom = CustomDifficultyToColor("Difficulty_Medium")
local stage = StageToColor("Stage_Final")
local judge = JudgmentLineToColor("JudgmentLine_W1")
local light = LightenColor(color("#202020"))
local blend = BlendColors(Color.Red, Color.Blue)
local alpha = Color.Alpha(Color.White, 0.25)
local named = Color("Black")
local stroke = JudgmentLineToStrokeColor("JudgmentLine_W1")
local step = StepsOrTrailToColor({ GetDifficulty=function() return "Difficulty_Hard" end })
local hex = ColorToHex(color("#00000080"))
local has_alpha = HasAlpha(color("#00000080"))

mod_actions = {
    {
        1,
        string.format(
            "%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%.3f:%s:%.3f",
            wrap[1],
            wrap[2],
            itg[1],
            p2[3],
            hard[2],
            edit[1],
            dark[1],
            custom[1],
            stage[2],
            judge[1],
            light[1],
            blend[1],
            alpha[4],
            named[1],
            stroke[1],
            step[1],
            hex,
            has_alpha
        ),
        true,
    },
}

return Def.ActorFrame{}
"##,
            )
            .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Color Helpers"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1.000:0.365:0.799:0.000:0.490:0.706:0.290:0.996:0.027:0.749:0.157:0.465:0.250:0.000:0.375:1.000:00000080:0.502"
    );
}

#[test]
fn compile_song_lua_exposes_simply_love_namespace_helpers() {
    let song_dir = test_dir("simply-love-namespace-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r##"
SL.Global.ActiveColorIndex = 2
local original = { nested = { 1 } }
local copied = DeepCopy(original)
copied.nested[1] = 7

mod_actions = {
    {
        1,
        string.format(
            "%.0f:%.1f:%s:%.0f:%.1f:%d:%d:%d:%.3f:%.2f:%s:%s:%s:%s",
            SL.Global.ActiveColorIndex,
            SL.Global.ActiveModifiers.MusicRate,
            SL.P1.ActiveModifiers.SpeedModType,
            SL.P1.ActiveModifiers.SpeedMod,
            SL_WideScale(10, 20),
            FindInTable(SL.Colors[12], SL.Colors),
            original.nested[1],
            copied.nested[1],
            SL.JudgmentColors["FA+"][7][1],
            round(1.234, 2),
            tostring(IsServiceAllowed(SL.GrooveStats.GetScores)),
            tostring(IsUsingWideScreen()),
            tostring(DarkUI()),
            tostring(SL.P1.ActiveModifiers.TimingWindows[4])
        ),
        true,
    },
}

return Def.ActorFrame{}
"##,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Simply Love Namespace Helpers");
    context.song_music_rate = 1.5;
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.players[0].speedmod = SongLuaSpeedMod::C(650.0);
    context.players[0].timing_windows = [true, true, true, false, false];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2:1.5:C:650:20.0:12:1:7:1.000:1.23:false:true:false:false"
    );
}

#[test]
fn compile_song_lua_exposes_lua51_stdlib_aliases() {
    let song_dir = test_dir("lua51-stdlib-aliases");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local values = {10, 20, 30}
assert(_VERSION == 'Lua 5.1')
assert(type(unpack) == 'function' and table.unpack == nil)
local a, b, c = unpack(values)
assert(a == 10 and b == 20 and c == 30)
local first, missing, last = unpack({[1]=10, [3]=30}, 1, 3)
assert(first == 10 and missing == nil and last == 30)
assert(select('#', unpack(values, 3, 2)) == 0)
assert(not pcall(table.getn, false))
mod_actions = {
    {1, string.format("%d:%d", math.mod(5, 2), table.getn(values)), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Lua51 Stdlib Aliases"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1:3");
}

#[test]
fn theme_helpers() {
    let song_dir = test_dir("theme-shuffle");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local input = {'a', 'b', 'c'}
local calls = 0
math.random = function(limit)
    calls = calls + 1
    assert(limit == calls)
    return 1
end
assert(table.shuffle == tableshuffle)
local output = table.shuffle(input)
assert(output ~= input and table.concat(output) == 'cba')
assert(table.concat(input) == 'abc' and calls == 3)
assert(#table.shuffle({}) == 0 and calls == 3)
assert(pname(PLAYER_1) == 'P1' and pname(PLAYER_2) == 'P2')
assert(type(Player) == 'table' and Player ~= ActorFrame)
return Def.ActorFrame{
    InitCommand=function(self)
        assert(Player.GetChild(self, 'child') == self:GetChild('child'))
    end,
    Def.Actor{Name='child'},
}
"#,
    )
    .unwrap();
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme helpers"),
    )
    .unwrap();
}

#[test]
fn compile_song_lua_exposes_ivalues_helper() {
    let song_dir = test_dir("ivalues-helper");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local sum = 0
for value in ivalues({10, 20, 30}) do
    sum = sum + value
end
mod_actions = {
    {1, tostring(sum), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "IValues")).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "60");
}

#[test]
fn compile_song_lua_keeps_native_value_iterator() {
    let song_dir = test_dir("native-value-iterator");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/value-iterator.lua"))
        .expect("write value iterator control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native Value Iterator"),
    )
    .expect("compile native value iterator");
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "iterator native");
}

#[test]
fn compile_song_lua_diffusecolor_preserves_alpha() {
    let song_dir = test_dir("diffusecolor-alias");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffusealpha(0.25):diffusecolor(0.85, 0.92, 0.99, 0.7)
            assert(self:GetDiffuseAlpha() == 0.25)
        end,
        TintMessageCommand=function(self)
            self:diffusecolor({0.5, 0.25, 0.125, 0.875})
            assert(self:GetDiffuseAlpha() == 0.25)
        end,
        TweenMessageCommand=function(self)
            self:linear(1):diffusecolor(0.125, 0.25, 0.5, 0)
            assert(self:GetDiffuseAlpha() == 0.25)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "DiffuseColor Alias"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.diffuse,
        [0.85, 0.92, 0.99, 0.25]
    );
    for (message, expected, duration) in [
        ("Tint", [0.5, 0.25, 0.125, 0.25], 0.0),
        ("Tween", [0.125, 0.25, 0.5, 0.25], 1.0),
    ] {
        let command = compiled.overlays[0]
            .message_commands
            .iter()
            .find(|command| command.message == message)
            .expect("color command");
        let block = command
            .blocks
            .iter()
            .find(|block| block.delta.diffuse.is_some())
            .expect("color block");
        assert_eq!(block.delta.diffuse, Some(expected));
        assert_eq!(block.duration, duration);
    }
}

#[test]
fn compile_song_lua_update_diffusecolor_preserves_alpha() {
    let song_dir = test_dir("update-diffusecolor");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local color = self:GetChild("color")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            color:diffusecolor(beat / 8, 0.25, 0.125, beat)
        end)
    end,
    Def.Quad{
        Name="color",
        InitCommand=function(self) self:diffusealpha(0.25) end,
    },
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Update Diffuse Color");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.overlays[0].initial_state.diffuse[3], 0.25);
    let tracks = compiled
        .overlay_updates
        .iter()
        .filter(|track| track.target == SongLuaOverlayUpdateTarget::Diffuse)
        .collect::<Vec<_>>();
    assert!(!tracks.is_empty());
    for track in tracks {
        assert!(!track.samples.is_empty());
        for sample in &track.samples {
            let SongLuaOverlayUpdateValue::Vec4(color) = sample.value else {
                panic!("expected sampled diffuse color");
            };
            assert_eq!(&color[1..], &[0.25, 0.125, 0.25]);
        }
    }
}

#[test]
fn compile_song_lua_exposes_theme_player_metrics() {
    let song_dir = test_dir("theme-metrics");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local standard = THEME:GetMetric("Player", "ReceptorArrowsYStandard")
local reverse = THEME:GetMetricF("Player", "ReceptorArrowsYReverse")
local missing = THEME:GetMetric("Player", "NoSuchMetric")
local p1 = THEME:GetMetric("ScreenGameplay", "PlayerP1TwoPlayersTwoSidesX")
local p2 = THEME:GetMetricF("ScreenGameplay", "PlayerP2TwoPlayersTwoSidesX")
assert(p1 == 160 and p2 == 480)
assert(THEME:HasMetric("ScreenGameplay", "PlayerP1TwoPlayersTwoSidesX"))

if standard ~= -125 then
    error("unexpected ReceptorArrowsYStandard: " .. tostring(standard))
end
if reverse ~= 145 then
    error("unexpected ReceptorArrowsYReverse: " .. tostring(reverse))
end
if missing ~= nil then
    error("unexpected metric fallback: " .. tostring(missing))
end

mod_actions = {
    {4, "theme-metrics-ok", true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Metrics"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "theme-metrics-ok");
}

#[test]
fn compile_song_lua_exposes_player_draw_distance_metrics() {
    let song_dir = test_dir("player-draw-distance-metrics");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local before = THEME:GetMetric("Player", "DrawDistanceBeforeTargetsPixels")
local before_f = THEME:GetMetricF("Player", "DrawDistanceBeforeTargetsPixels")
local before_i = THEME:GetMetricI("Player", "DrawDistanceBeforeTargetsPixels")
local after = THEME:GetMetric("Player", "DrawDistanceAfterTargetsPixels")

if not THEME:HasMetric("Player", "DrawDistanceBeforeTargetsPixels") then
    error("missing DrawDistanceBeforeTargetsPixels")
end
if not THEME:HasMetric("Player", "DrawDistanceAfterTargetsPixels") then
    error("missing DrawDistanceAfterTargetsPixels")
end

mod_actions = {
    {1, string.format("%.0f:%.0f:%d:%.0f", before, before_f, before_i, after), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Draw Distance Metrics");
    context.screen_height = 720.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1080:1080:1080:-130");
}

#[test]
fn compile_song_lua_exposes_theme_singleton_compat() {
    let song_dir = test_dir("theme-singletons");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local profile = PROFILEMAN:GetProfile(PLAYER_1)
ThemePrefs.Set("RainbowMode", true)
ThemePrefs.Save()
GAMESTATE:InsertCoin(-GAMESTATE:GetCoinsNeededToJoin())

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%d:%d:%s:%s:%s:%s:%s:%s:%s:%d",
            tostring(GAMESTATE:IsCourseMode()),
            tostring(GAMESTATE:IsEventMode()),
            GAMESTATE:GetMasterPlayerNumber(),
            GAMESTATE:GetCurrentGame():GetName(),
            GAMESTATE:GetNumSidesJoined(),
            GAMESTATE:GetNumStagesLeft(),
            GAMESTATE:GetCoinMode(),
            GAMESTATE:GetPremium(),
            THEME:GetString("Difficulty", "Difficulty_Challenge"),
            tostring(THEME:HasString("OptionTitles", "Yes")),
            ThemePrefs.Get("ThemeFont"),
            tostring(ThemePrefs.Get("UseImageCache")),
            profile:GetDisplayName(),
            PROFILEMAN:IsPersistentProfile(PLAYER_1) and 1 or 0
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Singletons");
    context.players[1].enabled = false;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "false:true:PlayerNumber_P1:dance:1:1:CoinMode_Free:Premium_Off:Challenge:true:Common:true:Player 1:0"
    );
}

#[test]
fn compile_song_lua_exposes_theme_enum_and_songutil_helpers() {
    let song_dir = test_dir("theme-enum-songutil");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local song = GAMESTATE:GetCurrentSong()
local playable = SongUtil.GetPlayableSteps(song)
local typed = SongUtil:GetPlayableStepsByStepsType(song, "StepsType_Dance_Single")
local player = PlayerNumber:Reverse()[PLAYER_2]
local difficulty = Difficulty:Reverse()["Difficulty_Hard"]
local other = OtherPlayer[PLAYER_1]

GAMESTATE:ApplyGameCommand("mod,1.0xmusic")

mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%d:%d:%s:%s",
            player,
            difficulty,
            other,
            #playable,
            #typed,
            FormatPercentScore(0.93456),
            ScreenSystemLayerHelpers.GetCreditsMessage(PLAYER_1)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Enum SongUtil");
    context.song_steps = test_song_steps();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1:3:PlayerNumber_P2:6:6:93.46%:Free Play"
    );
}

#[test]
fn compile_song_lua_initializes_capture_before_startup_tweens() {
    let song_dir = test_dir("startup-capture");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self)
            self:visible(false)
        end,
        OnCommand=function(self)
            self:accelerate(0.8):diffusealpha(1):xy(320, 240)
        end,
    },
}
"#,
    )
    .unwrap();

    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Startup Capture Song"),
    )
    .unwrap();
}

#[test]
fn compile_song_lua_runs_set_update_function_once() {
    let song_dir = test_dir("set-update-function");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                mods = {
                    {4, 1, "*100 no dark", "len"},
                }
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Song"),
    )
    .unwrap();
    assert_eq!(compiled.beat_mods.len(), 1);
    assert_eq!(compiled.beat_mods[0].start, 4.0);
}

#[test]
fn compile_song_lua_passes_update_delta_seconds() {
    let song_dir = test_dir("set-update-function-delta");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    SetUpdateRateCommand=function(self)
        self:SetUpdateRate(3)
    end,
    InitCommand=function(self)
        self:playcommand("SetUpdateRate")
    end,
    Def.Quad{Name="Panel"},
    Def.ActorFrame{
        OnCommand=function(self)
            local actor = self:GetParent():GetChild("Panel")
            self:SetUpdateFunction(function(frame, dt)
                actor:x(dt * 60):y(frame:GetParent():GetUpdateRate())
                mod_actions = {{
                    1,
                    string.format("%.0f:%.0f", actor:GetX(), actor:GetY()),
                    true,
                }}
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Delta Song"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0:3");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 0.0);
    assert_eq!(compiled.overlays[0].initial_state.y, 3.0);
}

#[test]
fn compile_song_lua_contains_update_errors_in_itg_order() {
    let song_dir = test_dir("set-update-function-error-order");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local order = ""
local recorded = false
local function mark(value)
    if not recorded then order = order .. value end
end

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            mark("p")
            if not recorded then
                recorded = true
                mod_actions = {{1, order, true}}
            end
        end)
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                mark("a")
                missing_global:x(1)
            end)
        end,
    },
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                mark("b")
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Error Order"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "abp");
}

#[test]
fn compile_song_lua_drains_update_function_queuecommands() {
    let song_dir = test_dir("set-update-function-queuecommand");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        PulseCommand=function(self)
            self:x(12)
            mod_actions = {
                {1, "update-queued", true},
            }
        end,
    },
    Def.ActorFrame{
        OnCommand=function(self)
            local panel = self:GetParent():GetChild("Panel")
            self:SetUpdateFunction(function() panel:queuecommand("Pulse") end)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction QueueCommand Song"),
    )
    .unwrap();
    assert_eq!(
        compiled
            .messages
            .iter()
            .filter(|event| event.message == "update-queued")
            .count(),
        1
    );
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.x, 0.0,
        "Actor::UpdateTweening(0) does not dispatch Pulse"
    );
    let queued = compiled.overlays[0]
        .message_commands
        .iter()
        .find(|command| command.message == "__songlua_queued_startup")
        .expect("next-frame Pulse capture");
    assert_eq!(
        crate::overlay_state_after_blocks(compiled.overlays[0].initial_state, &queued.blocks, 0.0)
            .x,
        12.0
    );
}

#[test]
fn compile_song_lua_samples_update_function_overlay_motion() {
    let song_dir = test_dir("set-update-function-overlay-motion");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self)
            target = self
            self:visible(false):zoomto(16, 16)
        end,
    },
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                target:visible(beat >= 2 and beat <= 4)
                target:x(beat * 10)
                target:rotationz(beat * 15)
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Overlay Motion");
    context.music_length_seconds = 6.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == 0
            && track.target == SongLuaOverlayUpdateTarget::X
            && track.samples.len() > 2
    }));
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == 0
            && track.target == SongLuaOverlayUpdateTarget::RotationZ
            && track.samples.len() > 2
    }));
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == 0 && track.target == SongLuaOverlayUpdateTarget::Visible
    }));
}

#[test]
fn compile_song_lua_captures_delayed_cross_actor_queuecommands() {
    let song_dir = test_dir("update-cross-actor-queuecommand");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local flash
local fired = false

return Def.ActorFrame{
    Def.Quad{
        Name="Flash",
        InitCommand=function(self)
            flash = self
            self:visible(false)
        end,
        HideCommand=function(self)
            self:visible(false)
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if not fired and GAMESTATE:GetSongBeat() >= 1 then
                    fired = true
                    flash:visible(true):diffusealpha(1)
                    flash:sleep(0.25):linear(0.25):diffusealpha(0):queuecommand("Hide")
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Cross Actor QueueCommand");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let visible = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::Visible)
        .expect("the flash should have a sampled visibility track");
    assert!(visible.samples.iter().any(|sample| {
        sample.time >= 1.0 && sample.value == SongLuaOverlayUpdateValue::Bool(true)
    }));
    assert!(visible.samples.iter().any(|sample| {
        (sample.time - (1.5 + 1.0 / 60.0)).abs() <= 1.0e-4
            && sample.value == SongLuaOverlayUpdateValue::Bool(false)
    }));
    let diffuse = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::Diffuse)
        .expect("the flash should have a sampled diffuse track");
    assert!(diffuse.samples.iter().any(|sample| {
        (sample.time - 1.25).abs() <= 1.0e-4
            && sample.value == SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, 1.0])
    }));
    assert!(diffuse.samples.iter().any(|sample| {
        (sample.time - 1.5).abs() <= 1.0e-4
            && sample.value == SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, 0.0])
    }));
}

#[test]
fn startup_sleep_delays_broadcast() {
    let song_dir = test_dir("startup-delay");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/startup-delay.lua"))
        .expect("write startup delay fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Startup Delay");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile startup delay");
    let messages = compiled
        .messages
        .iter()
        .filter(|event| event.message == "ShowAnswer")
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 1);
    assert!((messages[0].beat - 2.0).abs() < 0.02, "{:?}", messages[0]);
    let panel = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Panel"))
        .expect("panel");
    assert!(compiled.overlays[panel].initial_state.visible);
    let visibility = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == panel && track.target == SongLuaOverlayUpdateTarget::Visible
        })
        .expect("timed panel visibility");
    assert!(
        visibility
            .samples
            .iter()
            .any(|sample| sample.time >= 2.0
                && sample.value == SongLuaOverlayUpdateValue::Bool(false))
    );
    assert!(
        visibility
            .samples
            .iter()
            .all(|sample| sample.time >= 1.99
                || sample.value == SongLuaOverlayUpdateValue::Bool(true))
    );
}

#[test]
fn consuming_cross_message_uses_runtime_capture() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut context = SongLuaCompileContext::new(&song_dir, "Consumed Message");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&song_dir.join("consumed-message.lua"), &context).unwrap();
    assert!(
        compiled.info.skipped_message_command_captures.is_empty(),
        "{:?}",
        compiled.info.skipped_message_command_captures
    );
    assert_eq!(
        compiled
            .messages
            .iter()
            .filter(|event| event.message == "Consume")
            .count(),
        1
    );
    let target = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Target"))
        .unwrap();
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == target
            && track.target == SongLuaOverlayUpdateTarget::X
            && track.samples.iter().any(|sample| {
                sample.time >= 2.0 && sample.value == SongLuaOverlayUpdateValue::F32(150.0)
            })
    }));
}

#[test]
fn queued_broadcasts_keep_the_dispatch_beat() {
    let song_dir = test_dir("queued-broadcast-beat");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/nested-start.lua"))
        .expect("write queued broadcast fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Broadcast Beat");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile queued broadcast");
    let event = compiled
        .messages
        .iter()
        .find(|event| event.message == "BodyRotateBuildings")
        .expect("queued broadcast");
    assert!((event.beat - 1.7666667).abs() < 0.0001);
}

#[test]
fn queued_messages_deliver_empty_tables() {
    let song_dir = test_dir("queued-message-params");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
            return Def.ActorFrame {
                OnCommand = function(self)
                    self:queuemessage("Startup"):sleep(0.05):queuemessage("Later")
                end,
                Def.Quad {
                Name = "Probe",
                StartupMessageCommand = function(self, params)
                    if params ~= nil then assert(next(params) == nil); self:x(7) end
                end,
                LaterMessageCommand = function(self, params)
                    if params ~= nil then assert(next(params) == nil); self:x(13) end
                end,
                },
            }
        "#,
    )
    .expect("queued parameter fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued parameters");
    context.music_length_seconds = 0.2;
    let compiled = test_compile_song_lua(&entry, &context).expect("queued parameters compile");
    let probe = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Probe"))
        .expect("parameter probe");
    assert!(
        compiled
            .overlay_updates
            .iter()
            .any(|track| track.overlay_index == probe
                && track.target == SongLuaOverlayUpdateTarget::X
                && track.samples.iter().any(|sample| sample.time == 1.0 / 60.0
                    && sample.value == SongLuaOverlayUpdateValue::F32(7.0)))
    );
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == probe
            && track.target == SongLuaOverlayUpdateTarget::X
            && track.samples.iter().any(|sample| {
                sample.time >= 0.05 && sample.value == SongLuaOverlayUpdateValue::F32(13.0)
            })
    }));
}

#[test]
fn compile_song_lua_propagates_actorframe_play_and_queued_commands() {
    let song_dir = test_dir("actorframe-command-propagation");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target

mod_actions = {
    {1, function()
        target:playcommand("Hit")
    end, true},
    {2, function()
        target:queuecommand("Respawn")
    end, true},
}

return Def.ActorFrame{
    Def.ActorFrame{
        Name="Target",
        OnCommand=function(self) target = self end,
        Def.Quad{
            Name="Face",
            OnCommand=cmd(visible,true),
            HitCommand=cmd(visible,false),
            RespawnCommand=cmd(visible,true),
        },
        Def.ActorFrame{
            Def.Quad{
                Name="Flash",
                OnCommand=cmd(visible,false),
                HitCommand=cmd(visible,true),
                RespawnCommand=cmd(visible,false),
            },
        },
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "ActorFrame Commands");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let hit = &compiled
        .messages
        .iter()
        .find(|message| (message.beat - 1.0).abs() <= f32::EPSILON)
        .unwrap()
        .message;
    let respawn = &compiled
        .messages
        .iter()
        .find(|message| (message.beat - 2.0).abs() <= f32::EPSILON)
        .unwrap()
        .message;
    for name in ["Face", "Flash"] {
        let actor = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap();
        let hit_command = actor
            .message_commands
            .iter()
            .find(|command| command.message == *hit)
            .unwrap();
        let respawn_command = actor
            .message_commands
            .iter()
            .find(|command| command.message == *respawn)
            .unwrap();
        assert_eq!(hit_command.blocks[0].delta.visible, Some(name == "Flash"));
        assert_eq!(
            respawn_command.blocks[0].delta.visible,
            Some(name == "Face")
        );
    }
}

#[test]
fn compile_song_lua_message_capture_restores_external_state() {
    let song_dir = test_dir("message-capture-external-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
capture_fired = false

return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self)
            target = self
            self:visible(false):x(0):y(0)
        end,
    },
    Def.ActorFrame{
        FireMessageCommand=function(self)
            capture_fired = true
            target:aux(6)
            target:visible(true):x(99)
        end,
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if capture_fired or target:getaux() > 0 then target:y(77) end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Message Capture State");
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let (target_index, target) = compiled
        .overlays
        .iter()
        .enumerate()
        .find(|(_, overlay)| overlay.name.as_deref() == Some("Target"))
        .expect("the target actor should compile");
    assert!(!target.initial_state.visible);
    assert_eq!(target.initial_state.x, 0.0);
    assert!(!compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == target_index && track.target == SongLuaOverlayUpdateTarget::Y
    }));
}

#[test]
fn compile_song_lua_keeps_globally_referenced_actor_frames() {
    let song_dir = test_dir("globally-referenced-actor-frames");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
group_a = nil
groups = {}
mod_actions = {
    {1, function() group_a:linear(1):x(100) end, true},
}

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self) group_a = self end,
        Def.Quad{Name="ChildA"},
    },
    Def.ActorFrame{
        OnCommand=function(self) groups[1] = self end,
        Def.Quad{Name="ChildB"},
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                groups[1]:x(GAMESTATE:GetSongBeat() * 10)
                groups[1]:y(20)
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Referenced Actor Frames");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let child_parent = |name: &str| {
        compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some(name))
            .and_then(|overlay| overlay.parent_index)
            .expect("the child should retain its referenced parent")
    };
    let group_a_index = child_parent("ChildA");
    let group_b_index = child_parent("ChildB");
    assert!(matches!(
        compiled.overlays[group_a_index].kind,
        SongLuaOverlayKind::ActorFrame
    ));
    assert!(matches!(
        compiled.overlays[group_b_index].kind,
        SongLuaOverlayKind::ActorFrame
    ));
    assert!(
        compiled.overlays[group_a_index]
            .message_commands
            .iter()
            .flat_map(|command| &command.blocks)
            .any(|block| block.delta.x == Some(100.0) && block.duration == 1.0)
    );
    assert!(compiled.messages.iter().any(|event| {
        (event.beat - 1.0).abs() <= f32::EPSILON
            && event.message.starts_with("__songlua_overlay_fn_action_")
    }));
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == group_b_index
            && track.target == SongLuaOverlayUpdateTarget::Y
            && track
                .samples
                .iter()
                .any(|sample| sample.value == SongLuaOverlayUpdateValue::F32(20.0))
    }));
}

#[test]
fn compile_song_lua_replays_message_tweens_before_update_sampling() {
    let song_dir = test_dir("update-message-tween-replay");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper
local action = 1
local mod_actions = {
    {1, function() helper:linear(0.5):x(30) end},
}

return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self) helper = self end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                while action <= #mod_actions and beat >= mod_actions[action][1] do
                    mod_actions[action][2]()
                    action = action + 1
                end
                SCREENMAN:GetTopScreen():GetChild("PlayerP1"):rotationz(helper:GetX())
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Message Tween Replay");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.eases.iter().any(|ease| {
        ease.player == Some(1)
            && ease.target == SongLuaEaseTarget::PlayerRotationZ
            && (ease.from.abs() > 1.0e-4 || ease.to.abs() > 1.0e-4)
    }));
}

#[cfg(target_os = "windows")]
#[test]
fn compile_song_lua_uses_native_seeded_random() {
    let song_dir = test_dir("seeded-random");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/random.lua"),
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Seeded Random"),
    )
    .expect("compile native seeded fixture");
    assert_eq!(compiled.overlays.len(), 3);
    for (overlay, expected) in compiled.overlays.iter().zip([
        [
            -2.0,
            0.7203244895202072_f64 as f32,
            1.0,
            0.12812444777230592_f64 as f32,
        ],
        [
            0.7856989287092829_f64 as f32,
            0.3580021715717493_f64 as f32,
            445541548.0_f64 as f32,
            1.0,
        ],
        [
            -2.0,
            0.7203244895202072_f64 as f32,
            1.0,
            0.12812444777230592_f64 as f32,
        ],
    ]) {
        let state = &overlay.initial_state;
        assert_eq!([state.x, state.y, state.z, state.zoom], expected);
    }
}

#[test]
fn compile_song_lua_retargets_player_tween_destinations() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song-lua")
        .canonicalize()
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Player Tween Destinations");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 4.0;
    let compiled =
        test_compile_song_lua(&song_dir.join("player-tween-destination.lua"), &context).unwrap();
    for (beat, zoom_x, zoom_y) in [
        (1.5, 1.375, 1.0),
        (2.0, 1.25, 1.0),
        (2.5, 0.5625, 0.625),
        (3.5, 0.25, 0.5),
    ] {
        for (target, expected) in [
            (SongLuaEaseTarget::PlayerZoomX, zoom_x),
            (SongLuaEaseTarget::PlayerZoomY, zoom_y),
        ] {
            let ease = compiled.eases.iter().find(|ease| {
                ease.player == Some(1)
                    && ease.target == target
                    && (ease.start - beat).abs() < 0.00001
            });
            let actual = ease.map_or(1.0, |ease| ease.from);
            assert!(
                (actual - expected).abs() < 0.00001,
                "{target:?} at {beat}: {ease:?}"
            );
        }
    }
}

#[test]
fn compile_song_lua_samples_player_tween_starts() {
    let song_dir = test_dir("player-tween-starts");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/player-tween.lua"),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Player Tween Starts");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.1;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    // Native player-tween.lua: rendered transforms at frames 60 and 75.
    for (target, start, midpoint, finish) in [
        (SongLuaEaseTarget::PlayerRotationZ, 5.0, 1.25, 0.0),
        (SongLuaEaseTarget::PlayerRotationX, -30.0, -7.5, 0.0),
        (SongLuaEaseTarget::PlayerZoomY, 1.18, 1.045, 1.0),
        (SongLuaEaseTarget::PlayerSkewX, 0.1, 0.025, 0.0),
    ] {
        let eases: Vec<_> = compiled
            .eases
            .iter()
            .filter(|ease| ease.player == Some(1) && ease.target == target)
            .collect();
        for (beat, expected) in [(1.0, start), (1.25, midpoint)] {
            let ease = eases
                .iter()
                .find(|ease| (ease.start - beat).abs() < 0.00001)
                .unwrap();
            assert!(
                (ease.from - expected).abs() < 0.0001,
                "{target:?} at {beat}: {ease:?}"
            );
        }
        assert!(
            (eases.last().unwrap().to - finish).abs() < 0.0001,
            "{target:?} tail"
        );
    }
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.target == SongLuaEaseTarget::PlayerSkewX && ease.from == -0.1)
    );
    for target in [
        SongLuaEaseTarget::PlayerZoomX,
        SongLuaEaseTarget::PlayerZoomY,
        SongLuaEaseTarget::PlayerZoomZ,
    ] {
        let eases: Vec<_> = compiled
            .eases
            .iter()
            .filter(|ease| ease.player == Some(2) && ease.target == target)
            .collect();
        for (beat, expected) in [(1.0, 0.7), (1.25, 0.85)] {
            let ease = eases
                .iter()
                .find(|ease| (ease.start - beat).abs() < 0.00001)
                .unwrap();
            assert!(
                (ease.from - expected).abs() < 0.0001,
                "{target:?} at {beat}: {ease:?}"
            );
        }
        assert!((eases.last().unwrap().to - 1.0).abs() < 0.0001);
    }
    // Actor.h's non-positional Lua getters return destinations throughout.
    for track in &compiled.overlay_updates {
        match track.target {
            SongLuaOverlayUpdateTarget::Diffuse => {
                assert!(
                    track.samples.iter().all(|sample| sample.value
                        == SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, 0.5]))
                )
            }
            SongLuaOverlayUpdateTarget::X => assert!(
                track
                    .samples
                    .iter()
                    .all(|sample| sample.value == SongLuaOverlayUpdateValue::F32(1.0))
            ),
            SongLuaOverlayUpdateTarget::Y => assert!(
                track
                    .samples
                    .iter()
                    .all(|sample| sample.value == SongLuaOverlayUpdateValue::F32(0.0))
            ),
            _ => {}
        }
    }
    assert!(
        compiled
            .overlay_updates
            .iter()
            .all(|track| track.overlay_index < compiled.overlays.len())
    );
}

#[test]
fn compile_song_lua_samples_queued_particle_fades() {
    let song_dir = test_dir("queued-particle-fades");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/particle-fade.lua"),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Particle Fade");
    context.song_display_bpms = [132.0, 132.0];
    context.music_length_seconds = 16.0 * 60.0 / 132.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let alpha = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::Diffuse)
        .unwrap();
    let visible = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::Visible)
        .unwrap();
    // Captured by the native particle-fade.lua fixture, without RNG.
    for (frame, expected, shown) in [
        (130, 1.0, true),
        (132, 0.92, true),
        (136, 0.76, true),
        (150, 0.20, true),
        (155, 0.0, false),
        (362, 1.0, true),
        (368, 0.76, true),
        (387, 0.0, false),
    ] {
        let beat = frame as f32 * 132.0 / 3600.0;
        let sample = alpha
            .samples
            .iter()
            .find(|sample| (sample.time - beat).abs() < 0.00001)
            .unwrap();
        let SongLuaOverlayUpdateValue::Vec4(value) = sample.value else {
            panic!("alpha value");
        };
        assert!(
            (value[3] - expected).abs() < 0.0001,
            "frame {frame}: {sample:?}"
        );
        let sample = visible
            .samples
            .iter()
            .rev()
            .find(|sample| sample.time <= beat + 0.00001)
            .unwrap();
        assert_eq!(
            sample.value,
            SongLuaOverlayUpdateValue::Bool(shown),
            "frame {frame}"
        );
    }
}

#[test]
fn compile_song_lua_updates_read_current_action_tween_state() {
    let song_dir = test_dir("update-action-current-tween-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper
local result
local action = 1
local mod_actions = {
    {1, function() helper:linear(1):x(100) end},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Helper",
        InitCommand=function(self) helper = self self:x(0) end,
    },
    Def.Quad{
        Name="Result",
        InitCommand=function(self) result = self end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                while action <= #mod_actions and beat >= mod_actions[action][1] do
                    mod_actions[action][2]()
                    action = action + 1
                end
                result:x(helper:GetX())
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Current Action Tween State");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let result_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Result"))
        .unwrap();
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == result_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .unwrap();
    let midpoint = x
        .samples
        .iter()
        .min_by(|left, right| (left.time - 1.5).abs().total_cmp(&(right.time - 1.5).abs()))
        .unwrap();
    assert!((midpoint.time - 1.5).abs() <= 0.051);
    assert!(
        matches!(midpoint.value, SongLuaOverlayUpdateValue::F32(value) if (value - 50.0).abs() <= 5.1),
        "unexpected midpoint sample: {midpoint:?}"
    );
}

#[test]
fn compile_song_lua_updates_replay_tweens_queued_during_update() {
    let song_dir = test_dir("update-queued-tween-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper
local fired = false

return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self) helper = self self:x(0) end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if not fired and GAMESTATE:GetSongBeat() >= 1 then
                    helper:linear(0.2):x(90):sleep(1.4):linear(0.2):x(0)
                    fired = true
                end
                SCREENMAN:GetTopScreen():GetChild("PlayerP1"):rotationx(helper:GetX())
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Queued Tween State");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let max_rotation = compiled
        .eases
        .iter()
        .filter(|ease| ease.player == Some(1) && ease.target == SongLuaEaseTarget::PlayerRotationX)
        .flat_map(|ease| [ease.from, ease.to])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max_rotation >= 89.0,
        "queued tween plateau was not replayed: {max_rotation}"
    );
}

#[test]
fn compile_song_lua_update_tween_keeps_plateau_until_delayed_return() {
    let song_dir = test_dir("update-tween-plateau");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper
local fired = false

return Def.ActorFrame{
    Def.Quad{InitCommand=function(self) helper = self self:x(0) end},
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                if not fired and beat >= 79.95 then
                    helper:linear(60/160):x(40):sleep(45/160):decelerate(120/160):x(0)
                    fired = true
                end
                SCREENMAN:GetTopScreen():GetChild("PlayerP1"):rotationz(
                    helper:GetX() * math.sin(beat * math.pi)
                )
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Tween Plateau");
    context.song_display_bpms = [160.0, 160.0];
    context.music_length_seconds = 32.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let max_rotation = compiled
        .eases
        .iter()
        .filter(|ease| ease.player == Some(1) && ease.target == SongLuaEaseTarget::PlayerRotationZ)
        .flat_map(|ease| [ease.from.abs(), ease.to.abs()])
        .fold(0.0_f32, f32::max);
    assert!(
        max_rotation >= 39.5,
        "delayed return lost its full-amplitude plateau: {max_rotation}"
    );
}

#[test]
fn compile_song_lua_update_tween_replays_spring_curve() {
    let song_dir = test_dir("update-tween-spring");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper
local fired = false

return Def.ActorFrame{
    Def.Quad{InitCommand=function(self) helper = self self:x(0) end},
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if not fired and GAMESTATE:GetSongBeat() >= 1 then
                    helper:spring(0.5):x(1)
                    fired = true
                end
                SCREENMAN:GetTopScreen():GetChild("PlayerP1"):zoomy(helper:GetX())
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Tween Spring");
    context.song_display_bpms = [120.0, 120.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let max_zoom = compiled
        .eases
        .iter()
        .filter(|ease| ease.player == Some(1) && ease.target == SongLuaEaseTarget::PlayerZoomY)
        .flat_map(|ease| [ease.from, ease.to])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max_zoom > 1.05,
        "queued spring tween lost its overshoot: {max_zoom}"
    );
}

#[test]
fn compile_song_lua_action_boundaries_do_not_add_update_ticks() {
    let song_dir = test_dir("update-action-frame-clock");
    let script = |actions: &str| {
        r#"
mod_actions = { __ACTIONS__ }
local target
local updates = 0
return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                updates = updates + 1
                target:x(updates)
            end)
        end,
    },
}
"#
        .replace("__ACTIONS__", actions)
    };

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Action Frame Clock");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compile_final_x = |name: &str, actions: &str| {
        let entry = song_dir.join(name);
        fs::write(&entry, script(actions)).unwrap();
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        let target = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("Target"))
            .unwrap();
        let track = compiled
            .overlay_updates
            .iter()
            .find(|track| {
                track.overlay_index == target && track.target == SongLuaOverlayUpdateTarget::X
            })
            .unwrap();
        track
            .samples
            .last()
            .and_then(|sample| match sample.value {
                SongLuaOverlayUpdateValue::F32(value) => Some(value),
                _ => None,
            })
            .unwrap()
    };
    let baseline = compile_final_x("baseline.lua", "");
    let with_actions = compile_final_x(
        "actions.lua",
        "{0.25, function() end}, {0.50, function() end}, {0.75, function() end}",
    );
    assert_eq!(with_actions, baseline);
    assert_eq!(baseline, 61.0);
}

#[test]
fn compile_song_lua_first_update_write_is_a_step() {
    let song_dir = test_dir("first-update-write-step");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if GAMESTATE:GetSongBeat() >= 0.2 then target:x(100) end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "First Update Write Step");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let target_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Target"))
        .unwrap();
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .unwrap();
    assert!(x.samples.first().is_some_and(|sample| {
        sample.time >= 0.2 && sample.value == SongLuaOverlayUpdateValue::F32(100.0)
    }));
    assert!(
        x.samples
            .iter()
            .all(|sample| sample.value == SongLuaOverlayUpdateValue::F32(100.0))
    );
}

#[test]
fn compile_song_lua_holds_sparse_action_updates_until_the_next_action() {
    let song_dir = test_dir("sparse-action-overlay-updates");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
local action = 1
local mod_actions = {
    {1, function()
        target:accelerate(0.375)
        target:addx(100)
    end},
    {5, function()
        target:x(100)
        target:decelerate(0.75)
        target:x(20)
    end},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self self:x(0) end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                while action <= #mod_actions and beat >= mod_actions[action][1] do
                    mod_actions[action][2]()
                    action = action + 1
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Sparse Action Updates");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 7.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let (target_index, _) = compiled
        .overlays
        .iter()
        .enumerate()
        .find(|(_, overlay)| overlay.name.as_deref() == Some("Target"))
        .unwrap();
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .unwrap();
    let next = track.samples.partition_point(|sample| sample.time <= 4.0);
    assert_eq!(
        track.samples[next - 1].value,
        SongLuaOverlayUpdateValue::F32(100.0)
    );
    assert_eq!(
        track.samples[next].value,
        SongLuaOverlayUpdateValue::F32(100.0)
    );
}

#[test]
fn compile_song_lua_does_not_let_updates_rehide_restored_proxy() {
    let song_dir = test_dir("persistent-proxy-visibility");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
local action = 1
local mod_actions = {
    {0.5, function() target:visible(true) end, true},
    {1, function() target:visible(false) end, true},
    {2, function() target:visible(true) end, true},
}

return Def.ActorFrame{
    Def.ActorProxy{
        Name="PlayerProxy",
        InitCommand=function(self) target = self self:visible(false) end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                while action <= #mod_actions and beat >= mod_actions[action][1] do
                    mod_actions[action][2]()
                    action = action + 1
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Persistent Proxy Visibility");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let proxy_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("PlayerProxy"))
        .unwrap();
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == proxy_index
                && track.target == SongLuaOverlayUpdateTarget::Visible
        })
        .expect("proxy visibility update track should be captured");
    let at = |beat| {
        let next = track.samples.partition_point(|sample| sample.time <= beat);
        &track.samples[next.saturating_sub(1)].value
    };
    assert_eq!(at(1.0), &SongLuaOverlayUpdateValue::Bool(false));
    assert_eq!(at(2.0), &SongLuaOverlayUpdateValue::Bool(true));
    assert_eq!(at(2.5), &SongLuaOverlayUpdateValue::Bool(true));
}

#[test]
fn compile_song_lua_bounds_dense_update_overlay_sampling() {
    let song_dir = test_dir("dense-update-overlay-sampling");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local targets = {}
local root = Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for index, target in ipairs(targets) do
                target:x(beat + index)
            end
        end)
    end,
}
for index = 1, 32 do
    root[#root + 1] = Def.Quad{
        InitCommand=function(self)
            targets[#targets + 1] = self
        end,
    }
end
return root
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Dense Update Overlays");
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let max_samples = (context.music_length_seconds * 60.0).ceil() as usize + 2;
    assert_eq!(compiled.overlays.len(), 32);
    assert_eq!(compiled.overlay_updates.len(), 32);
    assert!(
        compiled
            .overlay_updates
            .iter()
            .all(|track| track.samples.len() <= max_samples)
    );
}

#[test]
fn compile_song_lua_samples_function_ease_endpoints_exactly() {
    let song_dir = test_dir("update-function-ease-boundaries");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local actor = Def.Quad{}
local level = 0
mods_ease = {
    {0.13, 0.37, 1, 0, function(value) level = value end, "len",
        function(t, b, c, d) return b + c * t / d end},
}
local action_done = false
local function Update()
    local beat = GAMESTATE:GetSongBeat()
    for _, ease in ipairs(mods_ease) do
        if beat >= ease[1] and beat <= ease[1] + ease[2] then
            ease[5](ease[7](beat - ease[1], ease[3], ease[4] - ease[3], ease[2]))
        end
    end
    actor:x(level)
end
return Def.ActorFrame{
    InitCommand=function(self) self:SetUpdateFunction(Update) end,
    actor,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Ease Boundary");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::X)
        .unwrap();
    assert!(x.samples.iter().any(|sample| {
        (sample.time - 0.5).abs() <= f32::EPSILON
            && sample.value == SongLuaOverlayUpdateValue::F32(0.0)
    }));
    assert_eq!(
        x.samples.last().map(|sample| &sample.value),
        Some(&SongLuaOverlayUpdateValue::F32(0.0))
    );
}

#[test]
fn compile_song_lua_keeps_zero_alpha_after_function_ease() {
    let song_dir = test_dir("update-function-alpha-persistence");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local flash
mod_actions = {
    {0, function() flash:diffusealpha(1) end, true},
}
mods_ease = {
    {0.13, 0.37, 1, 0, function(value) flash:diffusealpha(value) end, "len",
        function(t, b, c, d) return b + c * t / d end},
}
local function Update()
    local beat = GAMESTATE:GetSongBeat()
    if not action_done and beat >= mod_actions[1][1] then
        action_done = true
        mod_actions[1][2]()
    end
    for _, ease in ipairs(mods_ease) do
        if beat >= ease[1] and beat <= ease[1] + ease[2] then
            ease[5](ease[7](beat - ease[1], ease[3], ease[4] - ease[3], ease[2]))
        end
    end
end
return Def.ActorFrame{
    Def.Quad{
        Name="Flash",
        InitCommand=function(self) flash = self self:diffusealpha(0) end,
    },
    Def.ActorFrame{
        InitCommand=function(self) self:SetUpdateFunction(Update) end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Alpha Persistence");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let flash_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Flash"))
        .unwrap();
    let alpha = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == flash_index
                && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .unwrap();
    assert_eq!(
        alpha.samples.last().map(|sample| &sample.value),
        Some(&SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, 0.0]))
    );
    assert!(alpha.samples.iter().all(|sample| {
        sample.time <= 0.5 || sample.value != SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, 1.0])
    }));
}

#[test]
fn compile_song_lua_preserves_targets_abandoned_at_zero_ease_end() {
    let song_dir = test_dir("update-function-zero-end-reset");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local actor = Def.Quad{}
local level = 0
mods_ease = {
    {0.13, 0.37, 1, 0, function(value) level = value end, "len",
        function(t, b, c, d) return b + c * t / d end},
}
local function Update()
    local beat = GAMESTATE:GetSongBeat()
    for _, ease in ipairs(mods_ease) do
        if beat >= ease[1] and beat <= ease[1] + ease[2] then
            ease[5](ease[7](beat - ease[1], ease[3], ease[4] - ease[3], ease[2]))
        end
    end
    if level ~= 0 then
        actor:x(level * 80):skewx(level)
    end
end
return Def.ActorFrame{
    InitCommand=function(self) self:SetUpdateFunction(Update) end,
    actor,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Zero End Reset");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    for target in [
        SongLuaOverlayUpdateTarget::X,
        SongLuaOverlayUpdateTarget::SkewX,
    ] {
        let track = compiled
            .overlay_updates
            .iter()
            .find(|track| track.target == target)
            .unwrap();
        assert!(track.samples.iter().all(|sample| sample.time <= 0.5));
        assert!(matches!(
            track.samples.last().map(|sample| &sample.value),
            Some(SongLuaOverlayUpdateValue::F32(value)) if *value > 0.0
        ));
    }
}

#[test]
fn compile_song_lua_preserves_last_write_after_update_stops() {
    let song_dir = test_dir("update-function-post-end-reset");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local actor = Def.Quad{}
mods_ease = {
    {0.13, 0.37, 1, 0, function() end, "len",
        function(t, b, c, d) return b + c * t / d end},
}
local function Update()
    local beat = GAMESTATE:GetSongBeat()
    for _, ease in ipairs(mods_ease) do
        if beat >= ease[1] and beat <= ease[1] + ease[2] then
            ease[5](ease[7](beat - ease[1], ease[3], ease[4] - ease[3], ease[2]))
        end
    end
    if beat >= 0.13 and beat <= 0.5 then
        actor:x(80)
    end
end
return Def.ActorFrame{
    InitCommand=function(self) self:SetUpdateFunction(Update) end,
    actor,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Post End Reset");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::X)
        .unwrap();
    assert!(track.samples.iter().all(|sample| sample.time <= 0.5));
    assert_eq!(
        track.samples.last().map(|sample| &sample.value),
        Some(&SongLuaOverlayUpdateValue::F32(80.0))
    );
}

#[test]
fn compile_song_lua_keeps_static_column_rotation() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&dir, "Static column rotation");
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = test_compile_song_lua(&dir.join("static-column-rotation.lua"), &context)
        .expect("static rotation control");
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..Default::default()
        },
        &[],
    );
    for player in 0..2 {
        let windows = crate::gameplay::build_song_lua_column_offset_windows_for_player(
            &compiled, &timing, player, 0.0,
        );
        for second in [0.0, 0.5, 2.0, 4.0] {
            let (transforms, _) =
                deadsync_gameplay::song_lua_column_transforms(&windows, 4, second);
            for rotation in &transforms[3][..4] {
                assert!(
                    (*rotation - 180.0).abs() < 1e-5,
                    "player {player} at {second}: {transforms:?}"
                );
            }
        }
    }
}

#[test]
fn compile_song_lua_keeps_position_splines_separate_from_reverse() {
    let song_dir = test_dir("update-position-spline-reverse");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
            for _, column in ipairs(nf:GetColumnActors()) do
                local handler = column:GetPosHandler()
                local zoom_handler = column:GetZoomHandler()
                if beat >= 1 and beat <= 3 then
                    handler:SetSplineMode("NoteColumnSplineMode_Position")
                    local spline = handler:GetSpline()
                    spline:SetSize(2)
                    spline:SetPoint(1, {0, 100, 0})
                    spline:SetPoint(2, {0, 0, 0})
                    spline:Solve()
                    zoom_handler:SetSplineMode("NoteColumnSplineMode_Position")
                    local zoom = zoom_handler:GetSpline()
                    local amount = math.abs(beat - 2)
                    zoom:SetSize(2)
                    zoom:SetPoint(1, {amount, amount, amount})
                    zoom:SetPoint(2, {amount, amount, amount})
                    zoom:Solve()
                else
                    handler:SetSplineMode("NoteColumnSplineMode_Disabled")
                    zoom_handler:SetSplineMode("NoteColumnSplineMode_Disabled")
                end
            end
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Position Spline Reverse");
    context.music_length_seconds = 2.5;
    context.song_display_bpms = [60.0; 2];
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(
        !compiled
            .eases
            .iter()
            .any(|ease| ease.target == SongLuaEaseTarget::Mod("reverse".into()))
    );
    assert_eq!(compiled.column_splines.len(), 4);
    let track = &compiled.column_splines[0];
    let active = track.at_second(1.5).expect("active Position frame");
    assert_eq!(
        active
            .position
            .as_ref()
            .expect("position")
            .view()
            .receptor(1.5)[1],
        100.0
    );
    assert!(active.zoom.as_ref().expect("zoom").view().receptor(1.5)[0] < 0.99);
    assert!(track.at_second(0.5).expect("baseline").position.is_none());
    assert!(
        compiled
            .column_offsets
            .iter()
            .filter(|window| window.target == SongLuaColumnTransformTarget::OffsetX)
            .all(|window| window.from_y == 0.0 && window.to_y == 0.0)
    );
}

#[test]
fn compile_song_lua_samples_update_player_mods_and_transforms() {
    let song_dir = test_dir("set-update-function-player-mods");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= 2 and beat <= 4 then
                local amount = (beat - 2) / 2
                options:FromString(
                    tostring(amount * 100) .. " drunk," ..
                    tostring(amount * 50) .. " suddenoffset," ..
                    tostring(amount * 75) .. " randomspeed"
                )
                player:rotationx(amount * 180):skewx(amount)
            end
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Player Mods");
    context.song_display_bpms = [120.0, 120.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "drunk"))
    );
    assert!(compiled.eases.iter().any(
        |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "suddenoffset")
    ));
    assert!(compiled.eases.iter().any(
        |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "randomspeed")
    ));
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerRotationX))
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerSkewX))
    );
}

#[test]
fn compile_song_lua_keeps_chart_easing_globals() {
    let song_dir = test_dir("chart-easing-global");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/easing-global.lua"),
    )
    .expect("write chart easing guard");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Chart Easing"),
    )
    .expect("chart must load its own guarded nonlinear easing functions");
}

#[test]
fn compile_song_lua_replays_runtime_mod_tables() {
    let song_dir = test_dir("exact-runtime-mod-tables");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
mods = {
    {2, 2, "*100 50 flip", "len"},
}
mods_ease = {
    {4, 2, 0, 100, "mini", "len", ease.linear},
}

return Def.ActorFrame{
    OnCommand=function(self)
        local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for _, mod in pairs(mods) do
                if beat >= mod[1] and beat <= mod[1] + mod[2] then
                    options:FromString(mod[3])
                end
            end
            for _, mod in pairs(mods_ease) do
                if beat >= mod[1] and beat <= mod[1] + mod[2] then
                    options:Mini((beat - mod[1]) / mod[2], 9999)
                end
            end
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Exact Runtime Mod Tables");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..Default::default()
        },
        &[],
    );
    let (windows, unsupported) =
        crate::gameplay::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
    assert_eq!(unsupported, 0);
    let mut runtime = deadsync_gameplay::GameplayAttackRuntimeState::new(
        [Vec::new(), Vec::new()],
        [windows, Vec::new()],
    );
    for (second, mini) in [(1.0, 0.0), (2.5, 50.0), (3.5, 100.0)] {
        // Probe this target at an independent song position.
        runtime.reset_window_times();
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            deadsync_gameplay::SongLuaPlayerTransform::default(),
        );
        assert_eq!(runtime.visual[0].flip, Some(0.5));
        assert!((runtime.mini_percent[0].unwrap_or(0.0) - mini).abs() < 0.0001);
    }
}

#[test]
fn compile_song_lua_replays_mixed_runtime_mod_eases() {
    let song_dir = test_dir("mixed-exact-runtime-mod-eases");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
mods = {
    {0, 9999, "*1000 no dizzy", "end"},
}
mods_ease = {
    {2, 2, 0, 500, "dizzy", "len", ease.linear},
    {6, 2, 0, 90, function(value)
        SCREENMAN:GetTopScreen():GetChild("PlayerP1"):rotationx(value)
    end, "len", ease.linear},
}

return Def.ActorFrame{
    OnCommand=function(self)
        local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for _, mod in pairs(mods) do
                if beat >= mod[1] and beat <= mod[2] then
                    options:FromString(mod[3])
                end
            end
            for _, mod in pairs(mods_ease) do
                if beat >= mod[1] and beat <= mod[1] + mod[2] then
                    local value = mod[3] + (mod[4] - mod[3]) * (beat - mod[1]) / mod[2]
                    if type(mod[5]) == "function" then
                        mod[5](value)
                    else
                        options:Dizzy(value / 100, 10000)
                    end
                end
            end
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Mixed Exact Mod Eases");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 10.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..Default::default()
        },
        &[],
    );
    let (windows, unsupported) =
        crate::gameplay::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
    assert_eq!(unsupported, 0);
    let mut runtime = deadsync_gameplay::GameplayAttackRuntimeState::new(
        [Vec::new(), Vec::new()],
        [windows, Vec::new()],
    );
    let mut transform = deadsync_gameplay::SongLuaPlayerTransform::default();
    for (second, dizzy, rotation) in [
        (1.5, 2.5, 0.0),
        (2.5, 0.0, 0.0),
        (3.5, 0.0, 45.0),
        (4.5, 0.0, 90.0),
    ] {
        // Probe this target at an independent song position.
        runtime.reset_window_times();
        if let Some(next) = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
        assert!((runtime.visual[0].dizzy.unwrap_or(0.0) - dizzy).abs() < 0.0001);
        assert!((transform.rotation_x - rotation).abs() < 0.0001);
    }
}

#[test]
fn compile_song_lua_keeps_last_update_value() {
    let song_dir = test_dir("set-update-function-player-transform-tail");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= 2 and beat < 3 then
                local amount = beat - 2
                player:rotationx(180 * (1 - amount) * (1 - amount))
            end
            if beat >= 2 and beat < 4 then
                local amount = (beat - 2) / 2
                player:rotationz(-360 * amount)
                      :skewx((1 - amount) * (1 - amount))
            end
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Transform Tail");
    context.song_display_bpms = [120.0, 120.0];
    context.music_length_seconds = 75.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    // The conditional writer stops before the endpoint. Native Actor keeps
    // its last 60 Hz write; ending the callback does not reset its state.
    for (target, expected) in [
        (SongLuaEaseTarget::PlayerRotationX, 0.2),
        (SongLuaEaseTarget::PlayerRotationZ, -354.0),
        (SongLuaEaseTarget::PlayerSkewX, 1.0 / 3600.0),
    ] {
        let tail = compiled
            .eases
            .iter()
            .rev()
            .find(|ease| ease.target == target)
            .expect("update function should compile player transform samples");
        assert!(
            (tail.to - expected).abs() <= 1.0e-6,
            "last authored transform was not retained: {target:?} ({})",
            tail.to
        );
    }
}

#[test]
fn compile_song_lua_clears_update_function_with_nil() {
    let song_dir = test_dir("set-update-function-clear");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{Name="Panel"},
    Def.ActorFrame{
        OnCommand=function(self)
            local actor = self:GetParent():GetChild("Panel")
            self:SetUpdateFunction(function()
                actor:x(99)
                mod_actions = {
                    {1, "should-not-run", true},
                }
            end)
            self:SetUpdateFunction(nil)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Clear Song"),
    )
    .unwrap();
    assert!(compiled.messages.is_empty());
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 0.0);
}

#[test]
fn compile_song_lua_extracts_local_update_mod_actions() {
    let song_dir = test_dir("local-update-mod-actions");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local mod_actions = {
    {2, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:linear(1):x(SCREEN_CENTER_X + 24):z(3):zoom(0.5):rotationz(20)
        end
    end, true},
}
local curaction = 1
local mod_firstSeenBeat = 0

local domods = function()
    local beatupdate = GAMESTATE:GetSongBeat()
    if beatupdate > mod_firstSeenBeat + 0.1 then
        while curaction <= table.getn(mod_actions) and beatupdate >= mod_actions[curaction][1] do
            if type(mod_actions[curaction][2]) == "function" then
                mod_actions[curaction][2]()
            end
            curaction = curaction + 1
        end
    end
end

return Def.ActorFrame{
    InitCommand=function(self)
        table.sort(mod_actions, function(a, b) return a[1] < b[1] end)
    end,
    OnCommand=function(self)
        self:SetUpdateFunction(domods)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Local Update Mod Actions");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.player_actors[0].message_commands.is_empty());
    assert!(compiled.messages.is_empty());
    for (target, expected) in [
        (SongLuaEaseTarget::PlayerX, 344.0),
        (SongLuaEaseTarget::PlayerZ, 3.0),
        (SongLuaEaseTarget::PlayerZoomX, 0.5),
        (SongLuaEaseTarget::PlayerRotationZ, 20.0),
    ] {
        let ease = compiled
            .eases
            .iter()
            .rev()
            .find(|ease| ease.player == Some(1) && ease.target == target)
            .expect("sampled player action");
        assert_eq!(ease.to, expected);
        assert!(ease.start >= 2.0);
    }
}

#[test]
fn compile_judgment_sheet_changes_reach_playback() {
    let song_dir = test_dir("judgment-sheet-changes");
    for name in ["Normal", "Fake"] {
        image::RgbaImage::new(128, 384)
            .save(song_dir.join(format!("{name} 2x6.png")))
            .unwrap();
    }
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local sprites = {}
return Def.ActorFrame{
    OnCommand=function(self)
        for i, name in ipairs({"PlayerP1", "PlayerP2"}) do
            local judgment = SCREENMAN:GetTopScreen():GetChild(name):GetChild("Judgment")
            rec_print_children(judgment)
            sprites[i] = judgment:GetChild("JudgmentWithOffsets")
            sprites[i]:Load("Normal 2x6.png")
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local seconds = GAMESTATE:GetCurMusicSeconds()
            if phase == 0 and seconds >= 1 then
                phase = 1
                for _, sprite in ipairs(sprites) do sprite:Load("Fake 2x6.png") end
            elseif phase == 1 and seconds >= 2 then
                phase = 2
                for _, sprite in ipairs(sprites) do sprite:Load("Normal 2x6.png") end
            end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Judgment Sheets");
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    for player in &compiled.player_actors {
        let child = &player.judgment;
        assert_eq!(child.textures.len(), 3, "{:?}", child.textures);
        for (index, texture) in child.textures.iter().enumerate() {
            assert!((texture.second - index as f32).abs() < 1.0 / 59.0);
            assert_eq!(texture.frame_size, [64.0, 64.0]);
            assert_eq!((texture.frame_cols, texture.frame_rows), (2, 6));
        }
        for (second, expected) in [(0.0, "Normal"), (1.1, "Fake"), (2.1, "Normal")] {
            let texture = crate::judgment_texture_at(child, second).unwrap();
            assert_eq!(texture.path, song_dir.join(format!("{expected} 2x6.png")));
        }
        assert!(crate::judgment_texture_at(child, -0.01).is_none());
    }
}

#[test]
fn compile_song_lua_keeps_combo_child_callback() {
    let root = test_dir("combo-child-current-theme");
    let entry = root.join("combo-child.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/combo-child.lua"),
    )
    .unwrap();
    // Simply Love returns a leaf Actor when the current theme has no combo
    // fonts. Provide the directory entries this child-lookup test needs.
    let font = root
        .join(crate::SONG_LUA_THEME_PATH_PREFIX)
        .join("Fonts/_Combo Fonts/Wendy");
    fs::create_dir_all(&font).unwrap();
    fs::write(font.join("Wendy.ini"), "[main]\n").unwrap();
    fs::write(font.join("Wendy 4x4.png"), b"").unwrap();
    let mut context = SongLuaCompileContext::new(&root, "Combo Child Callback");
    context.music_length_seconds = 2.0;
    context.players = std::array::from_fn(|_| SongLuaPlayerContext {
        enabled: true,
        ..Default::default()
    });
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{:?}",
        compiled.info
    );
    for (player, expected) in [(1, 300.0), (2, 400.0)] {
        let movement = compiled
            .eases
            .iter()
            .rev()
            .find(|ease| ease.player == Some(player) && ease.target == SongLuaEaseTarget::PlayerX)
            .expect("player movement after Combo child lookup");
        assert_eq!(movement.to, expected);
    }
}

#[test]
fn compile_song_lua_captures_player_hud_child_actions() {
    let song_dir = test_dir("player-hud-child-actions");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
local judgment = player:GetChild("Judgment")
local combo = player:GetChild("Combo")
mod_actions = {
    {8, function()
        judgment:visible(false)
        combo:visible(false)
    end, true},
    {24, function() combo:visible(true) end, true},
    {71, function() judgment:visible(true) end, true},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player HUD Child Actions"),
    )
    .unwrap();

    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 3);
    assert_eq!(compiled.player_actors[0].judgment.message_commands.len(), 2);
    assert_eq!(compiled.player_actors[0].combo.message_commands.len(), 2);
    assert_eq!(
        compiled.player_actors[0].judgment.message_commands[0].blocks[0]
            .delta
            .visible,
        Some(false)
    );
    assert_eq!(
        compiled.player_actors[0].judgment.message_commands[1].blocks[0]
            .delta
            .visible,
        Some(true)
    );
    assert_eq!(
        compiled.player_actors[0].combo.message_commands[1].blocks[0]
            .delta
            .visible,
        Some(true)
    );
}

#[test]
fn compile_song_lua_guards_recursive_update_commands() {
    let song_dir = test_dir("recursive-update");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local runs = 0

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("Update")
        end,
        UpdateCommand=function(self)
            runs = runs + 1
            mod_actions = {
                {runs, "LoopSafe", true},
            }
            self:sleep(1/60)
            self:queuecommand("Update")
        end,
    },
}
"#,
    )
    .unwrap();

    let context = SongLuaCompileContext::new(&song_dir, "Recursive Update Song");
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].beat, 1.0);
    assert_eq!(compiled.messages[0].message, "LoopSafe");
}

#[test]
fn compile_song_lua_runs_recursive_updates_at_queued_interval() {
    let song_dir = test_dir("recursive-update-interval");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local ball

return Def.ActorFrame{
    Def.Quad{
        Name="Ball",
        OnCommand=function(self)
            ball = self
            self:x(0)
        end,
    },
    Def.Actor{
        OnCommand=function(self)
            self:queuecommand("Update")
        end,
        UpdateCommand=function(self)
            ball:addx(1)
            self:sleep(0.02)
            self:queuecommand("Update")
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Recursive Update Interval");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let ball_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Ball"))
        .unwrap();
    let max_x = compiled
        .overlay_updates
        .iter()
        .filter(|track| {
            track.overlay_index == ball_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .flat_map(|track| &track.samples)
        .filter_map(|sample| match sample.value {
            SongLuaOverlayUpdateValue::F32(value) => Some(value),
            _ => None,
        })
        .fold(f32::NEG_INFINITY, f32::max);
    let x_at_tenth = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == ball_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .and_then(|track| {
            track
                .samples
                .iter()
                .rev()
                .find(|sample| sample.time <= 0.100_01)
        })
        .and_then(|sample| match sample.value {
            SongLuaOverlayUpdateValue::F32(value) => Some(value),
            _ => None,
        })
        .expect("the recurring update should emit an x sample by 100 ms");
    assert_eq!(
        x_at_tenth, 6.0,
        "the immediate update plus five 20 ms updates must run by 100 ms"
    );
    assert!(
        max_x >= 40.0,
        "expected the 20 ms loop to run at least 40 times, got {max_x}"
    );
}

#[test]
fn startup_self_queue_moves() {
    let song_dir = test_dir("self-queued-startup-move");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.Quad{
    Name="Ball",
    InitCommand=function(self) self:x(0) end,
    OnCommand=function(self) self:playcommand("Move") end,
    MoveCommand=function(self)
        self:x(self:GetX() + 1)
        if self:GetX() < 6 then self:sleep(0.007):queuecommand("Move") end
    end,
}
"#,
    )
    .expect("write queued movement control");
    let mut context = SongLuaCompileContext::new(&song_dir, "Self-queued Startup Move");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 0.25;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile queued movement");
    let index = compiled
        .overlays
        .iter()
        .position(|o| o.name.as_deref() == Some("Ball"))
        .expect("movement actor");
    assert_eq!(compiled.overlays[index].initial_state.x, 1.0);
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::X)
        .expect("the startup queue must continue moving its own actor");
    for (time, expected) in [
        (1.0 / 60.0, 3.0),
        (2.0 / 60.0, 5.0),
        (0.05, 6.0),
        (0.25, 6.0),
    ] {
        let value = track
            .samples
            .iter()
            .rev()
            .find(|sample| sample.time <= time + 0.000_001)
            .expect("queued movement sample");
        assert!(
            matches!(value.value, SongLuaOverlayUpdateValue::F32(x) if x == expected),
            "movement at {time} must be {expected}, got {:?}",
            value.value
        );
    }
}

#[test]
fn compile_song_lua_runs_pool_actions_in_order() {
    for scope in ["", "local "] {
        let song_dir = test_dir(if scope.is_empty() {
            "global-pool-actions"
        } else {
            "local-pool-actions"
        });
        let entry = song_dir.join("default.lua");
        let script = r#"
local target
local pool = {}
local calls = 0
local next_action = 1
SCOPE mod_actions = {
    {0.5, function()
        calls = calls + 1
        pool[1].value = pool[1].value + calls
        target:x(pool[1].value):y(calls)
    end},
    {1, function()
        calls = calls + 1
        pool[1].value = pool[1].value + calls
        target:x(pool[1].value):y(calls)
    end},
}
return Def.ActorFrame{
    Def.Quad{InitCommand=function(self) target=self end},
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat > 0.2 and not pool[1] then pool[1]={value=10} end
            while next_action <= #mod_actions and beat >= mod_actions[next_action][1] do
                mod_actions[next_action][2]()
                next_action = next_action + 1
            end
        end)
    end,
}
"#
        .replace("SCOPE ", scope);
        fs::write(&entry, script).unwrap();
        let mut context = SongLuaCompileContext::new(&song_dir, "Pool Actions");
        context.song_display_bpms = [60.0, 60.0];
        context.music_length_seconds = 1.5;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(
            compiled
                .info
                .unsupported_function_action_captures
                .is_empty()
        );
        for (target, expected) in [
            (SongLuaOverlayUpdateTarget::X, vec![11.0, 13.0]),
            (SongLuaOverlayUpdateTarget::Y, vec![1.0, 2.0]),
        ] {
            let track = compiled
                .overlay_updates
                .iter()
                .find(|track| track.target == target)
                .expect("pool action track");
            let values = track
                .samples
                .iter()
                .filter_map(|sample| match sample.value {
                    SongLuaOverlayUpdateValue::F32(value) if value != 0.0 => Some(value),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                expected.iter().all(|value| values.contains(value)),
                "{scope}{target:?}: {values:?}"
            );
            assert_eq!(values.last(), expected.last());
        }
    }
}

#[test]
fn compile_song_lua_times_runtime_sound_calls() {
    let song_dir = test_dir("runtime-action-sound");
    fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local action = 1
mod_actions = {{0.51, function()
    SOUND:PlayOnce("effect.ogg")
    SOUND:PlayOnce("effect.ogg")
end}}
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            while action <= #mod_actions and GAMESTATE:GetSongBeat() >= mod_actions[action][1] do
                mod_actions[action][2]()
                action = action + 1
            end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Sound");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let events = compiled
        .messages
        .iter()
        .filter(|event| event.message.starts_with("__songlua_sound_call_"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].beat, events[1].beat);
    assert!((events[0].beat - 31.0 / 60.0).abs() < 0.000_001);
    assert_eq!(compiled.sound_paths, vec![song_dir.join("effect.ogg")]);
}

#[test]
fn compile_song_lua_captures_runtime_message_sounds_once() {
    let song_dir = test_dir("runtime-message-sound-once");
    fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local action = 1
mod_actions={{0.51, "Hit", true}}
return Def.ActorFrame{
    HitMessageCommand=function(self) SOUND:PlayOnce("effect.ogg") end,
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            while action <= #mod_actions and GAMESTATE:GetSongBeat() >= mod_actions[action][1] do
                MESSAGEMAN:Broadcast(mod_actions[action][2])
                action = action + 1
            end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Message Sound");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let sounds = compiled
        .overlays
        .iter()
        .filter(|overlay| matches!(overlay.kind, SongLuaOverlayKind::Sound { .. }))
        .collect::<Vec<_>>();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].message_commands.len(), 1);
    assert!(
        sounds[0].message_commands[0]
            .message
            .starts_with("__songlua_sound_call_")
    );
    let event = compiled
        .messages
        .iter()
        .find(|event| event.message == sounds[0].message_commands[0].message)
        .unwrap();
    assert!((event.beat - 31.0 / 60.0).abs() < 0.000_001);
}

#[test]
fn compile_song_lua_reports_runtime_action_errors() {
    let song_dir = test_dir("runtime-action-error");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
local action = 1
mod_actions={{0.5, function()
    target:x(7)
    error("broken pool callback")
end}}
return Def.ActorFrame{
    Def.Quad{InitCommand=function(self) target=self end},
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            while action <= #mod_actions and GAMESTATE:GetSongBeat() >= mod_actions[action][1] do
                mod_actions[action][2]()
                action = action + 1
            end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Action Error");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 1);
    assert_eq!(compiled.info.unsupported_perframe_captures.len(), 1);
    assert!(compiled.info.unsupported_perframe_captures[0].contains("broken pool callback"));
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::X)
        .unwrap();
    assert_eq!(
        track.samples.last().unwrap().value,
        SongLuaOverlayUpdateValue::F32(7.0)
    );
}

#[test]
fn compile_song_lua_omits_unreached_runtime_actions() {
    let song_dir = test_dir("unreached-runtime-actions");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions={{0.5, "Show", true}}
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self) self:visible(false) end,
        ShowMessageCommand=function(self) self:visible(true) end,
    },
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            if GAMESTATE:GetSongBeat() > 0.2 then self:SetUpdateFunction(nil) end
            for _, action in ipairs(mod_actions) do
                if GAMESTATE:GetSongBeat() >= action[1] then MESSAGEMAN:Broadcast(action[2]) end
            end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Unreached Runtime Actions");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.messages.is_empty());
    assert!(!compiled.overlays[0].initial_state.visible);
    assert!(
        compiled
            .overlay_updates
            .iter()
            .all(|track| track.target != SongLuaOverlayUpdateTarget::Visible)
    );
}

#[test]
fn compile_song_lua_captures_queued_startup_sound_once() {
    let song_dir = test_dir("queued-startup-sound");
    fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local action = 1
mod_actions={{0, function() SOUND:PlayOnce("effect.ogg") end}}
return Def.ActorFrame{
    OnCommand=function(self) self:queuecommand("Update") end,
    UpdateCommand=function(self)
        while action <= #mod_actions and GAMESTATE:GetSongBeat() >= mod_actions[action][1] do
            mod_actions[action][2]()
            action = action + 1
        end
        self:sleep(1/60):queuecommand("Update")
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Startup Sound");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let events = compiled
        .messages
        .iter()
        .filter(|event| event.message.starts_with("__songlua_sound_call_"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert!((events[0].beat - 1.0 / 60.0).abs() < 0.000_001);
}

#[test]
fn compile_song_lua_keeps_immediate_startup_sound_time() {
    let song_dir = test_dir("immediate-startup-sound");
    fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local sent = false
return Def.ActorFrame{
    OnCommand=function(self)
        SOUND:PlayOnce("effect.ogg")
        self:SetUpdateFunction(function()
            if not sent then SOUND:PlayOnce("effect.ogg"); sent = true end
        end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Immediate Startup Sound");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 0.5;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let events = compiled
        .messages
        .iter()
        .filter(|event| event.message.starts_with("__songlua_sound_call_"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event.beat == 0.0));
}

#[test]
fn compile_song_lua_does_not_replay_global_actions_before_update_loop() {
    let song_dir = test_dir("global-actions-update-authority");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
local action = 1

mod_actions = {
    {0, "BossOn", true},
    {1, function()
        target:linear(1)
        target:x(100)
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self)
            target = self
            self:x(0)
            self:visible(false)
        end,
        BossOnMessageCommand=function(self)
            self:visible(true)
        end,
    },
    Def.Actor{
        OnCommand=function(self)
            self:queuecommand("Update")
        end,
        UpdateCommand=function(self)
            local beat = GAMESTATE:GetSongBeat()
            while action <= #mod_actions and beat >= mod_actions[action][1] do
                if type(mod_actions[action][2]) == "string" then
                    MESSAGEMAN:Broadcast(mod_actions[action][2])
                else
                    mod_actions[action][2]()
                end
                action = action + 1
            end
            self:sleep(0.02)
            self:queuecommand("Update")
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Global Actions Update Authority");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(
        compiled
            .messages
            .iter()
            .all(|event| !event.message.starts_with("__songlua_overlay_fn_action_")),
        "the sampled UpdateCommand must be the sole action timing authority"
    );
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.message == "BossOn" && (event.beat - 1.0 / 60.0).abs() < 0.000_001),
        "named broadcasts must remain available to the renderer"
    );
    let target_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Target"))
        .unwrap();
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target_index && track.target == SongLuaOverlayUpdateTarget::X
        })
        .expect("the runtime action tween should produce an x track");
    let first_changed = track
        .samples
        .iter()
        .position(|sample| sample.value != SongLuaOverlayUpdateValue::F32(0.0))
        .expect("the runtime tween should eventually move the actor");
    let before = &track.samples[first_changed - 1];
    let first = &track.samples[first_changed];
    assert!(
        before.value == SongLuaOverlayUpdateValue::F32(0.0) && first.time > 1.0,
        "the runtime tween must start from the untouched initial state: {before:?}, {first:?}"
    );
}

#[test]
fn compile_song_lua_retimes_runtime_messages_to_update_wake() {
    let song_dir = test_dir("runtime-message-update-wake");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local action = 1

mod_actions = {
    {1.01, "BossOn", true},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Boss",
        InitCommand=function(self) self:visible(false) end,
        BossOnMessageCommand=function(self) self:visible(true) end,
    },
    Def.Actor{
        OnCommand=function(self) self:queuecommand("Update") end,
        UpdateCommand=function(self)
            local beat = GAMESTATE:GetSongBeat()
            while action <= #mod_actions and beat >= mod_actions[action][1] do
                MESSAGEMAN:Broadcast(mod_actions[action][2])
                action = action + 1
            end
            self:sleep(0.02):queuecommand("Update")
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Message Update Wake");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let message = compiled
        .messages
        .iter()
        .find(|event| event.message == "BossOn")
        .expect("runtime action should retain its named broadcast");
    assert_eq!(
        compiled
            .messages
            .iter()
            .filter(|event| event.message == "BossOn")
            .count(),
        1
    );

    assert!(
        message.beat > 1.01 && message.beat <= 1.06,
        "runtime message must use the first recurring update wake after beat 1.01, got {}",
        message.beat
    );
}

#[test]
fn compile_song_lua_keeps_runtime_broadcasts() {
    let song_dir = test_dir("runtime-local-messages");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local fired = false
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            if fired or GAMESTATE:GetSongBeat() < 1.01 then return end
            fired = true
            MESSAGEMAN:Broadcast("Pulse")
            MESSAGEMAN:Broadcast("Pulse")
            MESSAGEMAN:Broadcast("Tint", {alpha=0.25})
        end)
    end,
    Def.Quad{
        Name="Light",
        PulseMessageCommand=function(self) self:x(self:GetX()+1) end,
        TintMessageCommand=function(self, params)
            if params and params.alpha then self:diffusealpha(params.alpha) end
        end,
    },
}
"#,
    )
    .expect("write runtime broadcast fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime local messages");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile runtime broadcasts");
    let events = compiled
        .messages
        .iter()
        .filter(|event| event.message == "Pulse" || event.message == "Tint")
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events
            .iter()
            .map(|event| event.message.as_str())
            .collect::<Vec<_>>(),
        ["Pulse", "Pulse", "Tint"]
    );
    assert!(
        events
            .iter()
            .all(|event| event.beat > 1.01 && event.beat <= 1.06)
    );
    assert!(compiled.overlay_updates.iter().any(|track| track.target == SongLuaOverlayUpdateTarget::Diffuse && track.samples.iter().any(|sample| matches!(&sample.value, SongLuaOverlayUpdateValue::Vec4(color) if color[3] == 0.25))), "capture parameter-dependent listener state");
}

#[test]
fn compile_song_lua_keeps_startup_broadcasts() {
    let song_dir = test_dir("startup-broadcasts");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local fired = false
return Def.ActorFrame{
    OnCommand=function(self)
        MESSAGEMAN:Broadcast("Immediate")
        self:queuecommand("Bind")
    end,
    BindCommand=function(self)
        MESSAGEMAN:Broadcast("Startup")
        self:SetUpdateFunction(function()
            if fired then return end
            fired = true
            MESSAGEMAN:Broadcast("Ready")
        end)
    end,
    Def.Quad{
        ImmediateMessageCommand=function(self) self:x(5) end,
        StartupMessageCommand=function(self) self:x(10) end,
        ReadyMessageCommand=function(self) self:y(20) end,
    },
}
"#,
    )
    .expect("write startup broadcast fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Startup broadcasts");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile startup broadcasts");
    let events = compiled
        .messages
        .iter()
        .filter(|event| matches!(event.message.as_str(), "Immediate" | "Startup" | "Ready"))
        .map(|event| (event.beat, event.message.as_str()))
        .collect::<Vec<_>>();
    let first_beat = crate::song_beat_at_elapsed_seconds(1.0 / 60.0, &context);
    assert_eq!(events, [(first_beat, "Startup"), (first_beat, "Ready")]);
    let quad = compiled
        .overlays
        .iter()
        .find(|overlay| matches!(overlay.kind, SongLuaOverlayKind::Quad))
        .expect("startup quad");
    assert_eq!(quad.initial_state.x, 5.0);
}

#[test]
fn compile_song_lua_classifies_player_transform_function_eases() {
    let song_dir = test_dir("function-ease");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil
prefix_globals = {}

return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals.ease = {
            {3, 1, 320, 360, function(x) if target then target:x(x) end end, "len", ease.outQuad},
            {4, 1, 240, 210, function(x) if target then target:y(x) end end, "len", ease.outQuad},
            {5, 1, 0, -120, function(x) if target then target:z(x) end end, "len", ease.outQuad},
            {6, 2, 0, 20, function(x) if target then target:rotationx(x) end end, "len", ease.outQuad},
            {8, 2, 0, 10, function(x) if target then target:rotationz(x) end end, "len", ease.inOutQuad},
            {12, 1, 0, 0.15, function(x) if target then target:skewx(x) end end, "len", ease.outQuad},
            {13, 1, 0, 0.2, function(x) if target then target:skewy(x) end end, "len", ease.outQuad},
            {14, 1, 1, 0.75, function(x) if target then target:zoom(x) end end, "len", ease.outQuad},
            {15, 1, 1, 1.25, function(x) if target then target:zoomz(x) end end, "len", ease.outQuad},
        }
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("BindTarget")
        end,
        BindTargetCommand=function(self)
            target = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Function Ease Song"),
    )
    .unwrap();
    assert_eq!(compiled.eases.len(), 9);
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert!(matches!(
        compiled.eases[0].target,
        SongLuaEaseTarget::PlayerX
    ));
    assert!(matches!(
        compiled.eases[1].target,
        SongLuaEaseTarget::PlayerY
    ));
    assert!(matches!(
        compiled.eases[2].target,
        SongLuaEaseTarget::PlayerZ
    ));
    assert!(matches!(
        compiled.eases[3].target,
        SongLuaEaseTarget::PlayerRotationX
    ));
    assert!(matches!(
        compiled.eases[4].target,
        SongLuaEaseTarget::PlayerRotationZ
    ));
    assert!(matches!(
        compiled.eases[5].target,
        SongLuaEaseTarget::PlayerSkewX
    ));
    assert!(matches!(
        compiled.eases[6].target,
        SongLuaEaseTarget::PlayerSkewY
    ));
    assert!(matches!(
        compiled.eases[7].target,
        SongLuaEaseTarget::PlayerZoom
    ));
    assert!(matches!(
        compiled.eases[8].target,
        SongLuaEaseTarget::PlayerZoomZ
    ));
}

#[test]
fn compile_song_lua_extracts_overlay_message_tweens() {
    let song_dir = test_dir("overlay");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("door.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        OnCommand=function(self)
            self:diffusealpha(0)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
            self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
            self:cropright(0.5)
        end,
        SlideDoorMessageCommand=function(self)
            self:x(0)
            self:diffusealpha(1)
            self:linear(0.3)
            self:x(SCREEN_CENTER_X)
        end,
    }
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Overlay")).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let overlay = &compiled.overlays[0];
    assert_eq!(overlay.parent_index, None);
    assert!(matches!(
        overlay.kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("gfx/door.png")
    ));
    assert_eq!(overlay.initial_state.diffuse[3], 0.0);
    assert_eq!(overlay.initial_state.x, 320.0);
    assert_eq!(overlay.initial_state.y, 240.0);
    assert_eq!(overlay.initial_state.cropright, 0.5);
    assert_eq!(overlay.initial_state.stretch_rect, None);
    assert_eq!(overlay.initial_state.zoom_x, 640.0);
    assert_eq!(overlay.initial_state.zoom_y, 480.0);
    assert_eq!(overlay.message_commands.len(), 1);
    assert_eq!(overlay.message_commands[0].message, "SlideDoor");
    assert_eq!(overlay.message_commands[0].blocks.len(), 2);
    assert_eq!(overlay.message_commands[0].blocks[0].delta.x, Some(0.0));
    assert_eq!(
        overlay.message_commands[0].blocks[0].delta.diffuse.unwrap()[3],
        1.0
    );
    assert_eq!(overlay.message_commands[0].blocks[1].duration, 0.3);
    assert_eq!(overlay.message_commands[0].blocks[1].delta.x, Some(320.0));
}

#[test]
fn compile_song_lua_supports_spring_bounce_and_stoptweening_commands() {
    let song_dir = test_dir("overlay-spring-bounce");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("door.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        BounceDoorMessageCommand=function(self)
            self:stoptweening()
            self:bouncebegin(0.2):diffusealpha(0.5)
            self:bounceend(0.25):diffusealpha(1)
            self:spring(0.5):x(SCREEN_CENTER_X)
        end,
    }
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Spring Bounce"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let overlay = &compiled.overlays[0];
    assert_eq!(overlay.message_commands.len(), 1);
    assert_eq!(overlay.message_commands[0].message, "BounceDoor");
    assert_eq!(overlay.message_commands[0].blocks.len(), 3);
    assert_eq!(
        overlay.message_commands[0].blocks[0].easing.as_deref(),
        Some("bouncebegin")
    );
    assert_eq!(overlay.message_commands[0].blocks[0].duration, 0.2);
    assert_eq!(
        overlay.message_commands[0].blocks[0].delta.diffuse.unwrap()[3],
        0.5
    );
    assert_eq!(
        overlay.message_commands[0].blocks[1].easing.as_deref(),
        Some("bounceend")
    );
    assert_eq!(overlay.message_commands[0].blocks[1].start, 0.2);
    assert_eq!(overlay.message_commands[0].blocks[1].duration, 0.25);
    assert_eq!(
        overlay.message_commands[0].blocks[1].delta.diffuse.unwrap()[3],
        1.0
    );
    assert_eq!(
        overlay.message_commands[0].blocks[2].easing.as_deref(),
        Some("spring")
    );
    assert_eq!(overlay.message_commands[0].blocks[2].start, 0.45);
    assert_eq!(overlay.message_commands[0].blocks[2].duration, 0.5);
    assert_eq!(overlay.message_commands[0].blocks[2].delta.x, Some(320.0));
}

#[test]
fn compile_song_lua_tracks_tween_time_left_during_capture() {
    let song_dir = test_dir("tween-time-left");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local before = self:GetTweenTimeLeft()
            self:sleep(0.5)
            local after_sleep = self:GetTweenTimeLeft()
            self:linear(0.25):x(10)
            local after_linear = self:GetTweenTimeLeft()
            self:stoptweening()
            local after_stop = self:GetTweenTimeLeft()
            self:bounceend(0.125):diffusealpha(0.5)
            local after_bounce = self:GetTweenTimeLeft()
            mod_actions = {{
                1,
                string.format(
                    "%.2f:%.2f:%.2f:%.2f:%.3f",
                    before,
                    after_sleep,
                    after_linear,
                    after_stop,
                    after_bounce
                ),
                true,
            }}
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Tween Time Left"),
    )
    .unwrap();
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.message == "0.00:0.50:0.75:0.00:0.125")
    );
    assert_eq!(compiled.overlays.len(), 1);
    let overlay = &compiled.overlays[0];
    assert_eq!(overlay.initial_state.x, 0.0);
    assert_eq!(overlay.initial_state.diffuse[3], 1.0);
    let startup = overlay
        .message_commands
        .iter()
        .find(|command| command.message == "__songlua_actor_startup")
        .expect("startup bounce tween");
    assert_eq!(
        overlay_state_after_blocks(overlay.initial_state, &startup.blocks, 0.125).diffuse[3],
        0.5
    );
}

#[test]
fn compile_song_lua_captures_queued_message_command_timeline() {
    let song_dir = test_dir("queued-message-command-timeline");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local action = 1
local actions = {{0.4, "Wallop"}}

return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            while action <= #actions and beat >= actions[action][1] do
                MESSAGEMAN:Broadcast(actions[action][2])
                action = action + 1
            end
        end)
    end,
    Def.Quad{
        Name="Wallop",
        WallopMessageCommand=function(self)
            self:visible(true):diffusealpha(0):linear(0.15):diffusealpha(1)
            self:queuecommand("Wait")
        end,
        WaitCommand=function(self)
            self:sleep(1.2):queuecommand("Away")
        end,
        AwayCommand=function(self)
            self:linear(0.2):zoom(6):diffusealpha(0)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Queued Message Timeline"),
    )
    .unwrap();
    let overlay = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Wallop"))
        .unwrap();
    let command = overlay
        .message_commands
        .iter()
        .find(|command| command.message == "Wallop")
        .unwrap();
    assert_eq!(command.blocks.len(), 3);
    assert_eq!(command.blocks[0].duration, 0.0);
    assert_eq!(command.blocks[0].delta.diffuse.unwrap()[3], 0.0);
    assert_eq!(command.blocks[1].duration, 0.15);
    assert_eq!(command.blocks[1].delta.diffuse.unwrap()[3], 1.0);
    assert_eq!(command.blocks[2].start, 1.35);
    assert_eq!(command.blocks[2].duration, 0.2);
    assert_eq!(command.blocks[2].delta.diffuse.unwrap()[3], 0.0);
    assert_eq!(command.blocks[2].delta.zoom_x, Some(6.0));
    let overlay_index = compiled
        .overlays
        .iter()
        .position(|candidate| candidate.name.as_deref() == Some("Wallop"))
        .unwrap();
    assert!(
        compiled
            .overlay_updates
            .iter()
            .all(|track| track.overlay_index != overlay_index),
        "the recurring Update must not duplicate a direct message timeline"
    );
}

#[test]
fn compile_song_lua_captures_actor_queued_messages() {
    let song_dir = test_dir("actor-queued-message");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local source = nil

mod_actions = {
    {2, function() source:queuemessage("ShakeScreen") end, true},
}

return Def.ActorFrame{
    Def.Actor{
        InitCommand=function(self) source = self end,
    },
    Def.Quad{
        Name="ShakeTarget",
        ShakeScreenMessageCommand=function(self, params)
            if params ~= nil then
                assert(type(params) == "table" and next(params) == nil)
                self:linear(0.1):x(12)
            end
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Queued Message"),
    )
    .unwrap();
    let overlay = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("ShakeTarget"))
        .expect("queued message target");
    let event = compiled
        .messages
        .iter()
        .find(|event| event.beat == 2.0)
        .expect("queued action is retained at its scheduled beat");
    let command = overlay
        .message_commands
        .iter()
        .find(|command| command.message == event.message)
        .expect("queued action captures the listener with its empty table");
    assert_eq!(overlay.initial_state.x, 0.0);
    assert_eq!(
        overlay_state_after_blocks(overlay.initial_state, &command.blocks, 0.05).x,
        6.0
    );
    assert_eq!(
        overlay_state_after_blocks(overlay.initial_state, &command.blocks, 0.1).x,
        12.0
    );
}

#[test]
fn compile_song_lua_hurrytweening_scales_capture_timeline() {
    let song_dir = test_dir("hurrytweening-timeline");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        HurryMessageCommand=function(self)
            self:sleep(1):linear(1):x(20)
            self:linear(2):y(40)
            self:hurrytweening(2)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Hurrytweening Timeline"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Hurry");
    assert_eq!(command.blocks.len(), 2);
    assert_eq!(command.blocks[0].start, 2.0);
    assert_eq!(command.blocks[0].duration, 2.0);
    assert_eq!(command.blocks[0].delta.x, Some(20.0));
    assert_eq!(command.blocks[1].start, 4.0);
    assert_eq!(command.blocks[1].duration, 4.0);
    assert_eq!(command.blocks[1].delta.y, Some(40.0));
}

#[test]
fn compile_song_lua_finishtweening_collapses_queued_tweens() {
    let song_dir = test_dir("finishtweening-collapse");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        FinishMessageCommand=function(self)
            self:linear(1):x(10):diffusealpha(0.5)
            self:sleep(0.5):decelerate(1):y(20)
            self:finishtweening()
            self:linear(0.25):zoom(2)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Finishtweening Collapse"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Finish");
    assert_eq!(command.blocks.len(), 2);
    assert_eq!(command.blocks[0].start, 0.0);
    assert_eq!(command.blocks[0].duration, 0.0);
    assert_eq!(command.blocks[0].easing, None);
    assert_eq!(command.blocks[0].delta.x, Some(10.0));
    assert_eq!(command.blocks[0].delta.y, Some(20.0));
    assert_eq!(command.blocks[0].delta.diffuse.unwrap()[3], 0.5);
    assert_eq!(command.blocks[1].start, 0.0);
    assert_eq!(command.blocks[1].duration, 0.25);
    assert_eq!(command.blocks[1].easing.as_deref(), Some("linear"));
    assert_eq!(command.blocks[1].delta.zoom, Some(2.0));
}

#[test]
fn compile_song_lua_stoptweening_clears_queued_tweens() {
    let song_dir = test_dir("stoptweening-clear");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        StopMessageCommand=function(self)
            self:linear(1):x(10)
            self:sleep(0.5):decelerate(1):y(20)
            self:stoptweening()
            self:linear(0.25):zoom(2)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Stoptweening Clear"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Stop");
    assert_eq!(command.blocks.len(), 1);
    assert_eq!(command.blocks[0].start, 0.0);
    assert_eq!(command.blocks[0].duration, 0.25);
    assert_eq!(command.blocks[0].easing.as_deref(), Some("linear"));
    assert_eq!(command.blocks[0].delta.x, None);
    assert_eq!(command.blocks[0].delta.y, None);
    assert_eq!(command.blocks[0].delta.zoom, Some(2.0));
}

#[test]
fn compile_song_lua_exposes_named_children_and_duplicate_groups() {
    let song_dir = test_dir("actor-children");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local count = 0
        local children = self:GetChildren()
        for _name, _child in pairs(children) do
            count = count + 1
        end
        local panel = children.Panel
        local lines = self:GetChild("Line")
        mod_actions = {
            {
                1,
                string.format(
                    "%d:%s:%d:%s",
                    count,
                    panel and panel:GetName() or "nil",
                    type(lines) == "table" and #lines or 0,
                    lines:GetName()
                ),
                true,
            },
        }
    end,
    Def.ActorFrame{ Name="Panel" },
    Def.Quad{ Name="Line" },
    Def.Quad{ Name="Line" },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Children"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "2:Panel:2:Line");
}

#[test]
fn compile_song_lua_scopes_deferred_effects() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song-lua")
        .canonicalize()
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Deferred effects");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&song_dir.join("deferred-effects.lua"), &context).unwrap();
    assert!(
        compiled.info.skipped_message_command_captures.is_empty(),
        "{:?}",
        compiled.info.skipped_message_command_captures
    );
    let actor = |name: &str| {
        compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(name))
            .unwrap()
    };
    assert!(
        !actor("Oscillator")
            .message_commands
            .iter()
            .any(|c| c.message == "Unsent")
    );
    let commands = &actor("Receiver").message_commands;
    let blocks = &commands
        .iter()
        .find(|c| c.message == "Unsent")
        .expect("late receiver")
        .blocks;
    assert!(blocks.iter().any(|b| b.delta.zoom_x == Some(0.6)));
    assert!(
        blocks.iter().any(|b| b.delta.zoom_y == Some(0.7)),
        "own queued command"
    );
    let commands = &actor("Target").message_commands;
    assert!(
        commands
            .iter()
            .find(|c| c.message == "Unsent")
            .expect("causal cross actor queue")
            .blocks
            .iter()
            .any(|b| b.delta.rot_z_deg == Some(30.0))
    );
    let sounds: Vec<_> = compiled
        .overlays
        .iter()
        .filter(|a| {
            matches!(a.kind, SongLuaOverlayKind::Sound { .. })
                && a.message_commands
                    .iter()
                    .any(|c| c.message == "UnsentSound")
        })
        .collect();
    assert_eq!(sounds.len(), 2, "preserve repeated plays of one resource");
    assert_eq!(
        compiled.sound_paths,
        vec![song_dir.join("deferred-hit.wav")]
    );
    assert!(
        !compiled
            .messages
            .iter()
            .any(|m| m.message.starts_with("Unsent")),
        "probes cannot fire messages"
    );
}

#[test]
fn compile_song_lua_rejects_unrepresented_deferred_effects() {
    let song_dir = test_dir("deferred-unsupported-sounds");
    fs::write(song_dir.join("effect.wav"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
local ready
return Def.ActorFrame{
    OnCommand=function(self) self:SetUpdateFunction(function()
        if GAMESTATE:GetSongBeat() >= .5 then ready = {value=10} end
    end) end,
    Def.Quad{
        StopMessageCommand=function(self)
            self:x(ready.value); SOUND:PlayOnce("effect.wav"); SOUND:StopMusic()
        end,
        PartMessageCommand=function(self)
            self:x(ready.value); SOUND:PlayOnce("effect.wav"); SOUND:PlayMusicPart("effect.wav", 0, 1)
        end,
        MissingMessageCommand=function(self)
            self:x(ready.value); SOUND:PlayOnce("effect.wav"); SOUND:PlayOnce("absent.wav")
        end,
        BrokenMessageCommand=function(self) self:x(never_ready.value) end,
    },
}
"#).unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Unsupported deferred effects");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let details = &compiled.info.skipped_message_command_captures;
    assert_eq!(details.len(), 4, "{details:?}");
    for name in ["Stop", "Part", "Missing", "Broken"] {
        assert!(
            details
                .iter()
                .any(|s| s.contains(&format!("{name}MessageCommand")))
        );
    }
    assert!(
        !compiled
            .overlays
            .iter()
            .any(|a| matches!(a.kind, SongLuaOverlayKind::Sound { .. }))
    );
}

#[test]
fn compile_song_lua_defers_update_bound_messages() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song-lua")
        .canonicalize()
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Deferred Messages");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&song_dir.join("deferred-message.lua"), &context).unwrap();
    assert_eq!(
        compiled.info.skipped_message_command_captures.len(),
        1,
        "{:?}",
        compiled.info.skipped_message_command_captures
    );
    assert!(compiled.info.skipped_message_command_captures[0].contains("BrokenMessageCommand"));
    let receiver = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Receiver"))
        .unwrap();
    let target = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Target"))
        .unwrap();
    let unsent = |commands: &[SongLuaOverlayMessageCommand]| {
        commands
            .iter()
            .find(|command| command.message == "Unsent")
            .unwrap()
            .blocks
            .clone()
    };
    assert_eq!(
        unsent(&compiled.overlays[receiver].message_commands)[0]
            .delta
            .zoom_x,
        Some(0.6)
    );
    assert_eq!(
        unsent(&target.message_commands)[0].delta.rot_z_deg,
        Some(30.0)
    );
    assert_eq!(
        unsent(&compiled.player_actors[0].message_commands)[0]
            .delta
            .skew_x,
        Some(0.5)
    );
    for (commands, expected) in [
        (&compiled.overlays[receiver].message_commands, 0.7),
        (&target.message_commands, 0.8),
        (&compiled.player_actors[0].message_commands, 0.9),
    ] {
        let command = commands
            .iter()
            .find(|command| command.message == "UnsentAux")
            .unwrap();
        assert_eq!(command.blocks.len(), 1);
        assert_eq!(command.blocks[0].delta.aux, Some(expected));
    }
    let late = compiled
        .stateful_message_captures
        .iter()
        .find(|capture| capture.message == "Late")
        .unwrap();
    let alphas = late
        .writes
        .iter()
        .filter(|write| {
            write.overlay_index == receiver && write.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .collect::<Vec<_>>();
    assert_eq!(alphas.len(), 2);
    for (write, expected) in alphas.iter().zip([0.4, 0.8]) {
        assert_eq!(
            write.value,
            SongLuaOverlayUpdateValue::Vec4([1.0, 1.0, 1.0, expected])
        );
    }
    assert!(
        !compiled
            .messages
            .iter()
            .any(|message| message.message == "Unsent" || message.message == "Broken")
    );
}

#[test]
fn compile_song_lua_skips_failing_overlay_message_commands() {
    let song_dir = test_dir("overlay-message-error");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("door.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Name="door",
        Texture="gfx/door.png",
        BreakMeMessageCommand=function(self)
            local broken = nil
            broken:GetName()
        end,
    }
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Message Error"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.overlays[0].message_commands.is_empty());
    assert_eq!(compiled.info.skipped_message_command_captures.len(), 1);
    assert!(compiled.info.skipped_message_command_captures[0].contains("BreakMeMessageCommand"));
}

#[test]
fn compile_song_lua_captures_message_commands_with_default_params() {
    let song_dir = test_dir("message-command-default-params");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        LifeChangedMessageCommand=function(self, params)
            if params.Player == PLAYER_1 then
                self:playcommand("ChangeSize", {CropAmount=(1 - params.LifeMeter:GetLife())})
            end
        end,
        ChangeSizeCommand=function(self, params)
            self:smooth(0.2)
            self:croptop(params.CropAmount)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        ExCountsChangedMessageCommand=function(self, params)
            if params.Player == PLAYER_1 then
                self:x(params.ActualPossible)
            end
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Message Command Default Params"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].message,
        "LifeChanged"
    );
    assert_eq!(compiled.overlays[0].message_commands[0].blocks.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0].duration,
        0.2
    );
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .croptop,
        Some(0.5)
    );
    assert_eq!(compiled.overlays[1].message_commands.len(), 1);
    assert_eq!(
        compiled.overlays[1].message_commands[0].blocks[0].delta.x,
        Some(1.0)
    );
}

#[cfg(feature = "test-support")]
#[test]
fn broadcast_identity_and_probes_preserve_local_state() {
    let song_dir = test_dir("broadcast-locals");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local account = {nested={value=1}}
account.self = account
local original = account
local count = 0
local checked, selected = false, false
local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
return Def.ActorFrame{
    SpawnMessageCommand=function(self)
        count = count + 1
        account.nested.value = account.nested.value + 1
    end,
    OnCommand=function(self)
        assert(count == 0 and account == original and account.self == account)
        assert(account.nested.value == 1 and options:Mini() == 0)
        MESSAGEMAN:Broadcast("Spawn")
        self:SetUpdateFunction(function(self)
            if not checked then
                assert(count == 1 and account == original and account.self == account)
                assert(account.nested.value == 2 and math.abs(options:Mini() - (selected and 0.025 or 0)) < 0.00001)
                checked = true
            end
        end)
    end,
    Def.Quad{
        InitCommand=function(self) self:visible(false) end,
        SpawnMessageCommand=function(self)
            if account.nested.value == 2 then
                selected = true
                self:visible(true):x(count * 42)
                options:Mini(options:Mini() + 0.025)
            end
        end,
    },
}
"#,
        )
        .expect("write broadcast fixture");
    // Both pointer orders are valid; tree load order does not decide delivery.
    for parent_first in [true, false] {
        let mut context = SongLuaCompileContext::new(&song_dir, "Broadcast Locals");
        context.music_length_seconds = 1.0;
        context.message_actor_order = vec![
            ("1".into(), if parent_first { 1 } else { 2 }),
            ("1/1".into(), if parent_first { 2 } else { 1 }),
        ];
        let compiled = test_compile_song_lua(&entry, &context).expect("compile broadcast fixture");
        assert_eq!(compiled.overlays[0].initial_state.visible, parent_first);
        assert_eq!(
            compiled.overlays[0].initial_state.x,
            if parent_first { 42.0 } else { 0.0 }
        );
    }
}

#[test]
fn options_string_retains_initial_profile_speed() {
    let song_dir = test_dir("options-initial-speed");
    let entry = song_dir.join("default.lua");
    for (speed, token, method, value) in [
        (SongLuaSpeedMod::X(2.5), "2.5x", "XMod", 2.5),
        (SongLuaSpeedMod::C(600.0), "C600", "CMod", 600.0),
        (SongLuaSpeedMod::M(700.0), "m700", "MMod", 700.0),
    ] {
        fs::write(&entry, format!(r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
local po = ps:GetPlayerOptions("ModsLevel_Song")
assert(ps:GetPlayerOptionsString("ModsLevel_Song") == "NoHideLights, {token}, Overhead")
ps:SetPlayerOptions("ModsLevel_Song", ps:GetPlayerOptionsString("ModsLevel_Song") .. ", 50% Digital")
assert(po:{method}() == {value})
return Def.Quad{{}}
"#)).expect("write profile speed fixture");
        let mut context = SongLuaCompileContext::new(&song_dir, "Profile Speed");
        context.players[0].speedmod = speed;
        test_compile_song_lua(&entry, &context).expect("compile profile speed fixture");
    }
}

#[test]
fn initial_autoplay_precedes_song_loading() {
    let song_dir = test_dir("initial-autoplay");
    let entry = song_dir.join("default.lua");
    for autoplay in [0.0, 1.0] {
        fs::write(&entry, format!(r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
for _, level in ipairs({{'ModsLevel_Preferred', 'ModsLevel_Stage', 'ModsLevel_Song', 'ModsLevel_Current'}}) do
    assert(ps:GetPlayerOptions(level):PlayerAutoPlay() == {autoplay})
    assert(ps:GetPlayerOptionsString(level) == 'NoHideLights, {token}Overhead')
end
return Def.ActorFrame{{}}
"#, token = if autoplay == 1.0 { "PlayerAutoPlay, " } else { "" }))
                .expect("write initial controller input");
        let mut context = SongLuaCompileContext::new(&song_dir, "Initial Autoplay");
        context.players[0].autoplay = autoplay;
        test_compile_song_lua(&entry, &context).expect("compile with initial controller input");
    }
}

#[test]
fn options_string_reads_live_targets_and_native_percentages() {
    let song_dir = test_dir("options-live-string");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
local po = ps:GetPlayerOptions("ModsLevel_Song")
po:Mini(0.425, 0.05)
assert(ps:GetPlayerOptionsString("ModsLevel_Song") == "NoHideLights, 42% Mini, Overhead")
assert(GetPlayerOptionsString(PLAYER_1) == ps:GetPlayerOptionsString("ModsLevel_Preferred"))
assert(GetPlayerOptionsString(PLAYER_1) ~= ps:GetPlayerOptionsString("ModsLevel_Song"))
ps:SetPlayerOptions("ModsLevel_Song", ps:GetPlayerOptionsString("ModsLevel_Song") .. ", *5 50% Digital")
assert(math.abs(po:Mini() - 0.42) < 0.00001)
assert(po:Digital() == 0.5)
local mods = ps:GetPlayerOptionsArray("ModsLevel_Song")
assert(table.concat(mods, ", ") == "NoHideLights, 42% Mini, 50% Digital, Overhead")
return Def.Quad{}
"#).expect("write live option fixture");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Live Options"),
    )
    .expect("compile live option fixture");
}

#[test]
fn compile_song_lua_runs_messageman_broadcast_during_startup() {
    let song_dir = test_dir("broadcast-startup");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        assert(MESSAGEMAN:SetLogging(true) == MESSAGEMAN)
        assert(MESSAGEMAN.SetLogging(MESSAGEMAN, false) == MESSAGEMAN)
        assert(MESSAGEMAN:Broadcast("ProxyStart") == MESSAGEMAN)
    end,
    Def.Quad{
        InitCommand=function(self)
            self:visible(false)
            self:zoomto(12, 18)
        end,
        ProxyStartMessageCommand=function(self)
            self:visible(true)
            self:x(42)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast Startup"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 42.0);
    assert!(compiled.overlays[0].initial_state.visible);
    assert_eq!(compiled.overlays[0].initial_state.size, None);
    assert_eq!(compiled.overlays[0].initial_state.zoom_x, 12.0);
    assert_eq!(compiled.overlays[0].initial_state.zoom_y, 18.0);
}

#[test]
fn compile_song_lua_passes_messageman_broadcast_params() {
    let song_dir = test_dir("broadcast-params");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        MESSAGEMAN:Broadcast("Judgment", {
            Player=PLAYER_1,
            TapNoteScore="TapNoteScore_W1",
            FirstTrack=3,
        })
    end,
    Def.Quad{
        InitCommand=function(self)
            self:visible(false)
        end,
        JudgmentMessageCommand=function(self, params)
            if params.Player == PLAYER_1 and params.TapNoteScore == "TapNoteScore_W1" then
                self:visible(true)
                self:x(params.FirstTrack * 10)
            end
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast Params"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.overlays[0].initial_state.visible);
    assert_eq!(compiled.overlays[0].initial_state.x, 30.0);
}

#[test]
fn compile_song_lua_shapes_judgment_broadcast_tap_notes() {
    let song_dir = test_dir("broadcast-judgment-tap-notes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        MESSAGEMAN:Broadcast("Judgment", {
            Player=PLAYER_1,
            TapNoteScore="TapNoteScore_Miss",
            TapNoteOffset=-0.02,
            Notes={
                [2]={ TapNoteType="TapNoteType_HoldHead", Held=true },
            },
        })
    end,
    Def.BitmapText{
        Font="Common Normal",
        Text="",
        JudgmentMessageCommand=function(self, params)
            for col,tapnote in pairs(params.Notes) do
                local result = tapnote:GetTapNoteResult()
                self:settext(table.concat({
                    tostring(col),
                    ToEnumShortString(tapnote:GetTapNoteType()),
                    tostring(result:GetHeld()),
                    result:GetTapNoteScore(),
                    string.format("%.2f", result:GetTapNoteOffset()),
                    tapnote:GetPlayerNumber(),
                    tostring(TapNoteType:Reverse()[tapnote:GetTapNoteType()] ~= nil),
                }, "|"))
            end
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast Judgment Tap Notes"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::BitmapText { ref text, .. }
            if text.as_ref()
                == "2|HoldHead|true|TapNoteScore_Miss|-0.02|PlayerNumber_P1|true"
    ));
}

#[test]
fn compile_song_lua_respects_context_screen_dimensions() {
    let song_dir = test_dir("overlay-screen-dims");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("panel.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/panel.png",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
            self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
        end,
    }
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Overlay");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let overlay = &compiled.overlays[0];

    assert_eq!(compiled.screen_width, 854.0);
    assert_eq!(compiled.screen_height, 480.0);
    assert_eq!(overlay.initial_state.x, 427.0);
    assert_eq!(overlay.initial_state.y, 240.0);
    assert_eq!(overlay.initial_state.stretch_rect, None);
    assert_eq!(overlay.initial_state.zoom_x, 854.0);
    assert_eq!(overlay.initial_state.zoom_y, 480.0);
}

#[test]
fn compile_song_lua_exposes_display_compat_globals() {
    let song_dir = test_dir("display-compat");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%s",
            DISPLAY:GetDisplayWidth(),
            DISPLAY:GetDisplayHeight(),
            tostring(DISPLAY.SupportsRenderToTexture ~= nil),
            tostring(DISPLAY:SupportsRenderToTexture())
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Display Compat");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.display_width = 1600.0;
    context.display_height = 900.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1600:900:true:true");
}

#[test]
fn compile_song_lua_preserves_null_display_context() {
    let song_dir = test_dir("null-display-context");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(SCREEN_WIDTH == 854 and SCREEN_HEIGHT == 480)
assert(DISPLAY:GetDisplayWidth() == 0 and DISPLAY:GetDisplayHeight() == 0)
assert(PREFSMAN:GetPreference('DisplayWidth') == 0)
assert(PREFSMAN:GetPreference('DisplayHeight') == 0)
assert(math.abs(PREFSMAN:GetPreference('DisplayAspectRatio') - 16 / 9) < 1e-6)
assert(PREFSMAN:GetPreference('VideoRenderers') == 'null')
assert(DISPLAY:SupportsRenderToTexture() == false)
-- Simply Love's SL-SupportHelpers.lua reports true for null independently.
assert(SupportsRenderToTexture() == true)
local root = Def.ActorFrame{}
if DISPLAY:SupportsRenderToTexture() then
    root[#root + 1] = Def.ActorFrameTexture{Name='UnexpectedAFT'}
end
return root
"#,
    )
    .expect("write null-renderer control");
    let mut context = SongLuaCompileContext::new(&song_dir, "Null Display");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.display_width = 0.0;
    context.display_height = 0.0;
    context.video_renderers = "null".into();
    context.render_to_texture = false;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile native null branch");
    assert!(compiled.overlays.is_empty());
}

#[test]
fn display_preference_ignores_mode_size() {
    let song_dir = test_dir("display-preference-size");
    let entry = song_dir.join("default.lua");
    for (width, height, aspect) in [(0.0, 0.0, 16.0 / 9.0), (3840.0, 2160.0, 4.0 / 3.0)] {
        fs::write(
            &entry,
            format!(
                r#"
assert(math.abs(PREFSMAN:GetPreference('DisplayAspectRatio') - {aspect}) < 1e-6)
assert(PREFSMAN:GetPreference('DisplayWidth') == {width})
assert(PREFSMAN:GetPreference('DisplayHeight') == {height})
PREFSMAN:SetPreference('DisplayWidth', 1024)
PREFSMAN:SetPreference('DisplayHeight', 768)
assert(math.abs(PREFSMAN:GetPreference('DisplayAspectRatio') - {aspect}) < 1e-6)
return Def.ActorFrame{{}}
"#
            ),
        )
        .expect("display preference control");
        let mut context = SongLuaCompileContext::new(&song_dir, "Display Preference");
        context.display_width = width;
        context.display_height = height;
        context.display_aspect_ratio = aspect;
        test_compile_song_lua(&entry, &context).expect("independent display preference");
    }
}

#[test]
fn screen_aspect_uses_preference_with_rounded_logical_dimensions() {
    let song_dir = test_dir("rounded-screen-aspect");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(SCREEN_WIDTH == 854 and _screen.w == 854)
assert(GetScreenAspectRatio() == PREFSMAN:GetPreference('DisplayAspectRatio'),
    'physical display preference is independent of rounded logical dimensions')
assert(math.abs(_screen.w * 0.529 / GetScreenAspectRatio() - 254.1183731066706) < 1e-10)
PREFSMAN:SetPreference('DisplayWidth', 1280)
PREFSMAN:SetPreference('DisplayHeight', 720)
PREFSMAN:SetPreference('DisplayAspectRatio', 4 / 3)
assert(math.abs(GetScreenAspectRatio() - 4 / 3) < 1e-6)
assert(SCREEN_WIDTH == 854, 'changing a preference does not reload screen dimensions')
return Def.ActorFrame{}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Rounded Screen Aspect");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    context.display_aspect_ratio = 16.0 / 9.0;
    test_compile_song_lua(&entry, &context).expect("native display aspect contract");
}

#[test]
fn compile_song_lua_exposes_display_specs_shape() {
    let song_dir = test_dir("display-specs-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local specs = DISPLAY:GetDisplaySpecs()
local spec = specs[1]
local mode = spec:GetCurrentMode()
local modes = spec:GetSupportedModes()
mod_actions = {
    {
        1,
        string.format(
            "%d:%s:%s:%s:%d:%d:%d:%d:%s:%s:%s",
            #specs,
            spec:GetId(),
            spec:GetName(),
            tostring(spec:IsVirtual()),
            mode:GetWidth(),
            mode:GetHeight(),
            mode:GetRefreshRate(),
            #modes,
            tostring(modes[1] == mode),
            tostring(tostring(specs):find("DisplaySpecs") ~= nil),
            tostring(DISPLAY:SupportsFullscreenBorderlessWindow())
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Display Specs Shape");
    context.screen_width = 1366.0;
    context.screen_height = 768.0;
    context.display_width = 1366.0;
    context.display_height = 768.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1:Default:Default Display:false:1366:768:60:1:true:true:false"
    );
}

#[test]
fn compile_song_lua_exposes_date_compat_globals() {
    let song_dir = test_dir("date-compat");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%d:%d:%d:%d",
            Year(),
            MonthOfYear(),
            DayOfMonth(),
            Hour(),
            Minute(),
            Second()
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Date Compat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    let parts = compiled.messages[0]
        .message
        .split(':')
        .map(|value| value.parse::<i32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), 6);
    let now = Local::now();
    assert_eq!(parts[0], now.year());
    assert_eq!(parts[1], now.month0() as i32);
    assert_eq!(parts[2], now.day() as i32);
    assert!((0..=23).contains(&parts[3]));
    assert!((0..=59).contains(&parts[4]));
    assert!((0..=59).contains(&parts[5]));
}

#[test]
fn compile_song_lua_exposes_charman_compat_helpers() {
    let song_dir = test_dir("charman-compat");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local characters = CHARMAN:GetAllCharacters()
assert(type(characters) == "table")
assert(#characters == 0)
assert(CHARMAN:GetCharacterCount() == 0)
assert(CHARMAN:GetCharacter("unused") == nil)
assert(CHARMAN:GetDefaultCharacter() == nil)
assert(CHARMAN:GetRandomCharacter() == nil)

mod_actions = {
    {1, string.format("%d:%d:%s", #characters, CHARMAN:GetCharacterCount(), tostring(CHARMAN:GetRandomCharacter())), true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Character Manager Compat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0:0:nil");
}

#[test]
fn compile_song_lua_exposes_course_trail_and_song_position_helpers() {
    let song_dir = test_dir("course-trail-position-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local course = GAMESTATE:GetCurrentCourse()
local trail = GAMESTATE:GetCurrentTrail(PLAYER_1)
local entries = trail:GetTrailEntries()
local entry = trail:GetTrailEntry(0)
local pos = GAMESTATE:GetSongPosition()
local player_pos = GAMESTATE:GetPlayerState(PLAYER_1):GetSongPosition()

assert(course:GetDisplayFullTitle() == "Course Trail Position")
assert(course:GetCourseDir():match("compat%-course%.crs$") ~= nil)
assert(course:GetCourseType() == "CourseType_Nonstop")
assert(course:GetEstimatedNumStages() == 1)
assert(course:AllSongsAreFixed())
assert(course:IsAutogen() == false)
assert(course:IsEndless() == false)
assert(#course:GetCourseEntries() == 1)
assert(course:GetAllTrails()[1] == trail)
assert(course:GetTrail("StepsType_Dance_Single") == trail)

assert(#entries == 1)
assert(entries[1] == entry)
assert(entry:GetSong() == GAMESTATE:GetCurrentSong())
assert(entry:GetSteps() == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(entry:GetCourseEntryType() == "CourseEntryType_Fixed")
assert(entry:IsSecret() == false)
assert(trail:GetStepsType() == "StepsType_Dance_Single")
assert(trail:GetDisplayBpms()[1] == 120)
assert(pos:GetMusicSeconds() == pos:GetMusicSecondsVisible())
assert(pos:GetSongBeat() == pos:GetSongBeatVisible())
assert(pos:GetCurBPS() > 0)
assert(player_pos:GetCurBPS() == pos:GetCurBPS())

mod_actions = {
    {1, table.concat({
        course:GetDisplayFullTitle(),
        tostring(#course:GetCourseEntries()),
        tostring(#entries),
        tostring(entry:GetSteps():GetMeter()),
        tostring(player_pos:GetMusicSecondsVisible()),
    }, "|"), true},
    {2, function()
        GAMESTATE:SetCurrentTrail(PLAYER_1, trail)
    end, true},
}

return Def.Banner{
    OnCommand=function(self)
        self:LoadFromSong(GAMESTATE:GetCurrentSong())
        self:LoadFromCourse(course)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Course Trail Position");
    context.song_display_bpms = [120.0, 180.0];
    context.players[0].display_bpms = [120.0, 180.0];
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "Course Trail Position|1|1|10|0"
    );
}

#[test]
fn compile_song_lua_exposes_song_and_steps_display_bpms() {
    let song_dir = test_dir("display-bpms");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local song_bpms = GAMESTATE:GetCurrentSong():GetDisplayBpms()
local step_bpms = GAMESTATE:GetCurrentSteps(PLAYER_1):GetDisplayBpms()
mod_actions = {
    {
        1,
        string.format(
            "%s:%d:%d:%d:%d",
            GAMESTATE:GetCurrentSong():GetDisplayMainTitle(),
            song_bpms[1],
            song_bpms[2],
            step_bpms[1],
            step_bpms[2]
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Display BPMs");
    context.song_display_bpms = [120.0, 180.0];
    context.players[0].display_bpms = [150.0, 200.0];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Display BPMs:120:180:150:200");
}

#[test]
fn compile_song_lua_exposes_timing_bpm_segments() {
    let song_dir = test_dir("timing-bpm-segments");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local song_timing = GAMESTATE:GetCurrentSong():GetTimingData()
local steps_timing = GAMESTATE:GetCurrentSteps(PLAYER_1):GetTimingData()
local song_bpms = song_timing:GetBPMs()
local step_bpms = steps_timing:GetBPMs()

mod_actions = {{
    1,
    string.format(
        "%s:%d:%d:%s:%d:%d",
        tostring(song_timing:HasBPMChanges()),
        #song_bpms,
        song_bpms[2],
        tostring(steps_timing:HasBPMChanges()),
        #step_bpms,
        step_bpms[1]
    ),
    true,
}}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Timing BPM Segments");
    context.song_display_bpms = [120.0, 180.0];
    context.song_timing_bpms = vec![(0.0, 120.0), (4.0, 180.0)];
    context.players[0].display_bpms = [150.0, 150.0];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:2:180:false:1:150");
}

#[test]
fn event_mode_uses_preference() {
    let song_dir = test_dir("event-mode-native-default");
    let entry = song_dir.join("default.lua");
    let source = r#"
            assert(GAMESTATE:IsEventMode() == PREFSMAN:GetPreference('EventMode'))
            assert(GAMESTATE:IsEventMode() == expected)
            PREFSMAN:SetPreference('EventMode', not expected)
            assert(GAMESTATE:IsEventMode() == not expected)
            return Def.ActorFrame{}
        "#;
    for event_mode in [true, false] {
        let mut context = SongLuaCompileContext::new(&song_dir, "event mode");
        context.event_mode = event_mode;
        fs::write(&entry, format!("expected = {event_mode}\n{source}")).expect("write probe input");
        test_compile_song_lua(&entry, &context).expect("live EventMode preference");
    }
}

#[test]
fn compile_song_lua_exposes_song_steps_type_selectors() {
    let song_dir = test_dir("song-steps-type-selectors");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local song = GAMESTATE:GetCurrentSong()
local steps_type = GAMESTATE:GetCurrentStyle():GetStepsType()
local all_steps = song:GetAllSteps()
local single_steps = song:GetStepsByStepsType(steps_type)
local pump_steps = song:GetStepsByStepsType("StepsType_Pump_Single")
local hard_steps = song:GetOneSteps(steps_type, "Difficulty_Hard")
local edit_steps = song:GetOneSteps("dance-single", "Edit")

mod_actions = {{
    1,
    string.format(
        "%d:%d:%d:%s:%d:%s:%s:%s:%s:%s",
        #all_steps,
        #single_steps,
        #pump_steps,
        hard_steps:GetDifficulty(),
        hard_steps:GetMeter(),
        edit_steps:GetDifficulty(),
        tostring(song:HasStepsType(steps_type)),
        tostring(song:HasStepsTypeAndDifficulty(steps_type, "Expert")),
        tostring(song:HasEdits(steps_type)),
        tostring(song:GetOneSteps("StepsType_Pump_Single", "Hard") == nil)
    ),
    true,
}}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Song Steps Type Selectors");
    context.song_steps = test_song_steps();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "6:6:0:Difficulty_Hard:10:Difficulty_Edit:true:true:true:true"
    );
}

#[test]
fn compile_song_lua_exposes_itgmania_actor_classes() {
    let song_dir = test_dir("itgmania-actor-classes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(type(NoteField) == "table")
assert(type(NoteField.get_column_actors) == "function")
assert(NoteField.GetChildAt == nil)
assert(type(Player.SetLife) == "function")
assert(type(Player.get_oitg_zoom_mode) == "function")
assert(type(Player.set_oitg_zoom_mode) == "function")
assert(NoteField.set_skin == nil)
assert(Player.SetNoteData == nil and Player.SetNoteDataFromLua == nil)
return Def.ActorFrame {
    InitCommand = function()
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local field = player:GetChild("NoteField")
        assert(#NoteField.get_column_actors(field) == 4)
    end,
}
"#,
    )
    .expect("actor class script");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "ITGmania Actor Classes"),
    )
    .expect("ITGmania class probes and forwarding");
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn message_probe_does_not_start_recurring_commands() {
    let song_dir = test_dir("message-probe-queues");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame {
    OnCommand = function(self) self:SetUpdateFunction(function() end) end,
    Def.Actor {
        Name = "looper",
        BeginMessageCommand = function(self)
            loop_ready = 0
            self:queuecommand("Loop")
        end,
        LoopCommand = function(self)
            assert(loop_ready ~= nil, "message probe leaked its recurring queue")
            loop_ready = loop_ready + 1
            self:sleep(0.1):queuecommand("Loop")
        end,
    },
}
"#,
    )
    .expect("message probe fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Message Probe Queues");
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile message probe");
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{:?}",
        compiled.info.unsupported_perframe_captures
    );
}

#[test]
fn shared_definitions_create_separate_actors() {
    let song_dir = test_dir("shared-actor-definition");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/shared-actor.lua")).expect("shared actor fixture");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Shared Actor Definition"),
    )
    .expect("shared definitions instantiate separate native actors");
    assert_eq!(compiled.info.unsupported_perframes, 0);
}

#[test]
fn init_queues_wait_for_on() {
    let song_dir = test_dir("init-queue-order");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/init-queue.lua"))
        .expect("native Init queue fixture");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Init Queue Order"),
    )
    .expect("Init queue must run after the parent's and child's OnCommand");
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.message == "InitQueueLate" && event.beat > 0.0)
    );
}

#[test]
fn compile_song_lua_exposes_song_options_object_music_rate() {
    let song_dir = test_dir("song-options-object");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local so = GAMESTATE:GetSongOptionsObject("ModsLevel_Song")
local before = so:MusicRate()
so:MusicRate(0.75)
mod_actions = {
    {1, string.format("%.2f:%.2f", before, so:MusicRate()), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Song Options Object");
    context.song_music_rate = 1.5;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.50:0.75");
}

#[test]
fn compile_song_lua_exposes_song_options_string_music_rate() {
    let song_dir = test_dir("song-options-string");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, GAMESTATE:GetSongOptions("ModsLevel_Song"), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Song Options String");
    context.song_music_rate = 1.25;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.25xMusic");
}

#[test]
fn screen_request_keeps_current_name() {
    let song_dir = test_dir("queued-screen-name");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.Quad{OnCommand=function(self)
    local screen = SCREENMAN:GetTopScreen()
    local name = screen:GetName()
    if not REQUESTED_SCREEN then
        REQUESTED_SCREEN = true
        SCREENMAN:SetNewScreen("LastRequestMustReplaceThis")
        assert(SCREENMAN:GetTopScreen() == screen and screen:GetName() == name)
        SCREENMAN:SetNewScreen("ScreenGameplay")
        assert(SCREENMAN:GetTopScreen() == screen and screen:GetName() == name)
    end
    self:x(31)
end}
"#,
    )
    .expect("write screen request control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Queued Screen Name"),
    )
    .expect("compile native screen request control");
    assert!(
        compiled
            .overlays
            .iter()
            .any(|actor| actor.initial_state.x == 31.0)
    );
}

#[test]
fn compile_song_lua_exposes_save_your_tears_compat_helpers() {
    let song_dir = test_dir("save-your-tears-compat");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local steps = GAMESTATE:GetCurrentSong():GetStepsByStepsType("StepsType_Dance_Single")
        if GAMESTATE:GetCurrentSteps(PLAYER_1) ~= steps[2] then
            GAMESTATE:SetCurrentSteps(PLAYER_1, steps[2])
            SCREENMAN:SetNewScreen("ScreenGameplay")
        end
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        ps:SetPlayerOptions("ModsLevel_Song", "1x, Overhead")
        mod_actions = {
            {1, string.format("%d:%s", #steps, ps:GetPlayerOptionsString("ModsLevel_Song")), true},
        }
    end,
    Def.Sound{
        File="thunder.ogg",
        OnCommand=function(self)
            self:play():pause():stop():playforplayer(PLAYER_1):load("rain.ogg"):volume(0.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Save Your Tears Compat");
    context.song_steps = test_song_steps();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "6:NoHideLights, 1x, Overhead");
}

#[test]
fn sound_length_reload() {
    let song_dir = test_dir("sound-length");
    for (name, frames) in [("first.wav", 8_000_u32), ("second.wav", 2_000)] {
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + frames).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(b"\x01\0\x08\0data");
        wav.extend_from_slice(&frames.to_le_bytes());
        wav.resize(44 + frames as usize, 128);
        fs::write(song_dir.join(name), wav).unwrap();
    }
    fs::write(song_dir.join("broken.ogg"), b"invalid audio").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.Sound{
    File='first',
    OnCommand=function(self)
        local sound = self:get()
        assert(sound == self:get())
        assert(sound:get_length() == 1 and sound:get_length() == 1)
        self:load('second.wav')
        assert(sound:get_length() == 0.25)
        self:sleep(sound:get_length()):play()
        self:load('broken.ogg')
        assert(sound:get_length() == -1)
        self:load(nil)
        assert(sound:get_length() == -1)
        assert(sound:SetProperty('UnsupportedProperty', 1) == false)
        self:load('second.wav')
    end,
}
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sound length"),
    )
    .unwrap();
    // ActorSound::play is immediate even after sleep(). Its file is the
    // one loaded at the call, rather than the final File on the actor.
    assert_eq!(
        test_compiled_sound_calls(&compiled),
        vec![(0.0, song_dir.join("second.wav"))]
    );
}

#[test]
fn compile_song_lua_extracts_sound_actor_assets() {
    let song_dir = test_dir("sound-actor-assets");
    fs::write(song_dir.join("hit.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, "Ding", true},
}
return Def.ActorFrame{
    Def.Sound{
        Name="HitSound",
        File="hit.ogg",
        OnCommand=function(self)
            self:play()
        end,
        DingMessageCommand=function(self)
            self:sleep(0.25):playforplayer(PLAYER_1)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sound Actor Assets"),
    )
    .unwrap();
    assert_eq!(compiled.sound_paths, vec![song_dir.join("hit.ogg")]);
    // Native On and Ding each call ActorSound::Play once. sleep() queues
    // actor state, while playforplayer() still executes in that callback.
    assert_eq!(
        test_compiled_sound_calls(&compiled),
        vec![
            (0.0, song_dir.join("hit.ogg")),
            (1.0, song_dir.join("hit.ogg")),
        ]
    );
}

#[test]
fn compile_song_lua_extracts_sound_load_assets() {
    let song_dir = test_dir("sound-load-assets");
    for name in ["initial.ogg", "lower.ogg", "upper.ogg"] {
        fs::write(song_dir.join(name), b"not decoded during compile").unwrap();
    }
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sound{
        Name="LowerLoad",
        File="initial.ogg",
        OnCommand=function(self)
            self:load("lower.ogg")
        end,
    },
    Def.Sound{
        Name="UpperLoad",
        OnCommand=function(self)
            self:Load("upper.ogg")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sound Load Assets"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
        panic!("expected lower sound overlay");
    };
    assert_eq!(sound_path, &song_dir.join("lower.ogg"));
    let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[1].kind else {
        panic!("expected upper sound overlay");
    };
    assert_eq!(sound_path, &song_dir.join("upper.ogg"));
}

#[test]
fn compile_song_lua_extracts_sound_singleton_assets() {
    let song_dir = test_dir("sound-singleton-assets");
    fs::write(song_dir.join("effect.ogg"), b"not decoded during compile").unwrap();
    fs::write(song_dir.join("music.wav"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
SOUND:PlayOnce("effect.ogg")
SOUND:PlayOnce("missing.ogg")
SOUND:PlayMusicPart("music.wav", 0, 1, 0, 0)
assert(SOUND:PlayOnce("effect.ogg") == SOUND)
assert(SOUND:PlayMusicPart("music.wav", 0, 1, 0, 0, false, true, true) == SOUND)
assert(SOUND:DimMusic(0.5, 1.0) == SOUND)
assert(SOUND:StopMusic() == SOUND)
assert(SOUND:PlayAnnouncer("common start") == SOUND)
assert(SOUND:GetPlayerBalance(PLAYER_1) == 0)
assert(SOUND:IsTimingDelayed() == false)

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sound Singleton Assets"),
    )
    .unwrap();
    assert_eq!(
        compiled.sound_paths,
        vec![song_dir.join("effect.ogg"), song_dir.join("music.wav")]
    );
}

#[test]
fn compile_song_lua_schedules_function_sound_calls() {
    let song_dir = test_dir("timed-sound-singleton");
    fs::write(song_dir.join("begin.ogg"), b"not decoded during compile").unwrap();
    fs::write(song_dir.join("music.wav"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions={{0.1, function()
    SOUND:PlayOnce("begin.ogg")
    SOUND:PlayMusicPart("music.wav", 0, 1, 0, 0)
end}}
return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Timed Sound Singleton"),
    )
    .unwrap();
    let sound = compiled
        .overlays
        .iter()
        .find(|overlay| matches!(overlay.kind, SongLuaOverlayKind::Sound { .. }))
        .expect("function sound call should compile into a sound actor");
    let SongLuaOverlayKind::Sound { sound_path } = &sound.kind else {
        unreachable!();
    };
    assert_eq!(sound_path, &song_dir.join("begin.ogg"));
    assert_eq!(
        compiled
            .overlays
            .iter()
            .filter(|overlay| matches!(overlay.kind, SongLuaOverlayKind::Sound { .. }))
            .count(),
        1
    );
    assert_eq!(compiled.messages.len(), 1);
    assert!((compiled.messages[0].beat - 0.1).abs() <= f32::EPSILON);
    assert_eq!(sound.message_commands.len(), 1);
    assert_eq!(
        sound.message_commands[0].blocks[0].delta.sound_play,
        Some(true)
    );
}

#[test]
fn compile_song_lua_schedules_message_sound_calls() {
    let song_dir = test_dir("message-sound-singleton");
    fs::write(song_dir.join("slap.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions={{4, "GoatSlap", true}}
return Def.ActorFrame{
    Def.ActorFrame{
        GoatSlapMessageCommand=function(self)
            SOUND:PlayOnce("slap.ogg")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Message Sound Singleton"),
    )
    .unwrap();
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.beat == 4.0 && event.message == "GoatSlap")
    );
    let sound = compiled
        .overlays
        .iter()
        .find(|overlay| matches!(overlay.kind, SongLuaOverlayKind::Sound { .. }))
        .expect("message sound call should compile into a sound actor");
    let SongLuaOverlayKind::Sound { sound_path } = &sound.kind else {
        unreachable!();
    };
    assert_eq!(sound_path, &song_dir.join("slap.ogg"));
    let command = sound
        .message_commands
        .iter()
        .find(|command| command.message == "GoatSlap")
        .expect("message sound actor should listen for its source message");
    assert_eq!(command.blocks[0].delta.sound_play, Some(true));
}

#[test]
fn compile_song_lua_captures_stable_cross_actor_messages() {
    let song_dir = test_dir("stable-cross-actor-message");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target=self end,
    },
    Def.ActorFrame{
        Name="Controller",
        ZapMessageCommand=function(self)
            target:sleep(0.5):linear(0.1):zoom(1.2)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Stable Cross Actor Message"),
    )
    .unwrap();
    let target = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Target"))
        .expect("target overlay should compile");
    let command = target
        .message_commands
        .iter()
        .find(|command| command.message == "Zap")
        .expect("stable cross-actor message should move to its target");
    let block = command.blocks.last().expect("zoom tween should compile");
    assert_eq!(block.start, 0.5);
    assert_eq!(block.duration, 0.1);
    assert_eq!(block.delta.zoom, Some(1.2));
}

#[test]
fn queued_message_probes_restore_commands() {
    let song_dir = test_dir("queued-message-probe-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        Name="Body",
        OnCommand=function(self) self:bob():effectmagnitude(0,-10,0) end,
        FallMessageCommand=function(self)
            self:queuecommand("StopLayer"):stopeffect():accelerate(5):rotationz(80)
        end,
        Def.ActorFrame{
            Name="Limb",
            OnCommand=function(self) self:wag():effectmagnitude(0,0,-20) end,
            StopLayerCommand=function(self)
                self:queuecommand("StopArm"):stopeffect():rotationz(20):linear(3):rotationz(50)
            end,
            Def.Quad{StopArmCommand=function(self) self:stopeffect() end},
        },
        Def.Quad{
            Name="Flash",
            OnCommand=function(self) self:diffusealpha(0) end,
            StopLayerCommand=function(self) self:queuecommand("Blink") end,
            BlinkCommand=function(self) self:diffusealpha(1):sleep(0.3):diffusealpha(0) end,
        },
    },
}
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Queued Probe State"),
    )
    .unwrap();
    assert!(
        compiled.info.skipped_message_command_captures.is_empty(),
        "{:?}",
        compiled.info.skipped_message_command_captures
    );
    let limb = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Limb"))
        .expect("limb");
    let fall = limb
        .message_commands
        .iter()
        .find(|command| command.message == "Fall")
        .expect("cross-actor fall command");
    assert_eq!(
        fall.blocks
            .iter()
            .filter(|block| block.delta.rot_z_deg == Some(50.0))
            .count(),
        1
    );
    let flash = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Flash"))
        .expect("flash");
    let fall = flash
        .message_commands
        .iter()
        .find(|command| command.message == "Fall")
        .expect("cross-actor flash command");
    assert_eq!(
        fall.blocks
            .iter()
            .filter(|block| block.delta.diffuse.is_some())
            .count(),
        2
    );
    assert_eq!(fall.blocks.last().expect("flash tail").start, 0.3);
}

#[test]
fn zoomto_preserves_intrinsic_size() {
    let song_dir = test_dir("zoomto-intrinsic-size");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        Name="Frame",
        InitCommand=function(self) self:zoomto(1.25,0.75) end,
        Def.Quad{
            Name="Shape",
            OnCommand=function(self)
                self:setsize(20,10):zoomto(60,40):zoomx(6)
                assert(self:GetWidth()==20 and self:GetHeight()==10)
                assert(self:GetZoomedWidth()==120 and self:GetZoomedHeight()==40)
                self:zoom(2)
            end,
        },
    },
}
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "ZoomTo Intrinsic Size"),
    )
    .unwrap();
    let frame = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Frame"))
        .expect("frame");
    assert_eq!(frame.initial_state.zoom_x, 1.25);
    assert_eq!(frame.initial_state.zoom_y, 0.75);
    let shape = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Shape"))
        .expect("shape");
    assert_eq!(shape.initial_state.size, Some([20.0, 10.0]));
    assert_eq!(shape.initial_state.zoom, 2.0);
    assert_eq!(shape.initial_state.zoom_x, 2.0);
    assert_eq!(shape.initial_state.zoom_y, 2.0);
}

#[test]
fn compile_song_lua_reports_stateful_cross_actor_messages() {
    let song_dir = test_dir("stateful-cross-actor-message");
    let entry = song_dir.join("default.lua");
    for indirect in [false, true] {
        fs::write(
                &entry,
                r#"
local targets={}
local cursor=1
return Def.ActorFrame{
    Def.Quad{
        Name="TargetA",
        InitCommand=function(self) targets[1]=self end,
    },
    Def.Quad{
        Name="TargetB",
        InitCommand=function(self) targets[2]=self end,
    },
    Def.ActorFrame{
        Name="Controller",
        FireMessageCommand=function(self)
            targets[cursor]:x(cursor*10)
            cursor=cursor%2+1
        end,
    },
}
"#.replace(
                    "FireMessageCommand=function(self)",
                    if indirect {
                        "FireMessageCommand=function(self) self:playcommand('Select') end, SelectCommand=function(self)"
                    } else {
                        "FireMessageCommand=function(self)"
                    },
                ),
            )
            .unwrap();

        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Stateful Cross Actor Message"),
        )
        .unwrap();
        assert!(compiled.overlays.iter().all(|actor| {
            actor.name.as_deref().is_none_or(|name| {
                !name.starts_with("Target")
                    || actor
                        .message_commands
                        .iter()
                        .all(|command| command.message != "Fire")
            })
        }));
        assert!(
            compiled
                .info
                .skipped_message_command_captures
                .iter()
                .any(|detail| detail.contains("FireMessageCommand changes cross-actor"))
        );
    }
}

#[test]
fn compile_song_lua_preserves_hidden_gameplay_layers() {
    let song_dir = test_dir("hidden-gameplay-layers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local screen=SCREENMAN:GetTopScreen()
            screen:GetChild("Underlay"):visible(false)
            screen:GetChild("Overlay"):visible(false)
        end)
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Hidden Gameplay Layers"),
    )
    .unwrap();
    assert!(!compiled.screen_layers[0].initial_state.visible);
    assert!(!compiled.screen_layers[1].initial_state.visible);
}

#[test]
fn compile_song_lua_set_current_steps_updates_selected_steps() {
    let song_dir = test_dir("set-current-steps");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local song_steps = GAMESTATE:GetCurrentSong():GetStepsByStepsType("StepsType_Dance_Single")
        local before = ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty())
        GAMESTATE:SetCurrentSteps(PLAYER_1, song_steps[2])
        local after = GAMESTATE:GetCurrentSteps(PLAYER_1)
        local bpms = after:GetDisplayBpms()
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%d:%d:%s",
                    before,
                    ToEnumShortString(after:GetDifficulty()),
                    bpms[1],
                    bpms[2],
                    ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_2):GetDifficulty())
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Set Current Steps");
    context.song_steps = test_song_steps();
    context.song_display_bpms = [120.0, 180.0];
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    context.players[0].display_bpms = [200.0, 240.0];
    context.players[1].difficulty = SongLuaDifficulty::Hard;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Challenge:Easy:120:180:Hard");
}

#[test]
fn compile_song_lua_supports_get_column_actors_alias() {
    let song_dir = test_dir("column-actors-alias");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
        mod_actions = {
            {1, tostring(#nf:get_column_actors()), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Column Actors Alias"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "4");
}

#[test]
fn startup_reload_keeps_globals_and_deletes_old_queues() {
    let song_dir = test_dir("startup-reload-lifecycle");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
SCENES = (SCENES or 0) + 1
local scene = SCENES
TRACE = TRACE or {}
local function mark(phase) TRACE[#TRACE + 1] = phase .. scene end
local stats = STATSMAN:GetCurStageStats()
local pss = stats:GetPlayerStageStats(PLAYER_1)
local screen = SCREENMAN:GetTopScreen()
if scene == 1 then
    FIRST_STATS, FIRST_PSS, FIRST_SCREEN, FIRST_SCREENMAN = stats, pss, screen, SCREENMAN
else
    assert(FIRST_STATS == stats and FIRST_PSS == pss)
    assert(FIRST_SCREEN ~= screen and FIRST_SCREENMAN == SCREENMAN)
    assert(FIRST_SCREENMAN:GetTopScreen() == screen)
    assert(pss:GetScore() == 0)
end
return Def.ActorFrame {
    InitCommand = function(self) mark("init") end,
    BeginCommand = function(self) mark("begin") end,
    ScreenChangedMessageCommand = function(self) mark("changed") end,
    EndCommand = function(self) mark("end") end,
    ObsoleteCommand = function(self) error("deleted screen queue executed") end,
    OnCommand = function(self)
        mark("on")
        if scene == 1 then
            pss:SetScore(123)
            self:sleep(1):queuecommand("Obsolete")
            SCREENMAN:SetNewScreen("SupersededScreen")
            SCREENMAN:SetNewScreen("ScreenGameplay")
            assert(SCREENMAN:GetTopScreen() == screen)
        else
            mod_actions = {{1, table.concat(TRACE, ":"), true}}
        end
    end,
}
"#,
    )
    .expect("reload lifecycle control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Reload lifecycle"),
    )
    .expect("startup screen replacement");
    assert_eq!(compiled.startup_reloads, 1);
    assert!(
        compiled.messages.iter().any(
            |event| event.message == "init1:begin1:changed1:on1:end1:init2:begin2:changed2:on2"
        ),
        "{:?}",
        compiled.messages
    );
}

#[test]
fn startup_reload_discards_init_requests() {
    let song_dir = test_dir("startup-reload-init-error");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, "return Def.ActorFrame{InitCommand=function() SCREENMAN:SetNewScreen('ScreenGameplay') end}")
            .expect("Init request control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Init reload"),
    )
    .expect("native reports the invalid request and keeps the initialized screen");
    assert_eq!(compiled.startup_reloads, 0);
}

#[test]
fn startup_reload_accepts_zero_delta_updates() {
    let song_dir = test_dir("startup-reload-update-zero");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
ZERO_SCENES = (ZERO_SCENES or 0) + 1
local scene = ZERO_SCENES
return Def.ActorFrame {
    OnCommand = function(self)
        if scene == 1 then
            self:SetUpdateFunction(function(_, dt)
                assert(dt == 0)
                SCREENMAN:SetNewScreen("ScreenGameplay")
            end)
        else mod_actions = {{1, tostring(ZERO_SCENES), true}} end
    end,
}
"#,
    )
    .expect("zero-delta reload control");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Zero-delta reload"),
    )
    .expect("reload is consumed at the Update(0) boundary");
    assert_eq!(compiled.startup_reloads, 1);
    assert!(compiled.messages.iter().any(|event| event.message == "2"));
}

#[test]
fn startup_reload_bounds_endless_requests() {
    let song_dir = test_dir("startup-reload-endless");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        "return Def.ActorFrame{OnCommand=function() SCREENMAN:SetNewScreen('ScreenGameplay') end}",
    )
    .expect("endless reload control");
    let error = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Endless reload"),
    )
    .expect_err("do not hang the loader on a script that always reloads");
    assert!(error.contains("128 startup gameplay reloads"), "{error}");
}

#[test]
fn compile_song_lua_accepts_screen_transition_and_sm_helpers() {
    let song_dir = test_dir("screen-transition-sm");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        SM("hello")
        SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen")
        mod_actions = {
            {1, "ok", true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Screen Transition"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
}

#[test]
fn compile_song_lua_exposes_common_prefsmgr_preferences() {
    let song_dir = test_dir("prefsmgr-preferences");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
mod_actions = {
    {
        1,
        string.format(
            "%.4f:%d:%d:%s:%.2f:%.2f",
            PREFSMAN:GetPreference("DisplayAspectRatio"),
            PREFSMAN:GetPreference("DisplayWidth"),
            PREFSMAN:GetPreference("DisplayHeight"),
            tostring(string.find(string.lower(PREFSMAN:GetPreference("VideoRenderers")), "opengl") ~= nil),
            PREFSMAN:GetPreference("BGBrightness"),
            PREFSMAN:GetPreference("GlobalOffsetSeconds")
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "PrefsMgr Preferences");
    context.screen_width = 1280.0;
    context.screen_height = 720.0;
    context.display_width = 1280.0;
    context.display_height = 720.0;
    context.global_offset_seconds = 0.02;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1.7778:1280:720:false:0.70:0.02"
    );
}

#[test]
fn compile_song_lua_video_driver_preference_is_known_and_mutable() {
    let song_dir = test_dir("video-driver-preference");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
assert(PREFSMAN:PreferenceExists("LastSeenVideoDriver"), "native video driver preference must exist")
assert(PREFSMAN:GetPreference("LastSeenVideoDriver") == "", "native fresh preference is an empty string")
PREFSMAN:SetPreference("LastSeenVideoDriver", "NVIDIA Test Adapter")
assert(PREFSMAN:GetPreference("LastSeenVideoDriver") == "NVIDIA Test Adapter")
assert(string.find(string.lower(PREFSMAN:GetPreference("LastSeenVideoDriver")), "nvidia"))
return Def.ActorFrame{}
"#).unwrap();
    let context = SongLuaCompileContext::new(&song_dir, "Video Driver");
    test_compile_song_lua(&entry, &context).expect("native video driver preference must exist");

    fs::write(
        &entry,
        r#"
local driver = PREFSMAN:GetPreference("LastSeenVideoDriver")
local nvidia = string.find(string.lower(driver), "nvidia") ~= nil
assert(PREFSMAN:PreferenceExists("lastseenvideodriver"))
assert(PREFSMAN:GetPreference("lastseenvideodriver") == driver)
assert(PREFSMAN:GetPreference("VideoRenderers") == "null")
mod_actions = {{1, driver .. ":" .. tostring(nvidia), true}}
return Def.ActorFrame{}
"#,
    )
    .unwrap();
    let mut context = context;
    context.video_renderers = "null".into();
    for (driver, expected) in [
        ("NVIDIA Test Adapter", "NVIDIA Test Adapter:true"),
        ("OpenGL", "OpenGL:false"),
    ] {
        context.last_seen_video_driver = driver.into();
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages[0].message, expected);
    }
}

#[test]
fn compile_song_lua_respects_video_renderer_aft_guard() {
    let song_dir = test_dir("video-renderer-aft-guard");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local renderers = string.lower(PREFSMAN:GetPreference("VideoRenderers"))
local show_afts = string.find(renderers, "opengl") ~= nil
local root = Def.ActorFrame{}
if show_afts then
    root[#root + 1] = Def.ActorFrameTexture{Name="ScreenTex"}
    root[#root + 1] = Def.Quad{}
end
return root
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "AFT Guard");
    context.video_renderers = "Vulkan (wgpu)".to_string();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.overlays.is_empty());

    context.video_renderers = "opengl".to_string();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(
        compiled
            .overlays
            .iter()
            .any(|overlay| matches!(overlay.kind, TestOverlayKind::ActorFrameTexture { .. }))
    );
}

#[test]
fn compile_song_lua_exposes_after_dark_runtime_helpers() {
    let song_dir = test_dir("after-dark-runtime-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local leaf = nil

return Def.ActorFrame{
    OnCommand=function(self)
        local spline = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField"):GetColumnActors()[1]:GetPosHandler():GetSpline()
        local polygonal = spline:SetPolygonal(true) ~= nil
        self:runcommandsonleaves(function(actor)
            actor:visible(false)
        end)
        mod_actions = {
            {1, string.format(
                "%s:%.2f:%s:%s",
                GAMESTATE:GetCurrentStyle():GetName(),
                GAMESTATE:GetSongBPS(),
                tostring(leaf:GetVisible()),
                tostring(polygonal)
            ), true},
        }
    end,
    Def.ActorFrame{
        Def.Quad{
            InitCommand=function(self)
                leaf = self
            end,
        },
    },
}
"#,
        )
        .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "After Dark Helpers");
    context.song_display_bpms = [120.0, 180.0];
    context.style_name = "double".to_string();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "double:3.00:false:true");
}

#[test]
fn compile_song_lua_exposes_scale_helper() {
    let song_dir = test_dir("scale-helper");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local WideScale = function(AR4_3, AR16_9)
    local w = 480 * PREFSMAN:GetPreference("DisplayAspectRatio")
    return scale(w, 640, 854, AR4_3, AR16_9)
end

mod_actions = {
    {1, string.format("%.2f", WideScale(100, 200)), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Scale Helper");
    context.screen_width = 1280.0;
    context.screen_height = 720.0;
    context.display_width = 1280.0;
    context.display_height = 720.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "199.69");
}

#[test]
fn compile_song_lua_exposes_difficulty_enum_globals() {
    let song_dir = test_dir("difficulty-enum");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s",
            ToEnumShortString(Difficulty[1]),
            ToEnumShortString(Difficulty[#Difficulty]),
            ToEnumShortString(GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty()),
            Difficulty[4]
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Difficulty Enum");
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "Beginner:Edit:Hard:Difficulty_Hard"
    );
}

#[test]
fn compile_song_lua_exposes_gamestate_easiest_steps_difficulty() {
    let song_dir = test_dir("easiest-steps-difficulty");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, ToEnumShortString(GAMESTATE:GetEasiestStepsDifficulty()), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Easiest Steps Difficulty");
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    context.players[1].difficulty = SongLuaDifficulty::Medium;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Medium");
}

#[test]
fn compile_song_lua_supports_center_methods() {
    let song_dir = test_dir("actor-center-methods");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:CenterX()
            self:CenterY()
            self:Center()
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetX(), self:GetY()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Actor Center Methods");
    context.screen_width = 1280.0;
    context.screen_height = 720.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "640:360");
}

#[test]
fn compile_song_lua_supports_hibernate_chain_method() {
    let song_dir = test_dir("actor-hibernate");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:hibernate(0):diffusealpha(0.25):sleep(1)
            mod_actions = {
                {1, string.format("%.2f", self:GetDiffuseAlpha()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Hibernate"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0.25");
}

#[test]
fn hibernate_active_tween() {
    let song_dir = test_dir("hibernate-active-tween");
    let entry = song_dir.join("default.lua");
    // This assertion runs cleanly in linked ITGmania 0.1.7. Hibernation
    // freezes an already active tween while preserving GetVisible().
    fs::write(
        &entry,
        r#"
local held
return Def.ActorFrame{
    OnCommand=function(self)
        local actor=self:GetChild("Sleeping")
        self:SetUpdateFunction(function()
            local beat=GAMESTATE:GetSongBeat()
            if beat >= .05 and not held then
                held=actor:getaux()
                actor:hibernate(.125)
            elseif held and beat < .175 then
                assert(actor:getaux()==held, "runtime hibernation freezes the active tween")
                assert(actor:GetVisible(), "runtime hibernation preserves own visibility")
            end
        end)
    end,
    Def.Quad{
        Name="Sleeping",
        OnCommand=function(self) self:setsize(16,16):linear(.25):aux(1):diffusealpha(0) end
    }
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Native runtime hibernation probe");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{:?}",
        compiled.info
    );
    let actor = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Sleeping"))
        .unwrap();
    let startup = actor
        .message_commands
        .iter()
        .find(|command| command.message == "__songlua_actor_startup")
        .unwrap();
    let state = overlay_state_after_blocks(actor.initial_state, &startup.blocks, 0.25);
    assert!(
        (state.diffuse[3] - 0.5).abs() < 1.0e-6,
        "{:?}",
        state.diffuse
    );
}

#[test]
fn hibernate_startup_tweens() {
    let song_dir = test_dir("hibernate-startup-tweens");
    let entry = song_dir.join("default.lua");
    // Expected frame-eight values are from the linked native Actor fixture
    // hibernate-rate.lua: parent rate 2, child rate 3, wake remainder 1/120.
    fs::write(
            &entry,
            r#"
local calls = 0
return Def.ActorFrame{
    OnCommand=function(self)
        local sleeping = self:GetChild("Sleeping")
        sleeping:SetUpdateFunction(function(actor, delta)
            calls = calls + 1
            assert(actor:GetVisible())
            if calls == 1 then
                assert(math.abs(delta - 1/60) < 0.000001)
                assert(actor:GetDiffuseAlpha() == 0, "GetDiffuseAlpha reads the queued destination")
                assert(actor:GetChild("Child"):GetDiffuseAlpha() == 0, "child getter reads the queued destination")
            end
        end)
        self:SetUpdateFunction(function()
            if GAMESTATE:GetSongBeat() == 0 then
                assert(sleeping:GetTweenTimeLeft() == 0.375)
                assert(self:GetTweenTimeLeft() == 0.375)
            end
            if GAMESTATE:GetSongBeat() < 0.125 then
                assert(calls == 0)
                assert(sleeping:getaux() == 0, "sleeping aux "..sleeping:getaux())
                assert(sleeping:GetVisible())
            end
        end)
    end,
    Def.ActorFrame{
        Name="Sleeping",
        OnCommand=function(self)
            self:SetUpdateRate(2):hibernate(0.125):linear(0.25):aux(1):diffusealpha(0)
        end,
        Def.ActorFrame{
            Name="Child",
            OnCommand=function(self) self:SetUpdateRate(3):linear(0.25):diffusealpha(0) end,
        },
    },
}
"#,
        )
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Hibernate Startup Tweens");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{:?}",
        compiled.info
    );
    for (name, alpha) in [("Sleeping", 14.0 / 15.0), ("Child", 0.8)] {
        let actor = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(name))
            .unwrap();
        let startup = actor
            .message_commands
            .iter()
            .find(|command| command.message == "__songlua_actor_startup")
            .unwrap();
        let before = overlay_state_after_blocks(actor.initial_state, &startup.blocks, 7.0 / 60.0);
        assert_eq!(before.diffuse[3], 1.0, "{name} advances while sleeping");
        let wake = overlay_state_after_blocks(actor.initial_state, &startup.blocks, 8.0 / 60.0);
        assert!(
            (wake.diffuse[3] - alpha).abs() < 1.0e-6,
            "{name}: {:?}",
            wake.diffuse
        );
    }
}

#[test]
fn hibernate_keeps_visible() {
    let song_dir = test_dir("actor-hibernate-visibility");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        PulseMessageCommand=function(self)
            self:hibernate(0.5):diffusealpha(0.25)
            assert(self:GetVisible())
        end,
    },
}
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Hibernate Visibility"),
    )
    .unwrap();
    assert!(!compiled.overlays[0].initial_state.hibernating);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Pulse");
    assert!(
        command
            .blocks
            .iter()
            .all(|block| block.delta.visible.is_none())
    );
    let state =
        overlay_state_after_blocks(compiled.overlays[0].initial_state, &command.blocks, 0.0);
    assert!(state.visible);
    assert!(state.hibernating);
    assert!(!state.draw_visible());
    assert_eq!(state.diffuse[3], 0.25);
}

#[test]
fn compile_song_lua_supports_fullscreen_method() {
    let song_dir = test_dir("actor-fullscreen");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:FullScreen():Center()
            assert(self:GetZoom() == 1280 and self:GetZoomX() == 1280 and self:GetZoomY() == 720)
            assert(self:GetZoomedWidth() == 1280 and self:GetZoomedHeight() == 720)
            mod_actions = {
                {1, string.format("%.0f:%.0f:%.0f:%.0f", self:GetX(), self:GetY(), self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Actor FullScreen");
    context.screen_width = 1280.0;
    context.screen_height = 720.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "640:360:1:1");
}

#[test]
fn compile_song_lua_stretch_keeps_native_intrinsic_size_and_scale() {
    let song_dir = test_dir("native-stretch-scale");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"return Def.Quad { OnCommand=function(native_target)

native_target:setsize(1440,1080):zoom(2)
native_target:stretchto(0,0,640,480)
assert(native_target:GetWidth() == 1440 and native_target:GetHeight() == 1080)
assert(native_target:GetZoom() == native_target:GetZoomX())
assert(native_target:GetZoom() == 0.4444444477558136)
assert(native_target:GetZoomedWidth() == 640 and native_target:GetZoomedHeight() == 480)
assert(native_target:GetX() == 320 and native_target:GetY() == 240)
native_target:zoomx(0.25)
assert(native_target:GetZoom() == 0.25 and native_target:GetZoomY() == 0.4444444477558136)
native_target:zoom(1.1)
assert(native_target:GetZoom() == 1.100000023841858 and native_target:GetZoomY() == 1.100000023841858)
assert(native_target:GetZoomedWidth() == 1584)
native_target:stretchto(10,20,330,260)
assert(native_target:GetWidth() == 1440 and native_target:GetHeight() == 1080)
assert(native_target:GetX() == 170 and native_target:GetY() == 140)
assert(native_target:GetZoomedWidth() == 320 and native_target:GetZoomedHeight() == 240)

        end }"#).unwrap();
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native StretchTo"),
    )
    .unwrap();
}

#[test]
fn compile_song_lua_supports_additive_transform_methods() {
    let song_dir = test_dir("actor-additive-transforms");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:x(10):addx(5)
            self:y(20):addy(-3)
            self:z(4):addz(6)
            self:rotationx(15):addrotationx(5)
            self:rotationy(25):addrotationy(10)
            self:rotationz(45):addrotationz(90)
            mod_actions = {
                {1, string.format(
                    "%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
                    self:GetX(),
                    self:GetY(),
                    self:GetZ(),
                    self:GetRotationX(),
                    self:GetRotationY(),
                    self:GetRotationZ()
                ), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Additive Transforms"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "15:17:10:20:35:135");
}

#[test]
fn compile_song_lua_reads_sprite_image_dimensions() {
    let song_dir = test_dir("sprite-dimensions");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(10, 20).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Dimensions"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "10:20");
    assert!(compiled.overlays[0].initial_state.sprite_animate);
}

#[test]
fn compile_song_lua_reads_native_indexed_dimensions() {
    let song_dir = test_dir("native-indexed-dimensions");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files");
    let entry = song_dir.join("default.lua");
    for (name, expected) in [
        ("gif-offset.gif", "16:24:16:24:16:32"),
        ("gif-first-frame.gif", "16:24:16:24:16:32"),
        ("os2-4.bmp", "8:8:8:8:8:8"),
    ] {
        fs::copy(root.join(name), song_dir.join(name)).expect("copy native indexed file");
        fs::write(
            &entry,
            format!(
                r#"
return Def.Sprite{{
    Texture="{name}",
    OnCommand=function(self)
        local texture = self:GetTexture()
        mod_actions = {{{{1, string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
            self:GetWidth(), self:GetHeight(),
            texture:GetSourceWidth(), texture:GetSourceHeight(),
            texture:GetTextureWidth(), texture:GetTextureHeight()), true}}}}
    end,
}}
"#
            ),
        )
        .expect("indexed Sprite Lua");
        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Native Indexed Dimensions"),
        )
        .expect("compile indexed Sprite source getters");
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, expected, "{name}");
    }
}

#[test]
fn compile_song_lua_tiny_image_offsets() {
    let song_dir = test_dir("tiny-image-offsets");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/texture-sprite-preparation/bitmap-sprite-preparation");
    fs::copy(root.join("tiny.png"), song_dir.join("tiny.png")).expect("native tiny input");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.Sprite{Texture="tiny.png", OnCommand=function(self)
    mod_actions={{1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true}}
    self:addimagecoords(1,2)
end}
"#,
    )
    .expect("tiny image offset Lua");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native Tiny Offsets"),
    )
    .expect("compile offsets");
    assert_eq!(compiled.messages[0].message, "3:2");
    // Sprite.cpp AddImageCoords divides by native allocation axes. This
    // native 3x2 input fills its prepared 8x8 allocation (bitmap golden).
    assert_eq!(
        compiled.overlays[0].initial_state.custom_texture_rect,
        Some([0.125, 0.25, 1.125, 1.25])
    );
}

#[test]
fn compile_song_lua_sprite_resolution_hints() {
    let song_dir = test_dir("sprite-resolution-hints");
    let entry = song_dir.join("default.lua");
    for (name, expected) in [
        ("panel 4x2.png", "32:32:128:64:128:64"),
        ("panel 4x2 (doubleres).png", "16:16:64:32:128:64"),
        ("panel 4x2 (res 80x48).png", "20:24:80:48:128:64"),
        (
            "panel 4x2 (res 80x48) (doubleres).png",
            "10:12:40:24:128:64",
        ),
    ] {
        image::RgbaImage::new(128, 64)
            .save(song_dir.join(name))
            .unwrap();
        fs::write(
            &entry,
            format!(
                r#"
return Def.Sprite{{
    Texture="{name}",
    OnCommand=function(self)
        local texture = self:GetTexture()
        mod_actions = {{{{1, string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
            self:GetWidth(), self:GetHeight(),
            texture:GetSourceWidth(), texture:GetSourceHeight(),
            texture:GetTextureWidth(), texture:GetTextureHeight()), true}}}}
        self:setstate(0):addimagecoords(16, 8)
    end,
}}
"#
            ),
        )
        .unwrap();
        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Sprite Resolution Hints"),
        )
        .unwrap();
        assert_eq!(compiled.messages[0].message, expected, "{name}");
        // Pixel-coordinate offsets still use the physical texture dimensions.
        assert_eq!(
            compiled.overlays[0].initial_state.custom_texture_rect,
            Some([0.125, 0.125, 0.375, 0.625]),
            "{name}"
        );
    }
}

#[test]
fn compile_song_lua_setstate_uses_sprite_sheet_cell_size() {
    let song_dir = test_dir("sprite-setstate");
    let image_path = song_dir.join("panel 4x3.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        OnCommand=function(self)
            self:setstate(5)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite SetState"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "10:10");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.sprite_state_index,
        Some(5)
    );
    assert_eq!(compiled.overlays[0].initial_state.custom_texture_rect, None);
}

#[test]
fn compile_song_lua_tracks_sprite_animation_state() {
    let song_dir = test_dir("sprite-animate");
    let image_path = song_dir.join("panel 4x3.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        Frame0000=2, Delay0000=1,
        Frame0001=5, Delay0001=0.25,
        OnCommand=function(self)
            self:setstate(1):animate(true):SetAllStateDelays(0.5)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Animate"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "10:10");
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert!(state.sprite_animate);
    assert!(state.sprite_loop);
    assert_eq!(state.sprite_playback_rate, 1.0);
    assert_eq!(state.sprite_state_delay, 0.5);
    assert_eq!(state.sprite_state_index, Some(1));
    assert_eq!(state.custom_texture_rect, None);
    let SongLuaOverlayKind::Sprite { states, .. } = &compiled.overlays[0].kind else {
        panic!("expected sprite overlay");
    };
    assert_eq!(
        states.as_ref(),
        [
            SongLuaSpriteState {
                frame: 2,
                delay: 1.0,
            },
            SongLuaSpriteState {
                frame: 5,
                delay: 0.25,
            },
        ]
    );
}

#[test]
fn compile_song_lua_loadactor_exposes_texture_proxy_methods() {
    let song_dir = test_dir("loadactor-texture-proxy");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(12, 34).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local loaded = nil

return Def.ActorFrame{
    LoadActor("panel.png")..{
        OnCommand=function(self)
            loaded = self
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            self:SetTexture(loaded:GetTexture())
            local texture = self:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%.0f:%.0f",
                    tostring(texture:GetPath():match("panel%.png$") ~= nil),
                    texture:GetSourceWidth(),
                    texture:GetSourceHeight()
                ), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Texture Proxy"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:12:34");
}

#[test]
fn compile_song_lua_loadactor_resolves_extensionless_image() {
    let song_dir = test_dir("loadactor-image-no-ext");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&lua_dir).unwrap();
    let image_path = lua_dir.join("panel.png");
    image::RgbaImage::new(12, 34).save(&image_path).unwrap();
    let entry = lua_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local loaded = nil

return Def.ActorFrame{
    LoadActor("panel")..{
        OnCommand=function(self)
            loaded = self
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            local texture = loaded:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%.0f:%.0f",
                    tostring(texture:GetPath():match("panel%.png$") ~= nil),
                    texture:GetSourceWidth(),
                    texture:GetSourceHeight()
                ), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor NoExt Image"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:12:34");
}

#[test]
fn compile_song_lua_loadactor_autowildcards_sprite_hints() {
    let song_dir = test_dir("loadactor-autowildcard-sprite-hints");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&lua_dir).unwrap();
    let image_path = lua_dir.join("overlay 3x4.png");
    image::RgbaImage::new(12, 8).save(&image_path).unwrap();
    let entry = lua_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    LoadActor("overlay")..{
        OnCommand=function(self)
            mod_actions = {
                {1, tostring(self:GetNumStates()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Autowildcard"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "12");
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("overlay 3x4.png")
    ));
}

#[test]
fn compile_song_lua_loadactor_rejects_ambiguous_autowildcards() {
    let song_dir = test_dir("loadactor-ambiguous-autowildcard");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&lua_dir).unwrap();
    image::RgbaImage::new(1, 1)
        .save(lua_dir.join("overlay 2x2.png"))
        .unwrap();
    image::RgbaImage::new(1, 1)
        .save(lua_dir.join("overlay 3x4.png"))
        .unwrap();
    let entry = lua_dir.join("default.lua");
    fs::write(&entry, "return LoadActor(\"overlay\")").unwrap();

    let error = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Ambiguous Autowildcard"),
    )
    .unwrap_err();
    assert!(error.contains("multiple matches"), "{error}");
    assert!(error.contains("overlay 2x2.png"), "{error}");
    assert!(error.contains("overlay 3x4.png"), "{error}");
}

#[test]
fn loadactor_varargs() {
    let song_dir = test_dir("loadactor-varargs");
    let parts = song_dir.join("parts");
    fs::create_dir_all(&parts).unwrap();
    fs::write(
        parts.join("factory.lua"),
        "local n = ...; return function(x) return x*n end",
    )
    .unwrap();
    fs::write(parts.join("broken.lua"), "assert(..., 'child assertion')").unwrap();
    fs::write(
        parts.join("child.lua"),
        r#"
assert(select('#', ...) == 4)
local width, empty, data, f = ...
assert(empty == nil and data.value == 7 and type(f) == 'function')
return Def.Quad { OnCommand=function(self) self:setsize(width, f(data.value)) end }
"#,
    )
    .unwrap();
    fs::write(
        parts.join("bridge.lua"),
        r#"
local ok, err = pcall(LoadActor, 'broken', false)
assert(not ok and string.find(tostring(err), 'child assertion'))
return LoadActor('child', ...)
"#,
    )
    .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local f = LoadActor('parts/factory', 3)
return LoadActor('parts/bridge', 12, nil, {value=7}, f)
"#,
    )
    .unwrap();
    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Varargs")).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.size, Some([12.0, 21.0]));
    fs::write(&entry, "return LoadActor('parts/broken', false)").unwrap();
    let error = test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Assertion"))
        .unwrap_err();
    assert!(error.contains("child assertion"), "{error}");
}

#[test]
fn compile_song_lua_loadactor_resolves_extensionless_script() {
    let song_dir = test_dir("loadactor-script-no-ext");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&lua_dir).unwrap();
    fs::write(
        lua_dir.join("child.lua"),
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(12, 34)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();
    let entry = lua_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    LoadActor("child"),
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor NoExt Script"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "12:34");
}

#[test]
fn compile_song_lua_loadactor_treats_binary_video_as_media() {
    let song_dir = test_dir("loadactor-video-media");
    let video_path = song_dir.join("clip.mp4");
    fs::write(&video_path, [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    LoadActor("clip.mp4")..{
        OnCommand=function(self)
            local texture = self:GetTexture()
            mod_actions = {
                {1, string.format(
                    "%s:%s",
                    tostring(texture:GetPath():match("clip%.mp4$") ~= nil),
                    tostring(texture:GetSourceWidth() > 0 and texture:GetSourceHeight() > 0)
                ), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Video Media"),
    )
    .unwrap();
    assert!(compiled.messages.iter().any(|event| {
        event.message != SONG_LUA_STARTUP_MESSAGE && event.message == "true:true"
    }));
}

#[test]
fn compile_song_lua_supports_sprite_decode_movie_methods() {
    let song_dir = test_dir("sprite-decode-movie");
    fs::write(song_dir.join("clip.mp4"), [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    LoadActor("clip.mp4")..{
        OnCommand=function(self)
            local before = self:GetDecodeMovie()
            self:SetDecodeMovie(false)
            mod_actions = {
                {1, string.format("%s:%s", tostring(before), tostring(self:GetDecodeMovie())), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Decode Movie"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:false");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(!compiled.overlays[0].initial_state.decode_movie);
}

#[test]
fn compile_song_lua_loadactor_treats_binary_audio_as_media() {
    let song_dir = test_dir("loadactor-audio-media");
    let audio_path = song_dir.join("clip.ogg");
    fs::write(&audio_path, [0xff_u8, 0xd8, 0x00, 0x81]).unwrap();
    fs::write(song_dir.join("other.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    LoadActor("clip.ogg")..{
        OnCommand=function(self)
            self:play():pause():stop():load("other.ogg"):volume(0.5)
            mod_actions = {
                {1, string.format(
                    "%s:%s",
                    tostring(self.File == "other.ogg"),
                    tostring(self:GetTexture() == nil)
                ), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Audio Media"),
    )
    .unwrap();
    assert!(compiled.messages.iter().any(|event| {
        event.message != SONG_LUA_STARTUP_MESSAGE && event.message == "true:true"
    }));
    assert!(compiled.overlays.iter().any(|actor| matches!(
            &actor.kind, SongLuaOverlayKind::Sound { sound_path }
                if sound_path == &song_dir.join("other.ogg"))));
    assert_eq!(
        test_compiled_sound_calls(&compiled),
        vec![(0.0, audio_path)],
        "the play call keeps its binary audio file across a later load"
    );
}

#[test]
fn compile_song_lua_supports_animate_loop_rate_chain_methods() {
    let song_dir = test_dir("actor-animate-loop-rate");
    let image_path = song_dir.join("panel 4x3.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel 4x3.png",
        OnCommand=function(self)
            local texture = self:GetTexture()
            texture:loop(false):rate(1.5)
            self:setstate(2):position(0):play():pause():play():diffusealpha(0.2)
            mod_actions = {
                {1, string.format("%.2f:%d:%d", self:GetDiffuseAlpha(), self:GetNumStates(), texture:GetNumFrames()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Animate Loop Rate"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0.20:12:12");
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert!(state.sprite_animate);
    assert!(!state.sprite_loop);
    assert_eq!(state.sprite_playback_rate, 1.5);
    assert_eq!(state.sprite_state_index, Some(0));
}

#[test]
fn compile_song_lua_supports_sprite_load_and_text_compat_methods() {
    let song_dir = test_dir("sprite-load-text-compat");
    image::RgbaImage::new(10, 20)
        .save(song_dir.join("first.png"))
        .unwrap();
    image::RgbaImage::new(30, 40)
        .save(song_dir.join("second.png"))
        .unwrap();
    image::RgbaImage::new(50, 10)
        .save(song_dir.join("banner.png"))
        .unwrap();
    image::RgbaImage::new(90, 30)
        .save(song_dir.join("cached-banner.png"))
        .unwrap();
    image::RgbaImage::new(40, 40)
        .save(song_dir.join("sheet 2x2.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Sprite{
        Texture="first.png",
        OnCommand=function(self)
            self:Load("second.png")
            self:LoadBanner("banner.png")
            self:LoadBackground("second.png")
            self:LoadFromCached("Banner", "sheet 2x2.png")
            self:SetAllStateDelays(0.25):SetSecondsIntoAnimation(0.6):SetEffectMode("Normal")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%d:%.2f:%.0f:%.0f",
                    tostring(Sprite.LoadFromCached ~= nil),
                    tostring(texture:GetPath():match("sheet 2x2%.png$") ~= nil),
                    self:GetState(),
                    self:GetAnimationLengthSeconds(),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromCachedBanner("cached-banner.png")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%.0f:%.0f",
                    tostring(Banner.LoadFromCachedBanner ~= nil and Sprite.LoadFromCachedBanner == nil),
                    tostring(texture:GetPath():match("cached%-banner%.png$") ~= nil),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="TEXT",
        OnCommand=function(self)
            self:strokecolor(color("0.2,0.3,0.4,0.5"))
                :max_dimension_use_zoom(true)
                :textglowmode("Stroke")
                :set_mult_attrs_with_diffuse(true)
            local stroke = self:getstrokecolor()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%.1f:%.1f:%.1f:%.1f:%s",
                    stroke[1],
                    stroke[2],
                    stroke[3],
                    stroke[4],
                    tostring(self:get_mult_attrs_with_diffuse())
                ),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Load Text Compat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 3);
    assert_eq!(compiled.messages[0].message, "true:true:2:1.00:20:20");
    assert_eq!(compiled.messages[1].message, "true:true:90:30");
    assert_eq!(compiled.messages[2].message, "0.2:0.3:0.4:0.5:true");
    assert_eq!(compiled.overlays.len(), 3);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("sheet 2x2.png")
    ));
    assert_eq!(
        compiled.overlays[0].initial_state.sprite_state_index,
        Some(2)
    );
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("cached-banner.png")
    ));
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::BitmapText {
            stroke_color: Some([0.2, 0.3, 0.4, 0.5]),
            ..
        }
    ));
    assert_eq!(
        compiled.overlays[2].initial_state.text_glow_mode,
        SongLuaTextGlowMode::Stroke
    );
    assert!(compiled.overlays[2].initial_state.mult_attrs_with_diffuse);
}

#[test]
fn compile_song_lua_supports_banner_cached_load_aliases() {
    let song_dir = test_dir("banner-cached-load-aliases");
    image::RgbaImage::new(64, 24)
        .save(song_dir.join("rank-banner.png"))
        .unwrap();
    image::RgbaImage::new(120, 80)
        .save(song_dir.join("background.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Banner{
        InitCommand=function(self)
            self:LoadFromCachedBanner("rank-banner.png")
            mod_actions[#mod_actions + 1] = {
                1,
                string.format("%s:%s:%d:%d", self:GetName(), tostring(Banner.LoadFromCachedBanner ~= nil and Sprite.LoadFromCachedBanner == nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
    Def.Sprite{
        InitCommand=function(self)
            Sprite.LoadFromCached(self, "Background", "background.png")
            mod_actions[#mod_actions + 1] = {
                1,
                string.format("%s:%d:%d", tostring(Sprite.LoadFromCached ~= nil and Sprite.LoadFromCachedBackground == nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Banner Cached Load Aliases"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 2);
    assert_eq!(compiled.messages[0].message, ":true:64:24");
    assert_eq!(compiled.messages[1].message, "true:120:80");
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("rank-banner.png")
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("background.png")
    ));
}

#[test]
fn compile_song_lua_supports_song_and_course_sprite_loads() {
    let song_dir = test_dir("song-course-sprite-loads");
    image::RgbaImage::new(48, 16)
        .save(song_dir.join("banner.png"))
        .unwrap();
    image::RgbaImage::new(80, 60)
        .save(song_dir.join("background.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
mod_actions = {}

local function note(name, sprite)
    local texture = sprite:GetTexture()
    mod_actions[#mod_actions + 1] = {
        1,
        string.format(
            "%s:%s:%d:%d:%s",
            name,
            tostring(texture:GetPath():match(name == "background" and "background%.png$" or "banner%.png$") ~= nil),
            sprite:GetWidth(),
            sprite:GetHeight(),
            tostring(Banner.LoadFromSong ~= nil and Banner.LoadFromCourse ~= nil and Banner.LoadFromSongGroup ~= nil
                and Sprite.LoadFromSong == nil and Sprite.LoadFromCourse == nil and Sprite.LoadFromSongGroup == nil)
        ),
        true,
    }
end

return Def.ActorFrame{
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromSong(GAMESTATE:GetCurrentSong())
            note("song", self)
        end,
    },
    Def.Sprite{
        OnCommand=function(self)
            self:LoadFromSongBackground(GAMESTATE:GetCurrentSong())
            note("background", self)
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromCourse(GAMESTATE:GetCurrentCourse())
            note("course", self)
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadFromSongGroup(GAMESTATE:GetCurrentSong():GetGroupName())
            note("group", self)
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Song Course Sprite Loads"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 4);
    assert_eq!(compiled.messages[0].message, "song:true:48:16:true");
    assert_eq!(compiled.messages[1].message, "background:true:80:60:true");
    assert_eq!(compiled.messages[2].message, "course:true:48:16:true");
    assert_eq!(compiled.messages[3].message, "group:true:48:16:true");
    assert_eq!(compiled.overlays.len(), 4);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("banner.png")
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("background.png")
    ));
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("banner.png")
    ));
    assert!(matches!(
        compiled.overlays[3].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("banner.png")
    ));
}

#[test]
fn compile_song_lua_supports_banner_luna_methods() {
    let song_dir = test_dir("banner-luna-methods");
    for (file_name, width, height) in [
        ("icon.png", 24, 24),
        ("card.png", 32, 20),
        ("unlock-banner.png", 64, 18),
        ("unlock-bg.png", 96, 54),
    ] {
        image::RgbaImage::new(width, height)
            .save(song_dir.join(file_name))
            .unwrap();
    }
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
mod_actions = {}

local character = {}
function character:GetIconPath() return "icon.png" end
function character:GetCardPath() return "card.png" end

local unlock = {}
function unlock:GetBannerFile() return "unlock-banner.png" end
function unlock:GetBackgroundFile() return "unlock-bg.png" end

local function note(name, sprite, pattern)
    local texture = sprite:GetTexture()
    mod_actions[#mod_actions + 1] = {
        1,
        string.format(
            "%s:%s:%s:%.2f:%d:%d",
            name,
            tostring(texture:GetPath():match(pattern) ~= nil),
            tostring(sprite:GetScrolling()),
            sprite:GetPercentScrolling(),
            sprite:GetWidth(),
            sprite:GetHeight()
        ),
        true,
    }
end

return Def.ActorFrame{
    Def.Banner{
        OnCommand=function(self)
            self:SetScrolling(true, 0.25)
            self:LoadFromSortOrder("SortOrder_Recent_P1")
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "sort:%s:%.2f:%s:%s",
                    tostring(self:GetScrolling()),
                    self:GetPercentScrolling(),
                    tostring(texture:GetPath():match("__songlua_theme_path/G/Banner/Recent$") ~= nil),
                    tostring(Banner.LoadFromSortOrder ~= nil and Sprite.LoadFromSortOrder == nil and Banner.SetScrolling ~= nil and Sprite.SetScrolling == nil and Banner.GetScrolling ~= nil and Banner.GetPercentScrolling ~= nil and Sprite.GetScrolling == nil and Sprite.GetPercentScrolling == nil)
                ),
                true,
            }
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadIconFromCharacter(character)
            note("icon", self, "icon%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadCardFromCharacter(character)
            note("card", self, "card%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadBannerFromUnlockEntry(unlock)
            note("unlock-banner", self, "unlock%-banner%.png$")
        end,
    },
    Def.Banner{
        OnCommand=function(self)
            self:LoadBackgroundFromUnlockEntry(unlock)
            note("unlock-bg", self, "unlock%-bg%.png$")
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Banner Luna Methods"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 5);
    assert_eq!(compiled.messages[0].message, "sort:false:0.00:true:true");
    assert_eq!(compiled.messages[1].message, "icon:true:false:0.00:24:24");
    assert_eq!(compiled.messages[2].message, "card:true:false:0.00:32:20");
    assert_eq!(
        compiled.messages[3].message,
        "unlock-banner:true:false:0.00:64:18"
    );
    assert_eq!(
        compiled.messages[4].message,
        "unlock-bg:true:false:0.00:96:54"
    );
    assert_eq!(compiled.overlays.len(), 4);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("icon.png")
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("card.png")
    ));
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("unlock-banner.png")
    ));
    assert!(matches!(
        compiled.overlays[3].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("unlock-bg.png")
    ));
}

#[test]
fn compile_song_lua_supports_texture_translate_and_wrapping() {
    let song_dir = test_dir("actor-texture-translate-wrap");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:texturetranslate(0.25, -0.5):texturewrapping(true)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Texture Translate Wrap"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "40:30");
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert!(state.texture_wrapping);
    assert_eq!(state.texcoord_offset, Some([0.25, -0.5]));
    assert_eq!(state.custom_texture_rect, None);
}

#[test]
fn compile_song_lua_supports_sprite_texture_coord_helpers() {
    let song_dir = test_dir("sprite-texture-coord-helpers");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(100, 80).save(&image_path).unwrap();
    let sheet_path = song_dir.join("panel 2x2.png");
    image::RgbaImage::new(100, 80).save(&sheet_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:setstate(1):SetCustomImageRect(0.25, 0.5, 0.75, 1)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:customtexturerect(0, 0, 1, 1):stretchtexcoords(0.25, -0.5)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:addimagecoords(25, 20)
        end,
    },
    Def.Sprite{
        Texture="panel 2x2.png",
        OnCommand=function(self)
            self:setstate(1):addimagecoords(25, 20)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Texture Coord Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 4);
    assert_eq!(
        compiled.overlays[0].initial_state.sprite_state_index,
        Some(u32::MAX)
    );
    assert_eq!(
        compiled.overlays[0].initial_state.custom_texture_rect,
        Some([0.25, 0.5, 0.75, 1.0])
    );
    assert_eq!(
        compiled.overlays[1].initial_state.custom_texture_rect,
        Some([0.25, -0.5, 1.25, 0.5])
    );
    assert_eq!(
        compiled.overlays[2].initial_state.custom_texture_rect,
        Some([0.25, 0.25, 1.25, 1.25])
    );
    assert_eq!(
        compiled.overlays[3].initial_state.custom_texture_rect,
        Some([0.75, 0.25, 1.25, 0.75])
    );
}

#[test]
fn compile_song_lua_supports_sprite_fade_edges() {
    let song_dir = test_dir("actor-fade-edges");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:fadeleft(0.1):faderight(0.2):fadetop(0.3):fadebottom(0.4)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Fade Edges"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert_eq!(state.fadeleft, 0.1);
    assert_eq!(state.faderight, 0.2);
    assert_eq!(state.fadetop, 0.3);
    assert_eq!(state.fadebottom, 0.4);
}

#[test]
fn compile_song_lua_supports_overlay_skew_methods() {
    let song_dir = test_dir("actor-overlay-skew");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:skewx(0.25):skewy(-0.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Overlay Skew"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert!((state.skew_x - 0.25).abs() <= 0.000_1);
    assert!((state.skew_y + 0.5).abs() <= 0.000_1);
}

#[test]
fn compile_song_lua_supports_mask_methods() {
    let song_dir = test_dir("actor-mask-methods");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Source",
        OnCommand=function(self)
            self:zoomto(100, 100):MaskSource()
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="MASK",
        OnCommand=function(self)
            self:MaskDest()
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Mask Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    assert!(compiled.overlays[0].initial_state.mask_source);
    assert!(compiled.overlays[1].initial_state.mask_dest);
}

#[test]
fn compile_song_lua_supports_alignment_methods() {
    let song_dir = test_dir("actor-alignment-methods");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:halign(0):valign(1)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ALIGN",
        OnCommand=function(self)
            self:halign(1):valign(0):horizalign("right")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Alignment Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    let sprite = compiled.overlays[0].initial_state;
    assert_eq!(sprite.halign, 0.0);
    assert_eq!(sprite.valign, 1.0);

    let text = compiled.overlays[1].initial_state;
    assert_eq!(text.halign, 1.0);
    assert_eq!(text.valign, 0.0);
    assert_eq!(text.text_align, TextAlign::Right);
}

#[test]
fn compile_song_lua_supports_stepmania_alignment_enums() {
    let song_dir = test_dir("actor-alignment-enums");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:horizalign(HorizAlign_Left):vertalign(bottom)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ENUM",
        OnCommand=function(self)
            self:horizalign("HorizAlign_Right"):vertalign("VertAlign_Top")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Alignment Enums"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    let quad = compiled.overlays[0].initial_state;
    assert_eq!(quad.halign, 0.0);
    assert_eq!(quad.valign, 1.0);

    let text = compiled.overlays[1].initial_state;
    assert_eq!(text.halign, 1.0);
    assert_eq!(text.valign, 0.0);
    assert_eq!(text.text_align, TextAlign::Right);
}

#[test]
fn compile_song_lua_supports_shadow_methods() {
    let song_dir = test_dir("actor-shadow-methods");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:shadowlength(5):shadowcolor(0.1, 0.2, 0.3, 0.4)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="SHADOW",
        OnCommand=function(self)
            self:shadowlengthx(3):shadowlengthy(4)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Shadow Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);

    let sprite = compiled.overlays[0].initial_state;
    assert_eq!(sprite.shadow_len, [5.0, -5.0]);
    assert_eq!(sprite.shadow_color, [0.1, 0.2, 0.3, 0.4]);

    let text = compiled.overlays[1].initial_state;
    assert_eq!(text.shadow_len, [3.0, -4.0]);
    assert_eq!(text.shadow_color, [0.0, 0.0, 0.0, 0.5]);
}

#[test]
fn compile_song_lua_supports_glow_and_glowshift_methods() {
    let song_dir = test_dir("actor-glow-methods");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:glow(0.1, 0.2, 0.3, 0.4)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="GLOW",
        OnCommand=function(self)
            self:glowshift()
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Glow Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);

    let sprite = compiled.overlays[0].initial_state;
    assert_eq!(sprite.glow, [0.1, 0.2, 0.3, 0.4]);

    let text = compiled.overlays[1].initial_state;
    assert_eq!(
        text.effect_mode,
        deadlib_present::anim::EffectMode::GlowShift
    );
    assert_eq!(text.effect_color1, [1.0, 1.0, 1.0, 0.2]);
    assert_eq!(text.effect_color2, [1.0, 1.0, 1.0, 0.8]);
}

#[test]
fn compile_song_lua_accepts_vertex_diffuse_style_shims() {
    let song_dir = test_dir("actor-vertex-diffuse-shims");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r##"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffuseleftedge(0, 0, 0, 0.25)
                :diffuserightedge({1, 1, 1, 0.5})
                :diffusetopedge(color("#11223344"))
                :diffusebottomedge(0.8, 0.7, 0.6, 1)
                :diffuseupperleft(1, 0, 0, 1)
                :diffuseupperright(0, 1, 0, 1)
                :diffuselowerleft(0, 0, 1, 1)
                :diffuselowerright(1, 1, 0, 1)
            mod_actions = {
                {1, "ok", true},
            }
        end,
    },
}
"##,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Vertex Diffuse Shims"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
    assert_eq!(compiled.overlays.len(), 1);
    let colors = compiled.overlays[0].initial_state.vertex_colors.unwrap();
    // ITGmania Actor.h's diffuse state order is UL, UR, LL, LR.
    assert_eq!(colors[0], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(colors[1], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(colors[2], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(colors[3], [1.0, 1.0, 0.0, 1.0]);
}

#[test]
fn compile_song_lua_supports_actor_multi_vertex_shape() {
    let song_dir = test_dir("actor-multi-vertex-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r##"
local verts = {
    {{0, 0, 0}, color("#ff0000")},
    {{10, 0, 0}, {0, 1, 0, 1}},
    {{10, 10, 0}, {0, 0, 1, 1}},
    {{0, 10, 0}, {1, 1, 1, 1}},
}

return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetDrawState{Mode="DrawMode_Quads"}
                :SetNumVertices(#verts)
                :SetVertices(verts)
                :SetLineWidth(3)
            assert(self.GetDrawState == nil and self.GetLineWidth == nil)
            mod_actions = {{
                1,
                string.format("%s:%d", self:GetDestDrawMode(), self:GetNumVertices()),
                true,
            }}
        end,
    },
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetVertices{{{0, 0, 0}}, {{0, 10, 0}}}
                :SetDrawState{Mode="DrawMode_LineStrip"}:SetLineWidth(3)
        end,
    },
}
"##,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "DrawMode_Quads:4");
    assert_eq!(compiled.overlays.len(), 2);
    let SongLuaOverlayKind::ActorMultiVertex {
        vertices,
        texture_path,
        ..
    } = &compiled.overlays[0].kind
    else {
        panic!("expected ActorMultiVertex overlay");
    };
    assert!(texture_path.is_none());
    assert_eq!(vertices.len(), 6);
    assert_eq!(vertices[0].pos, [0.0, 0.0]);
    assert_eq!(vertices[0].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(vertices[5].pos, [0.0, 0.0]);
    let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &compiled.overlays[1].kind else {
        panic!("expected line strip overlay");
    };
    assert_eq!(vertices.len(), 6);
    let left = vertices
        .iter()
        .map(|vertex| vertex.pos[0])
        .fold(f32::INFINITY, f32::min);
    let right = vertices
        .iter()
        .map(|vertex| vertex.pos[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert_eq!(
        right - left,
        3.0,
        "native SetLineWidth controls drawn geometry"
    );
}

#[test]
fn capture_mesh_copies_vertex_input() {
    let song_dir = test_dir("capture-mesh-copy");
    let entry = song_dir.join("default.lua");
    for binding in [
        "self:GetParent():GetChild('Capture'):GetTexture()",
        "'Capture'",
    ] {
        fs::write(
            &entry,
            r#"
local vertices = {
    {{0, 0, 0}, {1, 1, 1, 1}, {0, 0}},
    {{32, 0, 0}, {1, 1, 1, 1}, {1, 0}},
    {{0, 32, 0}, {1, 1, 1, 1}, {0, 1}},
}
local state = {Mode="DrawMode_Triangles", First=1, Num=-1}
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="Capture",
        InitCommand=function(self) self:SetTextureName("Capture"):SetWidth(64):SetHeight(64):Create() end,
    },
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetTexture(CAPTURE_BINDING)
                :SetDrawState(state):SetVertices(vertices)
            vertices[2][1][1] = 999
            vertices[2][3][1] = 0.25
            state.First = 3
        end,
    },
}
"#.replace("CAPTURE_BINDING", binding),
        )
        .unwrap();
        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Capture mesh copy"),
        )
        .unwrap();
        let mesh = compiled
            .overlays
            .iter()
            .find_map(|overlay| match &overlay.kind {
                SongLuaOverlayKind::ActorMultiVertex {
                    vertices,
                    capture_name,
                    texture_path,
                    ..
                } => Some((vertices, capture_name, texture_path)),
                _ => None,
            })
            .expect("capture mesh must remain compiled");
        assert_eq!(mesh.1.as_deref(), Some("Capture"));
        assert!(mesh.2.is_none());
        assert_eq!(mesh.0.len(), 3);
        assert_eq!(mesh.0[1].pos, [32.0, 0.0]);
        assert_eq!(mesh.0[1].uv, [1.0, 0.0]);
    }
}

#[test]
fn compile_song_lua_triangulates_actor_multi_vertex_line_strip() {
    let song_dir = test_dir("actor-multi-vertex-line-strip");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetDrawState{Mode="DrawMode_LineStrip"}
                :SetLineWidth(4)
                :SetVertices{
                    {{0, 0, 0}, {1, 0, 0, 1}},
                    {{10, 0, 0}, {0, 1, 0, 1}},
                    {{10, 10, 0}, {0, 0, 1, 1}},
                }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Line Strip"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &compiled.overlays[0].kind else {
        panic!("expected ActorMultiVertex overlay");
    };
    assert_eq!(vertices.len(), 12);
    let assert_pos = |actual: [f32; 2], expected: [f32; 2]| {
        assert!(
            actual
                .iter()
                .zip(expected.iter())
                .all(|(a, b)| (a - b).abs() <= 0.000_1),
            "expected {expected:?}, got {actual:?}"
        );
    };
    assert_pos(vertices[0].pos, [0.0, 2.0]);
    assert_pos(vertices[1].pos, [8.0, 2.0]);
    assert_pos(vertices[2].pos, [12.0, -2.0]);
    assert_pos(vertices[5].pos, [0.0, -2.0]);
    assert_pos(vertices[6].pos, [8.0, 2.0]);
}

#[test]
fn compile_song_lua_captures_textured_actor_multi_vertex_uvs() {
    let song_dir = test_dir("actor-multi-vertex-texture");
    let entry = song_dir.join("default.lua");
    let texture_path = song_dir.join("panel.png");
    image::RgbaImage::new(16, 16).save(&texture_path).unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorMultiVertex{
        InitCommand=function(self)
            self:SetTexture("panel.png")
                :SetDrawState{Mode="DrawMode_Triangles"}
                :SetVertices{
                    {{0, 0, 0}, {1, 1, 1, 1}, {0, 0}},
                    {{16, 0, 0}, {1, 1, 1, 1}, {1, 0}},
                    {{0, 16, 0}, {1, 1, 1, 1}, {0, 1}},
                }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Texture"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::ActorMultiVertex {
        vertices,
        texture_path: Some(actual_texture),
        ..
    } = &compiled.overlays[0].kind
    else {
        panic!("expected textured ActorMultiVertex overlay");
    };
    assert_eq!(actual_texture, &texture_path);
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[1].uv, [1.0, 0.0]);
    assert_eq!(vertices[2].uv, [0.0, 1.0]);
}

#[test]
fn compile_song_lua_supports_diffuse_and_glow_blink_methods() {
    let song_dir = test_dir("actor-blink-effects");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffuseblink():effectperiod(0.25):effectcolor1(0,0,0,1):effectcolor2(1,1,1,1)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:glowblink():effectclock("beatnooffset")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Blink Effects"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);

    let diffuse = compiled.overlays[0].initial_state;
    assert_eq!(diffuse.effect_mode, EffectMode::DiffuseBlink);
    assert_eq!(diffuse.effect_period, 0.25);
    assert_eq!(diffuse.effect_color1, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(diffuse.effect_color2, [1.0, 1.0, 1.0, 1.0]);

    let glow = compiled.overlays[1].initial_state;
    assert_eq!(glow.effect_mode, EffectMode::GlowBlink);
    assert_eq!(glow.effect_clock, EffectClock::Beat);
    assert_eq!(glow.effect_color1, [1.0, 1.0, 1.0, 0.2]);
    assert_eq!(glow.effect_color2, [1.0, 1.0, 1.0, 0.8]);
}

#[test]
fn compile_song_lua_supports_overlay_multiply_and_subtract_blend() {
    let song_dir = test_dir("overlay-extra-blends");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(40, 30).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:blend("multiply")
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:blend("subtract")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Extra Blends"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    assert_eq!(
        compiled.overlays[0].initial_state.blend,
        SongLuaOverlayBlendMode::Multiply
    );
    assert_eq!(
        compiled.overlays[1].initial_state.blend,
        SongLuaOverlayBlendMode::Subtract
    );
}

#[test]
fn compile_song_lua_supports_bitmaptext_layout_methods() {
    let song_dir = test_dir("bitmaptext-layout-methods");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="WRAP",
        OnCommand=function(self)
            self:wrapwidthpixels(64):maxwidth(80):maxheight(40):zoom(2)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="POST",
        OnCommand=function(self)
            self:zoom(2):maxwidth(90):maxheight(50)
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="USEZOOM",
        OnCommand=function(self)
            self:maxwidth(70):maxheight(30):zoom(2):max_dimension_use_zoom(true)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Layout Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 3);

    let pre_zoom = compiled.overlays[0].initial_state;
    assert_eq!(pre_zoom.wrap_width_pixels, Some(64));
    assert_eq!(pre_zoom.max_width, Some(80.0));
    assert_eq!(pre_zoom.max_height, Some(40.0));
    assert!(pre_zoom.max_w_pre_zoom);
    assert!(pre_zoom.max_h_pre_zoom);

    let post_zoom = compiled.overlays[1].initial_state;
    assert_eq!(post_zoom.max_width, Some(90.0));
    assert_eq!(post_zoom.max_height, Some(50.0));
    assert!(!post_zoom.max_w_pre_zoom);
    assert!(!post_zoom.max_h_pre_zoom);

    let use_zoom = compiled.overlays[2].initial_state;
    assert_eq!(use_zoom.max_width, Some(70.0));
    assert_eq!(use_zoom.max_height, Some(30.0));
    assert!(use_zoom.max_w_pre_zoom);
    assert!(use_zoom.max_h_pre_zoom);
    assert!(use_zoom.max_dimension_uses_zoom);
}

#[test]
fn compile_song_lua_supports_bitmaptext_uppercase_and_vertspacing() {
    let song_dir = test_dir("bitmaptext-uppercase-vertspacing");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="Mixed Case",
        OnCommand=function(self)
            self:uppercase(true):vertspacing(18)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Uppercase VertSpacing"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);

    let text = compiled.overlays[0].initial_state;
    assert!(text.uppercase);
    assert_eq!(text.vert_spacing, Some(18));
}

#[test]
fn compile_song_lua_supports_bitmaptext_fit_methods() {
    let song_dir = test_dir("bitmaptext-fit-methods");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="FIT",
        OnCommand=function(self)
            self:zoomtowidth(120):zoomtoheight(30)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Fit Methods"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.size, Some([120.0, 30.0]));
}

#[test]
fn compile_song_lua_supports_actor_set_size_methods() {
    let song_dir = test_dir("actor-set-size");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(10, 20)
            self:SetWidth(30)
            self:SetHeight(40)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Set Size"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "30:40");
}

#[test]
fn compile_song_lua_supports_align_and_setsize_aliases() {
    let song_dir = test_dir("actor-align-setsize");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:setsize(12, 34):align(0, 1)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
    Def.BitmapText{
        Font="Common Normal",
        Text="ALIGN",
        OnCommand=function(self)
            self:align(1, 0.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Align SetSize"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "12:34");
    assert_eq!(compiled.overlays.len(), 2);

    let quad = compiled.overlays[0].initial_state;
    assert_eq!(quad.size, Some([12.0, 34.0]));
    assert_eq!(quad.halign, 0.0);
    assert_eq!(quad.valign, 1.0);

    let text = compiled.overlays[1].initial_state;
    assert_eq!(text.halign, 1.0);
    assert_eq!(text.valign, 0.5);
}

#[test]
fn compile_song_lua_supports_scale_to_clipped_size() {
    let song_dir = test_dir("scale-to-clipped-size");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(120, 60).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletoclipped(90, 36)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            assert(self.ScaleToClipped == nil)
            self:scaletoclipped(10, 20)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Scale To Clipped Size"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "90:36");
    assert_eq!(compiled.overlays.len(), 2);
    assert_eq!(compiled.overlays[0].initial_state.size, Some([90.0, 36.0]));
    assert_eq!(compiled.overlays[1].initial_state.size, Some([10.0, 20.0]));
}

#[test]
fn background_fit_modes() {
    let song_dir = test_dir("background-scale");
    image::RgbaImage::new(200, 100)
        .save(song_dir.join("background.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
assert(PREFSMAN:GetPreference('BackgroundFitMode') == 'BackgroundFitMode_CoverPreserve')
local root = Def.ActorFrame{}
for _, mode in ipairs({'CoverDistort', 'CoverPreserve', 'FitInside', 'FitInsideAvoidLetter', 'FitInsideAvoidPillar'}) do
    for _, center in ipairs({false, true}) do
        root[#root+1] = Def.Sprite {
            Texture=GAMESTATE:GetCurrentSong():GetBackgroundPath(),
            OnCommand=function(self)
                PREFSMAN:SetPreference('BackgroundFitMode', 'BackgroundFitMode_' .. mode)
                self:xy(17,29):halign(0):valign(1)
                if center then assert(self:scale_or_crop_background() == self)
                else assert(self:scale_or_crop_background_no_move() == self) end
            end,
        }
    end
end
return root
"#).unwrap();
    for width in [640.0, 854.0] {
        let mut context = SongLuaCompileContext::new(&song_dir, "Background scale");
        context.screen_width = width;
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.overlays.len(), 10);
        let x = width / 200.0;
        let y = context.screen_height / 100.0;
        for (index, actor) in compiled.overlays.iter().enumerate() {
            let state = actor.initial_state;
            let zoom = match index / 2 {
                0 => [x, y],
                1 => [x.max(y); 2],
                2 => [x.min(y); 2],
                3 => [y; 2],
                4 => [x; 2],
                _ => unreachable!(),
            };
            assert_eq!([state.zoom_x, state.zoom_y], zoom);
            assert_eq!(
                [state.x, state.y],
                if index % 2 == 0 {
                    [17.0, 29.0]
                } else {
                    [width * 0.5, 240.0]
                }
            );
        }
    }
}

#[test]
fn compile_song_lua_supports_scale_to_fit_and_cover() {
    let song_dir = test_dir("scale-to-fit-cover");
    let image_path = song_dir.join("panel.png");
    image::RgbaImage::new(200, 100).save(&image_path).unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletofit(100, 100, 300, 220)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:scaletocover(100, 100, 300, 220)
        end,
    },
    Def.Sprite{
        Texture="panel.png",
        OnCommand=function(self)
            self:halign(0):valign(1):scaletofit(100, 100, 300, 220)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Scale To Fit Cover"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 3);

    let fit = compiled.overlays[0].initial_state;
    assert_eq!(fit.x, 200.0);
    assert_eq!(fit.y, 160.0);
    assert_eq!(fit.zoom, 1.0);
    assert_eq!(fit.zoom_x, 1.0);
    assert_eq!(fit.zoom_y, 1.0);

    let cover = compiled.overlays[1].initial_state;
    assert_eq!(cover.x, 200.0);
    assert_eq!(cover.y, 160.0);
    assert!((cover.zoom - 1.2).abs() <= 0.000_1);
    assert!((cover.zoom_x - 1.2).abs() <= 0.000_1);
    assert!((cover.zoom_y - 1.2).abs() <= 0.000_1);

    let aligned = compiled.overlays[2].initial_state;
    assert_eq!(aligned.x, 100.0);
    assert_eq!(aligned.y, 220.0);
    assert_eq!(aligned.zoom, 1.0);
}

#[test]
fn compile_song_lua_supports_sprite_crop_to() {
    let song_dir = test_dir("sprite-crop-to");
    image::RgbaImage::new(200, 100)
        .save(song_dir.join("wide.png"))
        .unwrap();
    image::RgbaImage::new(100, 200)
        .save(song_dir.join("tall.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="wide.png",
        OnCommand=function(self)
            self:zoom(2):CropTo(100, 100)
            mod_actions = {
                {1, string.format("%.0f:%.0f:%.0f", self:GetWidth(), self:GetHeight(), self:GetZoomedWidth()), true},
            }
        end,
    },
    Def.Sprite{
        Texture="tall.png",
        OnCommand=function(self)
            self:CropTo(100, 100)
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite CropTo"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "100:100:100");
    assert_eq!(compiled.overlays.len(), 2);

    let wide = compiled.overlays[0].initial_state;
    assert_eq!(wide.size, Some([100.0, 100.0]));
    assert_eq!(wide.zoom, 1.0);
    assert_eq!(wide.zoom_x, 1.0);
    assert_eq!(wide.zoom_y, 1.0);
    assert_eq!(wide.sprite_state_index, Some(u32::MAX));
    assert_eq!(wide.custom_texture_rect, Some([0.25, 0.0, 0.75, 1.0]));

    let tall = compiled.overlays[1].initial_state;
    assert_eq!(tall.size, Some([100.0, 100.0]));
    assert_eq!(tall.custom_texture_rect, Some([0.0, 0.25, 1.0, 0.75]));
}

#[test]
fn compile_song_lua_supports_basezoom_axis_methods() {
    let song_dir = test_dir("basezoom-axis");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:basezoom(2)
            self:basezoomx(3)
            self:basezoomy(4)
            self:basezoomz(5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BaseZoom Axis"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.basezoom, 2.0);
    assert_eq!(compiled.overlays[0].initial_state.basezoom_x, 3.0);
    assert_eq!(compiled.overlays[0].initial_state.basezoom_y, 4.0);
    assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
}

#[test]
fn compile_song_lua_exposes_zoomed_actor_size() {
    let song_dir = test_dir("zoomed-actor-size");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(40, 20)
            self:zoomx(2)
            self:zoomy(3)
            self:basezoomx(0.5)
            self:basezoomy(2)
            mod_actions = {
                {1, string.format("%.0f:%.0f", self:GetZoomedWidth(), self:GetZoomedHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Zoomed Actor Size"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "40:120");
}

#[test]
fn compile_song_lua_supports_actor_state_getters() {
    let song_dir = test_dir("actor-state-getters");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local target

mod_actions = {
    {4, function()
        if target then
            target:x(target:GetSecsIntoEffect())
            target:y(target:GetEffectDelta())
        end
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:xy(10, 20):z(3):basezoom(2):basezoomx(3):basezoomy(4):basezoomz(5)
            self:diffuse(0.2, 0.4, 0.6, 0.8):glow(0.1, 0.2, 0.3, 0.4)
            self:halign(0):valign(1):effectmagnitude(8, 4, 2):effectclock("beat"):visible(false)
            assert(self.GetAlpha == nil)
            local d = self:GetDiffuse()
            local g = self:GetGlow()
            local mx, my, mz = self:geteffectmagnitude()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.0f:%.1f:%.1f:%.1f:%.1f:%s:%.0f:%.0f:%.0f:%.0f:%.0f",
                    self:GetDestX(),
                    self:GetDestY(),
                    self:GetDestZ(),
                    self:GetBaseZoomX(),
                    self:GetBaseZoomY(),
                    self:GetBaseZoomZ(),
                    self:GetHAlign(),
                    self:GetVAlign(),
                    self:GetDiffuseAlpha() * 10,
                    d[1],
                    d[3],
                    g[1],
                    g[4],
                    tostring(self:GetVisible()),
                    mx,
                    my,
                    mz,
                    self:GetSecsIntoEffect(),
                    self:GetEffectDelta()
                ),
                true
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor State Getters"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 2);
    assert_eq!(
        compiled.messages[0].message,
        "10:20:3:3:4:5:0:1:8:0.2:0.6:0.1:0.4:false:8:4:2:0:0"
    );
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let block = &compiled.overlays[0].message_commands[0].blocks[0];
    assert_eq!(block.delta.x, Some(4.0));
    assert_eq!(block.delta.y, Some(0.0));
}

#[test]
fn compile_song_lua_accepts_basezoomz_method() {
    let song_dir = test_dir("basezoom-z");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            self:basezoomz(5)
            mod_actions = {
                {1, "ok", true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BaseZoom Z"))
            .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.basezoom_z, 5.0);
}

#[test]
fn compile_song_lua_exposes_screen_globals() {
    let song_dir = test_dir("screen-globals");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {
        1,
        string.format(
            "%.0f:%.0f:%.0f:%.0f",
            _screen.w,
            _screen.h,
            _screen.cx,
            _screen.cy
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Screen Globals");
    context.screen_width = 800.0;
    context.screen_height = 600.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "800:600:400:300");
}

#[test]
fn compile_song_lua_supports_zoom_to_width_and_height() {
    let song_dir = test_dir("zoomto-width-height");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:SetSize(10, 20)
            self:zoomtowidth(30)
            self:zoomtoheight(40)
            mod_actions = {
                {1, string.format("%.0f:%.0f:%.0f:%.0f", self:GetWidth(), self:GetHeight(), self:GetZoomedWidth(), self:GetZoomedHeight()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Zoomto Width Height"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "10:20:30:40");
}

#[test]
fn compile_song_lua_zoom_sets_axis_state() {
    let song_dir = test_dir("zoom-axis-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:zoom(2)
            self:zoomx(3)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Zoom Axis State"),
    )
    .unwrap();
    let overlay = &compiled.overlays[0];
    assert_eq!(overlay.initial_state.zoom, 2.0);
    assert_eq!(overlay.initial_state.zoom_x, 3.0);
    assert_eq!(overlay.initial_state.zoom_y, 2.0);
    assert_eq!(overlay.initial_state.zoom_z, 2.0);
}

#[test]
fn compile_song_lua_exposes_debug_getinfo_source() {
    let song_dir = test_dir("debug-getinfo");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&lua_dir).unwrap();
    fs::write(
        lua_dir.join("child.lua"),
        r#"
local info = debug.getinfo(1)
mod_actions = {
    {1, info.source, true},
}
return Def.ActorFrame{}
"#,
    )
    .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir() .. "lua/child.lua"))()
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Debug Getinfo"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        format!("@{}", file_path_string(&lua_dir.join("child.lua")))
    );
}

#[test]
fn compile_song_lua_exposes_math_round_compat() {
    let song_dir = test_dir("math-round");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, string.format("%d:%d:%d", math.round(1.49), math.round(1.5), math.round(-1.5)), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Math Round"))
            .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1:2:-2");
}

#[test]
fn compile_song_lua_supports_xero_chunk_env_switching() {
    let song_dir = test_dir("xero-chunk-env");
    let template_dir = song_dir.join("template");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(
        template_dir.join("std.lua"),
        r#"
local xero = setmetatable(xero, xero)
xero.__index = _G

function xero:__call(f)
    setfenv(f or 2, self)
    return f
end

xero()

local stringbuilder_mt = {
    __index = {
        build = table.concat,
    },
    __call = function(self, value)
        table.insert(self, tostring(value))
        return self
    end,
}

function stringbuilder()
    return setmetatable({}, stringbuilder_mt)
end

return Def.Actor{}
"#,
    )
    .unwrap();
    fs::write(
        template_dir.join("template.lua"),
        r#"
xero()

local sb = stringbuilder()
sb("ok")
mod_actions = {
    {1, sb:build(), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();
    let entry = template_dir.join("main.lua");
    fs::write(
        &entry,
        r#"
_G.xero = {}

return Def.ActorFrame{
    assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir()..'template/std.lua'))(),
    assert(loadfile(GAMESTATE:GetCurrentSong():GetSongDir()..'template/template.lua'))(),
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Xero")).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
}

#[test]
fn compile_song_lua_supports_xero_require_env_switching() {
    let song_dir = test_dir("xero-require-env");
    let template_dir = song_dir.join("template");
    let lua_dir = song_dir.join("lua");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&lua_dir).unwrap();
    fs::write(
        template_dir.join("std.lua"),
        r#"
setmetatable(xero, {
    __index = _G,
    __call = function(self, f)
        setfenv(f or 2, self)
        return f
    end,
})

xero.package = {
    loaded = {},
    loaders = {
        function(modname)
            local loader, err = loadfile(xero.dir .. "lua/" .. modname .. ".lua")
            if loader then return xero(loader) end
            return err
        end,
    },
}

function xero.require(modname)
    local loaded = xero.package.loaded
    if not loaded[modname] then
        for _, loader in ipairs(xero.package.loaders) do
            local chunk = loader(modname)
            if type(chunk) == "function" then
                loaded[modname] = chunk() or true
                break
            end
        end
    end
    return loaded[modname]
end

xero()
return Def.Actor{}
"#,
    )
    .unwrap();
    fs::write(
        template_dir.join("template.lua"),
        r#"
xero()
xero.P = {"ok"}
xero.require("mods")
return Def.ActorFrame{}
"#,
    )
    .unwrap();
    fs::write(
        lua_dir.join("mods.lua"),
        r#"
mod_actions = {
    {1, P[1], true},
}
"#,
    )
    .unwrap();
    let entry = template_dir.join("main.lua");
    fs::write(
        &entry,
        r#"
_G.xero = {
    dir = GAMESTATE:GetCurrentSong():GetSongDir(),
}

return Def.ActorFrame{
    assert(loadfile(xero.dir .. "template/std.lua"))(),
    assert(loadfile(xero.dir .. "template/template.lua"))(),
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Xero Require"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
}

#[test]
fn compile_song_lua_returns_empty_fileman_listing_for_missing_dir() {
    let song_dir = test_dir("fileman-empty-listing");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local listing = FILEMAN:GetDirListing(GAMESTATE:GetCurrentSong():GetSongDir() .. "plugins/")
mod_actions = {
    {1, string.format("%s:%d", type(listing), #listing), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Fileman")).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "table:0");
}

#[test]
fn compile_song_lua_exposes_actorframe_class_methods() {
    let song_dir = test_dir("actorframe-class");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local child = Def.ActorFrame{
    Name="child",
    PingCommand=function(self) self:aux(self:getaux() + 1) end,
    Def.Quad{
        Name="leaf",
        PingCommand=function(self) self:aux(self:getaux() + 10) end,
    },
}
local removable = Def.ActorFrame{Name="remove"}
local disposable = Def.ActorFrame{Def.Quad{Name="gone"}}
local root = Def.ActorFrame{
    Name="root",
    child,
    removable,
    InitCommand=function(self)
        ActorFrame.SetFOV(self, 75)
        ActorFrame.fov(self, 80)
        ActorFrame.SetUpdateRate(self, 3)
        ActorFrame.vanishpoint(self, 120, 80)
        ActorFrame.SetDrawFunction(self, function(actor) actor:y(8) end)
        ActorFrame.GetDrawFunction(self)(self)
        local update_ok = ActorFrame.SetUpdateFunction(self, function(actor) actor:aux(11) end) == self
        ActorFrame.SetDrawByZPosition(self, true)
        ActorFrame.SortByDrawOrder(self)
        ActorFrame.SetAmbientLightColor(self, color("1,1,1,1"))
        ActorFrame.SetDiffuseLightColor(self, color("1,1,1,1"))
        ActorFrame.SetSpecularLightColor(self, color("1,1,1,1"))
        ActorFrame.SetLightDirection(self, {0, 0, 1})
        ActorFrame.AddChildFromPath(self, "missing.lua")
        local propagate_ok = ActorFrame.propagate(self, true) == self
        ActorFrame.propagate(self, false)
        ActorFrame.playcommandonchildren(self, "Ping")
        ActorFrame.playcommandonleaves(self, "Ping")
        ActorFrame.RunCommandsOnChildren(self, function(actor, params) actor:aux(actor:getaux() + params.direct) end, {direct=100})
        ActorFrame.runcommandsonleaves(self, function(actor) actor:aux(actor:getaux() + 1000) end)
        assert(ActorFrame.GetChildAt == nil and self.GetChildAt == nil)
        local picked = ActorFrame.GetChild(self, "child")
        local picked_method = self:GetChild("child")
        local second = ActorFrame.GetChild(self, "remove")
        local named = ActorFrame.GetChild(self, "child")
        local children = ActorFrame.GetChildren(self)
        local count_before = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveChild(self, "remove")
        local count_after = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveAllChildren(disposable)
        mod_actions = {
            {1, string.format(
                "%s:%s:%s:%s:%d:%d:%.0f:%.0f:%.0f:%.0f:%s:%s:%s",
                tostring(ActorFrame.fardistz == nil and self.fardistz == nil),
                picked and picked:GetName() or "nil",
                second and second:GetName() or "nil",
                tostring(picked_method == child and named == child and children["child"] == child),
                count_before,
                count_after,
                self:GetUpdateRate(),
                self:GetDestY(),
                child:getaux(),
                child:GetChild("leaf"):getaux(),
                tostring(update_ok),
                tostring(propagate_ok),
                tostring(ActorFrame.GetNumChildren(disposable) == 0)
            ), true},
        }
    end,
}

return root
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "ActorFrame Class"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "true:child:remove:true:2:1:3:8:101:1010:true:true:true"
    );
    let root = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("root"))
        .unwrap();
    assert_eq!(root.initial_state.fov, Some(80.0));
    assert_eq!(root.initial_state.vanishpoint, Some([120.0, 80.0]));
    assert!(root.initial_state.draw_by_z_position);
}

#[test]
fn compile_song_lua_supports_actorframe_child_methods() {
    let song_dir = test_dir("actorframe-child-methods");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function draw_fn(self)
    self:aux(7)
end

return Def.ActorFrame{
    Name="Root",
    Def.Quad{
        Name="Keep",
        PingCommand=function(self) self:aux(self:getaux() + 1) end,
    },
    Def.Quad{
        Name="RemoveMe",
        PingCommand=function(self) self:aux(99) end,
    },
    Def.ActorFrame{
        Name="Branch",
        Def.Quad{
            Name="Leaf",
            PingCommand=function(self) self:aux(self:getaux() + 3) end,
        },
    },
    OnCommand=function(self)
        self:SetFOV(75):SetUpdateRate(2):SetDrawFunction(draw_fn)
        self:SetDrawByZPosition(true):SortByDrawOrder():propagate(false)
        self:SetAmbientLightColor(color("1,1,1,1")):SetDiffuseLightColor(color("1,1,1,1"))
        self:SetSpecularLightColor(color("1,1,1,1")):SetLightDirection({0, 0, 1})
        self:playcommandonchildren("Ping")
        self:playcommandonleaves("Ping")
        local children = self:GetChildren()
        local keep = children["Keep"]
        local branch = children["Branch"]
        local leaf = branch:GetChild("Leaf")
        local before_remove = children["RemoveMe"] ~= nil
        self:RemoveChild("RemoveMe")
        local after_remove = self:GetChildren()["RemoveMe"] == nil
        mod_actions = {
            {1, string.format(
                "%.0f:%.0f:%.0f:%.0f:%s:%s:%s",
                keep:getaux(),
                branch:getaux(),
                leaf:getaux(),
                self:GetUpdateRate(),
                tostring(before_remove),
                tostring(after_remove),
                tostring(self:GetDrawFunction() ~= nil)
            ), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "ActorFrame Child Methods"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "2:0:3:2:true:true:true");
    assert!(
        compiled
            .overlays
            .iter()
            .all(|overlay| overlay.name.as_deref() != Some("RemoveMe"))
    );
    let root = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Root"))
        .unwrap();
    assert_eq!(root.initial_state.fov, Some(75.0));
    assert!(root.initial_state.draw_by_z_position);
}

#[test]
fn compile_song_lua_captures_draw_by_z_position_commands() {
    let song_dir = test_dir("draw-by-z-position-command");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="Root",
    Def.Quad{Name="Back", z=30},
    Def.Quad{Name="Front", z=-30},
    FlipMessageCommand=function(self)
        self:SetDrawByZPosition(true)
    end,
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Draw By Z")).unwrap();
    let root = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Root"))
        .unwrap();
    assert!(!root.initial_state.draw_by_z_position);
    assert_eq!(root.message_commands.len(), 1);
    assert_eq!(root.message_commands[0].message, "Flip");
    assert_eq!(root.message_commands[0].blocks.len(), 1);
    assert_eq!(
        root.message_commands[0].blocks[0].delta.draw_by_z_position,
        Some(true)
    );
}

#[test]
fn compile_song_lua_supports_add_child_from_path() {
    let song_dir = test_dir("add-child-from-path");
    let entry = song_dir.join("default.lua");
    fs::write(
        song_dir.join("child.lua"),
        r#"
return Def.Quad{
    Name="Loaded",
    InitCommand=function(self) self:x(42) end,
    OnCommand=function(self) self:y(24) end,
}
"#,
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="Root",
    InitCommand=function(self)
        self:AddChildFromPath("child")
        local loaded = self:GetChild("Loaded")
        mod_actions = {{1, loaded and loaded:GetName() or "nil", true}}
    end,
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Add Child")).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Loaded");
    let root_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Root"))
        .unwrap();
    let loaded = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Loaded"))
        .unwrap();
    assert_eq!(loaded.parent_index, Some(root_index));
    assert_eq!(loaded.initial_state.x, 42.0);
    assert_eq!(loaded.initial_state.y, 24.0);
}

#[test]
fn compile_song_lua_runs_late_add_child_from_path_commands() {
    let song_dir = test_dir("late-add-child-from-path");
    let entry = song_dir.join("default.lua");
    fs::write(
        song_dir.join("child.lua"),
        r#"
return Def.Quad{
    Name="Loaded",
    InitCommand=function(self) self:x(42) end,
    OnCommand=function(self) self:y(24) end,
}
"#,
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="Root",
    OnCommand=function(self)
        self:AddChildFromPath("child")
        self:queuecommand("Report")
    end,
    ReportCommand=function(self)
        local loaded = self:GetChild("Loaded")
        mod_actions = {{
            1,
            loaded and string.format("%.0f:%.0f", loaded:GetX(), loaded:GetY()) or "nil",
            true,
        }}
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Late Add Child"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "42:24");
    let root_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Root"))
        .unwrap();
    let loaded = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Loaded"))
        .unwrap();
    assert_eq!(loaded.parent_index, Some(root_index));
    assert_eq!(loaded.initial_state.x, 42.0);
    assert_eq!(loaded.initial_state.y, 24.0);
}

#[test]
fn compile_song_lua_passes_playcommand_params() {
    let song_dir = test_dir("playcommand-params");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="Root",
    SetCommand=function(self, params)
        self:aux(params.root)
    end,
    Def.Quad{
        Name="Child",
        SetCommand=function(self, params)
            self:aux(params.child)
        end,
    },
    Def.ActorFrame{
        Name="Branch",
        Def.Quad{
            Name="Leaf",
            LeafCommand=function(self, params)
                self:aux(params.leaf)
            end,
        },
    },
    OnCommand=function(self)
        self:playcommand("Set", {root=4})
        self:playcommandonchildren("Set", {child=7})
        self:playcommandonleaves("Leaf", {leaf=9})
        local children = self:GetChildren()
        mod_actions = {
            {1, string.format(
                "%.0f:%.0f:%.0f",
                self:getaux(),
                children["Child"]:getaux(),
                children["Branch"]:GetChild("Leaf"):getaux()
            ), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "PlayCommand Params"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "4:7:9");
}

#[test]
fn compile_song_lua_getchildren_scans_unnamed_actorframes() {
    let song_dir = test_dir("getchildren-unnamed-actorframes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function scan(actor, skip)
    if tostring(actor):find("table") and not skip then
        for _, child in pairs(actor) do
            scan(child)
        end
        return
    end
    if actor.GetChildren then
        for _, child in pairs(actor:GetChildren()) do
            scan(child)
        end
    end
    if actor.GetName and actor:GetName() == "TargetLeaf" then
        prefix_globals.found_leaf = true
    end
end

prefix_globals = {}

return Def.ActorFrame{
    OnCommand=function(self)
        scan(self, true)
        mod_actions = {{1, tostring(prefix_globals.found_leaf == true), true}}
    end,
    Def.ActorFrame{},
    Def.ActorFrame{
        Def.ActorFrame{
            Def.Quad{
                Name="TargetLeaf",
            },
        },
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "GetChildren Unnamed ActorFrames"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true");
}

#[test]
fn compile_song_lua_matches_native_child_tables() {
    let song_dir = test_dir("native-child-table");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/child-table.lua"),
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native Child Table"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.info.unsupported_perframes, 0);
}

#[test]
fn compile_song_lua_supports_propagate_command_helpers() {
    let song_dir = test_dir("propagate-command");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="Root",
    PingCommand=function(self) self:aux(100) end,
    Def.ActorFrame{
        Name="Branch",
        PingCommand=function(self, params) self:aux(params.branch) end,
        Def.Quad{
            Name="Leaf",
            PingCommand=function(self, params) self:aux(params.leaf) end,
        },
    },
    Def.Quad{
        Name="Direct",
        PingCommand=function(self, params) self:aux(params.direct) end,
    },
    OnCommand=function(self)
        self:propagatecommand("Ping", {branch=1, leaf=2, direct=4})
        local branch = self:GetChild("Branch")
        local leaf = branch:GetChild("Leaf")
        local direct = self:GetChild("Direct")
        local after_propagatecommand = string.format(
            "%.0f:%.0f:%.0f:%.0f",
            self:getaux(),
            branch:getaux(),
            leaf:getaux(),
            direct:getaux()
        )
        self:propagate(true):playcommand("Ping", {branch=8, leaf=16, direct=32}):propagate(false)
        mod_actions = {
            {1, after_propagatecommand .. "|" .. string.format(
                "%.0f:%.0f:%.0f:%.0f",
                self:getaux(),
                branch:getaux(),
                leaf:getaux(),
                direct:getaux()
            ), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Propagate Command"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0:1:2:4|0:8:16:32");
}

#[test]
fn compile_song_lua_accepts_skewy_probe_calls() {
    let song_dir = test_dir("skewy-probe");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil

mods_ease = {
    {1, 1, 0, 0.25, function(x)
        if target then
            target:skewy(x)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.ActorFrame{
        OnCommand=function(self)
            target = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SkewY Probe"),
    )
    .unwrap();
    assert_eq!(
        compiled.info.unsupported_function_eases, 0,
        "{:?}",
        compiled.info
    );
    assert_eq!(compiled.eases.len(), 1);
    assert!(matches!(
        compiled.eases[0].target,
        SongLuaEaseTarget::PlayerSkewY
    ));
}

#[test]
fn custom_draws_keep_capture_order_and_snapshots() {
    use crate::{SongLuaDrawOp, SongLuaDrawSource};
    let song_dir = test_dir("ordered-draw-capture");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local vertices = {
    {{0,0,0},{1,1,1,1},{0,0}}, {{0,10,0},{1,1,1,1},{0,1}},
    {{10,0,0},{1,1,1,1},{1,0}}, {{10,10,0},{1,1,1,1},{1,1}},
}
return Def.ActorFrame {
    Name="DrawRoot",
    InitCommand=function(self) self:xy(40,50):SetFOV(45) end,
    OnCommand=function(self)
        local target, mesh = self:GetChild("target"), self:GetChild("mesh")
        local screen = SCREENMAN:GetTopScreen()
        local children = screen:GetChildren()
        local foreground = screen:GetChild("SongForeground")
        assert(self:GetParent() == foreground)
        assert(foreground:GetChildren().DrawRoot == self)
        for _, name in ipairs({"LifeMeter", "LifeMeterBarP1", "SongTitle", "BPMDisplay"}) do
            assert(children[name] == nil, name .. " is not a top-level screen child")
        end
        local engine_hud = {}
        for _, name in ipairs({"LifeP1", "LifeP2", "ScoreP1", "ScoreP2", "StepsDisplayP1", "StepsDisplayP2"}) do
            assert(children[name]:GetVisible(), "hibernation preserves visibility")
            engine_hud[#engine_hud+1] = children[name]
        end
        target:SetSize(640,480):EnablePreserveTexture(true):Create()
        local texture = target:GetTexture()
        assert(texture:GetTextureWidth() == 1024 and texture:GetTextureHeight() == 512)
        mesh:SetTexture(texture):SetVertices(vertices)
            :SetDrawState{Mode="DrawMode_QuadStrip",First=1,Num=-1}
        self:SetDrawFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            texture:BeginRenderingTo(beat >= 1)
            mesh:xy(beat,0):diffuse(1,0,0,1):Draw()
            mesh:xy(beat+20,0):diffuse(0,1,0,1):Draw()
            vertices[1][1][2] = beat
            mesh:SetVertices(vertices):xy(beat+40,0):diffuse(0,0,1,1):Draw()
            texture:FinishRenderingTo()
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            player:visible(true):Draw():visible(false)
            local screen = SCREENMAN:GetTopScreen()
            screen:GetChild("Underlay"):Draw()
            screen:GetChild("Overlay"):Draw()
            screen:GetChild("SongBackground"):Draw()
            for _, actor in ipairs(engine_hud) do actor:visible(true):Draw() end
        end)
    end,
    Def.ActorFrameTexture{Name="target"},
    Def.ActorMultiVertex{Name="mesh"},
}
"#,
        )
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Ordered Draw");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "{:?}",
        compiled.info
    );
    assert_eq!(compiled.draw_frames.len(), 121);
    for (index, frame) in compiled.draw_frames.iter().enumerate() {
        assert!((frame.second - index as f32 / 60.0).abs() < 1e-5);
        assert_eq!(frame.ops.len(), 9);
        assert!(
            matches!(frame.ops[0], SongLuaDrawOp::Begin{preserve, ..} if preserve == (index >= 60))
        );
        assert!(matches!(frame.ops[4], SongLuaDrawOp::Finish { .. }));
        for pass in 0..3 {
            let SongLuaDrawOp::Draw {
                state,
                parents,
                camera,
                vertices,
                ..
            } = &frame.ops[pass + 1]
            else {
                panic!("missing mesh pass")
            };
            assert!((state.x - frame.second - pass as f32 * 20.0).abs() < 1e-5);
            assert_eq!(state.diffuse[pass], 1.0);
            assert_eq!(state.diffuse[(pass + 1) % 3], 0.0);
            assert!(parents.is_empty());
            assert!(camera.is_none());
            assert_eq!(vertices.as_ref().expect("copied mesh").len(), 6);
        }
        let SongLuaDrawOp::Draw {
            source,
            state,
            parents,
            camera,
            ..
        } = &frame.ops[5]
        else {
            panic!("missing player draw")
        };
        assert_eq!(*source, SongLuaDrawSource::Player(0));
        assert!(state.visible);
        assert!(
            parents
                .iter()
                .any(|state| state.x == 40.0 && state.y == 50.0)
        );
        assert_eq!(camera.expect("restored caller camera").fov, Some(45.0));
        for layer in 0..3 {
            assert!(matches!(frame.ops[layer + 6], SongLuaDrawOp::Draw {
                    source: SongLuaDrawSource::ScreenLayer(index), ..
                } if index == layer));
        }
    }
    let SongLuaDrawOp::Draw { vertices, .. } = &compiled.draw_frames[0].ops[1] else {
        unreachable!()
    };
    assert_eq!(vertices.as_ref().unwrap()[0].pos[1], 0.0);
}

#[test]
fn song_roots_have_layer_parents() {
    let song_dir = test_dir("song-layer-parents");
    let paths = [
        song_dir.join("background.lua"),
        song_dir.join("foreground.lua"),
    ];
    for (path, name, parent) in [
        (&paths[0], "BackgroundRoot", "SongBackground"),
        (&paths[1], "ForegroundRoot", "SongForeground"),
    ] {
        fs::write(
            path,
            format!(
                r#"
return Def.ActorFrame {{
    Name="{name}",
    InitCommand=function(self)
        if "{parent}" == "SongBackground" then
            assert(self:GetParent() == nil, 'native background MakeActor has no parent')
        else
            assert(self:GetParent():GetName() == "{parent}")
            assert(self:GetParent():GetChildren()[self:GetName()] == nil,
                "layer must attach the root after Init")
        end
    end,
    OnCommand=function(self)
        local screen = SCREENMAN:GetTopScreen()
        local layer = screen:GetChild("{parent}")
        assert(layer:GetParent() == screen)
        if "{parent}" == "SongBackground" then
            local owner = layer:GetChildren()['']
            assert(self:GetParent() == nil and owner:GetParent() == layer)
            assert(owner:GetChildren()[self:GetName()] == nil,
                'Background Layer keeps authored roots outside the ActorFrame child tree')
        else
            assert(self:GetParent() == layer and layer:GetChildren()[self:GetName()] == self)
        end
        mod_actions = mod_actions or {{}}
        table.insert(mod_actions, {{1,"{name}:{parent}",true}})
    end,
}}
"#
            ),
        )
        .unwrap();
    }
    let mut context = SongLuaCompileContext::new(&song_dir, "Layer Parents");
    context.background_layer_count = 1;
    let compiled =
        test_compile_song_lua_layers(&[paths[0].as_path(), paths[1].as_path()], 1, &context)
            .unwrap();
    assert_eq!(compiled.len(), 2);
    for layer in &compiled {
        let mut messages = layer
            .messages
            .iter()
            .map(|message| message.message.as_str())
            .collect::<Vec<_>>();
        messages.sort_unstable();
        assert_eq!(
            messages,
            [
                "BackgroundRoot:SongBackground",
                "ForegroundRoot:SongForeground"
            ]
        );
    }
}

#[test]
fn custom_draws_keep_empty_tail_frames() {
    let song_dir = test_dir("draw-empty-tail");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame {
    Name="Root",
    OnCommand=function(self)
        local child = self:GetChild("Quad")
        self:SetDrawFunction(function()
            if GAMESTATE:GetSongBeat() >= 1 then self:visible(false) return end
            child:Draw()
        end)
    end,
    Def.Quad {Name="Quad"},
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Draw Tail");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.draw_frames.len(), 121);
    assert_eq!(compiled.draw_frames[59].ops.len(), 1);
    assert!(
        compiled.draw_frames[60..]
            .iter()
            .all(|frame| frame.ops.is_empty() && frame.owners.len() == 1)
    );
}

#[test]
fn compile_song_lua_accepts_set_draw_function() {
    let song_dir = test_dir("set-draw-function");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function draw_fn(self)
    self:visible(true)
end

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetDrawFunction(draw_fn)
        self:queuecommand("Ready")
    end,
    ReadyCommand=function(self)
        mod_actions = {
            {1, tostring(self ~= nil), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Set Draw Function"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.overlays[0].initial_state.visible);
}

#[test]
fn draw_callbacks_replay_after_each_update() {
    let song_dir = test_dir("draw-replay");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Name="DrawRoot",
    OnCommand=function(self)
        local screen = SCREENMAN:GetTopScreen()
        self:SetDrawFunction(function()
            screen:x(GAMESTATE:GetSongBeat() * 10)
        end)
    end,
    Def.ActorFrame{
        Name="Skipped",
        OnCommand=function(self)
            self:SetDrawFunction(function() self:x(999) end)
        end,
    },
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Draw Replay");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let screen = compiled.screen_overlay_index.expect("captured screen");
    let skipped = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Skipped"))
        .unwrap();
    assert_eq!(compiled.overlays[skipped].initial_state.x, 0.0);
    assert!(
        !compiled
            .overlay_updates
            .iter()
            .any(|track| track.overlay_index == skipped)
    );
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == screen && track.target == SongLuaOverlayUpdateTarget::X
        })
        .expect("draw-only callbacks need chronological replay");
    for beat in [0.5, 1.0, 2.0] {
        assert!(x.samples.iter().any(|sample| (sample.time - beat).abs() < 1e-5 && matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if (value - beat * 10.0).abs() < 1e-4)), "missing draw state at beat {beat}");
    }
}

#[test]
fn compile_song_lua_accepts_theme_actor_compat_methods() {
    let song_dir = test_dir("theme-actor-compat-methods");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        ExpandForDoubleCommand=function(self)
            self:aux(2)
        end,
        OnCommand=function(self)
            local command = self:GetCommand("ExpandForDouble")
            local missing = self:GetCommand("MissingCommand")
            if command then command(self) end
            assert(self.jitter == nil and self.distort == nil and self.undistort == nil)
            self:rainbow():hurrytweening(2)
            mod_actions = {
                {1, string.format("%s:%s:%.0f:%.0f", tostring(command ~= nil), tostring(missing == nil), self:getaux(), self:GetTweenTimeLeft()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Actor Compat Methods"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:true:2:0");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.overlays[0].initial_state.rainbow);
}

#[test]
fn compile_song_lua_supports_aux_and_actor_compat_shims() {
    let song_dir = test_dir("actor-aux-compat-shims");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local before = self:getaux()
            self:aux(before + 0.25)
            self:SetTextureFiltering(false):zbuffer(false):ztest(false):ztestmode("WriteOnFail"):draworder(100)
            self:zwrite(true):zbias(2):backfacecull(true):cullmode("CullMode_Back")
            self:aux(self:getaux() + 0.75)
            mod_actions = {
                {1, string.format("%.2f", self:getaux()), true},
            }
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:ztest(false):ztestmode("WriteOnFail")
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Aux Compat Shims"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.00");
    assert_eq!(compiled.overlays.len(), 2);
    assert_eq!(compiled.overlays[0].initial_state.draw_order, 100);
    assert_eq!(compiled.overlays[0].initial_state.z_bias, 2.0);
    assert!(compiled.overlays[0].initial_state.depth_test);
    assert!(!compiled.overlays[0].initial_state.texture_filtering);
    assert!(compiled.overlays[1].initial_state.depth_test);
}

#[test]
fn compile_song_lua_captures_actor_draw_order() {
    let song_dir = test_dir("actor-draw-order");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Late",
        InitCommand=function(self)
            self:draworder(100)
        end,
    },
    Def.Quad{
        Name="Early",
        InitCommand=function(self)
            self:draworder(-10)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Draw Order"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    let late = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Late"))
        .unwrap();
    let early = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Early"))
        .unwrap();
    assert_eq!(late.initial_state.draw_order, 100);
    assert_eq!(early.initial_state.draw_order, -10);
}

#[test]
fn compile_song_lua_reports_draw_function_errors() {
    let song_dir = test_dir("set-draw-function-error");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function draw_fn(self)
    self:MissingDrawMethod()
end

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetDrawFunction(draw_fn)
        mod_actions = {
            {1, "draw-ok", true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Set Draw Function Error"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "draw-ok");
    assert_eq!(compiled.info.unsupported_perframes, 1);
    assert!(compiled.info.unsupported_perframe_captures[0].contains("draw callback"));
}

#[test]
fn compile_many_queued_closures() {
    let song_dir = test_dir("many-queued-closures");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local count = 1024
local account = {nested = {count = 0}}
account.self = account
local original = account
for index = 1, 512 do _G["queue_alias_" .. index] = account end
queue_account = {count = 0}
local global_original = queue_account
local root = Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            assert(account == original and account.self == account)
            assert(queue_alias_1 == original and queue_alias_512 == original)
            assert(queue_account == global_original)
            if GAMESTATE:GetSongBeat() == 0 then
                assert(account.nested.count == 0 and queue_account.count == 0)
            elseif GAMESTATE:GetSongBeat() >= 0.5 then
                assert(account.nested.count == count and queue_account.count == count)
            end
        end)
    end,
}
for index = 1, count do
    root[#root + 1] = Def.Quad{
        OnCommand=function(self) self:queuecommand("Bind") end,
        BindCommand=function(self)
            account.nested.count = account.nested.count + 1
            queue_account.count = queue_account.count + 1
            self:x(index)
        end,
    }
end
return root
"#,
    )
    .expect("queued closure fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued closures");
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("shared startup state");
    assert_eq!(compiled.overlays.len(), 1024);
    assert!(
        compiled
            .overlays
            .iter()
            .all(|actor| actor.initial_state.x == 0.0)
    );
}

#[test]
fn compile_song_lua_defers_queuecommand_until_after_oncommand() {
    let song_dir = test_dir("queuecommand-order");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local child_ready = false

return Def.ActorFrame{
    OnCommand=function(self)
        self:queuecommand("BeginUpdate")
    end,
    BeginUpdateCommand=function(self)
        mod_actions = {
            {1, tostring(child_ready), true},
        }
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            child_ready = true
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Queuecommand Order"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true");
}

#[test]
fn compile_song_lua_layers_share_init_globals_and_actor_refs() {
    let song_dir = test_dir("shared-layer-globals");
    let background = song_dir.join("background.lua");
    let foreground = song_dir.join("foreground.lua");
    fs::write(song_dir.join("helpers.lua"), "helper_loaded = true").unwrap();
    fs::write(
        &background,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="SharedPole",
        OnCommand=function(self)
            self:queuecommand("Bind")
        end,
        BindCommand=function(self)
            shared_poles[1] = self
        end,
    },
}
"#,
    )
    .unwrap();
    fs::write(
        &foreground,
        r#"
mod_actions = {
    {4, function()
        shared_poles[1]:linear(1):x(240)
    end, true},
}

return Def.ActorFrame{
    OnCommand=function(self)
        self:queuecommand("MovePole")
    end,
    MovePoleCommand=function(self)
        shared_poles[1]:x(123)
    end,
    Def.Actor{},
    LoadActor("helpers.lua"),
    Def.Actor{
        InitCommand=function(self)
            assert(helper_loaded)
            shared_poles = {}
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua_layers(
        &[background.as_path(), foreground.as_path()],
        1,
        &SongLuaCompileContext::new(&song_dir, "Shared Layer Globals"),
    )
    .unwrap();

    assert_eq!(compiled.len(), 2);
    assert_eq!(compiled[0].overlays.len(), 1);
    assert_eq!(compiled[0].overlays[0].name.as_deref(), Some("SharedPole"));
    assert_eq!(compiled[0].overlays[0].initial_state.x, 0.0);
    assert_eq!(compiled[0].overlays[0].message_commands.len(), 2);
    let startup = compiled[0].overlays[0]
        .message_commands
        .iter()
        .find(|command| command.message == "__songlua_queued_startup")
        .unwrap();
    assert_eq!(startup.blocks[0].delta.x, Some(123.0));
    let movement = compiled[0]
        .messages
        .iter()
        .find(|event| event.beat == 4.0)
        .unwrap();
    let command = compiled[0].overlays[0]
        .message_commands
        .iter()
        .find(|command| command.message == movement.message)
        .unwrap();
    assert_eq!(command.blocks[0].delta.x, Some(240.0));
    assert!(compiled[1].overlays.is_empty());
    assert!(
        compiled[1]
            .messages
            .iter()
            .any(|event| { event.beat == movement.beat && event.message == movement.message })
    );
}

#[test]
fn compile_song_lua_loads_sparse_actor_children() {
    let song_dir = test_dir("sparse-actor-children");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {{4, "Move", true}}
local actor = Def.ActorFrame{}
actor[2] = Def.Quad{
    Name="First",
    InitCommand=function(self) self:x(12) end,
    OnCommand=function(self) self:y(34) end,
    MoveMessageCommand=function(self) self:x(56) end,
}
actor[1000000] = Def.ActorFrame{
    [3] = Def.Quad{
        Name="Last",
        InitCommand=function(self) self:x(78) end,
        OnCommand=function(self) self:y(90) end,
        MoveMessageCommand=function(self) self:x(123) end,
    },
}
return actor
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sparse Actor Children"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    for (actor, (name, x, y, moved_x)) in compiled
        .overlays
        .iter()
        .zip([("First", 12.0, 34.0, 56.0), ("Last", 78.0, 90.0, 123.0)])
    {
        assert_eq!(actor.name.as_deref(), Some(name));
        assert_eq!(actor.initial_state.x, x);
        assert_eq!(actor.initial_state.y, y);
        let command = actor
            .message_commands
            .iter()
            .find(|command| command.message == "Move")
            .unwrap();
        assert_eq!(command.blocks[0].delta.x, Some(moved_x));
    }
}

#[test]
fn compile_song_lua_exposes_top_screen_player_positions() {
    let song_dir = test_dir("overlay-player-position");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            self:x(player:GetX()):y(player:GetY())
            self:zoomto(48, 64)
        end,
    }
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Overlay Player Position");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            screen_x: 123.0,
            screen_y: 234.0,
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let overlay = &compiled.overlays[0];

    assert_eq!(overlay.initial_state.x, 123.0);
    assert_eq!(overlay.initial_state.y, 234.0);
    assert_eq!(overlay.initial_state.size, None);
    assert_eq!(overlay.initial_state.zoom_x, 48.0);
    assert_eq!(overlay.initial_state.zoom_y, 64.0);
}

#[test]
fn compile_song_lua_captures_direct_player_startup_state() {
    let song_dir = test_dir("player-startup-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        p:x(111):y(222):z(3)
        p:rotationx(10):rotationy(20):rotationz(30)
        p:zoom(0.75):zoomx(0.5):zoomy(1.25)
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Startup State"),
    )
    .unwrap();
    let player = &compiled.player_actors[0];
    assert_eq!(player.initial_state.x, 111.0);
    assert_eq!(player.initial_state.y, 222.0);
    assert_eq!(player.initial_state.z, 3.0);
    assert_eq!(player.initial_state.rot_x_deg, 10.0);
    assert_eq!(player.initial_state.rot_y_deg, 20.0);
    assert_eq!(player.initial_state.rot_z_deg, 30.0);
    assert_eq!(player.initial_state.zoom, 0.75);
    assert_eq!(player.initial_state.zoom_x, 0.5);
    assert_eq!(player.initial_state.zoom_y, 1.25);
}

#[test]
fn compile_song_lua_supports_notefield_column_api() {
    let song_dir = test_dir("notefield-column-api");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function(actor)
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            local pp = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local nf = pp:GetChild("NoteField")
            local cols = nf:GetColumnActors()
            if type(cols) ~= "table" or #cols ~= 4 then
                error("expected four note columns")
            end
            nf:SetDidTapNoteCallback(function() end)
            local zh = cols[1]:GetZoomHandler()
            zh:SetSplineMode("NoteColumnSplineMode_Offset")
                :SetSubtractSongBeat(false)
                :SetReceptorT(0.0)
                :SetBeatsPerT(1/48)
            local spline = zh:GetSpline()
            spline:SetSize(2)
            spline:SetPoint(1, {0, 0, 0})
            spline:SetPoint(2, {-1, -1, -1})
            spline:Solve()
            local po = ps:GetCurrentPlayerOptions()
            if po:Mirror() ~= false or po:Left() ~= false or po:Right() ~= false then
                error("unexpected lane permutation")
            end
            if po:Skew() ~= 0 or po:Tilt() ~= 0 then
                error("unexpected skew or tilt")
            end
            if po:GetReversePercentForColumn(0) ~= 0 then
                error("unexpected reverse percent")
            end
            assert(ArrowEffects.GetYPos(ps, 1, 32) == -103)
            assert(ArrowEffects.GetYPos(ps, 1, 0, 340) == -170)
            po:Reverse(0.5)
            assert(ArrowEffects.GetYPos(ps, 1, 32) == 0)
            po:Reverse(1)
            po:Mini(0.5)
            assert(ArrowEffects.GetYPos(ps, 1, 32) == 148)
            po:Centered(0.5)
            assert(ArrowEffects.GetYPos(ps, 1, 32) == 58)
            assert(ArrowEffects.GetYPos(ps, 1, 32, 360) == 88)
            po:Reverse(0)
            po:Mini(0)
            po:Centered(0)
            mod_actions = {
                {4, string.format("%.0f:%.0f", ArrowEffects.GetXPos(ps, 1, 0), ArrowEffects.GetYPos(ps, 1, 0)), true},
            }
        end)
    end,
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "NoteField Column API"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    // Native YReverseOffset uses 145 - (-125) = 270. GetYPos returns
    // the NoteField-local -135; Player supplies its ten-pixel midpoint.
    assert_eq!(compiled.messages[0].message, "-96:-135");
    assert_eq!(compiled.note_hides.len(), 1);
    assert_eq!(compiled.note_hides[0].player, 0);
    assert_eq!(compiled.note_hides[0].column, 0);
}

#[test]
fn compile_song_lua_captures_column_position_function_eases() {
    let song_dir = test_dir("column-position-function-eases");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local function bounce_col(v)
    local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
    local ca = nf:GetColumnActors()[2]
    local ph = ca:GetPosHandler()
    ph:SetSplineMode("NoteColumnSplineMode_Offset")
    ph:SetBeatsPerT(10)
    local spline = ph:GetSpline()
    spline:SetSize(2)
    spline:SetPoint(1, {0, v, 0})
    spline:SetPoint(2, {0, -v, 0.01*v})
    spline:Solve()
end

mods_ease = {
    {4, 0.5, 33.75, 0, bounce_col, "len", ease.outSine},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Column Position Function Eases"),
    )
    .unwrap();

    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert!(compiled.eases.is_empty());
    assert_eq!(compiled.column_offsets.len(), 5);
    let window = compiled
        .column_offsets
        .iter()
        .find(|window| {
            window.target == SongLuaColumnTransformTarget::PositionPoint { point: 0, axis: 1 }
        })
        .expect("first point Y");
    assert_eq!(window.player, 0);
    assert_eq!(window.column, 1);
    assert_eq!(window.unit, SongLuaTimeUnit::Beat);
    assert_eq!(window.span_mode, SongLuaSpanMode::Len);
    assert_eq!(window.start, 4.0);
    assert_eq!(window.limit, 0.5);
    assert!((window.from_y - 33.75).abs() <= 0.001);
    assert!(window.to_y.abs() <= 0.001);
    assert_eq!(window.easing.as_deref(), Some("outSine"));
    let second = compiled
        .column_offsets
        .iter()
        .find(|window| {
            window.target == SongLuaColumnTransformTarget::PositionPoint { point: 1, axis: 1 }
        })
        .expect("second point Y");
    assert!((second.from_y + 33.75).abs() <= 0.001);
    assert_eq!(second.to_y, 0.0);
    let depth = compiled
        .column_offsets
        .iter()
        .find(|window| {
            window.target == SongLuaColumnTransformTarget::PositionPoint { point: 1, axis: 2 }
        })
        .expect("second point Z");
    assert!((depth.from_y - 0.3375).abs() <= 0.001);
    assert_eq!(depth.to_y, 0.0);
}

#[test]
fn column_position_spline_updates_and_disable_reach_runtime() {
    let song_dir = test_dir("column-position-two-point-update");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
return Def.ActorFrame{InitCommand=function(self)
    self:SetUpdateFunction(function()
        local h = SCREENMAN:GetTopScreen():GetChild("PlayerP2"):GetChild("NoteField"):GetColumnActors()[3]:GetPosHandler()
        if GAMESTATE:GetSongBeat() >= 2 then
            h:SetSplineMode("NoteColumnSplineMode_Disabled")
        else
            h:SetSplineMode("NoteColumnSplineMode_Offset")
            h:SetBeatsPerT(6)
            h:SetReceptorT(0.25)
            h:SetSubtractSongBeat(false)
            local s = h:GetSpline()
            s:SetSize(2)
            s:SetPoint(1, {-12,108,0})
            s:SetPoint(2, {36,-108,0.2})
            s:Solve()
        end
    end)
end}
"#).expect("write Lua fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Column Position Update");
    context.music_length_seconds = 4.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile fixture");
    let timing = deadsync_rules::timing::TimingData::default();
    let windows = crate::gameplay::build_song_lua_column_offset_windows_for_player(
        &compiled, &timing, 1, 0.0,
    );
    let before = deadsync_gameplay::song_lua_column_transforms(&windows, 3, 0.5).1[2];
    assert!(before.enabled);
    assert_eq!(before.beats_per_t, 6.0);
    assert_eq!(before.receptor_t, 0.25);
    assert!(!before.subtract_song_beat);
    assert_eq!(before.points, [[-12.0, 108.0, 0.0], [36.0, -108.0, 0.2]]);
    let (position, derivative) = before.sample(20.0, 3.0);
    assert_eq!(position, [12.0, 0.0, 0.1]);
    assert_eq!(derivative, [48.0, -216.0, 0.2]);
    assert!(!deadsync_gameplay::song_lua_column_transforms(&windows, 3, 3.0).1[2].enabled);
    assert!(!deadsync_gameplay::song_lua_column_transforms(&windows, 3, 0.5).1[1].enabled);
    let p1 = crate::gameplay::build_song_lua_column_offset_windows_for_player(
        &compiled, &timing, 0, 0.0,
    );
    assert!(p1.is_empty());
}

#[test]
fn compile_song_lua_captures_column_rotation_function_eases() {
    let song_dir = test_dir("column-rotation-function-eases");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local function spin_col(v)
    local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
    for _, column in ipairs(nf:GetColumnActors()) do
        assert(column:get_pos_handler() == column:GetPosHandler())
        assert(column:get_zoom_handler() == column:GetZoomHandler())
        local handler = column:get_rot_handler()
        assert(handler == column:GetRotHandler())
        handler:set_spline_mode("NoteColumnSplineMode_Offset")
        handler:set_subtract_song_beat(true):set_receptor_t(0):set_beats_per_t(1)
        local spline = handler:get_spline()
        assert(spline == handler:GetSpline())
        spline:set_loop(true):set_size(1):set_point(1, {0, 0, v}):solve()
        assert(spline:get_loop())
    end
end

mods_ease = {
    {104, 3, 0, math.pi, spin_col, "len", ease.inCubic},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Column Rotation Function Eases"),
    )
    .unwrap();

    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.column_offsets.len(), 4);
    assert!(compiled.column_offsets.iter().all(|window| {
        window.target == SongLuaColumnTransformTarget::RotationZ
            && window.player == 0
            && window.from_y.abs() <= 0.001
            && (window.to_y - std::f32::consts::PI).abs() <= 0.001
            && window.easing.as_deref() == Some("inCubic")
    }));
}

#[test]
fn compile_song_lua_supports_double_style_notefield_columns() {
    let song_dir = test_dir("double-style-notefield-columns");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function(actor)
            local style = GAMESTATE:GetCurrentStyle()
            assert(GAMESTATE:GetCurrentSteps(PLAYER_1):GetStepsType() == style:GetStepsType())
            assert(GAMESTATE:GetCurrentSteps(PLAYER_2):GetStepsType() == style:GetStepsType())
            assert(GAMESTATE:GetCurrentTrail(PLAYER_1):GetStepsType() == style:GetStepsType())
            local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
            assert(stats:GetPlayedSteps()[1]:GetStepsType() == style:GetStepsType())
            local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
            local cols = nf:GetColumnActors()
            local col8 = style:GetColumnInfo(PLAYER_1, 8)
            mod_actions = {
                {
                    1,
                    string.format(
                        "%s:%s:%s:%d:%.0f:%d:%.0f:%d:%.0f",
                        style:GetName(),
                        style:GetStepsType(),
                        style:GetStyleType(),
                        style:ColumnsPerPlayer(),
                        style:GetWidth(PLAYER_1),
                        #cols,
                        cols[8]:GetX(),
                        col8.Track,
                        col8.XOffset
                    ),
                    true
                },
            }
        end)
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Double Style NoteField Columns");
    context.style_name = "double".to_string();
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "double:StepsType_Dance_Double:StyleType_OnePlayerTwoSides:8:512:8:224:7:224"
    );
}

#[test]
fn compile_song_lua_player_options_getters_return_scalars() {
    let song_dir = test_dir("player-options-getters");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        if po:Reverse() ~= 0 then
            error("expected reverse getter to default to 0")
        end
        if po:Mini() ~= 0 then
            error("expected mini getter to default to 0")
        end
        po:Reverse(1, 1)
        po:Mini(0.25, 1)
        po:Mirror(true)
        mod_actions = {
            {1, string.format("%.2f:%.2f:%s", po:Reverse(), po:Mini(), tostring(po:Mirror())), true},
        }
    end,
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Options Getters"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.00:0.25:true");
}

#[test]
fn compile_song_lua_custom_mod_vars_do_not_probe_as_player_options() {
    let song_dir = test_dir("custom-mod-vars-player-options");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
local activeMods = { ConstellationBg = { value = 1 } }

local function getVar(name)
    local value = 0
    if not po[name] then
        local activeMod = activeMods[name]
        if activeMod and activeMod.value then
            value = activeMod.value
        end
    end
    return value
end

return Def.ActorFrame{
    InitCommand=function(self)
        if type(po["MoveX16"]) ~= "function" then
            error("expected real multicol PlayerOptions method")
        end
        if po["ConstellationBg"] ~= nil then
            error("custom mod variable should not be a PlayerOptions method")
        end
    end,
    Def.ActorFrame{
        Name="ConstellationFrame",
        ConstellationBgShowMessageCommand=function(self)
            if getVar("ConstellationBg") ~= 1 then
                error("custom mod variable masked by PlayerOptions")
            end
            local decoy = Def.ActorFrame{ Name="Decoy" }
            local constellationFrame = decoy:GetChild("ConstellationFrame")
            local starfieldFrame = constellationFrame and constellationFrame:GetChild("StarfieldFrame")
            local starFrame = starfieldFrame and starfieldFrame:GetChild("StarFrame")
            if starFrame ~= nil then
                error("missing actor children should not synthesize a StarFrame")
            end
            self:x(12)
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                MESSAGEMAN:Broadcast("ConstellationBgShow")
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Custom Mod Vars"),
    )
    .unwrap();
}

#[test]
fn compile_song_lua_player_options_speed_setters_chain() {
    let song_dir = test_dir("player-options-speed-setters");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        local initial = string.format("%.2f:%s:%s", po:XMod(), tostring(po:CMod()), tostring(po:NoMines()))
        po:XMod(3.5, 9e9, true):Overhead(true, 9e9, true):Mini(0.15, 9e9, true)
        local after_x = string.format("%.2f:%s:%.2f", po:XMod(), tostring(po:Overhead()), po:Mini())
        po:CMod(650, 1)
        local after_c = string.format("%s:%.0f:%s", tostring(po:XMod()), po:CMod(), tostring(po:MMod()))
        po:CMod(nil, 1, true):MMod(700, 1)
        local after_m = string.format("%s:%s:%.0f", tostring(po:XMod()), tostring(po:CMod()), po:MMod())
        mod_actions = {
            {1, table.concat({initial, after_x, after_c, after_m}, "|"), true},
        }
    end,
}
"#,
        )
        .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Player Options Speed Setters");
    context.players[0].speedmod = SongLuaSpeedMod::X(2.25);
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2.25:nil:false|3.50:true:0.15|nil:650:nil|1:nil:700"
    );
}

#[test]
fn compile_song_lua_player_options_from_string_parses_common_mods() {
    let song_dir = test_dir("player-options-from-string");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local po = ps:GetPlayerOptions("ModsLevel_Song")
        po:FromString("*4 350% Drunk, 50% Mini, Overhead, NoMines, 3.5x")
        local parsed_x = string.format(
            "%.1f:%.1f:%.2f:%s:%s",
            po:XMod(),
            po:Drunk(),
            po:Mini(),
            tostring(po:Overhead()),
            tostring(po:NoMines())
        )
        po:FromString("C650, 0% Overhead")
        local parsed_c = string.format("%s:%.0f:%s", tostring(po:XMod()), po:CMod(), tostring(po:Overhead()))
        ps:SetPlayerOptions("ModsLevel_Song", "M700, Shuffle, 25% Tiny")
        local parsed_set = string.format(
            "%s:%.0f:%s:%.2f:%s",
            tostring(po:CMod()),
            po:MMod(),
            tostring(po:Shuffle()),
            po:Tiny(),
            ps:GetPlayerOptionsString("ModsLevel_Song")
        )
        mod_actions = {
            {1, table.concat({parsed_x, parsed_c, parsed_set}, "|"), true},
        }
        po:FromString("*50 30+0% Beat")
        assert(math.abs(po:Beat() - 0.3) < 0.000001)
    end,
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Options FromString"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "3.5:3.5:0.50:true:true|nil:650:true|nil:700:true:0.25:NoHideLights, m700, 25% Tiny, Shuffle, Overhead"
    );
}

#[test]
fn compile_song_lua_player_options_exposes_modchart_gates() {
    let song_dir = test_dir("player-options-modchart-gates");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local required = {
            "CAMod",
            "DrawSize",
            "TanDrunk",
            "LifeSetting",
            "DrainSetting",
            "HideLightSetting",
            "ModTimerSetting",
            "WavePeriod",
            "PulseInner",
            "BounceZ",
            "TanDigitalZPeriod",
            "MoveX16",
            "ConfusionOffset16",
            "MinTNSToHideNotes",
            "VisualDelay",
            "UsingReverse",
            "GetStepAttacks",
            "IsEasierForSongAndSteps",
            "IsEasierForCourseAndTrail",
        }
        for _, name in ipairs(required) do
            if not PlayerOptions[name] then
                error("expected PlayerOptions capability gate: " .. name)
            end
        end
        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        po:CAMod(640, 9e9, true):DrawSize(0.25, 9e9, true):DizzyHolds(true, true):StealthPastReceptors(true)
        local direct = string.format(
            "%s:%.0f:%.2f:%s:%s",
            tostring(po:CMod()),
            po:CAMod(),
            po:DrawSize(),
            tostring(po:DizzyHolds()),
            tostring(po:StealthPastReceptors())
        )
        po:LifeSetting("LifeType_Battery")
        po:DrainSetting("DrainType_NoRecover")
        po:HideLightSetting("HideLightType_HideAllLights")
        po:ModTimerSetting("ModTimerType_Beat", true)
        po:FailSetting("FailType_Off")
        po:MinTNSToHideNotes("TapNoteScore_W3")
        po:WavePeriod(2.5)
        po:PulseInner(0.25)
        po:BounceZ(3)
        po:TanDigitalZPeriod(4)
        po:MoveX16(0.75)
        po:Reverse(1)
        po:VisualDelay(0.12)
        po:BatteryLives(4)
        po:Passmark(0.2)
        local surface = string.format(
            "%s:%s:%s:%s:%s:%s:%.1f:%.2f:%.0f:%.0f:%.2f:%.0f:%s:%.0f:%.0f:%s:%s:%.1f",
            po:LifeSetting(),
            po:DrainSetting(),
            po:HideLightSetting(),
            po:ModTimerSetting(),
            po:FailSetting(),
            po:MinTNSToHideNotes(),
            po:WavePeriod(),
            po:PulseInner(),
            po:BounceZ(),
            po:TanDigitalZPeriod(),
            po:MoveX16(),
            po:BatteryLives(),
            tostring(po:UsingReverse()),
            po:GetReversePercentForColumn(0),
            po:GetStepAttacks(),
            tostring(po:IsEasierForSongAndSteps(GAMESTATE:GetCurrentSong(), GAMESTATE:GetCurrentSteps(PLAYER_1), PLAYER_1)),
            tostring(po:IsEasierForCourseAndTrail(GAMESTATE:GetCurrentCourse(), GAMESTATE:GetCurrentTrail(PLAYER_1))),
            po:Passmark()
        )
        po:FromString("*9999 DizzyHolds, *9999 StealthPastReceptors, CA720")
        local parsed = string.format("%s:%.0f:%s", tostring(po:CMod()), po:CAMod(), tostring(po:DizzyHolds()))
        mod_actions = {
            {1, direct .. "|" .. parsed .. "|" .. surface, true},
        }
    end,
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Options Modchart Gates"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "nil:640:0.25:true:true|nil:720:true|LifeType_Battery:DrainType_NoRecover:HideLightType_HideAllLights:ModTimerType_Beat:FailType_Off:TapNoteScore_W3:2.5:0.25:3:4:0.75:4:true:1:1:false:false:0.2"
    );
}

#[test]
fn compile_song_lua_supports_player_option_timing_windows() {
    let song_dir = test_dir("player-options-timing-windows");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local po = ps:GetPlayerOptions("ModsLevel_Preferred")
        po:DisableTimingWindow("TimingWindow_W5", true)
            :DisableTimingWindow("W3", true)
            :DisableTimingWindow(1, true)
            :DisableTimingWindow("TimingWindow_W5")
        local before = po:GetDisabledTimingWindows()
        po:ResetDisabledTimingWindows()
        po:DisableTimingWindow("TimingWindow_W4")
        local after = po:GetDisabledTimingWindows()
        mod_actions = {
            {
                1,
                string.format(
                    "%d:%s:%s:%s:%d:%s:%s",
                    #before,
                    before[1],
                    before[2],
                    before[3],
                    #after,
                    after[1],
                    ps:GetPlayerController()
                ),
                true,
            },
            {
                2,
                function()
                    po:ResetDisabledTimingWindows()
                    po:DisableTimingWindow("TimingWindow_W1")
                end,
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Option Timing Windows"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "3:TimingWindow_W2:TimingWindow_W3:TimingWindow_W5:1:TimingWindow_W4:PlayerController_Human"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_life_meter_and_health_state_helpers() {
    let song_dir = test_dir("life-meter-health-state");
    fs::create_dir_all(song_dir.join("metal")).unwrap();
    fs::write(song_dir.join("metal/metrics.ini"), "[Global]\n").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local ps = GAMESTATE:GetPlayerState(PLAYER_1)
        local top = SCREENMAN:GetTopScreen()
        local current_options = ps:GetCurrentPlayerOptions()
        local requested_options = ps:GetPlayerOptions("ModsLevel_Preferred")
        current_options:NoteSkin("metal")
        assert(current_options:NoteSkin() == "metal")
        assert(requested_options:NoteSkin() ~= "metal", "direct Current writes stay independent")
        local life = top:GetLifeMeter(ps:GetPlayerNumber())
        local child_life = top:GetChild("Life"..ToEnumShortString(ps:GetPlayerNumber()))
        local generic_life = top:GetChild("LifeMeter")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%.1f:%.1f:%s:%s:%s:%s:%d:%d",
                    ps:GetPlayerNumber(),
                    ps:GetHealthState(),
                    ps:GetPlayerController(),
                    life:GetLife(),
                    child_life:GetLife(),
                    tostring(life:IsFailing()),
                    tostring(life:IsInDanger()),
                    tostring(life:IsHot()),
                    tostring(generic_life ~= nil),
                    HealthState:Reverse()[ps:GetHealthState()],
                    PlayerController:Reverse()[ps:GetPlayerController()]
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Life Meter Health State"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "PlayerNumber_P1:HealthState_Alive:PlayerController_Human:0.5:0.5:false:false:false:false:1:0"
    );
}

#[test]
fn compile_song_lua_exposes_top_screen_score_percent_children() {
    let song_dir = test_dir("top-screen-score-percent-children");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local p1 = top:GetChild("ScoreP1"):GetChild("ScoreDisplayPercentage Percent"):GetChild("PercentP1")
        local p2 = top:GetChild("ScoreP2"):GetChild("ScoreDisplayPercentage Percent"):GetChild("PercentP2")
        local score1 = tonumber(string.sub(p1:GetText(), 1, -2))
        local score2 = tonumber(string.sub(p2:GetText(), 1, -2))
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%.0f:%.0f:%s:%s:%s",
                    top:GetChild("ScoreP1"):GetName(),
                    top:GetChild("ScoreP2"):GetName(),
                    score1,
                    score2,
                    p1:GetText(),
                    p2:GetName(),
                    tostring(p1:GetParent():GetName())
                ),
                true,
            },
        }
    end,
}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Score Percent Children"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ScoreP1:ScoreP2:0:0:0.00%:PercentP2:ScoreDisplayPercentage Percent"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_top_screen_theme_actor_shapes() {
    let song_dir = test_dir("top-screen-theme-actor-shapes");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local underlay = top:GetChild("Underlay")
        local function bpm_leaf(underlay)
            local owners=underlay:GetChildren()['']
            if owners and tostring(owners):find('ActorFrame',1,true) then owners={owners} end
            for _, owner in ipairs(owners or {}) do
                if owner.GetChild and owner:GetChild('RatemodDisplay') then
                    local inner=owner:GetChild('')
                    return inner:GetChild('BPMDisplay') or inner:GetChild('DisplayP1')
                end
            end
            error('native shared BPM owner missing')
        end
        local bpm = bpm_leaf(underlay)
        assert(underlay:GetChild("BPMDisplay") == nil)
        assert(top:GetScreenType() == "ScreenType_Gameplay")
        assert(self.GetScreenType == nil)
        assert(top:GetChild("BPMDisplay") == nil)
        assert(top:GetChild("SongTitle") == nil)
        assert(top:GetChild("SongMeterDisplayP1") == nil)
        local steps = top:GetChild("StepsDisplayP1")
        local p1_score = underlay:GetChild("P1Score")
        local song_meter_title = underlay:GetChild("SongMeter"):GetChild("SongTitle")
        local song_meter = underlay:GetChild("SongMeter")
        local overlay = top:GetChild("Overlay")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s:%s:%s",
                    bpm:GetName(),
                    bpm:GetText(),
                    song_meter_title:GetText(),
                    tostring(steps.GetText == nil),
                    p1_score:GetName(),
                    p1_score:GetText(),
                    song_meter:GetName(),
                    overlay:GetName()
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Actor Shapes");
    context.song_display_bpms = [120.0, 180.0];
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "DisplayP1::Theme Actor Shapes:true:P1Score:0.00%:SongMeter:Overlay"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_underlay_has_only_enabled_player_children() {
    let song_dir = test_dir("underlay-enabled-players");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
        local children = underlay:GetChildren()
        for p=1,2 do
            local enabled = GAMESTATE:IsPlayerEnabled('PlayerNumber_P'..p)
            for _, name in ipairs({'P'..p..'Score', 'StepStatsPaneP'..p, 'DangerP'..p}) do
                assert((underlay:GetChild(name) ~= nil) == enabled, name)
                assert((children[name] ~= nil) == enabled, name..' enumeration')
            end
        end
        assert(underlay:GetChild('BPMDisplay') == nil)
        assert(underlay:GetChild('SongMeter'))
    end,
}
"#,
    )
    .unwrap();
    for enabled in [[true, false], [false, true], [true, true]] {
        let mut context = SongLuaCompileContext::new(&song_dir, "enabled underlay players");
        context.screen_width = 854.0;
        for (player, enabled) in context.players.iter_mut().zip(enabled) {
            player.enabled = enabled;
        }
        test_compile_song_lua(&entry, &context).expect("native per-player underlay ownership");
    }
}

#[test]
fn compile_song_lua_keeps_score_targets_and_layer_tweens() {
    let song_dir = test_dir("theme-score-layer-targets");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local score
return Def.ActorFrame{
    OnCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local underlay = top:GetChild("Underlay")
        underlay:visible(false)
        score = underlay:GetChild("P1Score")
        score:visible(true):diffusealpha(1)
        top:GetChild("Overlay"):smooth(1.5):diffusealpha(0)
        top:GetChild("SongBackground"):visible(false)
    end,
    Def.ActorProxy{
        OnCommand=function(self) self:queuecommand("Set") end,
        SetCommand=function(self) self:SetTarget(score) end,
    },
}
"#,
    )
    .expect("write score/layer fixture");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Layer Targets"),
    )
    .expect("compile score/layer fixture");
    assert!(compiled.overlays.iter().any(|actor| matches!(
        actor.kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Score { player_index: 0 }
        }
    )));
    assert!(!compiled.overlays.iter().any(|actor| matches!(
        actor.kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Underlay { .. }
        }
    )));
    assert!(!compiled.screen_layers[0].initial_state.visible);
    assert!(!compiled.screen_layers[2].initial_state.visible);
    let overlay = &compiled.screen_layers[1];
    assert_eq!(overlay.initial_state.diffuse[3], 1.0);
    assert!(
        overlay
            .message_commands
            .iter()
            .flat_map(|command| &command.blocks)
            .any(|block| block.duration == 1.5
                && block.delta.diffuse.is_some_and(|color| color[3] == 0.0))
    );
}

#[test]
fn compile_song_lua_enumerates_top_screen_theme_children() {
    let song_dir = test_dir("top-screen-theme-child-enumeration");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local children = top:GetChildren()
        local wanted = {
            "PlayerP1",
            "PlayerP2",
            "Underlay",
            "Overlay",
            "SongForeground",
            "SongBackground",
            "ScoreP1",
            "ScoreP2",
            "StepsDisplayP1",
        }
        for _, name in ipairs(wanted) do
            assert(children[name], name)
        end
        local function bpm_leaf(underlay)
            local owners=underlay:GetChildren()['']
            if owners and tostring(owners):find('ActorFrame',1,true) then owners={owners} end
            for _, owner in ipairs(owners or {}) do
                if owner.GetChild and owner:GetChild('RatemodDisplay') then
                    local inner=owner:GetChild('')
                    return inner:GetChild('BPMDisplay') or inner:GetChild('DisplayP1')
                end
            end
            error('native shared BPM owner missing')
        end
        local underlay_children = children.Underlay:GetChildren()
        local bpm=bpm_leaf(children.Underlay)
        assert(underlay_children.BPMDisplay == nil)
        assert(underlay_children.P1Score:GetText() == "0.00%")
        assert(underlay_children.SongMeter:GetChild("SongTitle"):GetText() == "Enumeration")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s",
                    tostring(top:GetNumChildren() >= 12),
                    bpm:GetText(),
                    tostring(children.StepsDisplayP1.GetText == nil),
                    underlay_children.P1Score:GetName(),
                    underlay_children.SongMeter:GetChild("SongTitle"):GetText(),
                    tostring(children.PlayerP1 == top:GetChild("PlayerP1"))
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Enumeration");
    context.song_display_bpms = [150.0, 150.0];
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "true::true:P1Score:Enumeration:true"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn actor_string_scans() {
    let song_dir = test_dir("actor-tostring-scans");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local children = top:GetChildren()
        local player = tostring(children.PlayerP1)
        local underlay = tostring(children.Underlay)
        local steps = tostring(children.StepsDisplayP1)
        local score = tostring(children.ScoreP1:GetChild("ScoreDisplayPercentage Percent"))
        assert(not steps:find("StepsDisplayP1", 1, true))
        assert(not player:find("PlayerP1", 1, true))
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s",
                    tostring(player:find("Player") ~= nil),
                    tostring(underlay:find("ActorFrame") ~= nil),
                    tostring(steps:find("^StepsDisplay %(") ~= nil),
                    tostring(score:find("PercentageDisplay") ~= nil)
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Tostring Scans"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:true:true:true");
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_lowercase_getrotation() {
    let song_dir = test_dir("actor-lowercase-getrotation");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        player:rotationx(10):rotationy(20):rotationz(30)
        player:addrotationx(5):addrotationy(6):addrotationz(7)
        local rx, ry, rz = player:getrotation()
        mod_actions = {
            {
                1,
                string.format("%.0f:%.0f:%.0f", rx, ry, rz),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Lowercase Getrotation"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "15:26:37");
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_top_screen_edit_state() {
    let song_dir = test_dir("top-screen-edit-state");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(EditState:Reverse()["EditState_Playing"] == 3)

return Def.ActorFrame{
    OnCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        mod_actions = {
            {1, top:GetEditState(), true},
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Top Screen Edit State"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "EditState_Playing");
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_supports_gameplay_layout_and_note_field_shims() {
    let song_dir = test_dir("gameplay-layout-note-field-shims");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local mods = SL.P1.ActiveModifiers
        local opts = GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()
        local layout = GetGameplayLayout(PLAYER_1, opts:Reverse() ~= 0)
        local notefield = GetPlayerAF("P1"):GetChild("NoteField")
        local set_bars = notefield:SetBeatBars(true)
        local set_alpha = notefield:SetBeatBarsAlpha(0.75, 0.5, 0.25, 0)
        local alpha = notefield.__songlua_beat_bars_alpha
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s:%s:%.0f:%.0f:%s:%s:%s:%.2f:%.2f",
                    mods.ErrorBar,
                    mods.MeasureCounter,
                    mods.MeasureLines,
                    tostring(mods.ColumnCues),
                    mods.Spacing,
                    tostring(mods.MeasureCounterUp),
                    tostring(mods.SubtractiveScoring),
                    layout.Combo.y,
                    layout.SubtractiveScoring.y,
                    tostring(set_bars == notefield),
                    tostring(set_alpha == notefield),
                    tostring(notefield.__songlua_beat_bars),
                    alpha[1],
                    alpha[3]
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Gameplay Layout Note Field Shims"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "None:None:Off:false:0%:true:false:270:182:true:true:true:0.75:0.25"
    );
}

#[test]
fn compile_song_lua_supports_named_player_tap_note_shim() {
    let song_dir = test_dir("named-player-tap-note-shim");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local player = top:GetChild("PlayerP1")
        assert(top:GetChildren()["PlayerP1"] == player)
        local nf = player:GetChild("NoteField")
        local seen = {}
        nf:set_did_tap_note_callback(function(col, score, bright)
            seen = {col, score, bright}
        end)
        local ret = nf:did_tap_note(2, "TapNoteScore_W1", true)
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%d:%s:%s:%s:%d:%s:%s",
                    player:GetName(),
                    seen[1],
                    seen[2],
                    tostring(seen[3]),
                    tostring(ret == nf),
                    nf.__songlua_last_tap_note_column,
                    nf.__songlua_last_tap_note_score,
                    tostring(nf.__songlua_last_tap_note_bright)
                ),
                true,
            },
        }
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Named Player Tap Note Shim"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "PlayerP1:2:TapNoteScore_W1:true:true:2:TapNoteScore_W1:true"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_extracts_actorframe_overlay_hierarchy() {
    let song_dir = test_dir("overlay-hierarchy");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("grid.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
local wrapper = nil

mod_actions = {
    {8, function()
        if wrapper then
            wrapper:visible(true)
            wrapper:zoom(2)
        end
    end, true},
}

return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self)
            wrapper = self
            self:visible(false)
        end,
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
        end,
        Def.Sprite{
            Texture="gfx/grid.png",
            OnCommand=function(self)
                self:xy(10, 20)
            end,
        },
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Hierarchy"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrame
    ));
    assert_eq!(compiled.overlays[0].parent_index, None);
    assert_eq!(compiled.overlays[0].initial_state.x, 320.0);
    assert_eq!(compiled.overlays[0].initial_state.y, 240.0);
    assert!(!compiled.overlays[0].initial_state.visible);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .zoom,
        Some(2.0)
    );
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .visible,
        Some(true)
    );
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("gfx/grid.png")
    ));
    assert_eq!(compiled.overlays[1].parent_index, Some(0));
    assert_eq!(compiled.overlays[1].initial_state.x, 10.0);
    assert_eq!(compiled.overlays[1].initial_state.y, 20.0);
}

#[test]
fn compile_song_lua_captures_player_and_song_foreground_actions() {
    let song_dir = test_dir("player-foreground-actions");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {0, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local fg = SCREENMAN:GetTopScreen():GetChild("SongForeground")
        if p then
            p:linear(1):x(SCREEN_CENTER_X + 40):z(5):zoom(0.6):rotationz(15)
        end
        if fg then
            fg:z(4)
        end
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Foreground Actions"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
    assert_eq!(compiled.song_foreground.message_commands.len(), 1);
    let player_block = &compiled.player_actors[0].message_commands[0].blocks[0];
    assert_eq!(player_block.delta.x, Some(360.0));
    assert_eq!(player_block.delta.z, Some(5.0));
    assert_eq!(player_block.delta.zoom, Some(0.6));
    assert_eq!(player_block.delta.rot_z_deg, Some(15.0));
    let fg_block = &compiled.song_foreground.message_commands[0].blocks[0];
    assert_eq!(fg_block.delta.z, Some(4.0));
}

#[test]
fn compile_song_lua_captures_function_actions_via_broadcast() {
    let song_dir = test_dir("broadcast-function-action");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {2, function()
        MESSAGEMAN:Broadcast("Flash")
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        FlashMessageCommand=function(self)
            self:linear(0.5)
            self:x(96)
            self:diffusealpha(0.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast Function Action"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let block = &compiled.overlays[0].message_commands[0].blocks[0];
    assert_eq!(block.duration, 0.5);
    assert_eq!(block.delta.x, Some(96.0));
    assert_eq!(block.delta.diffuse.unwrap()[3], 0.5);
}

#[test]
fn compile_song_lua_preserves_direct_effects_with_broadcasts() {
    let song_dir = test_dir("broadcast-and-direct-function-action");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local direct
mod_actions = {
    {2, function()
        MESSAGEMAN:Broadcast("Flash")
        direct:x(99)
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        FlashMessageCommand=function(self)
            self:diffusealpha(0.5)
        end,
    },
    Def.Quad{
        InitCommand=function(self)
            direct = self
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast And Direct Function Action"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.message == "Flash" && event.beat == 2.0)
    );
    let generated = compiled
        .messages
        .iter()
        .find(|event| event.message.starts_with("__songlua_overlay_fn_action_"))
        .expect("direct actor effect should have a generated message");
    let flash_actor = compiled
        .overlays
        .iter()
        .find(|actor| {
            actor
                .message_commands
                .iter()
                .any(|command| command.message == "Flash")
        })
        .expect("Flash listener should compile");
    assert!(
        flash_actor
            .message_commands
            .iter()
            .all(|command| command.message != generated.message)
    );
    let direct_block = compiled
        .overlays
        .iter()
        .flat_map(|actor| &actor.message_commands)
        .find(|command| command.message == generated.message)
        .and_then(|command| command.blocks.first())
        .expect("direct actor effect should compile separately from the broadcast");
    assert_eq!(direct_block.delta.x, Some(99.0));
}

#[test]
fn compile_song_lua_accepts_side_effect_only_function_actions() {
    let song_dir = test_dir("function-action-side-effects");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, function() SCREENMAN:SystemMessage("hello") end, true},
    {2, function() SM("hello") end, true},
    {3, function() SCREENMAN:SetNewScreen("ScreenGameplay") end, true},
    {4, function() SCREENMAN:GetTopScreen():StartTransitioningScreen("SM_DoNextScreen") end, true},
    {5, function() MESSAGEMAN:Broadcast("NoListeners") end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Function Action Side Effects"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_accepts_offline_theme_io_network_helpers() {
    let song_dir = test_dir("offline-theme-io-network");
    let plugin_dir = song_dir.join("plugins");
    fs::create_dir_all(plugin_dir.join("nested")).unwrap();
    fs::write(plugin_dir.join("alpha.lua"), "payload").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local function has_value(values, wanted)
    for _, value in ipairs(values) do
        if value == wanted then return true end
    end
    return false
end

local theme_info = IniFile.ReadFile(THEME:GetCurrentThemeDirectory() .. "ThemeInfo.ini").ThemeInfo
assert(theme_info.DisplayName == "Simply Love")
local missing_ini = IniFile.ReadFile("Save/Missing.ini")
assert(type(missing_ini) == "table" and next(missing_ini) == nil)
assert(IniFile.WriteFile("Save/ThemePrefs.ini", {SimplyLove={DarkUI=true}}) == true)
local plugins = GAMESTATE:GetCurrentSong():GetSongDir() .. "plugins/"
local plugin_files = FILEMAN:GetDirListing(plugins)
local plugin_dirs = FILEMAN:GetDirListing(plugins, true, false)
local plugin_lua_paths = FILEMAN:GetDirListing(plugins .. "*.lua", false, true)
assert(#plugin_files == 2 and has_value(plugin_files, "alpha.lua") and has_value(plugin_files, "nested"))
assert(#plugin_dirs == 1 and plugin_dirs[1] == "nested")
assert(#plugin_lua_paths == 1 and plugin_lua_paths[1]:match("alpha%.lua$"))
assert(FILEMAN:DoesFileExist(plugins .. "alpha.lua") == true)
assert(FILEMAN:DoesFileExist(plugins .. "nested") == true)
assert(FILEMAN:DoesFileExist(plugins .. "missing.lua") == false)
assert(FILEMAN:GetFileSizeBytes(plugins .. "alpha.lua") == 7)
assert(FILEMAN:GetFileSizeBytes(plugins .. "missing.lua") == 0)
assert(FILEMAN:GetHashForFile(plugins .. "alpha.lua") == 0)
local encoded = JsonEncode({a=1, b="two words", nested={true,false}})
local decoded = JsonDecode(encoded)
assert(decoded.a == 1)
assert(BinaryToHex(CRYPTMAN:SHA1String("chart")) == string.rep("0", 40))
assert(BinaryToHex(CRYPTMAN:SHA1File("scores.json")) == string.rep("0", 40))
assert(CRYPTMAN:GenerateRandomUUID() == "00000000-0000-4000-8000-000000000000")
assert(NETWORK:IsUrlAllowed("https://example.invalid") == false)
assert(NETWORK:EncodeQueryParameters({b="two words", a=1}) == "a=1&b=two%20words")
local request = NETWORK:HttpRequest{url="https://example.invalid"}
assert(request.body == "" and request.status == 0 and request.code == 0 and request.error == "offline")
assert(type(request.headers) == "table")
assert(request:IsFinished() == true)
assert(request:GetResponse() == request)
local ws = NETWORK:WebSocket{url="wss://example.invalid"}
assert(ws.is_open == false and ws:IsOpen() == false)
assert(ws:Send(JsonEncode({uuid=CRYPTMAN:GenerateRandomUUID()})) == nil)
assert(ws:Close() == nil)
local file = RageFileUtil:CreateRageFile()
assert(file:Open("Save/Offline.json", 2))
assert(file:Write(encoded) == #encoded)
assert(not pcall(function() file:Read() end))
assert(file:Close() == file.Close)
assert(file:destroy() == file.destroy)
local dot_file = RageFileUtil.CreateRageFile()
assert(not pcall(function() dot_file:Read() end))
assert(FILEMAN:Unzip("archive.zip", "Songs/Pack") == false)
assert(GetTimingWindow(2) > GetTimingWindow(1))
assert(GetWorstJudgment({{0, GetTimingWindow(3)}}) == 3)
local ex, points, possible = CalculateExScore(PLAYER_1)
assert(ex == 0 and points == 0 and possible == 0)

mod_actions = {
    {1, function()
        assert(NETWORK:HttpRequest{url="https://example.invalid", body=JsonEncode(decoded)}:Cancel() == nil)
        GAMESTATE:JoinPlayer(PLAYER_1)
        assert(CRYPTMAN:SignFileToFile("Save/Offline.json", "Save/Offline.sig") == false)
        assert(FILEMAN:Copy(plugins .. "alpha.lua", "Save/alpha.lua") == false)
        assert(FILEMAN:CreateDir("Save") == true)
        assert(FILEMAN:Remove("Save/Offline.json") == true)
        assert(FILEMAN:FlushDirCache() == nil)
        assert(IsHumanPlayer(PLAYER_1) == GAMESTATE:IsSideJoined(PLAYER_1))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Offline Theme Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_sha256_crypt_helpers() {
    let song_dir = test_dir("sha256-crypt-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local key = CRYPTMAN:SHA256String("player-scores")
local file_key = CRYPTMAN:SHA256File("scores.json")

mod_actions = {
    {
        1,
        string.format(
            "%d:%d:%s:%s",
            #key,
            #file_key,
            BinaryToHex(key),
            BinaryToHex(file_key)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SHA256 Crypt Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        format!("32:32:{}:{}", "0".repeat(64), "0".repeat(64))
    );
}

#[test]
fn compile_song_lua_accepts_lua_file_and_profile_helpers() {
    let song_dir = test_dir("lua-file-profile-helpers");
    fs::write(song_dir.join("favorites.txt"), "Group/Song\n").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
assert(lua.ReadFile("favorites.txt") == "Group/Song\n")
assert(lua.ReadFile("missing.txt") == nil)
Warn("compile warning")
lua.ReportScriptError("compile error report")

local profile = PROFILEMAN:GetProfile(PLAYER_1)
assert(PROFILEMAN:GetProfile(PLAYER_1) == profile)
assert(PROFILEMAN:GetProfile(ProfileSlot[1]) == profile)
assert(PROFILEMAN:GetProfile(ProfileSlot[3]) == PROFILEMAN:GetMachineProfile())
assert(PROFILEMAN:GetProfileDir(ProfileSlot[3]) == "/Save/MachineProfile/")
assert(PROFILEMAN:LocalProfileIDToDir("abc") == "/Save/LocalProfiles/abc/")
assert(PROFILEMAN:GetLocalProfile("missing") == nil)
assert(PROFILEMAN:GetLocalProfileIndexFromID("missing") == -1)
assert(#PROFILEMAN:GetLocalProfileIDs() == 0)
assert(#PROFILEMAN:GetLocalProfileDisplayNames() == 0)
assert(PROFILEMAN:IsSongNew(GAMESTATE:GetCurrentSong()) == false)
assert(PROFILEMAN:ProfileWasLoadedFromMemoryCard(PLAYER_1) == false)
assert(PROFILEMAN:LastLoadWasTamperedOrCorrupt(PLAYER_1) == false)
assert(PROFILEMAN:ProfileFromMemoryCardIsNew(PLAYER_1) == false)
assert(PROFILEMAN:GetSongNumTimesPlayed(GAMESTATE:GetCurrentSong(), ProfileSlot[1]) == 0)
assert(PROFILEMAN:SaveProfile(PLAYER_1) == false)
assert(PROFILEMAN:SaveLocalProfile("missing") == false)
assert(PROFILEMAN:SetStatsPrefix("Stats") == PROFILEMAN)
assert(PROFILEMAN:GetStatsPrefix() == "Stats")
assert(profile:GetType() == "ProfileType_Normal")
assert(profile:GetPriority() == 0)
assert(profile:SetDisplayName("AAA") == profile)
assert(profile:GetDisplayName() == "AAA")
assert(profile:GetCaloriesBurnedToday() == 0)
assert(profile:GetNumTotalSongsPlayed() == 0)
assert(profile:GetTotalNumSongsPlayed() == 0)
assert(profile:GetTotalSessions() == 0)
assert(profile:GetIgnoreStepCountCalories() == false)
assert(profile:CalculateCaloriesFromHeartRate(120, 60) == 0)
assert(profile:SetWeightPounds(180) == profile)
assert(profile:GetWeightPounds() == 180)
assert(profile:SetVoomax(42.5) == profile)
assert(profile:GetVoomax() == 42.5)
assert(profile:SetBirthYear(2000) == profile)
assert(profile:GetBirthYear() == 2000)
assert(profile:SetIgnoreStepCountCalories(true) == profile)
assert(profile:GetIgnoreStepCountCalories() == true)
assert(profile:SetIsMale(false) == profile)
assert(profile:GetIsMale() == false)
assert(profile:SetGoalType("GoalType_Calories") == profile)
assert(profile:GetGoalType() == "GoalType_Calories")
assert(profile:SetGoalCalories(120) == profile)
assert(profile:GetGoalCalories() == 120)
assert(profile:SetGoalSeconds(90) == profile)
assert(profile:GetGoalSeconds() == 90)
assert(profile:AddCaloriesToDailyTotal(5) == profile)
assert(profile:GetCaloriesBurnedToday() == 5)
assert(profile:GetTotalCaloriesBurned() == 5)
assert(profile:GetDisplayTotalCaloriesBurned() == "5 Cal")
assert(profile:SetLastUsedHighScoreName("AAA") == profile)
assert(profile:GetLastUsedHighScoreName() == "AAA")
assert(profile:GetAllUsedHighScoreNames()[1] == "AAA")
assert(profile:GetCategoryHighScoreList("StepsType_Dance_Single", "RankingCategory_a"):GetHighScores()[1] ~= nil)
assert(profile:GetCharacter() == nil)
assert(profile:SetCharacter("default") == profile)
assert(profile:GetCharacter() == "default")
assert(profile:IsCodeUnlocked("code") == false)
assert(profile:GetSongsActual("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesActual("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetSongsPossible("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesPossible("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetSongsPercentComplete("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetCoursesPercentComplete("StepsType_Dance_Single", "Difficulty_Medium") == 0)
assert(profile:GetTotalStepsWithTopGrade("StepsType_Dance_Single", "Difficulty_Medium", "Grade_Tier07") == 0)
assert(profile:GetTotalTrailsWithTopGrade("StepsType_Dance_Single", "Difficulty_Medium", "Grade_Tier07") == 0)
assert(profile:GetTotalSessionSeconds() == 0)
assert(profile:GetTotalGameplaySeconds() == 0)
assert(profile:GetSongsAndCoursesPercentCompleteAllDifficulties("StepsType_Dance_Single") == 0)
assert(profile:GetMostPopularSong() == nil)
assert(profile:GetMostPopularCourse() == nil)
assert(profile:GetSongNumTimesPlayed(GAMESTATE:GetCurrentSong()) == 0)
assert(profile:HasPassedAnyStepsInSong(GAMESTATE:GetCurrentSong()) == false)
assert(profile:GetNumToasties() == 0)
assert(profile:GetTotalTapsAndHolds() == 0)
assert(profile:GetTotalJumps() == 0)
assert(profile:GetTotalHolds() == 0)
assert(profile:GetTotalRolls() == 0)
assert(profile:GetTotalMines() == 0)
assert(profile:GetTotalHands() == 0)
assert(profile:GetTotalLifts() == 0)
assert(profile:GetTotalDancePoints() == 0)
assert(profile:GetLastPlayedSong() == nil)
assert(profile:GetLastPlayedCourse() == nil)
assert(#profile:get_songs() == 0)
profile:GetUserTable().note = "x"
assert(profile:GetUserTable().note == "x")
assert(PROFILEMAN:GetLocalProfileFromIndex(0):GetDisplayName() == "Local Profile")

mod_actions = {
    {1, function()
        lua.WriteFile("favorites.txt", "Group/Song\n")
        lua.ReportScriptError("action report")
        Warn("action warning")
        local p = PROFILEMAN:GetProfile(PLAYER_1)
        p:SetLastUsedHighScoreName("BBB")
        p:AddCaloriesToDailyTotal(p:CalculateCaloriesFromHeartRate(90, 30))
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Lua File Profile Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_accepts_stage_stat_and_high_score_helpers() {
    let song_dir = test_dir("stage-stat-high-score-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local accum = STATSMAN:GetAccumPlayedStageStats()
local final = STATSMAN:GetFinalEvalStageStats()
assert(STATSMAN:GetPlayedStageStats(2) == nil)
assert(STATSMAN:GetStagesPlayed() == 1)
assert(STATSMAN:GetFinalGrade(PLAYER_1) == "Grade_Tier07")
assert(STATSMAN:GetBestGrade() == "Grade_Tier07")
assert(STATSMAN:GetWorstGrade() == "Grade_Tier07")
assert(STATSMAN:GetBestFinalGrade() == "Grade_Tier07")
assert(accum:GetPlayerStageStats(PLAYER_1) ~= nil)
assert(final:GetPlayerStageStats(PLAYER_1) ~= nil)
assert(STATSMAN:GetCurStageStats():GetMultiPlayerStageStats(0) ~= nil)
assert(#STATSMAN:GetCurStageStats():GetPlayedSongs() == 1)
assert(#STATSMAN:GetCurStageStats():GetPossibleSongs() == 1)
assert(STATSMAN:GetCurStageStats():GetGameplaySeconds() >= 0)
assert(STATSMAN:GetCurStageStats():GetStepsSeconds() >= 0)
assert(STATSMAN:GetCurStageStats():GetStage() == "Stage_1st")
assert(STATSMAN:GetCurStageStats():GetStageIndex() == 0)
assert(STATSMAN:GetCurStageStats():OnePassed() == true)
assert(STATSMAN:GetCurStageStats():PlayerHasHighScore(PLAYER_1) == false)
assert(STATSMAN:GetCurStageStats():GetEarnedExtraStage() == false)
local steps = pss:GetPlayedSteps()[1]
assert(steps:GetMeter() >= 0)
assert(steps:GetDifficulty() ~= nil)
assert(#pss:GetPossibleSteps() == 1)
assert(#pss:GetComboList() == 0)
assert(pss:GetMachineHighScoreIndex() == -1)
assert(pss:GetPersonalHighScoreIndex() == -1)
assert(pss:IsDisqualified() == false)
assert(pss:FullComboOfScore(0) == false)
assert(pss:FullCombo() == false)
assert(pss:MaxCombo() == 0)
assert(pss:GetCurrentPossibleDancePoints() == 0)
assert(pss:GetCurrentCombo() == 0)
assert(pss:GetCurrentMissCombo() == 0)
assert(pss:GetCurrentScoreMultiplier() == 1)
assert(pss:GetCurMaxScore() == 0)
assert(pss:GetCaloriesBurned() == 0)
assert(pss:GetNumControllerSteps() == 0)
assert(pss:GetSurvivalSeconds() == 0)
assert(pss:GetAliveSeconds() == 0)
assert(pss:GetLessonScoreActual() == 0)
assert(pss:GetLessonScoreNeeded() == 0)
assert(pss:GetStageAward() == "StageAward_None")
assert(pss:GetPeakComboAward() == "PeakComboAward_None")
assert(pss:GetPercentageOfTaps("TapNoteScore_W1") == 0)
assert(pss:GetBestFullComboTapNoteScore() == "TapNoteScore_None")
assert(pss:GetSongsPassed() == 0)
assert(pss:GetSongsPlayed() == 0)

local highscore = pss:GetHighScore()
assert(highscore:GetHoldNoteScore("HoldNoteScore_Held") == 0)
assert(highscore:GetMaxCombo() == 0)
assert(highscore:GetSurvivalSeconds() == 0)
assert(highscore:GetStageAward() == "StageAward_None")
assert(highscore:GetPeakComboAward() == "PeakComboAward_None")
assert(highscore:IsFillInMarker() == false)
assert(highscore:GetRadarValues():GetValue("RadarCategory_TapsAndHolds") == 0)
local machine_list = PROFILEMAN:GetMachineProfile():GetHighScoreList(GAMESTATE:GetCurrentSong(), steps)
assert(machine_list:GetRankOfName("Machine") == 1)
assert(machine_list:GetRankOfName("Missing") == 0)
assert(machine_list:GetHighestScoreOfName("Machine"):GetName() == "Machine")
assert(machine_list:GetHighestScoreOfName("Missing") == nil)
assert(STATSMAN:GetCurStageStats():GaveUp() == false)
STATSMAN:Reset()

mod_actions = {
    {1, function()
        local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
        stats:SetScore(12)
        stats:SetCurMaxScore(24)
        stats:SetDancePointLimits(8, 10)
        stats:FailPlayer()
        assert(stats:GetFailed() == true)
        assert(stats:GetScore() == 12)
        assert(stats:GetCurMaxScore() == 24)
        assert(stats:GetActualDancePoints() == 8)
        assert(stats:GetPossibleDancePoints() == 10)
        assert(math.abs(stats:GetPercentDancePoints() - 0.8) < 0.000001)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Stage Stat High Score Helpers"),
    )
    .unwrap();
    assert_eq!(
        compiled.info.unsupported_function_actions, 0,
        "{:?}",
        compiled.info
    );
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_accepts_theme_pref_and_gamestate_control_helpers() {
    let song_dir = test_dir("theme-pref-gamestate-control-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(PREFSMAN:PreferenceExists("EventMode"))
assert(PREFSMAN:GetPreference("EventMode") == true)
assert(PREFSMAN:GetPreference("MaxHighScoresPerListForMachine") == 10)
assert(PREFSMAN:GetPreference("LongVerSongSeconds") == 150)
assert(PREFSMAN:GetPreference("EditorNoteSkinP1") == "default")
assert(PREFSMAN:PreferenceExists("MissingPreference") == false)
assert(PREFSMAN:GetPreference("MissingPreference") == nil)
assert(PREFSMAN:SetPreference("EventMode", false) == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == false)
assert(PREFSMAN:SetPreferenceToDefault("EventMode") == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == true)
assert(PREFSMAN:SetPreference("ShowNativeLanguage", true) == PREFSMAN)
assert(PREFSMAN:GetPreference("ShowNativeLanguage") == true)
assert(PREFSMAN:SetPreference("MissingPreference", "ignored") == PREFSMAN)
assert(PREFSMAN:GetPreference("MissingPreference") == nil)
assert(ThemePrefs.Get("EditModeLastSeenSong") == "")

assert(THEME:GetCurLanguage() == "en")
assert(THEME:GetSelectableThemeNames()[1] == "Simply Love")
assert(GAMESTATE:GetPlayMode() == "PlayMode_Regular")
assert(GAMESTATE:GetCurrentStageIndex() == 0)
assert(GAMESTATE:GetCourseSongIndex() == 0)
assert(GAMESTATE:GetPlayerFailType(PLAYER_1) == "FailType_Immediate")

local style = GAMESTATE:GetCurrentStyle()
assert(style:ColumnsPerPlayer() == 4)
assert(style:GetStepsType() == "StepsType_Dance_Single")
assert(style:GetStyleType() == "StyleType_OnePlayerOneSide")
assert(style:GetWidth() == 256)
assert(style:GetColumnInfo(PLAYER_1, 1).Name == "Left")
assert(StepsType:Reverse()["StepsType_Dance_Single"] ~= nil)
assert(StyleType:Reverse()["StyleType_OnePlayerOneSide"] ~= nil)
assert(GAMEMAN:GetStylesForGame(GAMESTATE:GetCurrentGame():GetName())[1]:GetName() == "single")

local song = GAMESTATE:GetCurrentSong()
GAMESTATE:SetCurrentSong(song)
assert(GAMESTATE:GetCurrentSong() == song)
GAMESTATE:SetCurrentStyle("single")
assert(GAMESTATE:GetCurrentStyle():GetName() == "single")

mod_actions = {
    {1, function()
        assert(PREFSMAN:SetPreferenceToDefault("EventMode") == PREFSMAN)
        assert(PREFSMAN:SavePreferences() == PREFSMAN)
        THEME:ReloadMetrics()
        THEME:SetTheme("Simply Love")
        GAMESTATE:AddStageToPlayer(PLAYER_1)
        GAMESTATE:ResetPlayerOptions(PLAYER_1)
        GAMESTATE:SetPreferredDifficulty(PLAYER_1, "Difficulty_Hard")
        GAMESTATE:UnjoinPlayer(PLAYER_2)
        GAMESTATE:SetCurrentTrail(PLAYER_1, nil)
        GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSteps(PLAYER_1))
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Pref GameState Control Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_theme_support_bpm_profile_helpers() {
    let song_dir = test_dir("theme-support-bpm-profile-helpers");
    fs::create_dir_all(song_dir.join("audio")).unwrap();
    fs::write(song_dir.join("audio/pass.ogg"), "").unwrap();
    fs::write(song_dir.join("audio/skip.wav"), "").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local parts = GetVersionParts("1.2.0-git")
assert(parts[1] == 1 and parts[2] == 2 and parts[3] == 0)
local product = GetProductVersion()
assert(product[1] == 1 and product[2] == 2 and product[3] == 0)
assert(IsProductVersion(1, 2))
assert(IsMinimumProductVersion(1, 2, 0))
assert(IsITGmania())
assert(StepManiaVersionIsSupported())
assert(MinimumVersionString() == "1.2.0")
assert(CurrentGameIsSupported())
assert(GetThemeVersion() == ProductVersion())
assert(GetAuthor() ~= "")
assert(SupportsRenderToTexture())

local bpms = GetDisplayBPMs(PLAYER_1)
assert(bpms[1] == 120 and bpms[2] == 180)
assert(StringifyDisplayBPMs(PLAYER_1) == "120 - 180")

local song, steps = GetSongAndSteps(PLAYER_1)
assert(song == GAMESTATE:GetCurrentSong())
assert(steps == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(#getAuthorTable(steps) == 0)
assert(totalLengthSongOrCourse(PLAYER_1) == 123)
assert(currentTimeSongOrCourse(PLAYER_1) == 0)
assert(SecondsToHMMSS(3661) == "01:01:01")
assert(GetPlayerAvatarPath(PLAYER_1) == nil)
assert(GetAvatarPath("", "") == nil)

local files = findFiles("audio", "ogg")
assert(#files == 1 and files[1]:match("pass%.ogg$"))
assert(cleanGSub("a.b", ".", "-") == "a-b")
assert(force_to_range(1, 10, 5) == 5)
assert(wrapped_index(3, 2, 4) == 1)
assert(table.concat(table.rotate_left({1,2,3}, 1), ",") == "2,3,1")
assert(table.concat(table.rotate_right({1,2,3}, 1), ",") == "3,1,2")
assert(TableToString({1, 2}, "Demo"):match("^Demo = "))

mod_actions = {
    {1, function()
        LoadGuest(PLAYER_1)
        LoadProfileCustom(PROFILEMAN:GetProfile(PLAYER_1), PROFILEMAN:GetProfileDir(PLAYER_1))
        SaveProfileCustom(PROFILEMAN:GetProfile(PLAYER_1), PROFILEMAN:GetProfileDir(PLAYER_1))
        local parsed = ParseChartInfo(steps, "P1")
        assert(parsed.PeakNPS == 0)
        assert(#parsed.NotesPerMeasure == 0)
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Support BPM Profile Helpers");
    context.song_display_bpms = [120.0, 180.0];
    context.music_length_seconds = 123.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_gameman_style_list() {
    let song_dir = test_dir("gameman-style-list");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local explicit = GAMEMAN:GetStylesForGame("dance")
local current = GAMEMAN:GetStylesForGame(GAMESTATE:GetCurrentGame():GetName())
assert(#explicit == 1 and #current == 1)
assert(explicit[1]:GetName() == "single")
assert(current[1]:GetName() == "single")
assert(explicit[1]:GetStepsType() == "StepsType_Dance_Single")
assert(explicit[1]:GetStyleType() == "StyleType_OnePlayerOneSide")
assert(explicit[1]:ColumnsPerPlayer() == 4)
assert(explicit[1]:GetWidth() == 256)

local col = explicit[1]:GetColumnInfo(PLAYER_1, 4)
mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%d:%.0f:%s:%d:%.0f",
            GAMESTATE:GetCurrentGame():GetName(),
            explicit[1]:GetName(),
            explicit[1]:GetStepsType(),
            explicit[1]:ColumnsPerPlayer(),
            explicit[1]:GetWidth(),
            col.Name,
            col.Track,
            col.XOffset
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Gameman Style List"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "dance:single:StepsType_Dance_Single:4:256:Right:3:96"
    );
}

#[test]
fn compile_song_lua_exposes_theme_menu_manager_helpers() {
    let song_dir = test_dir("theme-menu-manager-helpers");
    fs::write(song_dir.join("logo.png"), "").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local top = SCREENMAN:GetTopScreen()
local wheel = top:GetMusicWheel()
assert(wheel:GetSelectedType() == "WheelItemDataType_Song")
assert(wheel:Move(1) == wheel)
assert(wheel:Move(-1) == wheel)
assert(MEMCARDMAN:GetCardState(PLAYER_1) == "MemoryCardState_none")
assert(MEMCARDMAN:GetName(PLAYER_1) == "")
assert(UNLOCKMAN:IsSongLocked(GAMESTATE:GetCurrentSong()) == 0)
assert(UNLOCKMAN:IsCourseLocked(GAMESTATE:GetCurrentCourse()) == 0)

local song = GAMESTATE:GetCurrentSong()
assert(song:GetFirstSecond() == 0)
assert(song:GetLastSecond() == 90)
assert(song:GetFirstBeat() == 0)
assert(song:GetLastBeat() == 180)
assert(song:GetOrTryAtLeastToGetSimfileAuthor() == "")

local resolved = ActorUtil.ResolvePath("logo.png", 1, true)
assert(ActorUtil.GetFileType(resolved) == "FileType_Bitmap")
assert(ActorUtil.GetFileType("sound.ogg") == "FileType_Sound")
assert(ActorUtil.GetFileType("clip.mp4") == "FileType_Movie")
assert(ActorUtil.GetFileType("notes.ssc") == "FileType_Text")
assert(ActorUtil.IsRegisteredClass("ActorFrame") == true)
assert(ActorUtil.IsRegisteredClass("MissingActorClass") == false)
local gamecommand = Var("GameCommand")
assert(gamecommand:GetIndex() == 0 and gamecommand:GetText() == "")
assert(gamecommand:GetName() == "" and gamecommand:GetScreen() == "")
assert(gamecommand:GetProfileID() == "")
assert(gamecommand:GetAnnouncer() == "")
assert(gamecommand:GetPreferredModifiers() == "")
assert(gamecommand:GetStageModifiers() == "")
assert(gamecommand:GetMultiPlayer() == -1)
assert(gamecommand:GetStyle():GetName() == GAMESTATE:GetCurrentStyle():GetName())
assert(gamecommand:GetSong() == GAMESTATE:GetCurrentSong())
assert(gamecommand:GetSteps() == GAMESTATE:GetCurrentSteps(PLAYER_1))
assert(gamecommand:GetCourse() == GAMESTATE:GetCurrentCourse())
assert(gamecommand:GetTrail() == GAMESTATE:GetCurrentTrail(PLAYER_1))
assert(gamecommand:GetCharacter() == nil)
assert(gamecommand:GetSongGroup() == song:GetGroupName())
assert(gamecommand:GetUrl() == nil)
assert(gamecommand:GetDifficulty() == "Difficulty_Invalid")
assert(gamecommand:GetCourseDifficulty() == "Difficulty_Invalid")
assert(gamecommand:GetPlayMode() == "PlayMode_Invalid")
assert(gamecommand:GetSortOrder() == "SortOrder_Invalid")
assert(Var("LoadingScreen") == "LoadingScreen")

local ps = GAMESTATE:GetPlayerState(PLAYER_1)
ps:SetPlayerOptions("ModsLevel_Preferred", "1x, Overhead, 50% Mini")
local options = ps:GetPlayerOptionsArray("ModsLevel_Preferred")
assert(#options == 4 and options[3] == "50% Mini" and options[4] == "Overhead")
assert(GetPlayerOptionsString(PLAYER_1) == "NoHideLights, 1x, 50% Mini, Overhead")

mod_actions = {
    {1, function()
        top:Continue()
        top:GetOptionRow(1):GetChoiceInRowWithFocus(PLAYER_1)
        local metric_actor = Def.Actor{Name="MetricActor"}
        assert(ActorUtil.LoadAllCommands(metric_actor, "ScreenSystemLayer") == nil)
        assert(ActorUtil.LoadAllCommandsFromName(metric_actor, "ScreenSystemLayer", "Actor") == nil)
        assert(ActorUtil.LoadAllCommandsAndSetXY(metric_actor, Var("LoadingScreen")) == nil)
        assert(MEMCARDMAN:MountCard(PLAYER_1) == false)
        assert(MEMCARDMAN:UnmountCard(PLAYER_1) == false)
        wheel:Move(0)
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Menu Manager Helpers");
    context.song_display_bpms = [120.0, 120.0];
    context.music_length_seconds = 90.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_empty_unlockman_shape() {
    let song_dir = test_dir("empty-unlockman-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local songs = UNLOCKMAN:GetSongsUnlockedByEntryID("missing")
local step_songs, step_difficulties = UNLOCKMAN:GetStepsUnlockedByEntryID("missing")
assert(UNLOCKMAN:GetNumUnlocks() == 0)
assert(UNLOCKMAN:GetNumUnlocked() == 0)
assert(UNLOCKMAN:GetPoints("UnlockRequirement_ArcadePoints") == 0)
assert(UNLOCKMAN:GetPointsForProfile(PROFILEMAN:GetMachineProfile(), "UnlockRequirement_SongPoints") == 0)
assert(UNLOCKMAN:GetPointsUntilNextUnlock("UnlockRequirement_DancePoints") == 0)
assert(UNLOCKMAN:AnyUnlocksToCelebrate() == false)
assert(UNLOCKMAN:GetUnlockEntryIndexToCelebrate() == -1)
assert(UNLOCKMAN:FindEntryID("missing") == nil)
assert(UNLOCKMAN:GetUnlockEntry(0) == nil)
assert(#songs == 0 and #step_songs == 0 and #step_difficulties == 0)
assert(UNLOCKMAN:IsSongLocked(GAMESTATE:GetCurrentSong()) == 0)
assert(UNLOCKMAN:IsCourseLocked(GAMESTATE:GetCurrentCourse()) == 0)
assert(UNLOCKMAN:IsStepsLocked(GAMESTATE:GetCurrentSong(), GAMESTATE:GetCurrentSteps(PLAYER_1)) == 0)

mod_actions = {
    {1, function()
        assert(UNLOCKMAN:PreferUnlockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:UnlockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:UnlockEntryIndex(0) == UNLOCKMAN)
        assert(UNLOCKMAN:LockEntryID("missing") == UNLOCKMAN)
        assert(UNLOCKMAN:LockEntryIndex(0) == UNLOCKMAN)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Empty Unlockman Shape"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_song_time_to_function_actions() {
    let song_dir = test_dir("function-action-song-time");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {4, function()
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local beat = GAMESTATE:GetSongBeat()
        local seconds = GAMESTATE:GetCurMusicSeconds()
        local pos = GAMESTATE:GetSongPosition():GetSongBeat()
        if p then
            p:x(beat)
            p:y(seconds * 100)
            p:rotationz(pos)
        end
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Function Action Song Time");
    context.song_display_bpms = [120.0, 120.0];
    context.song_music_rate = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
    let block = &compiled.player_actors[0].message_commands[0].blocks[0];
    assert_eq!(block.delta.x, Some(4.0));
    // Music time is unscaled by rate: beat 4 at 120 BPM is two music seconds.
    assert_eq!(block.delta.y, Some(200.0));
    assert_eq!(block.delta.rot_z_deg, Some(4.0));
}

#[test]
fn compile_song_lua_extracts_actorproxy_targets() {
    let song_dir = test_dir("overlay-proxy");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local proxy = nil

mod_actions = {
    {8, function()
        if proxy then
            proxy:visible(true)
        end
    end, true},
}

return Def.ActorFrame{
    Def.ActorProxy{
        Name="p1_proxy",
        OnCommand=function(self)
            proxy = self
            self:queuecommand("Bind")
        end,
        BindCommand=function(self)
            local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local nf = p and p:GetChild("NoteField") or nil
            if nf and nf:GetNumWrapperStates() == 0 then
                nf:AddWrapperState()
            end
            local wrapper = nf and nf:GetWrapperState(1) or nil
            if wrapper then
                self:SetTarget(wrapper)
            end
            self:visible(false)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Proxy"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::NoteField { player_index: 0 }
        }
    ));
    let overlay = &compiled.overlays[0];
    assert!(overlay.initial_state.visible);
    assert_eq!(overlay.message_commands.len(), 2);
    let startup = compiled
        .messages
        .iter()
        .find(|event| event.message == "__songlua_queued_startup")
        .expect("queued binding runs on the first positive update");
    assert!(startup.beat > 0.0 && startup.beat < 8.0);
    let hide = overlay
        .message_commands
        .iter()
        .find(|command| command.message == startup.message)
        .unwrap();
    let hidden = overlay_state_after_blocks(overlay.initial_state, &hide.blocks, 0.0);
    assert!(!hidden.visible);
    let show_event = compiled
        .messages
        .iter()
        .find(|event| event.beat == 8.0)
        .unwrap();
    let show = overlay
        .message_commands
        .iter()
        .find(|command| command.message == show_event.message)
        .unwrap();
    assert!(overlay_state_after_blocks(hidden, &show.blocks, 0.0).visible);
}

#[test]
fn compile_song_lua_resolves_queued_update_bound_player_proxy() {
    let song_dir = test_dir("queued-update-bound-player-proxy");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local player = nil
local bound = false

return Def.ActorFrame{
    Def.Actor{
        OnCommand=function(self)
            self:queuecommand("Update")
        end,
        UpdateCommand=function(self)
            if not bound and GAMESTATE:GetSongBeat() >= 0.2 then
                assert(PlayerNumberToString(0) == "P1")
                player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                player:visible(false)
                MESSAGEMAN:Broadcast("BindPlayerProxy")
                bound = true
            end
            self:sleep(0.02):queuecommand("Update")
        end,
    },
    Def.ActorProxy{
        Name="LatePlayerProxy",
        BindPlayerProxyMessageCommand=function(self)
            if player then
                self:SetTarget(player)
            end
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Update Bound Player Proxy");
    context.music_length_seconds = 2.0;
    context.song_display_bpms = [120.0, 120.0];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let proxy = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("LatePlayerProxy"))
        .unwrap();
    assert!(matches!(
        proxy.kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Player { player_index: 0 }
        }
    ));
    assert!(compiled.hidden_players[0]);
}

#[test]
fn compile_begin_proxies() {
    let song_dir = test_dir("begin-player-proxy");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        include_str!("../../../../tests/fixtures/song-lua/begin-player-proxy.lua"),
    )
    .expect("write root Begin player binding");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Begin Player Proxy"),
    )
    .expect("compile root and proxy Begin commands");
    for player_index in 0..2 {
        let name = format!("ProxyP{}", player_index + 1);
        let proxy = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(&name))
            .expect("retain player proxy");
        assert!(
            matches!(proxy.kind, SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Player { player_index: target },
            } if target == player_index),
            "{name}: {:?}",
            proxy.kind
        );
    }
}

#[test]
fn compile_song_lua_extracts_player_judgment_and_combo_proxy_targets() {
    let song_dir = test_dir("player-judgment-combo-proxy");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorProxy{
        Name="judgment_proxy",
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local target = player:GetChild("Judgment")
            self:SetTarget(target)
            target:visible(false)
        end,
    },
    Def.ActorProxy{
        Name="combo_proxy",
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local target = player:GetChild("Combo")
            if target:GetNumWrapperStates() == 0 then
                target:AddWrapperState()
            end
            target:GetWrapperState(1):addy(9999)
            self:SetTarget(target)
            target:visible(false)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Judgment Combo Proxy"),
    )
    .unwrap();
    assert!(compiled.overlays.iter().any(|overlay| {
        matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Judgment { player_index: 0 }
            }
        )
    }));
    assert!(compiled.overlays.iter().any(|overlay| {
        matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Combo { player_index: 0 }
            }
        )
    }));
}

#[test]
fn compile_song_lua_runs_cmd_queuecommand_builders() {
    let song_dir = test_dir("overlay-proxy-cmd");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorProxy{
        Name="p1_proxy",
        OnCommand=cmd(queuecommand, "Bind"),
        BindCommand=function(self)
            local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            if p then
                self:SetTarget(p)
            end
            self:visible(false)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Proxy Cmd"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Player { player_index: 0 }
        }
    ));
    let overlay = &compiled.overlays[0];
    assert!(overlay.initial_state.visible);
    let event = compiled
        .messages
        .iter()
        .find(|event| event.message == "__songlua_queued_startup")
        .expect("cmd queues binding for the first positive update");
    assert!(event.beat > 0.0);
    let command = overlay
        .message_commands
        .iter()
        .find(|command| command.message == event.message)
        .unwrap();
    assert!(!overlay_state_after_blocks(overlay.initial_state, &command.blocks, 0.0).visible);
}

#[test]
fn compile_song_lua_runs_legacy_cmd_keyword() {
    let song_dir = test_dir("legacy-cmd-keyword");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="LegacyCmd",
        OnCommand=cmd(x,SCREEN_CENTER_X;y,SCREEN_CENTER_Y;diffusealpha,0;scaletocover,0,0,SCREEN_WIDTH,SCREEN_HEIGHT),
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Legacy Cmd Keyword"),
    )
    .unwrap();
    let actor = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("LegacyCmd"))
        .unwrap();
    assert_eq!(actor.initial_state.x, 320.0);
    assert_eq!(actor.initial_state.y, 240.0);
    assert_eq!(actor.initial_state.diffuse[3], 0.0);
}

#[test]
fn compile_song_lua_extracts_actorframetexture_capture_sprite_and_hidden_player() {
    let song_dir = test_dir("overlay-aft");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local capture = nil

return Def.ActorFrame{
    OnCommand=function(self)
        local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if p then
            p:visible(false)
        end
    end,
    Def.ActorFrameTexture{
        Name="CaptureAFT",
        InitCommand=function(self)
            capture = self
            self:EnableAlphaBuffer(true)
            self:EnableDepthBuffer(true)
            self:EnablePreserveTexture(true)
            self:SetSize(640, 480):Create()
        end,
        Def.ActorProxy{
            Name="ProxyP1",
            OnCommand=function(self)
                local p = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                if p then
                    local nf = p:GetChild("NoteField")
                    if nf and nf:GetNumWrapperStates() == 0 then
                        nf:AddWrapperState()
                    end
                    self:SetTarget(nf and nf:GetWrapperState(1) or nf)
                end
                self:visible(true)
            end,
        },
    },
    Def.Sprite{
        Name="AFTSpriteR",
        OnCommand=function(self)
            if capture then
                self:SetTexture(capture:GetTexture())
            end
            self:diffuse(1, 0, 0, 1)
            self:blend("add")
            self:vibrate()
            self:effectmagnitude(8, 4, 0)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay AFT"),
    )
    .unwrap();
    assert!(compiled.hidden_players[0]);
    assert_eq!(compiled.overlays.len(), 3);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture {
            alpha_buffer: true,
            float_buffer: false,
            depth_buffer: true,
            ..
        }
    ));
    assert!(compiled.overlays[0].initial_state.aft_preserve);
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::NoteField { player_index: 0 }
        }
    ));
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::AftSprite { ref capture_name }
            if capture_name == "ActorFrameTexture 1"
    ));
    assert_eq!(
        compiled.overlays[2].initial_state.blend,
        SongLuaOverlayBlendMode::Add
    );
    assert!(compiled.overlays[2].initial_state.vibrate);
    assert_eq!(
        compiled.overlays[2].initial_state.effect_magnitude,
        [8.0, 4.0, 0.0]
    );
}

#[test]
fn compile_song_lua_freezes_aft_creation_and_captures_late_creation() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song-lua")
        .canonicalize()
        .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "AFT creation");
    context.music_length_seconds = 2.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = test_compile_song_lua(&song_dir.join("aft-create.lua"), &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    let index = |name: &str| {
        compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap()
    };
    let frozen = &compiled.overlays[index("Frozen")];
    assert!(matches!(&frozen.kind, TestOverlayKind::ActorFrameTexture {
            capture_name, capture_size: Some([65.0, 33.0]),
            alpha_buffer: true, depth_buffer: true, float_buffer: true,
        } if capture_name == "FrozenCapture"));
    assert_eq!(frozen.initial_state.size, Some([17.0, 9.0]));
    assert!(frozen.initial_state.aft_created);
    assert!(matches!(
        compiled.overlays[index("Unused")].kind,
        TestOverlayKind::ActorFrameTexture {
            capture_size: None,
            ..
        }
    ));
    assert!(!compiled.overlays[index("Unused")].initial_state.aft_created);
    let late = index("Late");
    assert!(matches!(
        compiled.overlays[late].kind,
        TestOverlayKind::ActorFrameTexture {
            capture_size: Some([23.0, 11.0]),
            ..
        }
    ));
    assert!(!compiled.overlays[late].initial_state.aft_created);
    assert!(matches!(
        compiled.overlays[index("LateOutput")].kind,
        TestOverlayKind::AftSprite { .. }
    ));
    assert!(
        !compiled.overlays[index("LateOutput")]
            .initial_state
            .sprite_texture
    );
    for (overlay, target) in [
        (late, SongLuaOverlayUpdateTarget::AftCreated),
        (
            index("LateOutput"),
            SongLuaOverlayUpdateTarget::SpriteTexture,
        ),
        (index("Frozen"), SongLuaOverlayUpdateTarget::AftPreserve),
    ] {
        let track = compiled
            .overlay_updates
            .iter()
            .find(|track| track.overlay_index == overlay && track.target == target)
            .unwrap_or_else(|| panic!("missing {target:?} track"));
        assert!(
            track.samples.iter().any(|sample| sample.time >= 1.0
                && sample.value == SongLuaOverlayUpdateValue::Bool(true))
        );
        assert!(
            track
                .samples
                .iter()
                .filter(|sample| sample.time < 1.0)
                .all(|sample| sample.value == SongLuaOverlayUpdateValue::Bool(false))
        );
    }
}

#[test]
fn custom_draws_skip_uncreated_afts_and_wrap_created_callbacks() {
    let song_dir = test_dir("aft-draw-lifetime");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local unused, late = self:GetChild("Unused"), self:GetChild("Late")
        local child = unused:GetChild("DirectChild")
        self:SetDrawFunction(function()
            unused:Draw()
            late:Draw()
            -- Explicit draws use the caller's stack even for an uncreated parent.
            child:Draw()
        end)
        self:SetUpdateFunction(function()
            if GAMESTATE:GetSongBeat() >= 1 and late:GetTexture() == nil then
                late:SetSize(32,16):Create():SetSize(99,88)
            end
        end)
    end,
    Def.ActorFrameTexture{
        Name="Unused",
        InitCommand=function(self)
            self:SetDrawFunction(function() error("uncreated callback must not run") end)
        end,
        Def.Quad{ Name="DirectChild", InitCommand=function(self) self:zoomto(4,4) end },
    },
    Def.ActorFrameTexture{
        Name="Late",
        OnCommand=function(self)
            local child = self:GetChild("CapturedChild")
            self:SetDrawFunction(function() child:Draw() end)
        end,
        Def.Quad{ Name="CapturedChild", InitCommand=function(self) self:zoomto(4,4) end },
    },
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "AFT draw lifetime");
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let index = |name: &str| {
        compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap()
    };
    let late = index("Late");
    assert!(matches!(
        compiled.overlays[late].kind,
        TestOverlayKind::ActorFrameTexture {
            capture_size: Some([32.0, 16.0]),
            ..
        }
    ));
    assert!(!compiled.draw_frames.is_empty());
    for frame in compiled.draw_frames.iter() {
        let captures: Vec<_> = frame
            .ops
            .iter()
            .filter_map(|op| {
                if let SongLuaDrawOp::Begin { capture, .. } = op {
                    Some(*capture)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            captures,
            if frame.second < 1.0 {
                vec![]
            } else {
                vec![late]
            }
        );
        assert!(matches!(frame.ops.last(), Some(SongLuaDrawOp::Draw {
                source: SongLuaDrawSource::Overlay(child), ..
            }) if *child == index("DirectChild")));
        if frame.second >= 1.0 {
            assert!(matches!(frame.ops[1], SongLuaDrawOp::Draw {
                    source: SongLuaDrawSource::Overlay(child), ..
                } if child == index("CapturedChild")));
            assert!(matches!(frame.ops[2], SongLuaDrawOp::Finish {capture} if capture == late));
        }
    }
}

#[test]
fn compile_song_lua_keeps_aft_actor_and_texture_names() {
    let song_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song-lua")
        .canonicalize()
        .unwrap();
    let compiled = test_compile_song_lua(
        &song_dir.join("aft-identity.lua"),
        &SongLuaCompileContext::new(&song_dir, "AFT identity"),
    )
    .unwrap();
    let captures = compiled
        .overlays
        .iter()
        .filter_map(|overlay| {
            let SongLuaOverlayKind::ActorFrameTexture { capture_name, .. } = &overlay.kind else {
                return None;
            };
            Some((overlay.name.as_deref(), capture_name.as_str()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        captures,
        [
            (Some("ActorLabel"), "TextureLabel"),
            (None, "ActorFrameTexture 2")
        ]
    );
    for (name, expected) in [("NamedLength", 10.0), ("AnonymousLength", 0.0)] {
        let overlay = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(overlay.initial_state.x, expected);
    }
    for (name, expected) in [
        ("NamedTexture", "TextureLabel"),
        ("AnonymousTexture", "ActorFrameTexture 2"),
    ] {
        let overlay = compiled
            .overlays
            .iter()
            .find(|overlay| overlay.name.as_deref() == Some(name))
            .unwrap();
        assert!(
            matches!(&overlay.kind, SongLuaOverlayKind::AftSprite { capture_name } if capture_name == expected)
        );
    }
}

#[test]
fn compile_song_lua_supports_anonymous_actorframetexture_gettexture() {
    let song_dir = test_dir("overlay-anonymous-aft");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local capture = nil

return Def.ActorFrame{
    Def.ActorFrameTexture{
        InitCommand=function(self)
            capture = self
            self:SetSize(640, 480):Create()
        end,
        Def.ActorProxy{
            ProxyStartMessageCommand=function(self)
                local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                self:SetTarget(player)
                player:visible(false)
            end,
        },
    },
    Def.Sprite{
        ProxyStartMessageCommand=function(self)
            self:SetTexture(capture:GetTexture())
        end,
    },
    Def.Actor{
        OnCommand=function(self)
            self:queuecommand("Bind")
        end,
        BindCommand=function(self)
            MESSAGEMAN:Broadcast("ProxyStart")
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Anonymous AFT"),
    )
    .unwrap();
    assert!(compiled.hidden_players[0]);
    assert_eq!(compiled.overlays.len(), 3);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture { .. }
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Player { player_index: 0 }
        }
    ));
    let SongLuaOverlayKind::AftSprite { capture_name } = &compiled.overlays[2].kind else {
        panic!("expected anonymous AFT output sprite");
    };
    assert!(capture_name.starts_with("ActorFrameTexture "));
    assert_eq!(compiled.overlays[0].name, None);
    assert!(matches!(
        &compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture { capture_name: target, .. }
            if target == capture_name
    ));
}

#[test]
fn compile_song_lua_resolves_local_and_hidden_screen_proxy_targets() {
    let song_dir = test_dir("overlay-local-screen-proxies");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local local_target = nil
local capture = nil

return Def.ActorFrame{
    Def.ActorFrame{
        Name="LocalTarget",
        InitCommand=function(self) local_target = self end,
        Def.Quad{
            Name="LocalMarker",
            OnCommand=function(self) self:zoomto(640, 480) end,
        },
    },
    Def.ActorFrameTexture{
        InitCommand=function(self)
            capture = self
            self:SetSize(640, 480):Create()
        end,
        Def.ActorProxy{
            Name="LocalProxy",
            ProxyStartMessageCommand=function(self)
                local_target:visible(false)
                self:SetTarget(local_target)
            end,
        },
        Def.ActorProxy{
            Name="UnderlayProxy",
            ProxyStartMessageCommand=function(self)
                local target = SCREENMAN:GetTopScreen():GetChild("Underlay")
                target:visible(false)
                self:SetTarget(target)
            end,
        },
        Def.ActorProxy{
            Name="OverlayProxy",
            ProxyStartMessageCommand=function(self)
                local target = SCREENMAN:GetTopScreen():GetChild("Overlay")
                target:visible(false)
                self:SetTarget(target)
            end,
        },
    },
    Def.Sprite{
        ProxyStartMessageCommand=function(self)
            self:SetTexture(capture:GetTexture())
        end,
    },
    Def.Actor{
        OnCommand=function(self) self:queuecommand("Bind") end,
        BindCommand=function(self) MESSAGEMAN:Broadcast("ProxyStart") end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Local Screen Proxies"),
    )
    .unwrap();
    let local_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("LocalTarget"))
        .unwrap();
    let local = &compiled.overlays[local_index];
    assert!(local.initial_state.visible);
    let event = compiled
        .messages
        .iter()
        .find(|event| event.message == "__songlua_queued_startup")
        .expect("queued broadcast hides the local target on update");
    assert!(event.beat > 0.0);
    let command = local
        .message_commands
        .iter()
        .find(|command| command.message == event.message)
        .unwrap();
    assert!(!overlay_state_after_blocks(local.initial_state, &command.blocks, 0.0).visible);
    assert!(compiled.overlays.iter().any(|overlay| {
        matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Actor { overlay_index }
            } if overlay_index == local_index
        )
    }));
    assert!(compiled.overlays.iter().any(|overlay| {
        matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Underlay { hidden: true }
            }
        )
    }));
    assert!(compiled.overlays.iter().any(|overlay| {
        matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Overlay { hidden: true }
            }
        )
    }));
}

#[test]
fn compile_song_lua_supports_named_actorframetexture_sprites() {
    let song_dir = test_dir("overlay-aft-texture-name");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="CaptureAFT",
        InitCommand=function(self)
            self:SetTextureName("ScreenTex")
            self:SetWidth(640)
            self:SetHeight(480)
            self:EnableAlphaBuffer(false)
            self:Create()
        end,
    },
    Def.Sprite{
        Texture="ScreenTex",
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Named AFT Sprite"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture { .. }
    ));
    assert_eq!(
        compiled.overlays[0].initial_state.size,
        Some([640.0, 480.0])
    );
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::AftSprite { ref capture_name }
            if capture_name == "ScreenTex"
    ));
}

#[test]
fn compile_song_lua_accepts_actorframetexture_draw_call() {
    let song_dir = test_dir("overlay-aft-draw");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="ScreenTex",
        InitCommand=function(self)
            self:Create()
        end,
        OnCommand=function(self)
            self:visible(true)
            self:Draw()
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "AFT Draw")).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture {
            capture_size: Some([1.0, 1.0]),
            ..
        }
    ));
    assert!(compiled.overlays[0].initial_state.aft_created);
    assert!(compiled.overlays[0].initial_state.visible);
}

#[test]
fn compile_song_lua_preserves_player_state_at_manual_draw() {
    let song_dir = test_dir("manual-player-draw");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local judgment = player:GetChild("Judgment")
        player:x(111):visible(false)
        judgment:visible(false)
        self:SetDrawFunction(function()
            judgment:visible(true):Draw():visible(false)
            player:x(222):visible(true)
            player:Draw()
            player:x(333):visible(false)
        end)
    end,
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Manual Player Draw"),
    )
    .unwrap();
    assert!(compiled.hidden_players[0]);
    assert!(!compiled.player_actors[0].initial_state.visible);
    assert_eq!(compiled.player_actors[0].initial_state.x, 333.0);
    let draws: Vec<_> = compiled
        .draw_frames
        .iter()
        .flat_map(|frame| &frame.ops)
        .filter_map(|op| match op {
            SongLuaDrawOp::Draw {
                source: SongLuaDrawSource::Player(0),
                state,
                ..
            } => Some(state),
            _ => None,
        })
        .collect();
    assert!(
        !draws.is_empty(),
        "every explicit draw retains its own pose"
    );
    for state in draws {
        assert!(state.visible);
        assert_eq!(state.x, 222.0);
    }
    for frame in &compiled.draw_frames {
        let sources = frame
            .ops
            .iter()
            .filter_map(|op| match op {
                SongLuaDrawOp::Draw { source, .. } => Some(*source),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sources,
            [SongLuaDrawSource::Judgment(0), SongLuaDrawSource::Player(0)],
            "the theme judgment frame remains a distinct HUD draw source"
        );
    }
    assert!(compiled.player_actors[0].manual_hud_draw);
}

#[test]
fn compile_song_lua_extracts_overlay_function_actions_and_eases() {
    let song_dir = test_dir("overlay-functions");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil

mod_actions = {
    {8, function()
        if target then
            target:visible(true)
            target:diffusealpha(1)
        end
    end, true},
}

mods_ease = {
    {4, 2, 0, 320, function(a)
        if target then
            target:x(a)
            target:zoomx(1 + (a / 320))
            target:cropbottom(a / 640)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:visible(false)
            self:diffusealpha(0)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Functions"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert!(
        compiled.messages[0]
            .message
            .starts_with("__songlua_overlay_fn_action_")
    );
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands[0].blocks.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .visible,
        Some(true)
    );
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .diffuse
            .unwrap()[3],
        1.0
    );
    assert_eq!(compiled.overlay_eases.len(), 1);
    let ease = &compiled.overlay_eases[0];
    assert_eq!(ease.overlay_index, 0);
    assert_eq!(ease.easing.as_deref(), Some("outQuad"));
    assert_eq!(ease.from.x, Some(0.0));
    assert_eq!(ease.to.x, Some(320.0));
    assert_eq!(ease.from.zoom_x, Some(1.0));
    assert_eq!(ease.to.zoom_x, Some(2.0));
    assert_eq!(ease.from.cropbottom, Some(0.0));
    assert_eq!(ease.to.cropbottom, Some(0.5));
}

#[test]
fn tween_aux_remains_current_for_update_readers() {
    let song_dir = test_dir("tween-aux-reader");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local driver, target
mod_actions = {{1, function() driver:linear(1):aux(1) end, true}}
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function() target:x(driver:getaux()) end)
    end,
    Def.Actor{
        Name="Driver",
        InitCommand=function(self) driver = self; self:aux(0) end,
    },
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self end,
    },
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Tween Aux Reader");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(
        compiled
            .info
            .unsupported_function_action_captures
            .is_empty()
    );
    let target = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Target"))
        .unwrap();
    let track = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target && track.target == SongLuaOverlayUpdateTarget::X
        })
        .expect("aux getter reaches the update reader");
    for (time, expected) in [(0.5, 0.0), (1.0, 0.0), (1.5, 0.5), (2.5, 1.0)] {
        let sample = track
            .samples
            .iter()
            .min_by(|a, b| (a.time - time).abs().total_cmp(&(b.time - time).abs()))
            .unwrap();
        let SongLuaOverlayUpdateValue::F32(actual) = sample.value else {
            panic!("float x")
        };
        // Native queued work advances on the next 60 Hz update frame.
        assert!(
            (actual - expected).abs() <= 1.0 / 60.0 + 0.0001,
            "time {time}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn compile_song_lua_replays_function_action_aux_before_updates() {
    let song_dir = test_dir("overlay-function-aux");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local stone

mod_actions = {
    {1, function()
        stone:aux(3)
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Stone",
        OnCommand=function(self)
            stone = self
            self:visible(false)
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                if stone:getaux() > 0 then
                    stone:visible(true)
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Overlay Function Aux");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let (stone_index, stone) = compiled
        .overlays
        .iter()
        .enumerate()
        .find(|(_, overlay)| overlay.name.as_deref() == Some("Stone"))
        .unwrap();
    assert!(
        stone
            .message_commands
            .iter()
            .flat_map(|command| &command.blocks)
            .any(|block| block.delta.aux == Some(3.0))
    );
    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == stone_index
            && track.target == SongLuaOverlayUpdateTarget::Visible
            && track.samples.iter().any(|sample| {
                sample.time >= 1.0 && sample.value == SongLuaOverlayUpdateValue::Bool(true)
            })
    }));
}

#[test]
fn compile_song_lua_does_not_apply_future_action_globals_early() {
    let song_dir = test_dir("future-action-global");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
future_speed = 0

mod_actions = {
    {100, function()
        future_speed = 1
    end, true},
}

return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self)
            target = self
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                if beat > 2 and future_speed < 1 then
                    future_speed = future_speed + 0.1
                end
                target:x(future_speed * 10)
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Future Action Global");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 6.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == SongLuaOverlayUpdateTarget::X)
        .expect("the post-gate motion should produce an X track");

    assert!(
        x.samples
            .iter()
            .filter(|sample| sample.time <= 2.0)
            .all(|sample| sample.value == SongLuaOverlayUpdateValue::F32(0.0))
    );
    assert!(x.samples.iter().any(|sample| {
        sample.time > 2.0
            && matches!(sample.value, SongLuaOverlayUpdateValue::F32(value) if value > 0.0)
    }));
}

#[test]
fn compile_song_lua_scopes_unprobed_overlay_function_eases() {
    let song_dir = test_dir("unprobed-overlay-function-ease");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil

mods_ease = {
    {4, 2, 0, 2, function(amount)
        if amount ~= 1 and target then
            target:x(10 + amount * 5)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Unprobed Overlay Function Ease"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.overlay_eases.len(), 1);
    assert_eq!(compiled.overlay_eases[0].from.x, Some(10.0));
    assert_eq!(compiled.overlay_eases[0].to.x, Some(20.0));
}

#[test]
fn compile_song_lua_keeps_overlay_rotation_eases_out_of_player_transforms() {
    let song_dir = test_dir("overlay-rotation-ease");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil

mods_ease = {
    {4, 2, 0, 45, function(a)
        if target then
            target:rotationz(a)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Rotation Ease"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert!(compiled.eases.is_empty());
    assert_eq!(compiled.overlay_eases.len(), 1);
    assert_eq!(compiled.overlay_eases[0].overlay_index, 0);
    assert_eq!(compiled.overlay_eases[0].from.rot_z_deg, Some(0.0));
    assert_eq!(compiled.overlay_eases[0].to.rot_z_deg, Some(45.0));
}

#[test]
fn compile_song_lua_keeps_notefield_wrapper_skew_on_child() {
    let song_dir = test_dir("notefield-wrapper-skew");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local field, inner, outer
return Def.ActorFrame {
    OnCommand=function(self)
        field=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
        field:AddWrapperState()
        inner=field:GetWrapperState(1)
        field:AddWrapperState()
        outer=field:GetWrapperState(2)
        outer:x(17):spin():effectmagnitude(0,0,90)
        self:SetUpdateFunction(function()
            local beat=GAMESTATE:GetSongBeat()
            inner:skewx(beat < 1 and beat*0.03 or 0)
        end)
    end
}
"#,
    )
    .expect("write wrapper chart");
    let mut context = SongLuaCompileContext::new(&song_dir, "Wrapper Skew");
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).expect("capture child wrappers");
    let frames = &compiled.player_actors[0].note_field_frames;
    assert!(frames.len() > 30, "retain the child's animated poses");
    let frame = frames
        .iter()
        .find(|frame| (frame.second - 0.5).abs() < 0.001)
        .expect("native half-second sample");
    assert_eq!(frame.wrappers.len(), 2);
    assert_eq!(frame.wrappers[0].x, 17.0, "outer wrapper draws first");
    assert!(
        frame.wrappers[0].spin_baked,
        "native accumulated spin runs once"
    );
    assert!((frame.wrappers[0].rot_z_deg - 45.0).abs() < 0.001);
    assert!((frame.wrappers[1].skew_x - 0.015).abs() < 0.0001);
    assert_eq!(compiled.player_actors[0].initial_state.skew_x, 0.0);
    assert!(!compiled.eases.iter().any(|ease| {
        matches!(
            ease.target,
            SongLuaEaseTarget::PlayerSkewX | SongLuaEaseTarget::PlayerSkewY
        )
    }));
    assert_eq!(
        crate::runtime_player_option_ease_target("skewx", "skewx"),
        None
    );
    assert_eq!(
        crate::runtime_player_option_ease_target("skewy", "skewy"),
        None
    );
}

#[test]
fn compile_song_lua_extracts_xero_aux_overlay_node_eases() {
    let song_dir = test_dir("xero-aux-overlay-node");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
xero = {}
local eases = {}
local nodes = {}
local target = nil
local wagger = nil

local function schedule_ease(self)
    table.insert(eases, self)
end

local function node(self)
    table.insert(nodes, {{self[1]}, {}, self[2]})
end

local function definemod(self)
    node(self)
end

local function export(fn, name)
    local function inner(self)
        fn(self)
        return inner
    end
    xero[name] = inner
end

export(schedule_ease, 'ease')
export(definemod, 'definemod')

local function update(self)
    if eases[0] then self:x(0) end
end

return Def.ActorFrame{
    Def.Quad{
        Name='Flash',
        OnCommand=function(self)
            target = self
            self:diffuse(1,1,1,1):diffusealpha(0)
            xero.definemod {'flashalpha', function(a) target:diffusealpha(a) end}
            xero.ease {4, 1, ease.outQuad, 1, 'flashalpha'}
        end,
    },
    Def.Quad{
        Name='Wagger',
        OnCommand=function(self)
            wagger = self
            xero.definemod {'wagy', function(a)
                wagger:wag():effectmagnitude(0,a,0):effectperiod(1):effectclock('bgm')
            end}
            xero.ease {8, 1, ease.linear, 20, 'wagy'}
        end,
    },
    Def.ActorFrame{
        InitCommand=function(self)
            self:SetUpdateFunction(update)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Xero Aux Overlay Node"),
    )
    .unwrap();
    let flash_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Flash"))
        .expect("test flash actor should compile");
    let wagger_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Wagger"))
        .expect("test wag actor should compile");
    assert!(compiled.eases.iter().all(
        |ease| !matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "flashalpha")
    ));
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == flash_index
            && (ease.start - 4.0).abs() <= 0.001
            && (ease.limit - 1.0).abs() <= 0.001
            && ease.easing.as_deref() == Some("outQuad")
            && ease
                .from
                .diffuse
                .is_some_and(|color| (color[3] - 0.0).abs() <= 0.001)
            && ease
                .to
                .diffuse
                .is_some_and(|color| (color[3] - 1.0).abs() <= 0.001)
    }));
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == wagger_index
            && (ease.start - 8.0).abs() <= 0.001
            && (ease.limit - 1.0).abs() <= 0.001
            && ease.easing.as_deref() == Some("linear")
            && ease.to.effect_mode == Some(EffectMode::Wag)
            && ease
                .to
                .effect_magnitude
                .is_some_and(|value| value == [0.0, 20.0, 0.0])
            && ease.to.effect_clock == Some(EffectClock::Beat)
            && ease.to.effect_period.is_some_and(|value| value == 1.0)
    }));
}

#[test]
fn compile_song_lua_reads_table_color_calls_for_overlays() {
    let song_dir = test_dir("overlay-table-colors");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("grid.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
local function rgb(r, g, b, a)
    return {r / 255, g / 255, b / 255, a or 1}
end

return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/grid.png",
        OnCommand=function(self)
            self:diffuse(rgb(30, 30, 35, 0.5))
            self:diffuseshift()
            self:effectcolor1(rgb(30, 30, 35, 1))
            self:effectcolor2(rgb(70, 70, 70, 1))
            self:effectperiod(5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Table Colors"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let state = compiled.overlays[0].initial_state;
    assert_eq!(
        state.diffuse,
        [30.0 / 255.0, 30.0 / 255.0, 35.0 / 255.0, 0.5]
    );
    assert_eq!(state.effect_mode, EffectMode::DiffuseShift);
    assert_eq!(
        state.effect_color1,
        [30.0 / 255.0, 30.0 / 255.0, 35.0 / 255.0, 1.0]
    );
    assert_eq!(
        state.effect_color2,
        [70.0 / 255.0, 70.0 / 255.0, 70.0 / 255.0, 1.0]
    );
    assert_eq!(state.effect_period, 5.0);
}

#[test]
fn compile_song_lua_captures_effect_defaults_and_clocks_for_overlays() {
    let song_dir = test_dir("overlay-effect-defaults");
    let entry = song_dir.join("default.lua");
    let overlay_dir = song_dir.join("gfx");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("grid.png"),
        b"not-an-image-but-good-enough-for-parser",
    )
    .unwrap();
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sprite{
        Texture="gfx/grid.png",
        OnCommand=function(self)
            self:diffuseramp()
            self:effectclock("beat")
            self:effectoffset(0.25)
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bounce()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bob()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:pulse()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:wag()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:spin()
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:vibrate()
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Effect Defaults"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 7);

    let ramp = compiled.overlays[0].initial_state;
    assert_eq!(ramp.effect_mode, EffectMode::DiffuseRamp);
    assert_eq!(ramp.effect_clock, EffectClock::Beat);
    assert_eq!(ramp.effect_color1, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(ramp.effect_color2, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(ramp.effect_offset, 0.25);

    let bounce = compiled.overlays[1].initial_state;
    assert_eq!(bounce.effect_mode, EffectMode::Bounce);
    assert_eq!(bounce.effect_period, 2.0);
    assert_eq!(bounce.effect_magnitude, [0.0, 20.0, 0.0]);

    let bob = compiled.overlays[2].initial_state;
    assert_eq!(bob.effect_mode, EffectMode::Bob);
    assert_eq!(bob.effect_period, 2.0);
    assert_eq!(bob.effect_magnitude, [0.0, 20.0, 0.0]);

    let pulse = compiled.overlays[3].initial_state;
    assert_eq!(pulse.effect_mode, EffectMode::Pulse);
    assert_eq!(pulse.effect_period, 2.0);
    assert_eq!(pulse.effect_magnitude, [0.5, 1.0, 1.0]);

    let wag = compiled.overlays[4].initial_state;
    assert_eq!(wag.effect_mode, EffectMode::Wag);
    assert_eq!(wag.effect_period, 2.0);
    assert_eq!(wag.effect_magnitude, [0.0, 0.0, 20.0]);

    let spin = compiled.overlays[5].initial_state;
    assert_eq!(spin.effect_mode, EffectMode::Spin);
    assert_eq!(spin.effect_magnitude, [0.0, 0.0, 180.0]);

    let vibrate = compiled.overlays[6].initial_state;
    assert!(vibrate.vibrate);
    assert_eq!(vibrate.effect_magnitude, [10.0, 10.0, 10.0]);
}

#[test]
fn compile_song_lua_effect_controls_are_immediate_not_tweened() {
    let song_dir = test_dir("overlay-immediate-effects");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        TriggerMessageCommand=function(self)
            self:linear(3):x(100)
            self:vibrate():effectmagnitude(2, 3, 4)
        end,
        StopMessageCommand=function(self)
            self:stopeffect()
        end,
        DelayedMessageCommand=function(self)
            self:sleep(0.5):queuecommand("Shake")
        end,
        ShakeCommand=function(self)
            self:vibrate():effectmagnitude(10, 10, 0)
            self:sleep(0.3):queuecommand("Still")
        end,
        StillCommand=function(self)
            self:stopeffect()
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Immediate Overlay Effects"),
    )
    .unwrap();
    let actor = &compiled.overlays[0];
    let trigger = actor
        .message_commands
        .iter()
        .find(|command| command.message == "Trigger")
        .unwrap();
    assert_eq!(trigger.blocks.len(), 2);
    assert_eq!(trigger.blocks[0].start, 0.0);
    assert_eq!(trigger.blocks[0].duration, 0.0);
    assert_eq!(trigger.blocks[0].delta.vibrate, Some(true));
    assert_eq!(
        trigger.blocks[0].delta.effect_magnitude,
        Some([2.0, 3.0, 4.0])
    );
    assert_eq!(trigger.blocks[1].start, 0.0);
    assert_eq!(trigger.blocks[1].duration, 3.0);
    assert_eq!(trigger.blocks[1].delta.x, Some(100.0));
    assert_eq!(trigger.blocks[1].delta.vibrate, None);

    let stop = actor
        .message_commands
        .iter()
        .find(|command| command.message == "Stop")
        .unwrap();
    assert_eq!(stop.blocks[0].delta.vibrate, Some(false));
    assert_eq!(stop.blocks[0].delta.effect_mode, Some(EffectMode::None));
    assert_eq!(stop.blocks[0].delta.effect_magnitude, None);

    let delayed = actor
        .message_commands
        .iter()
        .find(|command| command.message == "Delayed")
        .unwrap();
    assert_eq!(delayed.blocks.len(), 2);
    assert_eq!(
        (delayed.blocks[0].start, delayed.blocks[0].duration),
        (0.5, 0.0)
    );
    assert_eq!(delayed.blocks[0].delta.vibrate, Some(true));
    assert_eq!(
        delayed.blocks[0].delta.effect_magnitude,
        Some([10.0, 10.0, 0.0])
    );
    assert_eq!(
        (delayed.blocks[1].start, delayed.blocks[1].duration),
        (0.8, 0.0)
    );
    assert_eq!(delayed.blocks[1].delta.vibrate, Some(false));
}

#[test]
fn compile_song_lua_captures_queued_parent_vibration() {
    let song_dir = test_dir("queued-parent-vibration");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, include_str!("../fixtures/queue-vibration.lua"))
        .expect("native queued vibration fixture");

    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Parent Vibration");
    context.music_length_seconds = 3.0;
    context.song_display_bpms = [60.0; 2];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let actor = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("Cala"))
        .unwrap();
    let fire = actor
        .message_commands
        .iter()
        .find(|command| command.message == "CalaFire")
        .unwrap();
    assert!(fire.blocks.iter().all(|block| !block.queued));
    let index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Cala"))
        .expect("Cala was compiled");
    let vibration = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == index && track.target == SongLuaOverlayUpdateTarget::Vibrate
        })
        .expect("queued effects are captured at their dispatch frames");
    for (value, native_beat) in [(true, 1.5166667), (false, 1.8166667)] {
        let sample = vibration
            .samples
            .iter()
            .find(|sample| sample.value == SongLuaOverlayUpdateValue::Bool(value))
            .expect("native vibration transition");
        assert!((sample.time - native_beat).abs() < 0.0001, "{sample:?}");
    }
    let magnitude = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == index
                && track.target == SongLuaOverlayUpdateTarget::EffectMagnitude
        })
        .expect("queued magnitude is captured");
    assert!(
        magnitude
            .samples
            .iter()
            .any(|sample| sample.value == SongLuaOverlayUpdateValue::Vec3([10.0, 10.0, 0.0]))
    );
}

#[test]
fn runtime_prefix_reader_owns_callbacks_and_messages() {
    let song_dir = test_dir("runtime-prefix-ownership");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
prefix_globals = {
    ease = {{0, 1, 0, 0, function(value) player:skewx(value) end, "len", "linear"}},
    actions = {{0.11, "Fade"}},
}
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for _, ease in ipairs(prefix_globals.ease) do
                if beat > ease[1] and beat < ease[2] then ease[5](ease[3]) end
            end
            local event = prefix_globals.actions[1]
            if event and beat > event[1] then
                MESSAGEMAN:Broadcast(event[2])
                table.remove(prefix_globals.actions, 1)
            end
        end)
    end,
    Def.Quad{
        OnCommand=function(self) self:diffusealpha(0) end,
        FadeMessageCommand=function(self) self:linear(.5):diffusealpha(1) end,
    },
}
"#,
    )
    .expect("write runtime prefix fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Prefix");
    context.song_display_bpms = [60.0; 2];
    context.music_length_seconds = 0.5;
    let compiled = test_compile_song_lua(&entry, &context).expect("compile prefix reader");
    let skew = compiled
        .eases
        .iter()
        .filter(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerSkewX))
        .collect::<Vec<_>>();
    assert!(skew.iter().all(|ease| ease.from == 0.0 && ease.to == 0.0));
    let messages = compiled
        .messages
        .iter()
        .filter(|event| event.message == "Fade")
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].beat > 0.11);
}

#[test]
fn compile_song_lua_keeps_stateful_cross_actor_broadcast_updates() {
    let song_dir = test_dir("stateful-broadcast-update");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
shots = 0
next_fire = 1.01
mod_actions = {{1.01, "Fire", true}}
local target
return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self self:x(0) end,
    },
    Def.ActorFrame{
        FireMessageCommand=function(self)
            shots = shots + 1
            target:x(shots * 10)
        end,
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                while next_fire <= 2.01 and beat >= next_fire do
                    MESSAGEMAN:Broadcast("Fire")
                    next_fire = next_fire + 1
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Stateful Broadcast Update");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 3.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let target = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Target"))
        .unwrap();
    let x = compiled
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target && track.target == SongLuaOverlayUpdateTarget::X
        })
        .expect("stateful cross-actor broadcasts must produce an update track");
    let values = x
        .samples
        .iter()
        .filter_map(|sample| match sample.value {
            SongLuaOverlayUpdateValue::F32(value) => Some(value),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(values.contains(&10.0));
    assert!(values.contains(&20.0));
    let fire = compiled
        .stateful_message_captures
        .iter()
        .find(|capture| capture.message == "Fire")
        .expect("stateful Fire provenance must be retained");
    assert_eq!(fire.overlay_targets.len(), 1);
    assert_eq!(fire.overlay_targets[0].0, target);
    assert!(
        fire.overlay_targets[0]
            .1
            .contains(&SongLuaOverlayUpdateTarget::X)
    );
    assert!(
        fire.writes.first().is_some_and(|write| write.beat > 1.01),
        "scheduled messages must wait for the next 60 Hz UpdateCommand frame"
    );
    assert!(
        compiled
            .info
            .skipped_message_command_captures
            .iter()
            .all(|detail| !detail.contains("FireMessageCommand"))
    );
}

#[test]
fn compile_song_lua_keeps_current_options_distinct() {
    let song_dir = test_dir("current-options-native");
    let entry = song_dir.join("default.lua");
    // Run the unchanged normal-API control also captured with native C++
    // getters. Its callbacks check approach, freeze and level propagation.
    fs::write(
        &entry,
        include_str!(
            "../../../../tests/fixtures/itgmania-song-lua-micro/current-options/default.lua"
        ),
    )
    .expect("native control Lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Current options");
    context.song_display_bpms = [60.0, 60.0];
    context.music_length_seconds = 1.0;
    let compiled =
        test_compile_song_lua(&entry, &context).expect("normal callback option-level assertions");
    assert!(compiled.info.skipped_message_command_captures.is_empty());
}

#[test]
fn compile_song_lua_queries_modifiers_without_changing_them() {
    let song_dir = test_dir("modifier-queries");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, r#"
local p = GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()
local function uses(text, player) return GAMESTATE:PlayerIsUsingModifier(player or PLAYER_1, text) end
assert(uses('1x', 0) and uses('1x', 1))
assert(not uses('0x') and not uses('2x'))
assert(p:XMod() == 1 and p:CMod() == nil and p:MMod() == nil)
local detected = 2.5
for i=0,10,0.01 do
    local rounded = math.floor(i * 100 + 0.5) / 100
    if uses(rounded .. 'x', 0) then detected = i; break end
end
assert(math.abs(detected - 1) < 0.000001)
assert(not string.find(GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptionsString('ModsLevel_Song'), '1x', 1, true))
assert(uses('no reverse'))
assert(not uses('reverse'))
assert(p:Reverse() == 0)
p:Reverse(0.5)
assert(uses('*999 50% reverse'))
assert(not uses('reverse'))
assert(uses('reverse, no reverse, 50% reverse'))
assert(uses('no reverse', PLAYER_2))
assert(uses('unknown-modifier'))
p:XMod(2)
assert(uses('2x') and not uses('1x'))
p:CMod(500)
assert(uses('C500') and not uses('2x'))
p:XMod(2)
assert(uses('2x') and not uses('C500'))
GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(1.5)
assert(uses('1.5xmusic') and not uses('1xmusic'))
assert(p:Reverse() == 0.5 and p:XMod() == 2)
return Def.ActorFrame{}
"#).unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Modifier queries");
    context.players[0].speedmod = SongLuaSpeedMod::X(1.0);
    context.players[1].speedmod = SongLuaSpeedMod::X(1.0);
    context.players[1].enabled = true;
    test_compile_song_lua(&entry, &context).unwrap();
}

#[test]
fn compile_song_lua_supports_background_and_environment_helpers() {
    let song_dir = test_dir("background-environment-helpers");
    image::RgbaImage::new(100, 50)
        .save(song_dir.join("background.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
assert(getenv('missing') == nil)
setenv('missionmode', 'test')
assert(GAMESTATE:Env().missionmode == 'test')
GAMESTATE:Env().missionmode = false
assert(getenv('missionmode') == false)
setenv('missionmode', nil)
assert(getenv('missionmode') == nil)
assert(THEME:GetMetric('Common', 'ScreenHeight') == 720)
assert(THEME:HasMetric('Common', 'ScreenHeight'))
local file = RageFileUtil.CreateRageFile()
assert(file:Open('must-not-write.txt', 2))
assert(file:PutLine('test') == 2)
file:Close()
file:destroy()
return Def.Sprite{OnCommand=function(self)
    assert(self:LoadFromCurrentSongBackground() == self)
    assert(self:GetWidth() == 100 and self:GetHeight() == 50)
    bg_fit_functions.BackgroundFitMode_CoverDistort(self, 300, 100)
    assert(self:GetZoomX() == 3 and self:GetZoomY() == 2)
    bg_fit_functions.BackgroundFitMode_CoverPreserve(self, 300, 100)
    assert(self:GetZoomX() == 3 and self:GetZoomY() == 3)
    bg_fit_functions.BackgroundFitMode_FitInside(self, 300, 100)
    assert(self:GetZoomX() == 2 and self:GetZoomY() == 2)
    bg_fit_functions.BackgroundFitMode_FitInsideAvoidLetter(self, 300, 100)
    assert(self:GetZoomX() == 2)
    bg_fit_functions.BackgroundFitMode_FitInsideAvoidPillar(self, 300, 100)
    assert(self:GetZoomX() == 3)
end}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&song_dir, "Background helpers");
    context.screen_height = 720.0;
    test_compile_song_lua(&entry, &context).unwrap();
    assert!(!song_dir.join("must-not-write.txt").exists());
}

#[test]
fn compile_song_lua_preserves_effect_timing_when_holding_at_full() {
    let song_dir = test_dir("effect-hold-at-full");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{OnCommand=function(self)
        self:bob():effectperiod(2):effect_hold_at_full(3)
    end},
    Def.Quad{OnCommand=function(self)
        self:bob():effecttiming(1, 2, 3, 4, 5):effect_hold_at_full(6)
        assert(not pcall(function() self:effect_hold_at_full(-1) end))
    end},
}
"#,
    )
    .unwrap();
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Effect hold"),
    )
    .unwrap();
    assert_eq!(
        compiled.overlays[0].initial_state.effect_timing,
        Some([1.0, 0.0, 1.0, 3.0, 0.0])
    );
    assert_eq!(compiled.overlays[0].initial_state.effect_period, 5.0);
    assert_eq!(
        compiled.overlays[1].initial_state.effect_timing,
        Some([1.0, 2.0, 3.0, 6.0, 4.0])
    );
    assert_eq!(compiled.overlays[1].initial_state.effect_period, 16.0);
}

#[test]
fn compile_song_lua_supports_overlay_effect_timing() {
    let song_dir = test_dir("overlay-effect-timing");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:bob()
            self:effecttiming(0.25, 0.5, 0.75, 1.25)
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:bounce()
            self:effecttiming(0.25, 0.5, 0.75, 1.25, 1.5)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Effect Timing"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);

    let bob = compiled.overlays[0].initial_state;
    assert_eq!(bob.effect_mode, EffectMode::Bob);
    assert_eq!(bob.effect_period, 2.75);
    assert_eq!(bob.effect_timing, Some([0.25, 0.5, 0.75, 0.0, 1.25]));

    let bounce = compiled.overlays[1].initial_state;
    assert_eq!(bounce.effect_mode, EffectMode::Bounce);
    assert_eq!(bounce.effect_period, 4.25);
    assert_eq!(bounce.effect_timing, Some([0.25, 0.5, 0.75, 1.5, 1.25]));
}

#[test]
fn compile_song_lua_captures_actorframe_perspective_state() {
    let song_dir = test_dir("overlay-perspective");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.ActorFrame{
        Name="PerspectiveRoot",
        OnCommand=function(self)
            self:fov(120)
            self:vanishpoint(400, 120)
        end,
        Def.Quad{},
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Perspective"),
    )
    .unwrap();
    let perspective = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("PerspectiveRoot"))
        .expect("expected actorframe overlay with perspective state");
    assert!(matches!(perspective.kind, SongLuaOverlayKind::ActorFrame));
    assert_eq!(perspective.initial_state.fov, Some(120.0));
    assert_eq!(perspective.initial_state.vanishpoint, Some([400.0, 120.0]));
}

#[test]
fn compile_song_lua_preserves_infinite_alpha() {
    let song_dir = test_dir("infinite-alpha");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local root = Def.ActorFrame{}
for index, alpha in ipairs{math.huge, -math.huge} do
    root[index] = Def.Quad{
        InitCommand=function(self)
            self:diffuse(0.2, 0.4, 0.6, 0.8):diffusealpha(alpha)
            assert(self:GetDiffuseAlpha() == alpha)
        end,
    }
end
return root
"#,
    )
    .expect("write infinite alpha fixture");
    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Infinite alpha"),
    )
    .expect("compile infinite alpha fixture");
    for (overlay, alpha) in compiled
        .overlays
        .iter()
        .zip([f32::INFINITY, f32::NEG_INFINITY])
    {
        assert_eq!(overlay.initial_state.diffuse, [0.2, 0.4, 0.6, alpha]);
    }
    assert_eq!(compiled.overlays.len(), 2);
}

#[test]
fn compile_song_lua_preserves_overlay_color_for_diffusealpha_eases() {
    let song_dir = test_dir("overlay-diffusealpha-color");
    write_easing_lib(&song_dir);
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local target = nil

mods_ease = {
    {4, 2, 0, 1, function(a)
        if target then
            target:diffusealpha(a)
        end
    end, "len", ease.outQuad},
}

return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            target = self
            self:diffuse(0, 0, 0, 0)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay Diffusealpha Color"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.diffuse,
        [0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(compiled.overlay_eases.len(), 1);
    assert_eq!(
        compiled.overlay_eases[0].from.diffuse,
        Some([0.0, 0.0, 0.0, 0.0])
    );
    assert_eq!(
        compiled.overlay_eases[0].to.diffuse,
        Some([0.0, 0.0, 0.0, 1.0])
    );
}

#[test]
fn compile_song_lua_exposes_theme_branch_and_path_helpers() {
    let song_dir = test_dir("theme-branch-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local path = "/Songs/Group/Song/"
local parts = split("/", path)
local method_parts = ("Group/Song"):split("/")
local player_af = GetPlayerAF(ToEnumShortString(PLAYER_1))

mod_actions = {
    {1, table.concat({
        parts[1],
        parts[3],
        parts[4],
        method_parts[2],
        Basename(path),
        ProfileSlot[PlayerNumber:Reverse()[PLAYER_1] + 1],
        GameController:Reverse()["GameController_2"],
        GetDefaultFailType(),
        GetComboThreshold("Maintain"),
        tostring(IsAutoplay(PLAYER_1)),
        tostring(IsW0Judgment({Player=PLAYER_1}, PLAYER_1)),
        tostring(IsW010Judgment({Player=PLAYER_1}, PLAYER_1)),
        string.format("%.0f", GetNotefieldWidth()),
        string.format("%.0f", GetNotefieldX(PLAYER_1)),
        tostring(player_af ~= nil),
        Branch.AfterSelectMusic(),
        Branch.GameplayScreen(),
        SelectMusicOrCourse(),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Helpers");
    context.players[0].screen_x = 123.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "|Group|Song|Song|Song|ProfileSlot_Player1|1|FailType_Immediate|TapNoteScore_W3|false|false|false|256|123|true|ScreenGameplay|ScreenGameplay|ScreenSelectMusic"
    );
}

#[test]
fn compile_song_lua_exposes_theme_utility_and_sort_helpers() {
    let song_dir = test_dir("theme-utility-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local values = range(1, 5, 2)
local doubled = map(function(value) return value * 2 end, values)
local labels = stringify(doubled, "%g")
local unique = deduplicate({"a", "a", "b"})
local wheel = SCREENMAN:GetTopScreen():GetMusicWheel()
local before = GAMESTATE:GetSortOrder()
wheel:ChangeSort("SortOrder_Preferred")
local after = GAMESTATE:GetSortOrder()

mod_actions = {
    {1, table.concat({
        tostring(#values),
        labels[2],
        tostring(#unique),
        ToEnumShortString(before),
        ToEnumShortString(after),
        tostring(SortOrder:Reverse()[after] ~= nil),
        tostring(TapNoteScore:Reverse()["TapNoteScore_W3"] ~= nil),
        tostring(HoldNoteScore:Reverse()["HoldNoteScore_Held"] ~= nil),
        THEME:GetString("TapNoteScore", "W1"),
        THEME:GetString("ScreenEvaluation", "Hands"),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Utilities"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "3|6|2|Group|Preferred|true|true|true|W1|Hands"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_theme_asset_option_helpers() {
    let song_dir = test_dir("theme-asset-option-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local filters = BackgroundFilterValues()
local columns = GetColumnMapping(PLAYER_1)
local credits = GetCredits()
local fallback = GetFallbackBanner()

mod_actions = {
    {1, table.concat({
        tostring(filters.Dark),
        tostring(NumJudgmentsAvailable()),
        tostring(DetermineTimingWindow(0.03)),
        tostring(credits.Credits),
        tostring(credits.CoinsPerCredit),
        StripSpriteHints("Love 2x6 (doubleres).png"),
        GetJudgmentGraphics()[1],
        GetHoldJudgments()[1],
        GetHeldMissGraphics()[1],
        tostring(#GetComboFonts()),
        tostring(#columns),
        tostring(columns[4]),
        tostring(#GetStepsCredit(PLAYER_1)),
        tostring(IsSpooky()),
        tostring(IsGameAndMenuButton("Left")),
        GetPlayerOptionsString(PLAYER_1),
        tostring(TotalCourseLength(PLAYER_1)),
        tostring(TotalCourseLengthPlayed(PLAYER_1)),
        fallback:sub(1, 21),
    }, "|"), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Asset Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "50|5|2|0|1|Love|None|None|None|0|4|4|0|false|false|NoHideLights, Overhead|0|0|__songlua_theme_path"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_theme_option_row_helpers() {
    let song_dir = test_dir("theme-option-row-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local speed = CustomOptionRow("SpeedModType")
assert(speed.Name == "SpeedModType")
assert(speed.LayoutType == "ShowOneInRow")
assert(speed.SelectType == "SelectOne")
assert(speed.HideOnDisable == false)
assert(speed.Choices[1] == "X")
assert(speed.Values == nil)
assert(speed.EnabledForPlayers()[1] == PLAYER_1)
assert(speed.ReloadRowMessages[1] == nil)

local list = speed:LoadSelections({})
assert(list[1] == true)
speed:SaveSelections({false, true, false}, PLAYER_2)
assert(SL.P2.ActiveModifiers.SpeedModType == "C")

local bg = CustomOptionRow("BackgroundFilter")
assert(bg.Values[1] == "Off")
local hide = CustomOptionRow("Hide")
assert(hide.SelectType == "SelectMultiple")
hide:SaveSelections({true, false, true}, PLAYER_1)
assert(SL.P1.ActiveModifiers.HideTargets == true)
assert(SL.P1.ActiveModifiers.HideSongBG == false)
assert(SL.P1.ActiveModifiers.HideCombo == true)
local variant = CustomOptionRow("NoteSkinVariant")
assert(variant.HideOnDisable == true)
assert(variant.ReloadRowMessages[1] == "RefreshActorProxy")
assert(CustomOptionRow("MissingRow") == false)

local pref = ThemePrefsRows.GetRow("AllowThemeVideos")
assert(pref.Name == "AllowThemeVideos")
assert(pref.Values[1] == true)
assert(pref:LoadSelections({})[1] == true)

local visual = ThemePrefsRows.GetRow("VisualStyle")
assert(visual.Choices[1] == "Hearts")
visual:SaveSelections({false, true}, PLAYER_1)
assert(ThemePrefs.Get("VisualStyle") == "Arrows")
ThemePrefsRows.GetRow("RainbowMode"):SaveSelections({true, false}, PLAYER_1)
assert(ThemePrefs.Get("RainbowMode") == true)
ThemePrefs.InitAll({})
ThemePrefsRows.InitAll({})

local op = OperatorMenuOptionRows.Theme()
assert(op.Name == "Theme")
assert(op.Choices[1] == THEME:GetCurThemeName())
local marathon = OperatorMenuOptionRows.LongAndMarathonTime("Marathon")
assert(marathon.Name == "Marathon Time")
assert(marathon.Values[2] == 450)
marathon:SaveSelections({false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MarathonVerSongSeconds") == 450)
local wheel = OperatorMenuOptionRows.MusicWheelSpeed()
assert(wheel.Values[3] == 15)
wheel:SaveSelections({false, false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MusicWheelSwitchSpeed") == 15)
local offset = OperatorMenuOptionRows.GlobalOffsetSeconds()
assert(offset.Values[4] == 0.5)
offset:SaveSelections({false, false, false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("GlobalOffsetSeconds") == 0.5)
local memory = OperatorMenuOptionRows.MemoryCards()
assert(memory.Values[1] == false and memory.Values[2] == true)
memory:SaveSelections({false, true}, PLAYER_1)
assert(PREFSMAN:GetPreference("MemoryCards") == true)
local fallback = OperatorMenuOptionRows.UnknownThing()
assert(fallback.Name == "UnknownThing")

mod_actions = {
    {1, function()
        CustomOptionRow("Mini"):SaveSelections({true, false}, PLAYER_1)
        assert(SL.P1.ActiveModifiers.Mini == "-100%")
        ThemePrefsRows.GetRow("VisualStyle"):SaveSelections({true, false}, PLAYER_1)
        assert(ThemePrefs.Get("VisualStyle") == "Hearts")
        OperatorMenuOptionRows.CustomSongsLoadTimeout():SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("CustomSongsLoadTimeout") == 3)
        OperatorMenuOptionRows.UnknownThing():SaveSelections({true}, PLAYER_1)
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Option Row Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_sl_custom_prefs_helpers() {
    let song_dir = test_dir("sl-custom-prefs-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local prefs = SL_CustomPrefs.Get()
assert(prefs.VisualStyle.Default ~= nil)
assert(prefs.VisualStyle.Choices[1] ~= nil)
assert(prefs.VisualStyle.Values[1] ~= nil)
assert(prefs.AllowThemeVideos.Values[1] == true)
assert(prefs.NumberOfContinuesAllowed.Values[1] == 0)
assert(prefs.QRLogin.Values[1] == "Always")
assert(ThemePrefs.Get("EditModeLastSeenSong") == "")
assert(ThemePrefs.Get("RainbowMode") == false)
ThemePrefs.Set("RainbowMode", true)
assert(ThemePrefs.Get("RainbowMode") == true)
ThemePrefs.Set("RainbowMode", nil)
assert(ThemePrefs.Get("RainbowMode") == false)
ThemePrefs.InitAll({CustomCompilePref={Default="initialized"}})
assert(ThemePrefs.Get("CustomCompilePref") == "initialized")
ThemePrefs.Set("CustomCompilePref", "override")
ThemePrefs.InitAll({CustomCompilePref={Default="ignored"}})
assert(ThemePrefs.Get("CustomCompilePref") == "override")

mod_actions = {
    {1, function()
        ThemePrefs.Set("RuntimePref", "value")
        assert(ThemePrefs.Get("RuntimePref") == "value")
        ThemePrefs.Save()
        SL_CustomPrefs.Validate()
        SL_CustomPrefs.Init()
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SL Custom Prefs Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_top_screen_option_row_shape() {
    let song_dir = test_dir("top-screen-option-row-shape");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local top = SCREENMAN:GetTopScreen()

local function FindOptionRowIndex(ScreenOptions, Name)
    if not ScreenOptions or not ScreenOptions.GetNumRows then return end
    for i=0, ScreenOptions:GetNumRows()-1 do
        if ScreenOptions:GetOptionRow(i):GetName() == Name then
            return i
        end
    end
end

local lines = THEME:GetMetric("ScreenPlayerOptions", "LineNames")
assert(lines:find("MusicRate"))

local speed = FindOptionRowIndex(top, "SpeedMod")
local mini = FindOptionRowIndex(top, "Mini")
local perspective = FindOptionRowIndex(top, "Perspective")
local variant = FindOptionRowIndex(top, "NoteSkinVariant")
local rate = FindOptionRowIndex(top, "MusicRate")
assert(speed and mini and perspective and variant and rate)
assert(top:GetNumRows() > rate)
assert(top:GetOptionRow(speed):GetName() == "SpeedMod")
assert(top:GetOptionRow(perspective):GetChoiceInRowWithFocus(PLAYER_1) == 1)

local speed_bmt = top:GetOptionRow(speed):GetChild(""):GetChild("Item")[PlayerNumber:Reverse()[PLAYER_1]+1]
assert(speed_bmt:GetText() == "1")
speed_bmt:settext("C400")
assert(speed_bmt:GetText() == "C400")

local mini_text = top:GetOptionRow(mini):GetChild(""):GetChild("Item")[1]:GetText():gsub("%%", "")
assert(mini_text == "0")

local title = top:GetOptionRow(rate):GetChild(""):GetChild("Title")
title:settext("Rate")
assert(title:GetText() == "Rate")

top:SetOptionRowIndex(PLAYER_1, rate)
assert(top:GetCurrentRowIndex(PLAYER_1) == rate)

mod_actions = {
    {1, function()
        top:RedrawOptions()
        top:GetOptionRow(variant):GetChild(""):GetChild("Item")[2]:settext("variant")
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Top Screen Option Row Shape"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_service_option_metrics() {
    let song_dir = test_dir("service-option-metrics");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
local names = THEME:GetMetric("ScreenOptionsService", "LineNames")
assert(names:find("SystemOptions"))
assert(THEME:HasMetric("ScreenSystemOptions", "LineNames"))
assert(THEME:GetMetric("ScreenSystemOptions", "Fallback") == "ScreenOptionsServiceChild")
assert(THEME:GetMetric("ScreenVisualOptions", "Fallback") == "ScreenOptionsServiceSub")
assert(THEME:GetMetric("ScreenSystemOptions", "LineGame") == "conf,Game")
assert(THEME:GetMetric("ScreenSystemOptions", "LineTheme") == "lua,OperatorMenuOptionRows.Theme()")
assert(THEME:GetMetric("ScreenGraphicsSoundOptions", "LineDisplayMode") == "lua,ConfDisplayMode()")
assert(THEME:GetMetric("ScreenGraphicsSoundOptions", "LineFullscreenType") == "lua,ConfFullscreenType()")
assert(THEME:HasString("OptionTitles", "DisplayMode"))
assert(THEME:GetString("OptionTitles", "DisplayMode") == "DisplayMode")

local child_count = 0
local row_count = 0
for childscreen_name in names:gmatch("([^,]+)") do
    local screen = "Screen"..childscreen_name
    if THEME:HasMetric(screen, "LineNames") then
        child_count = child_count + 1
        for optrow_name in THEME:GetMetric(screen, "LineNames"):gmatch("([^,]+)") do
            local line = THEME:GetMetric(screen, "Line"..optrow_name)
            assert(type(line) == "string" and line ~= "")
            row_count = row_count + 1
            if row_count > 8 then break end
        end
    end
end
assert(child_count >= 4)
assert(row_count > 8)

local screen = SCREENMAN:GetTopScreen()
SL.Global.PrevScreenOptionsServiceRow[screen:GetName()] = 3
screen:SetOptionRowIndex(GAMESTATE:GetMasterPlayerNumber(), SL.Global.PrevScreenOptionsServiceRow[screen:GetName()])
assert(screen:GetCurrentRowIndex(GAMESTATE:GetMasterPlayerNumber()) == 3)

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Service Option Metrics"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_conf_option_row_helpers() {
    let song_dir = test_dir("conf-option-row-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local aspect = ConfAspectRatio()
assert(aspect.Name == "DisplayAspectRatio")
assert(aspect.Choices[1] == "16:9")
assert(aspect.Values[1] > 1.7)
assert(aspect.OneChoiceForAllPlayers == true)
assert(aspect:LoadSelections({})[1] == true)

local resolution = ConfDisplayResolution()
assert(resolution.Name == "DisplayResolution")
assert(resolution.Choices[1]:find("x"))

local mode = ConfDisplayMode()
assert(mode.Name == "DisplayMode")
assert(mode.Choices[1] == "Windowed")

local rate = ConfRefreshRate()
assert(rate.Name == "RefreshRate")
assert(rate.Choices[1] == "60")
assert(rate.Values[2] == 120)

local fullscreen = ConfFullscreenType()
assert(fullscreen.Name == "FullscreenType")
assert(fullscreen.Choices[1] == "Borderless")

mod_actions = {
    {1, function()
        aspect:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayAspectRatio") > 1.7)
        resolution:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayResolution") == "1920x1080")
        mode:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("DisplayMode") == "Windowed")
        rate:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("RefreshRate") == 60)
        fullscreen:SaveSelections({true}, PLAYER_1)
        assert(PREFSMAN:GetPreference("FullscreenType") == "Borderless")
    end, true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Conf Option Row Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_exposes_theme_manager_compat() {
    let song_dir = test_dir("theme-manager-compat");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local env = GAMESTATE:Env()
env.P1PeakNPS = 123

local function has_value(values, wanted)
    if not values then return false end
    for _, value in ipairs(values) do
        if value == wanted then return true end
    end
    return false
end

local bg_path, bg_group, bg_name = THEME:GetPathInfoB("ScreenGameplay", "Overlay")
assert(bg_path == THEME:GetPathB("ScreenGameplay", "Overlay"))
assert(bg_group == "ScreenGameplay" and bg_name == "Overlay")
assert(THEME:GetNumSelectableThemes() == 1)
assert(THEME:GetSelectableThemeNames()[1] == "Simply Love")
assert(THEME:get_theme_fallback_list()[1] == "Simply Love")
assert(THEME:GetThemeAuthor() == "")
assert(THEME:DoesThemeExist("Simply Love") == true)
assert(THEME:IsThemeSelectable("Simply Love") == true)
assert(THEME:DoesThemeExist("Missing Theme") == false)
assert(THEME:IsThemeSelectable("Missing Theme") == false)
assert(THEME:DoesLanguageExist("en") == true)
assert(THEME:DoesLanguageExist("english") == true)
assert(THEME:DoesLanguageExist("zz") == false)
assert(has_value(THEME:GetMetricNamesInGroup("ScreenSystemOptions"), "LineTheme"))
assert(has_value(THEME:GetMetricNamesInGroup("Player"), "DrawDistanceBeforeTargetsPixels"))
assert(THEME:GetMetricNamesInGroup("MissingGroup") == nil)
assert(has_value(THEME:GetStringNamesInGroup("OptionTitles"), "DisplayMode"))
assert(has_value(THEME:GetStringNamesInGroup("Difficulty"), "Difficulty_Hard"))
assert(THEME:GetStringNamesInGroup("MissingStrings") == nil)
assert(THEME:RunLuaScripts("Scripts") == nil)

PREFSMAN:SetPreference("Theme", "Ignored")
PROFILEMAN:SetStatsPrefix("Stats")
PROFILEMAN:SaveMachineProfile()
GAMESTATE:SaveProfiles()
assert(SONGMAN:SetPreferredSongs("Favorites.txt", true) == SONGMAN)
assert(SONGMAN:SetPreferredCourses("Courses.txt", true) == SONGMAN)

local song = GAMESTATE:GetCurrentSong()
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local course = GAMESTATE:GetCurrentCourse()
local all = SONGMAN:GetAllSongs()
local courses = SONGMAN:GetAllCourses()
local groups = SONGMAN:GetSongGroupNames()
local course_groups = SONGMAN:GetCourseGroupNames()
local found = SONGMAN:FindSong(song:GetSongDir())
local found_course = SONGMAN:FindCourse(course:GetCourseDir())
local extra_song, extra_steps = SONGMAN:GetExtraStageInfo(false, GAMESTATE:GetCurrentStyle())
local pref_songs = SONGMAN:GetPreferredSortSongs()
local pref_courses = SONGMAN:GetPreferredSortCourses("CourseType_Nonstop")
local preferred_section = SONGMAN:SongToPreferredSortSectionName(song)
local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local played = STATSMAN:GetPlayedStageStats(1):GetPlayerStageStats(PLAYER_2)
local highscore = pss:GetHighScore()
local machine_scores = PROFILEMAN:GetMachineProfile():GetHighScoreList(song, steps):GetHighScores()

assert(SONGMAN:GetRandomSong() == song)
assert(SONGMAN:GetRandomCourse() == course)
assert(SONGMAN:GetSongFromSteps(steps) == song)
assert(found_course == course)
assert(SONGMAN:DoesCourseGroupExist(course_groups[1]))
assert(#SONGMAN:GetSongsInGroup(groups[1]) == 1)
assert(#SONGMAN:GetCoursesInGroup(course_groups[1]) == 1)
assert(SONGMAN:ShortenGroupName(groups[1]) == groups[1])
assert(SONGMAN:GetSongRank(song) == 1)
assert(extra_song == song and extra_steps == steps)
assert(#SONGMAN:GetPopularSongs() == 1)
assert(#SONGMAN:GetPopularCourses("CourseType_Nonstop") == 1)
assert(#SONGMAN:GetPreferredSortSongsBySectionName(preferred_section) == 1)
assert(SONGMAN:GetSongColor(song)[4] == 1)
assert(SONGMAN:GetSongGroupColor(groups[1])[4] == 1)
assert(SONGMAN:GetCourseColor(course)[4] == 1)
assert(SONGMAN:GetSongGroupBannerPath(groups[1]) ~= nil)
assert(SONGMAN:GetCourseGroupBannerPath(course_groups[1]) ~= nil)
assert(not SONGMAN:WasLoadedFromAdditionalSongs())
assert(not SONGMAN:WasLoadedFromAdditionalCourses())
assert(SONGMAN:GetNumLockedSongs() == 0)
assert(SONGMAN:GetNumUnlockedSongs() == 1)
assert(SONGMAN:GetNumSelectableAndUnlockedSongs() == 1)
assert(SONGMAN:GetNumAdditionalSongs() == 0)
assert(SONGMAN:GetNumCourses() == 1)
assert(SONGMAN:GetNumAdditionalCourses() == 0)
assert(SONGMAN:GetNumCourseGroups() == 1)

mod_actions = {
    {
        1,
        string.format(
            "%d:%s:%s:%.0f:%s:%d:%d:%d:%d:%d:%s:%s:%.0f:%d:%d:%s:%s:%d",
            GAMESTATE:GetNumPlayersEnabled(),
            THEME:GetCurThemeName(),
            THEME:GetThemeDisplayName(),
            GetTimeSinceStart(),
            tostring(HolidayCheer()),
            #pref_songs,
            #pref_courses,
            #all,
            #courses,
            #groups,
            found:GetDisplayMainTitle(),
            tostring(SONGMAN:DoesSongGroupExist(groups[1])),
            SONGMAN:GetGroup(song):GetSyncOffset(),
            pss:GetPossibleDancePoints(),
            played:GetActualDancePoints(),
            pss:GetGrade(),
            highscore:GetName(),
            #machine_scores
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Manager Compat"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2:Simply Love:Simply Love:0:false:1:1:1:1:1:Theme Manager Compat:true:0:0:0:Grade_Tier17:Player:1"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn center_player_utility() {
    let song_dir = test_dir("center-player-utility");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local before = Center1Player()
PREFSMAN:SetPreference("Center1Player", true)
mod_actions = {{0, tostring(before) .. ":" .. tostring(Center1Player()), true}}
return Def.ActorFrame{}
"#,
    )
    .unwrap();
    for (style, expected) in [
        ("single", "false:true"),
        ("double", "true:true"),
        ("versus", "false:false"),
    ] {
        let mut context = SongLuaCompileContext::new(&song_dir, "center player");
        context.style_name = style.into();
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert_eq!(compiled.messages.len(), 1);
        assert_eq!(compiled.messages[0].message, expected, "{style}");
    }
}

#[test]
fn runtime_text_tracks_frames() {
    let song_dir = test_dir("runtime-text-frames");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
    local ticks = 0
    local queued = false
    return Def.ActorFrame {
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            ticks = ticks + 1
            self:GetChild("Count"):settext(queued and ("queued:" .. ticks) or ticks)
        end)
    end,
    LoadFont("Common Normal") .. {
        Name="Count", Text="initial",
        OnCommand=function(self) self:settext("start"):sleep(0.025):queuecommand("Queued") end,
        QueuedCommand=function(self) queued = true; self:settext("queued") end,
    },
    }
    "#,
    )
    .expect("write text replay fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "text frames");
    context.music_length_seconds = 0.2;
    context.song_display_bpms = [60.0; 2];
    context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
        0.125,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 60.0)],
            stops: vec![deadsync_rules::timing::StopSegment {
                beat: 0.04,
                duration: 0.1,
            }],
            ..Default::default()
        },
        &[],
    ));
    let compiled = test_compile_song_lua(&entry, &context).expect("compile text replay");
    let actor = compiled
        .overlays
        .iter()
        .find(|actor| actor.name.as_deref() == Some("Count"))
        .expect("text actor");
    let SongLuaOverlayKind::BitmapText {
        text,
        text_changes,
        text_changes_in_seconds,
        ..
    } = &actor.kind
    else {
        panic!("text actor kind")
    };
    assert!(*text_changes_in_seconds);
    let origin = context
        .song_timing
        .as_ref()
        .expect("song timing")
        .get_time_for_beat_exact(0.0);
    for (frame, expected) in [
        (0, "1"),
        (1, "2"),
        (2, "queued:3"),
        (3, "queued:4"),
        (6, "queued:7"),
    ] {
        let seconds = frame as f32 / 60.0;
        assert_eq!(
            crate::overlay_text_at(text, text_changes, seconds + origin).as_ref(),
            expected,
            "frame {frame}"
        );
    }
    assert_eq!(
        crate::song_beat_at_elapsed_seconds(3.0 / 60.0, &context),
        crate::song_beat_at_elapsed_seconds(6.0 / 60.0, &context)
    );
}

#[test]
fn compile_song_lua_exposes_fallback_theme_utility_helpers() {
    let song_dir = test_dir("fallback-theme-utility-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    LoadFont("Common Normal")..{
        Text=table.concat({
            SecondsToMSS(125),
            SecondsToMMSS(65),
            SecondsToMSSMsMs(65.5),
            SecondsToMMSSMsMs(65.5),
            SecondsToHHMMSS(3661),
            FormatNumberAndSuffix(1),
            FormatNumberAndSuffix(2),
            FormatNumberAndSuffix(3),
            FormatNumberAndSuffix(11),
            FormatNumberAndSuffix(113),
        }, "|"),
        OnCommand=function(self)
            mod_actions = {
                {
                    1,
                    string.format(
                        "%.3f:%.1f:%.0f:%s",
                        GetScreenAspectRatio(),
                        WideScale(100, 200),
                        clamp(5, 0, 3),
                        self:GetText()
                    ),
                    true,
                },
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Utility Helpers");
    context.screen_width = 854.0;
    context.screen_height = 480.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1.778:200.0:3:2:05|01:05|1:05.50|01:05.50|01:01:01|1st|2nd|3rd|11th|113th"
    );
}

#[test]
fn compile_song_lua_exposes_theme_process_compat_helpers() {
    let song_dir = test_dir("theme-process-helpers");
    let sounds = song_dir.join(crate::SONG_LUA_THEME_PATH_PREFIX).join("S");
    fs::create_dir_all(&sounds).unwrap();
    for name in ["Common invalid.ogg", "_unlock.ogg"] {
        fs::write(sounds.join(name), b"asset lookup only").unwrap();
    }
    fs::write(song_dir.join("sample.ogg"), b"asset lookup only").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Sound{ Name="InvalidSound", File=THEME:GetPathS("", "Common invalid.ogg") },
    InitCommand=function(self)
local top = SCREENMAN:GetTopScreen()
local sound = self:GetChild("InvalidSound")
sound:play():stop()

SOUND:PlayOnce(THEME:GetPathS("", "_unlock.ogg"))
SOUND:DimMusic(0.5, 1.0)
SOUND:PlayMusicPart("sample.ogg", 0, 5)
SOUND:StopMusic()
top:SetNextScreenName("ScreenEvaluationStage")
top:AddInputCallback(function() end):PauseGame(true):RemoveInputCallback(function() end)
top:StartTransitioningScreen("SM_GoToNextScreen")

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%s:%s:%d:%.0f:%s:%s:%d",
            top:GetName(),
            top:GetNextScreenName(),
            THEME:GetMetric(top:GetName(), "Class"),
            THEME:GetMetric("Common", "DefaultNoteSkinName"),
            tostring(THEME:HasMetric("Player", "ReceptorArrowsYStandard")),
            tostring(THEME:GetMetricB("ScreenHeartEntry", "HeartEntryEnabled")),
            THEME:GetMetricI("MusicWheel", "NumWheelItems"),
            THEME:GetMetricF("GraphDisplay", "BodyWidth"),
            ScreenString("Cancel"),
            string.sub(THEME:GetPathG("Combo", "100Milestone"), 1, 20),
            top:GetCurrentRowIndex(PLAYER_1)
        ),
        true,
    },
}

MESSAGEMAN:Broadcast(mod_actions[1][2])
    end,
}
"#,
    )
    .unwrap();

    // Input callbacks are runtime behavior; offline compilation correctly
    // rejects them instead of silently dropping the registered function.
    let mut context = SongLuaCompileContext::new(&song_dir, "Theme Process Helpers");
    context.live = true;
    let mut compiled = test_compile_song_lua(&entry, &context).unwrap();
    let output = compiled
        .live_scene
        .as_mut()
        .expect("owned runtime callbacks")
        .take_startup_output();
    let messages = output
        .broadcasts
        .iter()
        .filter(|event| event.1.starts_with("ScreenGameplay:"))
        .map(|event| event.1.as_str())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 1, "startup output: {output:?}");
    assert_eq!(
        messages[0],
        "ScreenGameplay:ScreenEvaluationStage:ScreenGameplay:cel:true:false:15:300:Cancel:__songlua_theme_path:0"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_exposes_screen_process_shims() {
    let song_dir = test_dir("screen-process-shims");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
SCREENMAN:set_input_redirected(PLAYER_1, true)
SCREENMAN:AddNewScreenToTop("ScreenTextEntry")

local top = SCREENMAN:GetTopScreen()
top:Load({ Question="Search" })
    :SetPrevScreenName("ScreenSelectMusic")
    :SetNextScreenName("ScreenGameplay")
    :PostScreenMessage("SM_BeginFailed", 0)
    :SetProfileIndex(PLAYER_1, -1)
    :PauseGame(true)

local wheel = top:GetMusicWheel()
wheel:SetOpenSection(""):ChangeSort("SortOrder_Preferred")

mod_actions = {
    {
        1,
        string.format(
            "%s:%s:%s:%s:%s:%s:%s:%s",
            top:GetName(),
            top:GetPrevScreenName(),
            top:GetNextScreenName(),
            tostring(top:IsPaused()),
            tostring(top:AllAreOnLastRow()),
            tostring(wheel:IsLocked()),
            tostring(top:GetChild("Timer") == nil),
            tostring(top:GetNextCourseSong() == GAMESTATE:GetCurrentSong())
        ),
        true,
    },
}

top:Cancel():Finish():begin_backing_out()

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Screen Process Shims"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ScreenTextEntry:ScreenSelectMusic:ScreenGameplay:true:false:false:true:true"
    );
}

#[test]
fn compile_song_lua_resolves_next_course_song_background() {
    let song_dir = test_dir("next-course-song-background");
    image::RgbaImage::new(96, 54)
        .save(song_dir.join("background.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {}

return Def.ActorFrame{
    Def.Sprite{
        OnCommand=function(self)
            local song = SCREENMAN:GetTopScreen():GetNextCourseSong()
            self:LoadFromSongBackground(song)
            local texture = self:GetTexture()
            mod_actions[#mod_actions + 1] = {
                1,
                string.format(
                    "%s:%s:%d:%d",
                    tostring(song == GAMESTATE:GetCurrentSong()),
                    tostring(texture:GetPath():match("background%.png$") ~= nil),
                    self:GetWidth(),
                    self:GetHeight()
                ),
                true,
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Next Course Song Background"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:true:96:54");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Sprite { ref texture_path, .. }
            if texture_path.ends_with("background.png")
    ));
}

#[test]
fn compile_song_lua_exposes_song_and_steps_metadata() {
    let root_dir = test_dir("song-steps-metadata");
    let song_dir = root_dir.join("Pack A").join("Song A");
    fs::create_dir_all(&song_dir).unwrap();
    fs::write(song_dir.join("chart.ssc"), "").unwrap();
    fs::write(song_dir.join("music.ogg"), "").unwrap();
    image::RgbaImage::new(100, 40)
        .save(song_dir.join("banner.png"))
        .unwrap();
    image::RgbImage::new(320, 240)
        .save(song_dir.join("background.jpg"))
        .unwrap();
    image::RgbaImage::new(120, 120)
        .save(song_dir.join("jacket.png"))
        .unwrap();
    image::RgbaImage::new(80, 80)
        .save(song_dir.join("cdtitle.png"))
        .unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local song = GAMESTATE:GetCurrentSong()
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local song_bpms = song:GetTimingData():GetActualBPM()
local steps_timing = steps:GetTimingData()
local radar = steps:GetRadarValues(PLAYER_1)
mod_actions = {
    {
        1,
        string.format(
            "%s|%s|%s|%s|%s|%s|%s|%s|%s|%.0f|%s|%s|%s|%d|%d|%.1f|%.0f|%.0f|%.0f|%.0f",
            song:GetDisplayFullTitle(),
            song:GetTranslitMainTitle(),
            song:GetDisplaySubTitle(),
            song:GetGroupName(),
            tostring(song:HasMusic()),
            tostring(song:HasBanner()),
            tostring(song:HasBackground()),
            tostring(song:HasJacket()),
            tostring(song:HasCDImage()),
            song:GetStageCost(),
            tostring(song:GetMusicPath():match("music%.ogg$") ~= nil),
            tostring(song:GetBannerPath():match("banner%.png$") ~= nil),
            tostring(steps:GetFilename():match("chart%.ssc$") ~= nil),
            #song:GetAllSteps(),
            steps:GetMeter(),
            song:MusicLengthSeconds(),
            radar:GetValue("RadarCategory_Notes"),
            song_bpms[1],
            song_bpms[2],
            steps_timing:GetBPMAtBeat(0)
        ),
        true,
    },
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Song Metadata");
    context.song_steps = test_song_steps();
    context.song_display_bpms = [90.0, 180.0];
    context.song_timing_bpms = vec![(0.0, 90.0), (4.0, 180.0)];
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    context.players[0].display_bpms = [150.0, 210.0];
    context.music_length_seconds = 123.4;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "Song Metadata|Song Metadata||Pack A|true|true|true|true|true|1|true|true|true|6|10|123.4|0|90|180|150"
    );
}

#[test]
fn compile_song_lua_samples_overlay_perframes_into_overlay_eases() {
    let song_dir = test_dir("perframe-overlay");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
mod_perframes = {
    {8, 9, function(beat)
        if target then
            target:x((beat - 8) * 120)
            target:diffusealpha(1 - (beat - 8))
        end
    end},
}
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self)
            target = self
            self:zoomto(16, 16)
        end
    }
}
"#,
    )
    .unwrap();

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Perframe Overlay"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    assert!(
        compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.x.is_some() && ease.to.x.is_some()
        })
    );
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == 0 && ease.from.diffuse.is_some() && ease.to.diffuse.is_some()
    }));
}

#[test]
fn runtime_perframes_are_not_also_compiled_as_standalone_eases() {
    let song_dir = test_dir("runtime-perframe-single-authority");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local target
mod_perframes = {
    {1, 2, function(beat) target:x(beat * 10) end},
}
return Def.ActorFrame{
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self end,
    },
    Def.ActorFrame{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat = GAMESTATE:GetSongBeat()
                for _, perframe in pairs(mod_perframes) do
                    if beat > perframe[1] and beat < perframe[2] then
                        perframe[3](beat)
                    end
                end
            end)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Runtime Perframe");
    context.music_length_seconds = 2.0;
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let target_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Target"))
        .unwrap();

    assert!(compiled.overlay_updates.iter().any(|track| {
        track.overlay_index == target_index && track.target == SongLuaOverlayUpdateTarget::X
    }));
    assert!(!compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == target_index && (ease.from.x.is_some() || ease.to.x.is_some())
    }));
}

#[test]
fn overlay_perframes_read_in_flight_action_tweens() {
    let song_dir = test_dir("perframe-action-getter");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local scale
local target
mod_actions = {
    {4, function()
        scale:decelerate(1):x(1)
    end, true},
}
mod_perframes = {
    {3, 6, function()
        target:zoom(scale:GetX())
    end},
}
return Def.ActorFrame{
    Def.Quad{
        Name="Scale",
        InitCommand=function(self)
            scale = self
            self:x(10):zoomto(1, 1):visible(false)
        end,
    },
    Def.Quad{
        Name="Target",
        InitCommand=function(self)
            target = self
            self:zoomto(1, 1)
        end,
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Perframe Action Getter");
    context.song_display_bpms = [120.0, 120.0];
    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    let target_index = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("Target"))
        .unwrap();
    let sampled_zooms = compiled
        .overlay_eases
        .iter()
        .filter(|ease| ease.overlay_index == target_index && (4.0..6.0).contains(&ease.start))
        .flat_map(|ease| [ease.from.zoom, ease.to.zoom])
        .flatten()
        .collect::<Vec<_>>();

    assert!(sampled_zooms.iter().any(|zoom| (1.1..9.9).contains(zoom)));
    assert!(sampled_zooms.iter().any(|zoom| (zoom - 3.25).abs() < 0.75));
}

#[test]
fn mixed_multitap_callbacks() {
    let song_dir = test_dir("mixed-multitap-callbacks");
    let entry = song_dir.join("default.lua");
    let script = r#"
multitaps = {Challenge = {{lane=1, taps={1, 2}}}}
local phase
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"):Drunk(beat)
            player:rotationz(beat * 10)
            phase:y(beat * 20)
            local handler = player:GetChild("NoteField"):GetColumnActors()[1]:GetPosHandler()
            handler:SetSplineMode("NoteColumnSplineMode_Offset")
            local spline = handler:GetSpline()
            spline:SetSize(2)
            spline:SetPoint(1, {0, beat, 0})
            spline:SetPoint(2, {0, beat, 0})
            spline:Solve()
        end)
    end,
    Def.ActorFrame{
        Name="MultitapFrameP1",
        Def.ActorFrame{
            Name="MultitapP1_1",
            InitCommand=function(self) phase = self end,
            Def.Quad{Name="MultitapArrowP1_1"},
            Def.BitmapText{Name="MultitapTextP1_1", Text=""},
        },
    },
    Def.ActorFrame{
        Name="Update",
        InitCommand=function(self)
            __child_update__
        end,
    },
}
"#;
    for callback in [
        "self:SetUpdateFunction(function() phase:z(GAMESTATE:GetSongBeat() * 30) end)",
        "",
    ] {
        fs::write(&entry, script.replace("__child_update__", callback)).unwrap();
        let mut context = SongLuaCompileContext::new(&song_dir, "Mixed Multitap Callbacks");
        context.players[1].enabled = false;
        context.music_length_seconds = 0.5;
        context.song_timing_bpms = vec![(0.0, 120.0)];
        let compiled = test_compile_song_lua(&entry, &context).unwrap();
        assert!(compiled.info.unsupported_perframe_captures.is_empty());
        assert!(compiled.eases.iter().any(|ease| {
            ease.target == SongLuaEaseTarget::Mod("drunk".into()) && ease.to > 0.5
        }));
        assert!(
            compiled
                .eases
                .iter()
                .any(|ease| { ease.target == SongLuaEaseTarget::PlayerRotationZ && ease.to > 5.0 })
        );
        assert!(compiled.column_offsets.iter().any(|window| {
            window.target == SongLuaColumnTransformTarget::PositionPoint { point: 0, axis: 1 }
                && window.to_y > 0.5
        }));
        let phase = compiled
            .overlays
            .iter()
            .position(|actor| actor.name.as_deref() == Some("MultitapP1_1"))
            .unwrap();
        assert!(compiled.overlay_updates.iter().any(|track| {
            track.overlay_index == phase
                && track.target == SongLuaOverlayUpdateTarget::Y
                && track.samples.iter().any(
                    |sample| matches!(sample.value, SongLuaOverlayUpdateValue::F32(y) if y > 10.0),
                )
        }));
        if !callback.is_empty() {
            assert!(compiled.overlay_updates.iter().any(|track| {
                track.overlay_index == phase && track.target == SongLuaOverlayUpdateTarget::Z
                    && track.samples.iter().any(|sample| {
                        matches!(sample.value, SongLuaOverlayUpdateValue::F32(z) if z > 15.0)
                    })
            }));
        }
    }
}

#[test]
fn compile_song_lua_loadfile_accepts_existing_song_dir_relative_path() {
    let root =
        PathBuf::from("target").join(format!("song-lua-relative-loadfile-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("lua")).unwrap();
    fs::write(
        root.join("lua/default.lua"),
        r#"
local path = GAMESTATE:GetCurrentSong():GetSongDir() .. "lua/"
loadfile(path .. "helper.lua")()
return Def.ActorFrame {}
"#,
    )
    .unwrap();
    fs::write(
        root.join("lua/helper.lua"),
        "relative_loadfile_helper_ran = true\n",
    )
    .unwrap();

    let entry = root.join("lua/default.lua");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&root, "Relative Loadfile"),
    )
    .expect("loadfile should accept an existing relative path from GetSongDir");
}

#[test]
fn compile_song_lua_loadfile_inherits_lua51_env_table() {
    let root = test_dir("loadfile-lua51-env");
    let entry = root.join("default.lua");
    fs::write(
        &entry,
        r#"
local _ENV = {
    _G = _G,
    Def = Def,
}

function _ENV.loadfile(path)
    local chunk, err = _G.loadfile(path)
    if err then error(err, 2) end
    if setfenv then setfenv(chunk, _ENV) end
    return chunk()
end

function _ENV.module(name)
    return name
end

return Def.ActorFrame{
    InitCommand = function()
        _ENV.loadfile("child.lua")
    end,
}
"#,
    )
    .unwrap();
    fs::write(
        root.join("child.lua"),
        r#"assert(module("helpers") == "helpers")"#,
    )
    .unwrap();

    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&root, "Lua 5.1 Environment"),
    )
    .expect("loadfile should preserve a Lua 5.1-style environment table");
}

#[test]
fn compile_song_lua_supports_generated_runtime_modchart() {
    let root = test_dir("generated-runtime-modchart");
    write_easing_lib(&root);
    let entry = root.join("default.lua");
    fs::write(&entry, generated_runtime_mod_lua()).unwrap();

    let mut context = SongLuaCompileContext::new(&root, "Generated Runtime Modchart");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::C(516.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.beat_mods.is_empty());
    assert!(!compiled.time_mods.is_empty());
    assert!(compiled.eases.len() >= 8);
    assert!(
        compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy1")
        )
    );
    assert!(
        compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy4")
        )
    );
    assert!(
        compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "hidden")
        )
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "beat"))
    );
}

#[test]
fn compile_song_lua_supports_player_function_ease_fixture() {
    let (root, entry) = song_lua_fixture("player-eases.lua");

    let mut context = SongLuaCompileContext::new(&root, "Player Ease Fixture");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::C(516.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 2);
    assert_eq!(compiled.overlays.len(), 3);
    assert_eq!(compiled.eases.len(), 2);
    assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.easing.as_deref() == Some("outCirc"))
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.easing.as_deref() == Some("outExpo"))
    );
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerRotationZ))
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerSkewX))
    );
}

#[test]
fn compile_song_lua_supports_named_mod_ease_fixture() {
    let (root, entry) = song_lua_fixture("mod-eases.lua");

    let mut context = SongLuaCompileContext::new(&root, "Named Mod Ease Fixture");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Easy,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.time_mods.is_empty());
    assert_eq!(compiled.eases.len(), 11);
    assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.easing.as_deref() == Some("outCirc"))
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.easing.as_deref() == Some("inCirc"))
    );
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "tiny"))
    );
}

#[test]
fn compile_song_lua_preserves_runtime_mod_targets_from_fixture() {
    let (root, entry) = song_lua_fixture("mod-eases.lua");

    let mut context = SongLuaCompileContext::new(&root, "Runtime Mod Target Fixture");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Hard,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Hard,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.eases.len(), 11);
    assert_eq!(compiled.overlays.len(), 1);
    assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
    for target in [
        "tiny",
        "drunk",
        "tipsy",
        "brake",
        "beat",
        "stealth",
        "movey1",
        "confusionoffset1",
    ] {
        assert!(
            compiled.eases.iter().any(
                |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == target)
            ),
            "missing fixture runtime mod target {target}"
        );
    }
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_captures_double_column_bounce_fixture() {
    let (root, entry) = song_lua_fixture("column-bounces.lua");

    let mut context = SongLuaCompileContext::new(&root, "Column Bounce Fixture");
    context.style_name = "double".to_string();
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.column_offsets.is_empty());
    assert!(compiled.column_offsets.iter().any(|window| {
        window.player == 0
            && window.column == 7
            && (window.from_y - 33.75).abs() <= 0.001
            && window.to_y.abs() <= 0.001
            && window.easing.as_deref() == Some("outSine")
    }));
    assert!(
        compiled
            .column_offsets
            .iter()
            .any(|window| window.player == 0
                && window.column == 6
                && window.from_y.abs() <= 0.001
                && (window.to_y - 33.75).abs() <= 0.001)
    );
}

#[test]
fn compile_song_lua_supports_basic_mod_overlay_fixture() {
    let (root, entry) = song_lua_fixture("basic.lua");

    let mut context = SongLuaCompileContext::new(&root, "Basic Mod Overlay Fixture");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.beat_mods.is_empty());
    assert!(!compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_supports_elastic_mod_ease_fixture() {
    let (root, entry) = song_lua_fixture("mod-eases.lua");

    let mut context = SongLuaCompileContext::new(&root, "Elastic Mod Ease Fixture");
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Easy,
            speedmod: SongLuaSpeedMod::X(1.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = test_compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.time_mods.is_empty());
    assert_eq!(compiled.eases.len(), 11);
    assert!(compiled.eases.iter().all(|ease| ease.easing.is_some()));
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| ease.easing.as_deref() == Some("outElastic"))
    );
    assert!(
        compiled.eases.iter().any(
            |ease| matches!(ease.target, SongLuaEaseTarget::Mod(ref name) if name == "Bumpy1")
        )
    );
}

#[test]
fn compile_song_lua_supports_overlay_fixture() {
    let (root, entry) = song_lua_fixture("basic.lua");

    let compiled = test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&root, "Overlay Fixture"),
    )
    .unwrap();
    assert!(!compiled.overlays.is_empty());
}

fn test_compiled_sound_calls(compiled: &TestCompiledSongLua) -> Vec<(f32, PathBuf)> {
    let mut calls = Vec::new();
    for actor in &compiled.overlays {
        let SongLuaOverlayKind::Sound { sound_path } = &actor.kind else {
            continue;
        };
        for command in &actor.message_commands {
            for event in compiled
                .messages
                .iter()
                .filter(|event| event.message == command.message)
            {
                for block in &command.blocks {
                    if block.delta.sound_play == Some(true) {
                        assert_eq!(block.start, 0.0, "Sound:play is not a tween property");
                        calls.push((event.beat, sound_path.clone()));
                    }
                }
            }
        }
    }
    calls.sort_by(|a, b| a.0.total_cmp(&b.0));
    calls
}

#[test]
fn sprite_methods_match_native() {
    for name in ["sprite-load", "texture-path"] {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro");
        let input: serde_json::Value = serde_json::from_slice(
            &fs::read(fixtures.join(format!("{name}-input.json"))).expect("native Sprite input"),
        )
        .expect("native Sprite JSON");
        let body = input["lua_assertions"]
            .as_str()
            .expect("native Sprite assertions")
            .split_once('\n')
            .expect("fixture path boundary")
            .1;
        let script = fs::read_to_string(fixtures.join(format!("{name}.lua")))
            .expect("runtime Sprite assertions")
            .replace("\r\n", "\n");
        assert!(
            script.contains(body),
            "unchanged compiled native assertions"
        );
        let song_dir = test_dir(name);
        fs::copy(
            fixtures.join("sprite-load.png"),
            song_dir.join("fit-rect.png"),
        )
        .expect("copy native Sprite image");
        let entry = song_dir.join("default.lua");
        fs::write(&entry, script).expect("write runtime Sprite control");
        test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Native Sprite loading"),
        )
        .expect("native Sprite size, texture, validation and return behavior");
    }
}

#[test]
fn runtime_actor_contracts() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    for name in ["actor-string", "bitmap-bools"] {
        let input: serde_json::Value = serde_json::from_slice(
            &fs::read(fixtures.join(format!("{name}-input.json")))
                .expect("native runtime actor input"),
        )
        .expect("native runtime actor JSON");
        let body = input["lua_assertions"]
            .as_str()
            .expect("native runtime actor assertions");
        let script = fs::read_to_string(fixtures.join(format!("{name}.lua")))
            .expect("runtime actor assertions")
            .replace("\r\n", "\n");
        assert!(script.contains(body), "unchanged native assertions: {name}");
        let song_dir = test_dir(name);
        let entry = song_dir.join("default.lua");
        fs::write(&entry, script).expect("write runtime actor control");
        test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Native runtime actor contracts"),
        )
        .expect("native actor identity and strict BitmapText boolean arguments");
    }
}

#[test]
fn proxy_methods_match_native() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let input: serde_json::Value = serde_json::from_slice(
        &fs::read(fixtures.join("proxy-methods-input.json")).expect("native proxy input"),
    )
    .expect("native proxy JSON");
    let body = input["lua_assertions"]
        .as_str()
        .expect("native proxy Lua assertions");
    let song_dir = test_dir("native-proxy-methods");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, format!(
            "native_proxy=Def.ActorProxy{{}}\nnative_target=Def.Actor{{}}\n{body}\nreturn Def.ActorFrame{{native_proxy,native_target}}\n"
        )).expect("unchanged native proxy assertions");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native proxy methods"),
    )
    .expect("native proxy inventory, target identity and live inheritance");
}

#[test]
#[cfg(feature = "test-support")]
fn step_selection_matches_native() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("step-selection-native.json"))
            .expect("read native selection capture"),
    )
    .expect("native selection JSON");
    for native in expected["controls"].as_array().expect("native controls") {
        let kind = native["kind"].as_str().expect("control name");
        // RSSP currently loads single/double simfiles only. The routine
        // API control supplies the same chart metadata through the public
        // compile context; it does not certify routine simfile loading.
        let song = deadsync_simfile::song::parse_song_meta_file(
            &root.join("step-selection-single.ssc"),
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0,
            |_| 0.0,
        )
        .expect("parse native selection control");
        let mut context = SongLuaCompileContext::new(&root, "Native step selection");
        context.song_steps = song.charts.iter().map(crate::SongLuaStep::from).collect();
        if kind == "routine" {
            for steps in &mut context.song_steps {
                steps.steps_type = "StepsType_Dance_Routine".into();
            }
        }
        context.style_name = native["style"].as_str().expect("native style").into();
        let selected = native["initial_step_index"]
            .as_u64()
            .expect("native initial selection") as usize;
        context.player_step_indices = [Some(selected); crate::LUA_PLAYERS];
        for player in &mut context.players {
            player.difficulty = context.song_steps[selected].difficulty;
        }
        context.music_length_seconds = 1.0;
        let compiled = test_compile_song_lua(&root.join("step-selection.lua"), &context)
            .expect("native selection script");
        // On broadcasts are absorbed into the startup snapshot. Audit the
        // calls themselves rather than looking for scheduled messages.
        let snapshots: Vec<_> = compiled
            .state_calls
            .iter()
            .filter(|call| call.operation == "MessageManager.Broadcast")
            .filter_map(|call| call.args.first().and_then(serde_json::Value::as_str))
            .filter(|message| message.contains(':'))
            .collect();
        let native_snapshots: Vec<_> = native["snapshots"]
            .as_array()
            .expect("native snapshots")
            .iter()
            .map(|value| value.as_str().expect("native message"))
            .collect();
        assert_eq!(
            snapshots, native_snapshots,
            "{kind} native state and synchronous messages: {:?}",
            compiled.info
        );
        #[cfg(feature = "test-support")]
        assert_eq!(
            compiled
                .state_calls
                .iter()
                .filter(|call| call.operation == "GameState.SetCurrentSteps")
                .map(|call| &call.args)
                .collect::<Vec<_>>(),
            native["calls"]
                .as_array()
                .expect("native calls")
                .iter()
                .map(|call| call.as_array().expect("native arguments"))
                .collect::<Vec<_>>(),
            "{kind} native call order"
        );
    }
}

#[test]
fn song_steps_match_native() {
    use crate::{LUA_PLAYERS, SongLuaStep};
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let song = deadsync_simfile::song::parse_song_meta_file(
        &root.join("song-steps.sm"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 0.0,
    )
    .expect("parse native metadata control");
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("song-steps-native.json")).expect("read native values"),
    )
    .expect("native values JSON");
    let steps: Vec<SongLuaStep> = song.charts.iter().map(SongLuaStep::from).collect();
    let native = expected["charts"].as_array().expect("native chart list");
    assert_eq!(steps.len(), native.len());
    for (actual, expected) in steps.iter().zip(native) {
        assert_eq!(
            u64::from(actual.difficulty.sort_key()),
            expected["difficulty_code"]
        );
        assert_eq!(actual.description, expected["description"]);
        assert_eq!(actual.credit, expected["credit"]);
        assert_eq!(actual.meter, expected["meter"]);
    }
    let dir = test_dir("cardstone-native-branch");
    let entry = dir.join("default.lua");
    fs::write(&entry, r#"
            assert(GAMESTATE:IsEventMode())
            local all = GAMESTATE:GetCurrentSong():GetStepsByStepsType('StepsType_Dance_Single')
            assert(#all == 8)
            if not NATIVE_BRANCH_SELECTED then assert(all[1] == GAMESTATE:GetCurrentSteps(PLAYER_1)) end
            return Def.ActorFrame{OnCommand=function(self)
                if not NATIVE_BRANCH_SELECTED then
                    NATIVE_BRANCH_SELECTED = true
                    local r1 = math.random(1, #all)
                    local r2 = math.random(1, #all - 1)
                    if r2 >= r1 then r2 = r2 + 1 end
                    GAMESTATE:SetCurrentSteps(PLAYER_1, all[r1])
                    GAMESTATE:SetCurrentSteps(PLAYER_2, all[r2])
                    SCREENMAN:SetNewScreen("ScreenGameplay")
                end
                mod_actions={{1, GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty() .. ':' ..
                    GAMESTATE:GetCurrentSteps(PLAYER_2):GetDifficulty(), true}}
            end}
        "#).expect("write native branch control");
    let mut context = SongLuaCompileContext::new(&dir, "native selected charts");
    context.song_steps = steps;
    context.player_step_indices = [Some(0); LUA_PLAYERS];
    let compiled = test_compile_song_lua(&entry, &context).expect("native event-mode branch");
    assert_eq!(
        compiled.messages[0].message,
        "Difficulty_Easy:Difficulty_Edit"
    );
    #[cfg(feature = "test-support")]
    assert_eq!(
        compiled
            .state_calls
            .iter()
            .filter(|call| call.operation != "MessageManager.Broadcast")
            .map(|call| &call.args)
            .collect::<Vec<_>>(),
        expected["seeded_calls"]
            .as_array()
            .expect("native state calls")
            .iter()
            .map(|call| call.as_array().expect("native call arguments"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn native_song_rate_text() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro/song-options-string");
    let native: serde_json::Value = serde_json::from_slice(
        &fs::read(fixtures.join("native.json")).expect("native song options trace"),
    )
    .expect("valid native song options trace");
    let messages = native["message_dispatches"]
        .as_array()
        .expect("native message observations");
    let rates = [1.0, 1.25, 1.2, 2.0, 1.234, 1.005];
    assert_eq!(messages.len(), rates.len());
    let song_dir = test_dir("native-song-rate-text");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
mod_actions = {
    {1, "SongOptionsString:" .. GAMESTATE:GetSongOptionsString(), true},
    {2, "SongOptionsString:" .. GAMESTATE:GetSongOptions("ModsLevel_Song"), true},
}
return Def.ActorFrame{}
"#,
    )
    .expect("write song options query");
    for (rate, observed) in rates.into_iter().zip(messages) {
        let mut context = SongLuaCompileContext::new(&song_dir, "Native Song Rate Text");
        context.song_music_rate = rate;
        let compiled = test_compile_song_lua(&entry, &context).expect("compile song queries");
        assert_eq!(compiled.messages.len(), 2);
        let expected = observed["name"].as_str().expect("native option string");
        for message in &compiled.messages {
            assert_eq!(message.message, expected, "music rate {rate}");
        }
    }
}

#[test]
fn startup_reload_matches_native() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/itgmania-song-lua-micro/startup-reload-native.json",
    ))
    .expect("native reload snapshots");
    let song = deadsync_simfile::song::parse_song_meta_file(
        &fixtures.join("startup-reload.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 0.0,
    )
    .expect("reload chart metadata");
    let song_dir = test_dir("native-startup-reload");
    let entry = song_dir.join("reload.lua");
    fs::copy(fixtures.join("startup-reload.lua"), &entry).expect("unchanged native control");
    let observer = song_dir.join("observer.lua");
    fs::write(
        &observer,
        r#"
RELOAD_SNAPSHOTS = RELOAD_SNAPSHOTS or {}
return Def.ActorFrame {
    HarnessReloadControlMessageCommand = function(self, p)
        RELOAD_SNAPSHOTS[#RELOAD_SNAPSHOTS + 1] = string.format("%d:%s:%d:%d:%s",
            p.count, p.difficulty, p.possible, p.current_possible, p.top_name)
    end,
    OnCommand = function(self)
        if HARNESS_NATIVE_RELOAD_COUNT == 2 then
            mod_actions = {{1, table.concat(RELOAD_SNAPSHOTS, ";"), true}}
        end
    end,
}
"#,
    )
    .expect("reload observer");
    let mut context = SongLuaCompileContext::new(&song_dir, "Native reload");
    context.song_steps = song.charts.iter().map(crate::SongLuaStep::from).collect();
    let initial = context
        .song_steps
        .iter()
        .position(|steps| steps.difficulty == SongLuaDifficulty::Challenge)
        .expect("initial Challenge");
    context.player_step_indices = [Some(initial), None];
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    context.music_length_seconds = 1.0;
    let compiled = test_compile_song_lua_layers(&[&entry, &observer], 0, &context)
        .expect("native control reloads in the same Lua VM");
    let expected = native["snapshots"]
        .as_array()
        .expect("native startup snapshots")
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}:{}:{}",
                p["count"],
                p["difficulty"].as_str().expect("difficulty"),
                p["possible"],
                p["current_possible"],
                p["top_name"].as_str().expect("screen name")
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let easy = context
        .song_steps
        .iter()
        .position(|steps| steps.difficulty == SongLuaDifficulty::Easy)
        .expect("replacement Easy");
    assert!(
        compiled
            .iter()
            .flat_map(|layer| &layer.messages)
            .any(|event| event.message == expected),
        "{compiled:?}"
    );
    for layer in compiled {
        assert_eq!(layer.startup_reloads, 1);
        assert_eq!(layer.loaded_steps, Some([Some(easy), None]));
    }
}

#[test]
fn compile_song_lua_native_image_getters() {
    let song_dir = test_dir("native-image-getters");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro/texture-image-getters");
    let cases: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("cases.json")).expect("native image dimensions"),
    )
    .expect("captured native dimension cases");
    let entry = song_dir.join("default.lua");
    fs::copy(root.join("sheet 2x1.png"), song_dir.join("sheet 2x1.png"))
        .expect("replacement texture");
    for case in cases.as_array().expect("native cases") {
        let file = case["file"].as_str().expect("native source filename");
        fs::copy(root.join(file), song_dir.join(file)).expect("native image input");
        fs::write(
            &entry,
            format!(
                r#"
return Def.ActorFrame{{
    Def.Sprite{{Texture="{file}"}},
    Def.Sprite{{
    Texture="{file}",
    OnCommand=function(self)
        local texture = self:GetTexture()
        local sizes = string.format("%.0f:%.0f:%.0f:%.0f:%.0f:%.0f",
            texture:GetSourceWidth(), texture:GetSourceHeight(),
            texture:GetImageWidth(), texture:GetImageHeight(),
            texture:GetTextureWidth(), texture:GetTextureHeight())
        self:Load("sheet 2x1.png")
        mod_actions = {{{{1, sizes .. string.format(":%.0f:%.0f",
            texture:GetImageWidth(), texture:GetImageHeight()), true}}}}
    end,
    }},
}}
"#
            ),
        )
        .expect("texture getter Lua");
        let compiled = test_compile_song_lua(
            &entry,
            &SongLuaCompileContext::new(&song_dir, "Native Image Getters"),
        )
        .expect("compile native texture getters");
        let dims = &case["dimensions"];
        let expected = ["source", "image", "texture", "image"]
            .into_iter()
            .flat_map(|kind| dims[kind].as_array().expect("native dimension pair"))
            .map(|value| value.as_u64().expect("native dimension").to_string())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(compiled.messages.len(), 1, "{file}");
        assert_eq!(compiled.messages[0].message, expected, "{file}");
    }
}

#[test]
fn stage_points_match_native() {
    let native_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro/stage-points-native.json");
    let fixture: serde_json::Value = serde_json::from_slice(
        &fs::read(native_path).expect("captured native stage point control"),
    )
    .expect("native stage point JSON");
    let assertions = fixture["lua_assertions"]
        .as_str()
        .expect("native assertions");
    let song_dir = test_dir("native-stage-points");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        format!(
            "local function _ITG_STAGE_STATS(player)\n\
             return STATSMAN:GetCurStageStats():GetPlayerStageStats(player)\nend\n\
             {assertions}\nreturn Def.ActorFrame{{}}\n",
        ),
    )
    .expect("stage point control script");
    let mut context = SongLuaCompileContext::new(&song_dir, "Native stage points");
    context.players[1].enabled = true;
    test_compile_song_lua(&entry, &context).expect("same stage point assertions as native");
}

#[test]
fn stage_grade_matches_native() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/itgmania-song-lua-micro/stage-grade-native.json",
    ))
    .expect("native stage grade control");
    let assertions = fixture["lua_assertions"]
        .as_str()
        .expect("native grade assertions");
    let song_dir = test_dir("native-stage-grade");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        format!(
            "local function _ITG_STAGE_STATS(player)\n\
             return STATSMAN:GetCurStageStats():GetPlayerStageStats(player)\nend\n\
             {assertions}\nreturn Def.ActorFrame{{}}\n",
        ),
    )
    .expect("stage grade control script");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native grade"),
    )
    .expect("same empty and failed stage grades as native");
}

#[test]
fn queued_pulse_survives_startup_tweens() {
    let song_dir = test_dir("queued-pulse-tweens");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.Quad{
    OnCommand=function(self)
        self:linear(.5):diffusealpha(.5):sleep(.5)
        self:queuecommand("Pulse"):smooth(.5):rotationz(10)
        self:sleep(.5):queuecommand("Stronger"):linear(.5):rotationz(20)
        self:sleep(.5):queuecommand("Stop")
    end,
    PulseCommand=function(self)
        self:pulse():effectperiod(1):effectclock("bgm"):effectmagnitude(1, 1.015, 0)
    end,
    StrongerCommand=function(self) self:effectmagnitude(1, 1.04, 0) end,
    StopCommand=function(self) self:stopeffect() end,
}
"#,
    )
    .expect("write queued pulse fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Queued Pulse");
    context.music_length_seconds = 4.0;
    context.song_display_bpms = [60.0; 2];
    let compiled = test_compile_song_lua(&entry, &context).expect("compile queued pulse");
    let track = |target| {
        compiled
            .overlay_updates
            .iter()
            .find(|track| track.target == target)
            .expect("captured effect property")
    };
    let value = |target, time| {
        track(target)
            .samples
            .iter()
            .rev()
            .find(|sample| sample.time <= time)
            .expect("sample precedes probe")
            .value
            .clone()
    };
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as Value;
    // Actor::TweenState does not own the effect selector or its parameters.
    for time in [1.25, 1.75, 2.25, 2.75] {
        assert_eq!(
            value(Target::EffectMode, time),
            Value::EffectMode(EffectMode::Pulse)
        );
        assert_eq!(
            value(Target::EffectClock, time),
            Value::EffectClock(EffectClock::Beat)
        );
        assert_eq!(value(Target::EffectTimer, time), Value::Bool(false));
        assert_eq!(
            value(Target::EffectMagnitude, time),
            Value::Vec3([1.0, if time < 2.0 { 1.015 } else { 1.04 }, 0.0])
        );
    }
    assert_eq!(
        value(Target::EffectMode, 3.25),
        Value::EffectMode(EffectMode::None)
    );
}

#[test]
fn loadactor_redirects() {
    let song_dir = test_dir("loadactor-redirects");
    let parts = song_dir.join("parts");
    fs::create_dir_all(&parts).unwrap();
    fs::write(song_dir.join("alias.redir"), "parts/bridge\r\n").unwrap();
    fs::write(parts.join("bridge.redir"), "child").unwrap();
    fs::write(parts.join("child.lua"), "local width = ...; return Def.Quad{InitCommand=function(self) self:setsize(width, 17) end}").unwrap();
    fs::write(parts.join("audio.redir"), "tone").unwrap();
    fs::write(parts.join("tone.ogg"), b"not decoded during compile").unwrap();
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        "return Def.ActorFrame{LoadActor('alias', 23), LoadActor('parts/audio')}",
    )
    .unwrap();
    let compiled =
        test_compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Redirects")).unwrap();
    assert_eq!(compiled.overlays[0].initial_state.size, Some([23.0, 17.0]));
    assert_eq!(compiled.sound_paths, vec![parts.join("tone.ogg")]);

    fs::write(parts.join("loop.redir"), "loop").unwrap();
    fs::write(parts.join("empty.redir"), "").unwrap();
    fs::write(parts.join("missing.redir"), "absent").unwrap();
    fs::write(parts.join("ambiguous.redir"), "variant").unwrap();
    fs::write(parts.join("variant1.lua"), "return {}").unwrap();
    fs::write(parts.join("variant2.lua"), "return {}").unwrap();
    let mut host = deadsync_song_lua::rust_engine::RustSongRuntime::new(
        &SongLuaCompileContext::new(&song_dir, "Redirects"),
    )
    .unwrap();
    // The retired resolve_load_actor_path helper invented an "empty redirect"
    // error. The frozen native ActorUtil control resolves it to an empty path;
    // LoadActor then rejects that path using its ordinary invalid-file policy.
    let empty_path = song_dir.join("parts/empty");
    let vm = host.vm_mut();
    let deadlib_lua::Value::Table(utility) = vm.get_global(b"ActorUtil").unwrap() else {
        panic!("ActorUtil table")
    };
    let key = vm.try_intern(b"ResolvePath").unwrap();
    let resolve = vm.raw_table_get(utility, key).unwrap();
    let path = vm
        .try_intern(file_path_string(&empty_path).as_bytes())
        .unwrap();
    let deadlib_lua::Value::String(resolved) = vm
        .call(resolve, &[path, deadlib_lua::Value::Number(1.0)])
        .unwrap()[0]
    else {
        panic!("empty native redirect path")
    };
    assert_eq!(vm.string_bytes(resolved).unwrap(), b"");
    let error = host.load_actor(&empty_path, &[]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown file type (FileType_Invalid)"),
        "{error}"
    );
    for (name, expected) in [
        ("loop", "circular redirect"),
        ("missing", "not found"),
        ("ambiguous", "multiple matches"),
    ] {
        let error = host
            .load_actor(&song_dir.join(format!("parts/{name}")), &[])
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn bitmap_methods_match_native() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let input: serde_json::Value = serde_json::from_slice(
        &fs::read(fixtures.join("bitmap-methods-input.json")).expect("native bitmap input"),
    )
    .expect("native bitmap JSON");
    let body = input["lua_assertions"]
        .as_str()
        .expect("native bitmap assertions");
    // Check the native inventory before the fallback theme adds Lua helpers.
    let (inventory, assertions) = body
        .split_once("assert(type(BitmapText.GetX)")
        .expect("native inventory boundary");
    let mut host = native_actor_inventory::runtime();
    native_actor_inventory::execute(&mut host, inventory);
    let song_dir = test_dir("native-bitmap-methods");
    let entry = song_dir.join("default.lua");
    fs::write(&entry, format!(
            "native_label=Def.BitmapText{{}}\nassert(type(BitmapText.GetX){assertions}\nreturn Def.ActorFrame{{native_label}}\n"
        )).expect("unchanged native bitmap assertions");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native bitmap methods"),
    )
    .expect("native bitmap inventory, inheritance and boolean conversion");
}

#[test]
fn actor_classes_match_native() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let body = |name: &str| {
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(fixtures.join(name)).expect("native actor control input"),
        )
        .expect("native actor control JSON");
        value["aft_creation"]
            .as_str()
            .expect("native assertion body")
            .to_owned()
    };
    // The exact raw native inventory is checked before fallback Lua helpers.
    let mut host = native_actor_inventory::runtime();
    native_actor_inventory::execute(&mut host, body("actor-class-inventory-input.json"));

    let song_dir = test_dir("native-actor-classes");
    let entry = song_dir.join("default.lua");
    let script = format!(
        "frozen=Def.ActorFrameTexture{{}}\n{}\n{}\n{}\n{}\n{}\nreturn Def.ActorFrame{{frozen}}\n",
        body("actor-class-inheritance-input.json"),
        body("actor-class-override-input.json"),
        body("actor-class-callback-input.json"),
        body("actor-class-multi-return-input.json"),
        body("actor-class-wrapper-input.json")
    );
    fs::write(&entry, script).expect("unchanged native class controls");
    test_compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Native actor classes"),
    )
    .expect("native inheritance and live method replacement");
}
