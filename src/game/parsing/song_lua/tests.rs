use super::{
    EffectClock, EffectMode, GRAPH_DISPLAY_VALUE_RESOLUTION, SONG_LUA_INITIAL_LIFE,
    SONG_LUA_STARTUP_MESSAGE, SongLuaCompileContext, SongLuaDifficulty, SongLuaEaseTarget,
    SongLuaOverlayBlendMode, SongLuaOverlayKind, SongLuaPlayerContext, SongLuaProxyTarget,
    SongLuaSpanMode, SongLuaSpeedMod, SongLuaTextGlowMode, SongLuaTimeUnit, THEME_RECEPTOR_Y_STD,
    compile_song_lua, file_path_string,
};
use crate::engine::present::actors::TextAlign;
use chrono::{Datelike, Local};
use std::fs;
use std::path::PathBuf;

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("deadsync-song-lua-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn generated_runtime_mod_lua() -> &'static str {
    r#"
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
fn compile_song_lua_reads_mod_tables() {
    let song_dir = test_dir("direct");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Test Song")).unwrap();
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
            .all(|window| window.from == 10.0 && window.to == 10.0)
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Perframe Side Effects"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    assert!(compiled.eases.is_empty());
    assert!(compiled.overlay_eases.is_empty());
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

    let compiled = compile_song_lua(
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
fn compile_song_lua_runs_actor_init_commands() {
    let song_dir = test_dir("init-command");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
    Def.Actor{
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Def Actor Message Command"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "WorkerPulse");
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::Actor
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "PlayerNumber_P1");
}

#[test]
fn compile_song_lua_exposes_player_noteskin_name() {
    let song_dir = test_dir("player-noteskin");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
if string.lower(po:NoteSkin()) ~= "cyber" then
    error("unexpected NoteSkin getter: " .. tostring(po:NoteSkin()))
end
po:NoteSkin("lambda")
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "lambda");
}

#[test]
fn compile_song_lua_exposes_noteskin_helpers() {
    let song_dir = test_dir("noteskin-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local x = NOTESKIN:GetMetricF("", "TapNoteNoteColorTextureCoordSpacingX")
local y = NOTESKIN:GetMetricFForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", "cyber")
local xi = NOTESKIN:GetMetricI("", "TapNoteNoteColorTextureCoordSpacingX")
local yi = NOTESKIN:GetMetricIForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", "cyber")
local vivid = NOTESKIN:GetMetricBForNoteSkin("", "TapNoteAnimationIsVivid", "cyber")
local metric_cmd = NOTESKIN:GetMetricA("", "OnCommand")
local metric_cmd_for_skin = NOTESKIN:GetMetricAForNoteSkin("", "OnCommand", "cyber")
local path = NOTESKIN:GetPathForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")
local actor = NOTESKIN:LoadActorForNoteSkin("Down", "Tap Explosion Bright W1", "cyber")
local default_path = NOTESKIN:GetPath("Down", "Tap Explosion Bright W1")
local default_actor = NOTESKIN:LoadActor("Down", "Tap Explosion Bright W1")
local metric_actor = Def.Actor{Name="MetricActor"}

if math.abs(x - 0.125) > 0.0001 then
    error("unexpected noteskin metric x: " .. tostring(x))
end
if math.abs(y - 0.0) > 0.0001 then
    error("unexpected noteskin metric y: " .. tostring(y))
end
if xi ~= 0 or yi ~= 0 then
    error("unexpected noteskin integer metrics: " .. tostring(xi) .. ":" .. tostring(yi))
end
if vivid ~= false then
    error("unexpected noteskin vivid flag: " .. tostring(vivid))
end
if NOTESKIN:DoesNoteSkinExist("cyber") ~= true then
    error("expected cyber noteskin to exist")
end
if metric_cmd(metric_actor) ~= metric_actor or metric_cmd_for_skin(metric_actor) ~= metric_actor then
    error("expected noteskin actor-command metrics to preserve actors")
end
if type(path) ~= "string" or path == "" then
    error("expected noteskin path")
end
if type(default_path) ~= "string" or default_path == "" then
    error("expected default noteskin path")
end
if type(actor) ~= "table" or type(default_actor) ~= "table" then
    error("expected noteskin actor table")
end
if #NOTESKIN:GetNoteSkinNames(false) < 1 then
    error("expected noteskin names")
end

mod_actions = {
    {4, tostring(vivid) .. ":" .. tostring(x) .. ":" .. tostring(xi), true},
}

return Def.ActorFrame{
    actor..{
        Name="NoteskinExplosion",
    },
}
"#,
    )
    .unwrap();

    let mut context = SongLuaCompileContext::new(&song_dir, "Noteskin Helpers");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "false:0.125:0");
    assert!(
        compiled.overlays.iter().any(|overlay| {
            overlay.name.as_deref() == Some("NoteskinExplosion")
                && matches!(overlay.kind, SongLuaOverlayKind::Sprite { .. })
        }),
        "noteskin actor should materialize as a sprite overlay when it resolves to an image"
    );
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Noteskin Concat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ConcatNoteskin");
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BitmapText")).unwrap();
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Model Base Rotation Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ring_model.txt:ring_model.txt:ring_model.txt:-60:20:50"
    );
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_extracts_model_overlay_layers() {
    let song_dir = test_dir("model-overlay-layers");
    let entry = song_dir.join("default.lua");
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets/graphics/menu_bg_technique/ring_model.txt");
    let model_path = model_path.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        &entry,
        format!(
            r#"
return Def.ActorFrame{{
    Def.Model{{
        Meshes="{model_path}",
        Materials="{model_path}",
        Bones="{model_path}",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
                :zoom(0.75)
                :baserotationx(-60)
                :baserotationy(20)
                :baserotationz(50)
                :diffuse(1, 1, 1, 0.8)
        end,
    }},
}}
"#
        ),
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Model Overlay Layers"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::Model { layers } = &compiled.overlays[0].kind else {
        panic!("expected Model overlay");
    };
    assert!(!layers.is_empty());
    assert!(layers.iter().all(|layer| !layer.vertices.is_empty()));
    assert!(layers.iter().all(|layer| !layer.texture_key.is_empty()));
    assert!(layers[0].model_size[0] > 0.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_x_deg, -60.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_y_deg, 20.0);
    assert_eq!(compiled.overlays[0].initial_state.rot_z_deg, 50.0);
}

#[test]
fn compile_song_lua_supports_bitmap_text_style_shims() {
    let song_dir = test_dir("bitmap-text-style-shims");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.BitmapText{
        Font="Common Normal",
        Text="STYLE",
        OnCommand=function(self)
            self:_wrapwidthpixels(88)
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "BitmapText Style Shims"),
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Color Helpers"),
    )
    .unwrap();
    assert!(compiled.overlays.is_empty());
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

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
mod_actions = {
    {1, string.format("%d:%d", math.mod(5, 2), table.getn(values)), true},
}

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Lua51 Stdlib Aliases"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1:3");
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "IValues")).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "60");
}

#[test]
fn compile_song_lua_accepts_diffusecolor_alias() {
    let song_dir = test_dir("diffusecolor-alias");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self)
            self:diffusecolor(0.85, 0.92, 0.99, 0.7)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "DiffuseColor Alias"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(
        compiled.overlays[0].initial_state.diffuse,
        [0.85, 0.92, 0.99, 0.7]
    );
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "false:false:PlayerNumber_P1:dance:1:1:CoinMode_Free:Premium_Off:Challenge:true:Common:true:Player 1:0"
    );
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Manager Compat"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2:Simply Love:Simply Love:0:false:1:1:1:1:1:Theme Manager Compat:true:0:1:0:Grade_Tier07:Player:1"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Enum SongUtil"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1:3:PlayerNumber_P2:6:6:93.46%:Free Play"
    );
}

#[test]
fn compile_song_lua_exposes_fallback_theme_utility_helpers() {
    let song_dir = test_dir("theme-utility-helpers");
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1.779:200.0:3:2:05|01:05|1:05.50|01:05.50|01:01:01|1st|2nd|3rd|11th|113th"
    );
}

#[test]
fn compile_song_lua_exposes_theme_process_compat_helpers() {
    let song_dir = test_dir("theme-process-helpers");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local top = SCREENMAN:GetTopScreen()
local sound = LoadActor(THEME:GetPathS("", "Common invalid.ogg"))
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

return Def.ActorFrame{}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Process Helpers"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ScreenGameplay:ScreenEvaluationStage:ScreenGameplay:default:true:false:15:300:Cancel:__songlua_theme_path:0"
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
            "%s:%s:%s:%s:%s:%s:%.0f:%s",
            top:GetName(),
            top:GetPrevScreenName(),
            top:GetNextScreenName(),
            tostring(top:IsPaused()),
            tostring(top:AllAreOnLastRow()),
            tostring(wheel:IsLocked()),
            top:GetChild("Timer"):GetSeconds(),
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Screen Process Shims"),
    )
    .unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "ScreenTextEntry:ScreenSelectMusic:ScreenGameplay:true:false:false:0:true"
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

    let compiled = compile_song_lua(
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

    compile_song_lua(
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

    let compiled = compile_song_lua(
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
    Def.Quad{
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor, dt)
                actor:x(dt * 60):y(actor:GetParent():GetUpdateRate())
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction Delta Song"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "3:3");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 3.0);
    assert_eq!(compiled.overlays[0].initial_state.y, 3.0);
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
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
                actor:queuecommand("Pulse")
            end)
        end,
        PulseCommand=function(self)
            self:x(12)
            mod_actions = {
                {1, "update-queued", true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SetUpdateFunction QueueCommand Song"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "update-queued");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 12.0);
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(
        compiled.overlay_eases.iter().any(|ease| {
            ease.overlay_index == 0 && ease.from.x.is_some() && ease.to.x.is_some()
        })
    );
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == 0 && ease.from.rot_z_deg.is_some() && ease.to.rot_z_deg.is_some()
    }));
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == 0 && ease.from.visible.is_some() && ease.to.visible.is_some()
    }));
}

#[test]
fn compile_song_lua_clears_update_function_with_nil() {
    let song_dir = test_dir("set-update-function-clear");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        OnCommand=function(self)
            self:SetUpdateFunction(function(actor)
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Local Update Mod Actions"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].beat, 2.0);
    let block = &compiled.player_actors[0].message_commands[0].blocks[0];
    assert_eq!(block.delta.x, Some(344.0));
    assert_eq!(block.delta.z, Some(3.0));
    assert_eq!(block.delta.zoom, Some(0.5));
    assert_eq!(block.delta.rot_z_deg, Some(20.0));
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Recursive Update Song"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].beat, 1.0);
    assert_eq!(compiled.messages[0].message, "LoopSafe");
}

#[test]
fn compile_song_lua_classifies_player_transform_function_eases() {
    let song_dir = test_dir("function-ease");
    let entry = song_dir.join("default.lua");
    fs::write(
            &entry,
            r#"
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

    let compiled = compile_song_lua(
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Overlay")).unwrap();
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
    assert_eq!(
        overlay.initial_state.stretch_rect,
        Some([0.0, 0.0, 640.0, 480.0])
    );
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

    let compiled = compile_song_lua(
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
        Some("inBounce")
    );
    assert_eq!(overlay.message_commands[0].blocks[0].duration, 0.2);
    assert_eq!(
        overlay.message_commands[0].blocks[0].delta.diffuse.unwrap()[3],
        0.5
    );
    assert_eq!(
        overlay.message_commands[0].blocks[1].easing.as_deref(),
        Some("outBounce")
    );
    assert_eq!(overlay.message_commands[0].blocks[1].start, 0.2);
    assert_eq!(overlay.message_commands[0].blocks[1].duration, 0.25);
    assert_eq!(
        overlay.message_commands[0].blocks[1].delta.diffuse.unwrap()[3],
        1.0
    );
    assert_eq!(
        overlay.message_commands[0].blocks[2].easing.as_deref(),
        Some("outElastic")
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Tween Time Left"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0.00:0.50:0.75:0.00:0.125");
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 10.0);
    assert_eq!(compiled.overlays[0].initial_state.diffuse[3], 0.5);
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Hurrytweening Timeline"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Hurry");
    assert_eq!(command.blocks.len(), 2);
    assert_eq!(command.blocks[0].start, 0.5);
    assert_eq!(command.blocks[0].duration, 0.5);
    assert_eq!(command.blocks[0].delta.x, Some(20.0));
    assert_eq!(command.blocks[1].start, 1.0);
    assert_eq!(command.blocks[1].duration, 1.0);
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
                string.format("%d:%s:%d", count, panel and panel:GetName() or "nil", type(lines) == "table" and #lines or 0),
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Children"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "2:Panel:2");
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Broadcast Startup"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].initial_state.x, 42.0);
    assert!(compiled.overlays[0].initial_state.visible);
    assert_eq!(compiled.overlays[0].initial_state.size, Some([12.0, 18.0]));
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    let overlay = &compiled.overlays[0];

    assert_eq!(compiled.screen_width, 854.0);
    assert_eq!(compiled.screen_height, 480.0);
    assert_eq!(overlay.initial_state.x, 427.0);
    assert_eq!(overlay.initial_state.y, 240.0);
    assert_eq!(
        overlay.initial_state.stretch_rect,
        Some([0.0, 0.0, 854.0, 480.0])
    );
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "854:480:true:true");
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

return Def.ActorFrame{
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

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
    context.players[0].display_bpms = [150.0, 150.0];
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:2:180:false:1:150");
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
    context.song_display_bpms = [90.0, 180.0];
    context.players[0].difficulty = SongLuaDifficulty::Hard;
    context.players[0].display_bpms = [150.0, 210.0];
    context.music_length_seconds = 123.4;
    let compiled = compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "Song Metadata|Song Metadata||Pack A|true|true|true|true|true|1|true|true|true|6|10|123.4|0|90|180|150"
    );
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Song Steps Type Selectors"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "6:6:0:Difficulty_Hard:10:Difficulty_Edit:true:true:true:true"
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.25xMusic");
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
        GAMESTATE:SetCurrentSteps(PLAYER_1, steps[2])
        SCREENMAN:SetNewScreen("ScreenGameplay")
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Save Your Tears Compat"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "6:1x, Overhead");
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sound Actor Assets"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
        panic!("expected sound overlay");
    };
    assert_eq!(sound_path, &song_dir.join("hit.ogg"));
    assert!(
        compiled
            .messages
            .iter()
            .any(|event| event.message == SONG_LUA_STARTUP_MESSAGE && event.beat == 0.0)
    );
    let startup = compiled.overlays[0]
        .message_commands
        .iter()
        .find(|command| command.message == SONG_LUA_STARTUP_MESSAGE)
        .expect("expected startup sound command");
    assert_eq!(startup.blocks[0].delta.sound_play, Some(true));
    let ding = compiled.overlays[0]
        .message_commands
        .iter()
        .find(|command| command.message == "Ding")
        .expect("expected Ding sound command");
    assert_eq!(ding.blocks[0].start, 0.25);
    assert_eq!(ding.blocks[0].delta.sound_play, Some(true));
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
    context.song_display_bpms = [120.0, 180.0];
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    context.players[0].display_bpms = [200.0, 240.0];
    context.players[1].difficulty = SongLuaDifficulty::Hard;
    let compiled = compile_song_lua(&entry, &context).unwrap();

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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Column Actors Alias"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "4");
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

    let compiled = compile_song_lua(
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
    context.global_offset_seconds = 0.02;
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "1.7778:1280:720:true:1.00:0.02"
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();

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
    let compiled = compile_song_lua(&entry, &context).unwrap();

    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "Medium");
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Sprite Dimensions"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "10:20");
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor NoExt Image"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:12:34");
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Video Media"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:true");
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "LoadActor Audio Media"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true:true");
    assert_eq!(compiled.overlays.len(), 1);
    let SongLuaOverlayKind::Sound { sound_path } = &compiled.overlays[0].kind else {
        panic!("expected sound overlay");
    };
    assert_eq!(sound_path, &song_dir.join("other.ogg"));
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Hibernate"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "0.25");
}

#[test]
fn compile_song_lua_captures_hibernate_visibility_window() {
    let song_dir = test_dir("actor-hibernate-visibility");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    Def.Quad{
        PulseMessageCommand=function(self)
            self:hibernate(0.5):diffusealpha(0.25)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Hibernate Visibility"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    let command = &compiled.overlays[0].message_commands[0];
    assert_eq!(command.message, "Pulse");
    assert_eq!(command.blocks.len(), 2);
    assert_eq!(command.blocks[0].start, 0.0);
    assert_eq!(command.blocks[0].duration, 0.0);
    assert_eq!(command.blocks[0].delta.visible, Some(false));
    assert_eq!(command.blocks[1].start, 0.5);
    assert_eq!(command.blocks[1].duration, 0.0);
    assert_eq!(command.blocks[1].delta.visible, Some(true));
    assert_eq!(command.blocks[1].delta.diffuse.unwrap()[3], 0.25);
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "640:360:1280:720");
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Additive Transforms"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "15:17:10:20:35:135");
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

    let compiled = compile_song_lua(
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
                    tostring(Sprite.LoadFromCachedBanner ~= nil),
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

    let compiled = compile_song_lua(
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
                string.format("%s:%s:%d:%d", self:GetName(), tostring(Sprite.LoadFromCachedBanner ~= nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
    Def.Sprite{
        InitCommand=function(self)
            Sprite.LoadFromCachedBackground(self, "background.png")
            mod_actions[#mod_actions + 1] = {
                1,
                string.format("%s:%d:%d", tostring(Sprite.LoadFromCachedBackground ~= nil), self:GetWidth(), self:GetHeight()),
                true,
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = compile_song_lua(
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
            texture:GetPath():match(name == "background" and "background%.png$" or "banner%.png$") ~= nil,
            sprite:GetWidth(),
            sprite:GetHeight(),
            tostring(Sprite.LoadFromSong ~= nil and Sprite.LoadFromCourse ~= nil and Sprite.LoadFromSongGroup ~= nil)
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

    let compiled = compile_song_lua(
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
                    tostring(Sprite.LoadFromSortOrder ~= nil and Sprite.SetScrolling ~= nil and Sprite.GetScrolling ~= nil and Sprite.GetPercentScrolling ~= nil)
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
        crate::engine::present::anim::EffectMode::GlowShift
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Vertex Diffuse Shims"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "ok");
    assert_eq!(compiled.overlays.len(), 1);
    let colors = compiled.overlays[0].initial_state.vertex_colors.unwrap();
    assert_eq!(colors[0], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(colors[1], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(colors[2], [1.0, 1.0, 0.0, 1.0]);
    assert_eq!(colors[3], [0.0, 0.0, 1.0, 1.0]);
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
            mod_actions = {{
                1,
                string.format("%s:%d:%d", self:GetDrawState().Mode, self:GetNumVertices(), self:GetLineWidth()),
                true,
            }}
        end,
    },
}
"##,
        )
        .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Multi Vertex Shape"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "DrawMode_Quads:4:3");
    assert_eq!(compiled.overlays.len(), 1);
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Actor Blink Effects"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);

    let diffuse = compiled.overlays[0].initial_state;
    assert_eq!(diffuse.effect_mode, EffectMode::DiffuseShift);
    assert_eq!(diffuse.effect_period, 0.25);
    assert_eq!(diffuse.effect_color1, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(diffuse.effect_color2, [1.0, 1.0, 1.0, 1.0]);

    let glow = compiled.overlays[1].initial_state;
    assert_eq!(glow.effect_mode, EffectMode::GlowShift);
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
            self:ScaleToClipped(10, 20)
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
                    self:GetAlpha() * 10,
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

    let compiled = compile_song_lua(
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "BaseZoom Z")).unwrap();
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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
                {1, string.format("%.0f:%.0f", self:GetWidth(), self:GetHeight()), true},
            }
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Zoomto Width Height"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "30:40");
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Math Round")).unwrap();
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Xero")).unwrap();
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

    let compiled = compile_song_lua(
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Fileman")).unwrap();
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
        local picked = ActorFrame.GetChildAt(self, 0)
        local picked_method = self:GetChildAt(0)
        local second = ActorFrame.GetChildAt(self, 1)
        local named = ActorFrame.GetChild(self, "child")
        local children = ActorFrame.GetChildren(self)
        local count_before = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveChild(self, "remove")
        local count_after = ActorFrame.GetNumChildren(self)
        ActorFrame.RemoveAllChildren(disposable)
        mod_actions = {
            {1, string.format(
                "%s:%s:%s:%s:%d:%d:%.0f:%.0f:%.0f:%.0f:%s:%s:%s",
                tostring(ActorFrame.fardistz(self, 500) == self),
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Draw By Z")).unwrap();
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "Add Child")).unwrap();
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "GetChildren Unnamed ActorFrames"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true");
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

    let compiled = compile_song_lua(
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
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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
            target = self
        end,
    },
}
"#,
    )
    .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "SkewY Probe"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.eases.len(), 1);
    assert!(matches!(
        compiled.eases[0].target,
        SongLuaEaseTarget::PlayerSkewY
    ));
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

    let compiled = compile_song_lua(
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
            self:rainbow():jitter(true):distort(0.5):undistort():hurrytweening(2)
            mod_actions = {
                {1, string.format("%s:%s:%.0f:%.0f", tostring(command ~= nil), tostring(missing == nil), self:getaux(), self:GetTweenTimeLeft()), true},
            }
        end,
    },
}
"#,
        )
        .unwrap();

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
fn compile_song_lua_ignores_unsupported_draw_function_errors() {
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Set Draw Function Error"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "draw-ok");
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Queuecommand Order"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "true");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    let overlay = &compiled.overlays[0];

    assert_eq!(overlay.initial_state.x, 123.0);
    assert_eq!(overlay.initial_state.y, 234.0);
    assert_eq!(overlay.initial_state.size, Some([48.0, 64.0]));
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

    let compiled = compile_song_lua(
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
            local po = ps:GetPlayerOptions("ModsLevel_Song")
            if po:Mirror() ~= false or po:Left() ~= false or po:Right() ~= false then
                error("unexpected lane permutation")
            end
            if po:Skew() ~= 0 or po:Tilt() ~= 0 then
                error("unexpected skew or tilt")
            end
            if po:GetReversePercentForColumn(0) ~= 0 then
                error("unexpected reverse percent")
            end
            mod_actions = {
                {4, string.format("%.0f:%.0f", ArrowEffects.GetXPos(ps, 1, 0), ArrowEffects.GetYPos(ps, 1, 0)), true},
            }
        end)
    end,
}
"#,
        )
        .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "NoteField Column API"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "-96:-125");
    assert_eq!(compiled.note_hides.len(), 1);
    assert_eq!(compiled.note_hides[0].player, 0);
    assert_eq!(compiled.note_hides[0].column, 0);
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Options Getters"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.messages[0].message, "1.00:0.25:true");
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
        po:XMod(3.5, 9e9, true):Overhead(true, 9e9):Mini(0.15, 9e9, true)
        local after_x = string.format("%.2f:%s:%.2f", po:XMod(), tostring(po:Overhead()), po:Mini())
        po:CMod(650, 1)
        local after_c = string.format("%s:%.0f:%s", tostring(po:XMod()), po:CMod(), tostring(po:MMod()))
        po:CMod(nil, 1):MMod(700, 1)
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2.25:nil:false|3.50:true:0.15|nil:650:nil|nil:nil:700"
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
    end,
}
"#,
        )
        .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Player Options FromString"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "3.5:3.5:0.50:true:true|nil:650:false|nil:700:true:0.25:M700, Shuffle, 25% Tiny"
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
        po:CAMod(640, 9e9, true):DrawSize(0.25, 9e9):DizzyHolds(true):StealthPastReceptors(true)
        local direct = string.format(
            "%s:%.0f:%.2f:%s:%s",
            tostring(po:CMod()),
            po:CAMod(),
            po:DrawSize(),
            tostring(po:DizzyHolds()),
            tostring(po:StealthPastReceptors())
        )
        po:LifeSetting("LifeType_Battery")
            :DrainSetting("DrainType_NoRecover")
            :HideLightSetting("HideLightType_HideAllLights")
            :ModTimerSetting("ModTimerType_Beat")
            :FailSetting("FailType_Off")
            :MinTNSToHideNotes("TapNoteScore_W3")
            :WavePeriod(2.5)
            :PulseInner(0.25)
            :BounceZ(3)
            :TanDigitalZPeriod(4)
            :MoveX16(0.75)
            :Reverse(1)
            :VisualDelay(0.12)
            :BatteryLives(4)
            :Passmark(0.2)
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

    let compiled = compile_song_lua(
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
        po:DisableTimingWindow("TimingWindow_W5")
            :DisableTimingWindow("W3")
            :DisableTimingWindow(2)
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

    let compiled = compile_song_lua(
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
        if requested_options:NoteSkin() ~= "metal" then
            error("expected current and requested player options to share state")
        end
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Life Meter Health State"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "PlayerNumber_P1:HealthState_Alive:PlayerController_Human:0.5:0.5:false:false:false:true:1:0"
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

    let compiled = compile_song_lua(
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
        local bpm = top:GetChild("BPMDisplay")
        local title = top:GetChild("SongTitle")
        local steps = top:GetChild("StepsDisplayP1")
        local underlay = top:GetChild("Underlay")
        local p1_score = underlay:GetChild("P1Score")
        local song_meter_title = underlay:GetChild("SongMeter"):GetChild("SongTitle")
        local screen_meter = top:GetChild("SongMeterDisplayP1")
        local stream = screen_meter:GetChild("Stream")
        local overlay = top:GetChild("Overlay")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s:%s:%s:%s:%s",
                    bpm:GetName(),
                    bpm:GetText(),
                    title:GetText(),
                    steps:GetText(),
                    p1_score:GetName(),
                    p1_score:GetText(),
                    song_meter_title:GetText(),
                    screen_meter:GetName(),
                    stream:GetName(),
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "BPMDisplay:120 - 180:Theme Actor Shapes:Difficulty_Hard:P1Score:0.00%:Theme Actor Shapes:SongMeterDisplayP1:Stream:Overlay"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
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
            "BPMDisplay",
            "SongForeground",
            "SongBackground",
            "ScoreP1",
            "ScoreP2",
            "SongTitle",
            "SongMeterDisplayP1",
            "StepsDisplayP1",
        }
        for _, name in ipairs(wanted) do
            assert(children[name], name)
        end
        local underlay_children = children.Underlay:GetChildren()
        assert(underlay_children.P1Score:GetText() == "0.00%")
        assert(underlay_children.SongMeter:GetChild("SongTitle"):GetText() == "Enumeration")
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s:%s:%s",
                    tostring(top:GetNumChildren() >= 20),
                    children.BPMDisplay:GetText(),
                    children.StepsDisplayP1:GetText(),
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "true:150:Difficulty_Challenge:P1Score:Enumeration:true"
    );
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_labels_actors_for_tostring_scans() {
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
        mod_actions = {
            {
                1,
                string.format(
                    "%s:%s:%s:%s",
                    tostring(player:find("Player") ~= nil),
                    tostring(underlay:find("ActorFrame") ~= nil),
                    tostring(steps:find("StepsDisplayP1") ~= nil),
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
fn compile_song_lua_supports_nameless_player_group_and_tap_note_shim() {
    let song_dir = test_dir("nameless-player-group-tap-note-shim");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
    InitCommand=function(self)
        local top = SCREENMAN:GetTopScreen()
        local group = top:GetChild("")
        local player = group[1]
        local direct = top:GetChild("PlayerP1")
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
                    "%d:%s:%s:%d:%s:%s:%s:%d:%s:%s",
                    #group,
                    player:GetName(),
                    tostring(player == direct),
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Nameless Player Group Tap Note Shim"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "2:PlayerP1:true:2:TapNoteScore_W1:true:true:2:TapNoteScore_W1:true"
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
assert(file:Write(encoded))
assert(file:Read() == "")
assert(file:Close() == nil)
assert(file:destroy() == nil)
local dot_file = RageFileUtil.CreateRageFile()
assert(dot_file:Read() == "")
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
assert(pss:GetCurrentPossibleDancePoints() == 1)
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
        assert(stats:GetPercentDancePoints() == 0.8)
    end, true},
}

return Def.ActorFrame{}
"#,
        )
        .unwrap();

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Stage Stat High Score Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
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
assert(PREFSMAN:GetPreference("EventMode") == false)
assert(PREFSMAN:GetPreference("MaxHighScoresPerListForMachine") == 10)
assert(PREFSMAN:GetPreference("LongVerSongSeconds") == 150)
assert(PREFSMAN:GetPreference("EditorNoteSkinP1") == "default")
assert(PREFSMAN:PreferenceExists("MissingPreference") == false)
assert(PREFSMAN:GetPreference("MissingPreference") == nil)
assert(PREFSMAN:SetPreference("EventMode", true) == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == true)
assert(PREFSMAN:SetPreferenceToDefault("EventMode") == PREFSMAN)
assert(PREFSMAN:GetPreference("EventMode") == false)
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
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
assert(#options == 3 and options[2] == "Overhead")
assert(GetPlayerOptionsString(PLAYER_1) == "1x, Overhead, 50% Mini")

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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(compiled.player_actors[0].message_commands.len(), 1);
    let block = &compiled.player_actors[0].message_commands[0].blocks[0];
    assert_eq!(block.delta.x, Some(4.0));
    assert_eq!(block.delta.y, Some(100.0));
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

    let compiled = compile_song_lua(
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
    assert!(!compiled.overlays[0].initial_state.visible);
    assert_eq!(compiled.overlays[0].message_commands.len(), 1);
    assert_eq!(
        compiled.overlays[0].message_commands[0].blocks[0]
            .delta
            .visible,
        Some(true)
    );
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

    let compiled = compile_song_lua(
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
    assert!(!compiled.overlays[0].initial_state.visible);
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Overlay AFT"),
    )
    .unwrap();
    assert!(compiled.hidden_players[0]);
    assert_eq!(compiled.overlays.len(), 3);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture
    ));
    assert!(matches!(
        compiled.overlays[1].kind,
        SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::NoteField { player_index: 0 }
        }
    ));
    assert!(matches!(
        compiled.overlays[2].kind,
        SongLuaOverlayKind::AftSprite { ref capture_name }
            if capture_name == "CaptureAFT"
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Named AFT Sprite"),
    )
    .unwrap();
    assert_eq!(compiled.overlays.len(), 2);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture
    ));
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
        compile_song_lua(&entry, &SongLuaCompileContext::new(&song_dir, "AFT Draw")).unwrap();
    assert_eq!(compiled.overlays.len(), 1);
    assert!(matches!(
        compiled.overlays[0].kind,
        SongLuaOverlayKind::ActorFrameTexture
    ));
    assert!(compiled.overlays[0].initial_state.visible);
}

#[test]
fn compile_song_lua_extracts_overlay_function_actions_and_eases() {
    let song_dir = test_dir("overlay-functions");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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

    let compiled = compile_song_lua(
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
fn compile_song_lua_keeps_overlay_rotation_eases_out_of_player_transforms() {
    let song_dir = test_dir("overlay-rotation-ease");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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
fn compile_song_lua_preserves_overlay_color_for_diffusealpha_eases() {
    let song_dir = test_dir("overlay-diffusealpha-color");
    let entry = song_dir.join("default.lua");
    fs::write(
        &entry,
        r#"
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

    let compiled = compile_song_lua(
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
    let compiled = compile_song_lua(&entry, &context).unwrap();
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

    let compiled = compile_song_lua(
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
        GetComboFonts()[1],
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Theme Asset Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.messages.len(), 1);
    assert_eq!(
        compiled.messages[0].message,
        "50|5|2|0|1|Love|None|None|None|None|4|4|0|false|false||0|0|__songlua_theme_path"
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
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

    let compiled = compile_song_lua(
        &entry,
        &SongLuaCompileContext::new(&song_dir, "Conf Option Row Helpers"),
    )
    .unwrap();
    assert_eq!(compiled.info.unsupported_function_actions, 0);
    assert!(compiled.messages.is_empty());
    assert!(compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_supports_spooky_sample_if_present() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lua-songs/[07] Spooky (SM) [Scrypts]");
    let entry = root.join("lua/default.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "Spooky");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.messages.len(), 2);
    assert_eq!(compiled.overlays.len(), 3);
    assert!(compiled.eases.len() >= 300);
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
fn compile_song_lua_supports_media_offline_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../lua-songs/[10] media offline (SM) [Snap]");
    let entry = root.join("lua/_script.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "media offline");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.time_mods.is_empty());
    assert_eq!(compiled.eases.len(), 44);
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
fn compile_song_lua_supports_cosmic_railroad_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("songs/lua-songs/[11] CO5M1C R4ILR0AD (SH) [TaroNuke vs. Scrypts]");
    let entry = root.join("lua/default.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "CO5M1C R4ILR0AD");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.eases.len(), 548);
    assert_eq!(compiled.overlay_eases.len(), 8);
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
            "missing Cosmic runtime mod target {target}"
        );
    }
    assert!(
        compiled
            .eases
            .iter()
            .any(|ease| matches!(ease.target, SongLuaEaseTarget::PlayerRotationZ))
    );
    assert_eq!(compiled.info.unsupported_function_eases, 0);
    assert_eq!(compiled.info.unsupported_function_actions, 0);
}

#[test]
fn compile_song_lua_supports_step_your_game_up_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../lua-songs/Step Your Game Up (Director's Cut)");
    let entry = root.join("lua/default.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "Step Your Game Up");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.beat_mods.is_empty());
    assert!(!compiled.overlays.is_empty());
}

#[test]
fn compile_song_lua_supports_flip69_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("songs/ITL Online 2026 Unlocks/[10] flip69 (DX) [Telperion]");
    let entry = root.join("multitap/Default.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "[5604] [10] flip69");
    context.style_name = "double".to_string();
    context.music_length_seconds = 48.0;
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
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.overlays.is_empty());
    assert!(!compiled.overlay_eases.is_empty());
    assert!(!compiled.note_hides.is_empty());
    let field = compiled
        .overlays
        .iter()
        .find(|overlay| overlay.name.as_deref() == Some("MultitapFrameP1"))
        .unwrap();
    assert_eq!(field.initial_state.x, context.players[0].screen_x);
    assert_eq!(field.initial_state.y, context.players[0].screen_y);
    let first_arrow = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapArrowP1_1"))
        .unwrap();
    assert!(
        compiled
            .overlay_eases
            .iter()
            .any(|ease| { ease.overlay_index == first_arrow && ease.to.rot_z_deg == Some(90.0) })
    );
    let first_frame = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapP1_1"))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_frame
            && ease
                .to
                .y
                .is_some_and(|y| (y - THEME_RECEPTOR_Y_STD).abs() <= 0.01)
    }));
    let first_deco = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.name.as_deref() == Some("MultitapDeco1_1"))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_deco
            && ease
                .to
                .effect_color1
                .is_some_and(|color| color[0] > 0.99 && color[1] < 0.6 && color[2] < 0.6)
    }));
    let first_deco_child = compiled
        .overlays
        .iter()
        .position(|overlay| overlay.parent_index == Some(first_deco))
        .unwrap();
    assert!(compiled.overlay_eases.iter().any(|ease| {
        ease.overlay_index == first_deco_child
            && ease
                .to
                .effect_color1
                .is_some_and(|color| color[0] > 0.99 && color[1] < 0.6 && color[2] < 0.6)
    }));
    let explosion_message = "__songlua_multitap_explosion_p1_4";
    assert!(compiled.messages.iter().any(|message| {
        message.message == explosion_message && (message.beat - 40.0).abs() <= 0.01
    }));
    assert!(compiled.overlays.iter().any(|overlay| {
        overlay.message_commands.iter().any(|command| {
            command.message == explosion_message
                && command
                    .blocks
                    .iter()
                    .any(|block| block.delta.visible.is_some() || block.delta.diffuse.is_some())
        })
    }));
}

#[test]
fn compile_song_lua_supports_kenpo_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]");
    let entry = root.join("template/main.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "KENPO SAITO");
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
            speedmod: SongLuaSpeedMod::C(516.0),
            ..SongLuaPlayerContext::default()
        },
    ];

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.hidden_players[0] || compiled.hidden_players[1]);
    assert!(
        compiled
            .overlays
            .iter()
            .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture))
    );
    assert!(
        compiled
            .overlays
            .iter()
            .any(|overlay| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
    );
}

#[test]
fn compile_song_lua_supports_godspeed_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../lua-songs/[12] Godspeed (SX) [G. Rosewood]");
    let entry = root.join("lua/_script.lua");
    if !entry.is_file() {
        return;
    }

    let mut context = SongLuaCompileContext::new(&root, "Godspeed");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert!(!compiled.time_mods.is_empty());
    assert_eq!(compiled.eases.len(), 37);
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
fn compile_song_lua_supports_generated_runtime_modchart() {
    let root = test_dir("generated-runtime-modchart");
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

    let compiled = compile_song_lua(&entry, &context).unwrap();
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
fn compile_song_lua_supports_vector_field_sample_if_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lua-songs/Vector Field");
    let entry = root.join("template/main.lua");
    if !entry.is_file() {
        return;
    }

    let compiled =
        compile_song_lua(&entry, &SongLuaCompileContext::new(&root, "Vector Field")).unwrap();
    assert!(!compiled.overlays.is_empty());
}
