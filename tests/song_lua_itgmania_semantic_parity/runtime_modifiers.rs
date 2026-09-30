//! Compare recorded modifier writes with the production gameplay evaluator.
//! The headless oracle records Song-level targets, not native Current-level
//! approach state. Each probe settles approach in the production evaluator at
//! the recorded timestamp, leaving authored ease time and clamping intact.

use super::*;
use deadsync_gameplay::{AttackBaseEffects, GameplayAttackRuntimeState, SongLuaPlayerTransform};
use std::collections::BTreeMap;

#[derive(Debug)]
struct ModWrite {
    sequence: u64,
    second: f32,
    beat: f32,
    player: usize,
    key: String,
    value: f32,
}

/// Independent subset of PlayerOptions::FromOneModString: the last word is
/// the option, `*speed` is approach speed (probes settle it), `no` is level 0,
/// and a word starting with a digit or '-' is a percentage. Returns `None` for
/// other syntax rather than silently claiming coverage.
fn mod_string_level(words: &[&str]) -> Option<f32> {
    let mut level = 1.0;
    for word in &words[..words.len().saturating_sub(1)] {
        if word
            .strip_prefix('*')
            .is_some_and(|speed| speed.parse::<f32>().is_ok())
        {
            continue;
        }
        level = match *word {
            "no" => 0.0,
            // Lua's positive infinity does not start with a digit, so native
            // parsing leaves the level unchanged.
            "inf" => level,
            _ if word.starts_with(|c: char| c.is_ascii_digit() || c == '-') => {
                // strtof stops before a trailing '%' as in "150% drunk", and
                // StringToFloat turns a non-finite result into 0.
                let text = word.strip_suffix('%').unwrap_or(word);
                match text.parse::<f32>() {
                    Ok(value) if value.is_finite() => value / 100.0,
                    Ok(_) => 0.0,
                    // strtof also reads C spellings of NaN such as "-nan(ind)".
                    Err(_) if text.contains("nan") => 0.0,
                    Err(_) => return None,
                }
            }
            _ => return None,
        };
    }
    Some(level)
}

/// Returns the recorded Song-level option writes and how often each write the
/// reader does not cover (an unparsed `FromString` part or a non-numeric
/// target) occurred.
fn option_writes(trace: &NativeTrace) -> (Vec<ModWrite>, BTreeMap<String, usize>) {
    let mut writes = Vec::new();
    let mut unsupported = BTreeMap::<String, usize>::new();
    for track in &trace.timeline_tracks {
        let state_setter = track.operation == "PlayerState.SetPlayerOptions";
        let Some(player) = (0..2).find(|player| {
            track.actor.as_deref()
                == Some(
                    if state_setter {
                        format!("player-state:PLAYER_{}", player + 1)
                    } else {
                        format!("player-state:PLAYER_{}/options:ModsLevel_Song", player + 1)
                    }
                    .as_str(),
                )
        }) else {
            continue;
        };
        if !trace.enabled_players.unwrap_or([true; 2])[player] {
            continue;
        }
        let operation = if state_setter {
            "FromString"
        } else if let Some(operation) = track.operation.strip_prefix("PlayerOptions.") {
            operation
        } else {
            continue;
        };
        for (sequence, beat, second, args, _) in &track.samples {
            if state_setter && args.first().and_then(Value::as_str) != Some("ModsLevel_Song") {
                continue;
            }
            let (Some(beat), Some(second)) = (beat, second) else {
                continue;
            };
            let mut push = |key: String, value: f32| {
                writes.push(ModWrite {
                    sequence: *sequence,
                    second: *second,
                    beat: *beat,
                    player,
                    key,
                    value,
                })
            };
            // Hallway and Distant set perspective tilt and reset skew.
            let mut set_option = |key: String, value: f32| match key.as_str() {
                "overhead" if value > 0.5 => {
                    push("tilt".into(), 0.0);
                    push("skew".into(), 0.0);
                }
                "hallway" | "distant" => {
                    push("tilt".into(), if key == "hallway" { -value } else { value });
                    push("skew".into(), 0.0);
                }
                "stealthpastreceptors" => push(key, f32::from(value > 0.5)),
                _ => push(key, value),
            };
            if operation == "FromString" {
                let raw = args
                    .get(usize::from(state_setter))
                    .and_then(Value::as_str)
                    .expect("modifier string");
                for part in raw.split(',').filter(|part| !part.trim().is_empty()) {
                    let part = part.trim().to_ascii_lowercase();
                    let words = part.split_whitespace().collect::<Vec<_>>();
                    // GetString always includes this neutral lighting enum. It
                    // is metadata rather than a numeric notefield modifier.
                    if words.as_slice() == ["nohidelights"] {
                        continue;
                    }
                    let key = words.last().expect("nonempty modifier part");
                    let speed = key
                        .strip_suffix('x')
                        .map(|raw| ("xmod", raw))
                        .or_else(|| key.strip_prefix('c').map(|raw| ("cmod", raw)))
                        .or_else(|| key.strip_prefix('m').map(|raw| ("mmod", raw)));
                    if let Some((key, value)) = speed
                        .and_then(|(key, raw)| raw.parse::<f32>().ok().map(|value| (key, value)))
                    {
                        set_option(key.into(), value);
                        continue;
                    }
                    let Some((value, key)) = mod_string_level(&words).zip(words.last()) else {
                        *unsupported.entry(part.clone()).or_default() += 1;
                        continue;
                    };
                    set_option(key.to_string(), value);
                }
            } else {
                let value = if operation == "StealthPastReceptors" {
                    args.first().and_then(Value::as_bool).map(f32::from)
                } else {
                    value_f32(args.first())
                };
                match value {
                    Some(value) => set_option(operation.to_ascii_lowercase(), value),
                    None => {
                        let target = args.first().map_or_else(String::new, Value::to_string);
                        *unsupported
                            .entry(format!("{operation} {target}"))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    writes.sort_by(|a, b| {
        a.second
            .total_cmp(&b.second)
            .then(a.sequence.cmp(&b.sequence))
    });
    (writes, unsupported)
}

// Each arm maps native option units onto the public gameplay state consumed by
// the notefield/HUD. No DeadSync modifier parser is used to derive expectations.
fn runtime_mod_value(
    runtime: &GameplayAttackRuntimeState,
    player: usize,
    key: &str,
) -> Option<f32> {
    let visual = runtime.visual[player];
    let appearance = runtime.appearance[player];
    if let Some(column) = key
        .strip_prefix("stealth")
        .and_then(|suffix| suffix.parse::<usize>().ok())
    {
        return column
            .checked_sub(1)
            .and_then(|col| appearance.stealth_cols.get(col))
            .copied();
    }
    for (prefix, values) in [
        ("confusionoffset", visual.confusion_offset_cols),
        ("movex", visual.move_x_cols),
        ("movey", visual.move_y_cols),
        ("tiny", visual.tiny_cols),
        ("bumpy", visual.bumpy_cols),
        ("dark", runtime.visibility[player].dark_cols),
    ] {
        if let Some(column) = key
            .strip_prefix(prefix)
            .and_then(|suffix| suffix.parse::<usize>().ok())
        {
            return column
                .checked_sub(1)
                .and_then(|column| values.get(column))
                .map(|value| value.unwrap_or(0.0));
        }
    }
    Some(match key {
        "beat" => visual.beat.unwrap_or(0.0),
        "drunk" => visual.drunk.unwrap_or(0.0),
        "drunkoffset" => visual.drunk_offset.unwrap_or(0.0),
        "drunkspeed" => visual.drunk_speed.unwrap_or(0.0),
        "drunkperiod" => visual.drunk_period.unwrap_or(0.0),
        "tipsy" => visual.tipsy.unwrap_or(0.0),
        "tipsyoffset" => visual.tipsy_offset.unwrap_or(0.0),
        "tipsyspeed" => visual.tipsy_speed.unwrap_or(0.0),
        "dizzy" => visual.dizzy.unwrap_or(0.0),
        "twirl" => visual.twirl.unwrap_or(0.0),
        "parabolax" => visual.parabola_x.unwrap_or(0.0),
        "parabolaz" => visual.parabola_z.unwrap_or(0.0),
        "confusion" => visual.confusion.unwrap_or(0.0),
        "confusionoffset" => visual.confusion_offset.unwrap_or(0.0),
        "tiny" => visual.tiny.unwrap_or(0.0),
        "flip" => visual.flip.unwrap_or(0.0),
        "invert" => visual.invert.unwrap_or(0.0),
        "tornado" => visual.tornado.unwrap_or(0.0),
        "bumpy" => visual.bumpy.unwrap_or(0.0),
        "bumpyoffset" => visual.bumpy_offset.unwrap_or(0.0),
        "bumpyperiod" => visual.bumpy_period.unwrap_or(0.0),
        "pulseinner" => visual.pulse_inner.unwrap_or(0.0),
        "pulseouter" => visual.pulse_outer.unwrap_or(0.0),
        "pulseperiod" => visual.pulse_period.unwrap_or(0.0),
        "pulseoffset" => visual.pulse_offset.unwrap_or(0.0),
        "randomspeed" => visual.random_speed.unwrap_or(0.0),
        "brake" => runtime.accel[player].brake.unwrap_or(0.0),
        "boost" => runtime.accel[player].boost.unwrap_or(0.0),
        "wave" => runtime.accel[player].wave.unwrap_or(0.0),
        "expand" => runtime.accel[player].expand.unwrap_or(0.0),
        "boomerang" => runtime.accel[player].boomerang.unwrap_or(0.0),
        "hidden" => appearance.hidden,
        "hiddenoffset" => appearance.hidden_offset,
        "stealth" => appearance.stealth,
        "stealthpastreceptors" => f32::from(appearance.stealth_past_receptors),
        "sudden" => appearance.sudden,
        "suddenoffset" => appearance.sudden_offset,
        "blink" => appearance.blink,
        "randomvanish" => appearance.random_vanish,
        "dark" => runtime.visibility[player].dark.unwrap_or(0.0),
        "blind" => runtime.visibility[player].blind.unwrap_or(0.0),
        "cover" => runtime.visibility[player].cover.unwrap_or(0.0),
        "reverse" => runtime.scroll[player].reverse.unwrap_or(0.0),
        "split" => runtime.scroll[player].split.unwrap_or(0.0),
        "alternate" => runtime.scroll[player].alternate.unwrap_or(0.0),
        "cross" => runtime.scroll[player].cross.unwrap_or(0.0),
        "centered" => runtime.scroll[player].centered.unwrap_or(0.0),
        "tilt" => runtime.perspective[player].tilt.unwrap_or(0.0),
        "skew" => runtime.perspective[player].skew.unwrap_or(0.0),
        "mini" => runtime.mini_percent[player].unwrap_or(0.0) / 100.0,
        "xmod" => match runtime.scroll_speed[player] {
            Some(deadsync_rules::scroll::ScrollSpeedSetting::XMod(value)) => value,
            Some(deadsync_rules::scroll::ScrollSpeedSetting::MMod(_)) => 1.0,
            None => 1.0,
            _ => return None,
        },
        "cmod" => match runtime.scroll_speed[player] {
            Some(deadsync_rules::scroll::ScrollSpeedSetting::CMod(value)) => value,
            _ => return None,
        },
        "mmod" => match runtime.scroll_speed[player] {
            Some(deadsync_rules::scroll::ScrollSpeedSetting::MMod(value)) => value,
            _ => return None,
        },
        _ => return None,
    })
}

#[derive(Default)]
struct ModStats {
    samples: usize,
    failures: usize,
    first: Option<(f32, f32, f32)>,
    worst: (f32, f32, f32),
}

/// Builds the production runtime for the compiled song and counts the ease
/// targets it cannot evaluate.
fn modifier_runtime(
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
) -> (GameplayAttackRuntimeState, usize) {
    let unsupported_eases = std::cell::Cell::new(0);
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: context.song_timing_bpms.clone(),
            ..deadsync_rules::timing::TimingSegments::default()
        },
        &[],
    );
    let constants = std::array::from_fn(|player| {
        compiled
            .iter()
            .flat_map(|layer| {
                deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
                    layer, &timing, player, 0.0,
                )
            })
            .collect::<Vec<_>>()
    });
    let eases = std::array::from_fn(|player| {
        compiled
            .iter()
            .flat_map(|layer| {
                let (eases, unsupported) =
                    deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
                        layer,
                        &timing,
                        player,
                        0.0,
                        &constants[player],
                    );
                unsupported_eases.set(unsupported_eases.get() + unsupported);
                eases
            })
            .collect()
    });
    (
        GameplayAttackRuntimeState::new(constants, eases),
        unsupported_eases.get(),
    )
}

#[test]
fn runtime_reader_preserves_order_and_easing_body() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Runtime reader");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 3.0;
    let compiled = compile_song_lua_layers(
        &[directory.join("runtime-mod-reader.lua").as_path()],
        0,
        &context,
    )
    .expect("compile runtime reader");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, drunk, tipsy) in [
        (0.25, 0.1, 0.0),
        (0.5, 0.1, 0.05),
        (0.75, 0.0, 0.1125),
        (1.0, 0.0, 0.3),
        (1.5, 0.0, 1.05),
        (2.0, 0.0, 1.05),
    ] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        for (key, expected) in [("drunk", drunk), ("tipsy", tipsy)] {
            let actual = runtime_mod_value(&runtime, 0, key).expect("supported modifier");
            assert!(
                (actual - expected).abs() < 0.000001,
                "{key} at {second}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn state_option_strings_drive_sampled_targets() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create option fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local phase = 1
local player = GAMESTATE:GetPlayerState(PLAYER_1)
return Def.ActorFrame{OnCommand=function(self)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            player:SetPlayerOptions("ModsLevel_Song", "*7 50% Drunk, C500")
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions("ModsLevel_Song", "25% Mini")
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write option fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "State option strings");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile option strings");
    assert!(
        compiled[0].beat_mods.is_empty(),
        "recurring writes use sampled targets"
    );
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, drunk, mini, speed) in [
        (
            0.25,
            0.0,
            0.0,
            deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0),
        ),
        (
            0.5,
            0.5,
            0.0,
            deadsync_rules::scroll::ScrollSpeedSetting::CMod(500.0),
        ),
        (
            1.0,
            0.0,
            0.25,
            deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0),
        ),
        (
            1.5,
            0.0,
            0.25,
            deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0),
        ),
    ] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(
            runtime_mod_value(&runtime, 0, "drunk"),
            Some(drunk),
            "drunk at {second}"
        );
        assert_eq!(
            runtime_mod_value(&runtime, 0, "mini"),
            Some(mini),
            "mini at {second}"
        );
        assert_eq!(
            runtime.scroll_speed[0]
                .unwrap_or(deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0)),
            speed,
            "speed at {second}"
        );
    }
    let mut trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    trace.timeline_tracks = vec![NativeTimelineTrack {
        kind: "modifier".into(),
        actor: Some("player-state:PLAYER_1".into()),
        operation: "PlayerState.SetPlayerOptions".into(),
        samples: vec![
            (
                1,
                Some(1.0),
                Some(0.5),
                vec![
                    serde_json::json!("ModsLevel_Song"),
                    serde_json::json!("*7 50% Drunk, C500"),
                ],
                None,
            ),
            (
                2,
                Some(2.0),
                Some(1.0),
                vec![
                    serde_json::json!("ModsLevel_Song"),
                    serde_json::json!("25% Mini"),
                ],
                None,
            ),
        ],
    }];
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(parity.checks(), 3, "audit state-level string writes");
    parity.assert_complete("state option string targets");
    let mut missing = compiled;
    missing[0].eases.clear();
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &missing, &context, &mut parity);
    assert!(
        !parity.gaps.is_empty(),
        "the audit rejects missing sampled targets"
    );
}

#[test]
fn motion_suboptions_survive_lua_writes_and_reset() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create motion fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(&entry, r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 250% drunkoffset, *4 -50% drunkspeed, *3 -99% drunkperiod, *5 250% tipsyoffset, *6 -50% tipsyspeed, *7 150% hiddenoffset')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:DrunkOffset(200):DrunkSpeed(1.5):DrunkPeriod(0.75)
            options:TipsyOffset(-1):TipsySpeed(-1)
            options:HiddenOffset(-0.5)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#).expect("write motion fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Motion suboptions");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile motion suboptions");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.25, [2.5, -0.5, -0.99, 2.5, -0.5, 1.5]),
        (0.5, [200.0, 1.5, 0.75, -1.0, -1.0, -0.5]),
        (1.0, [0.0; 6]),
    ] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        for (key, expected) in [
            "drunkoffset",
            "drunkspeed",
            "drunkperiod",
            "tipsyoffset",
            "tipsyspeed",
            "hiddenoffset",
        ]
        .into_iter()
        .zip(expected)
        {
            let actual = runtime_mod_value(&runtime, 0, key).expect("supported motion modifier");
            assert!(
                (actual - expected).abs() < 0.000001,
                "{key} at {second}: {actual}"
            );
        }
    }
}

#[test]
fn lane_stealth_survives_lua_writes_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Stealth fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 25% stealth1, *4 100% stealth4, *0 stealthpastreceptors')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:Stealth1(0.75, 2):Stealth4(0):StealthPastReceptors(false)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '50% stealth2')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Stealth fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Lane Stealth");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Stealth fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected, past) in [
        (0.25, [0.25, 0.0, 0.0, 1.0], true),
        (0.5, [0.75, 0.0, 0.0, 0.0], false),
        (1.0, [0.0, 0.5, 0.0, 0.0], false),
    ] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(runtime.appearance[0].stealth_cols[..4], expected);
        assert_eq!(runtime.appearance[0].stealth_past_receptors, past);
    }
}

#[test]
fn twirl_survives_lua_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Twirl fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 -250% twirl')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:Twirl(7, 4)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Twirl fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Twirl");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Twirl fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [(0.25, -2.5), (0.5, 7.0), (1.0, 0.0)] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(runtime_mod_value(&runtime, 0, "twirl"), Some(expected));
        assert_eq!(runtime_mod_value(&runtime, 1, "twirl"), Some(0.0));
    }
}

#[test]
fn parabola_survives_lua_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Parabola fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 -250% parabolax,*4 600% parabolaz')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:ParabolaX(-7, 4)
            options:ParabolaZ(6, 3)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Parabola fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Parabola");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Parabola fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [(0.25, [-2.5, 6.0]), (0.5, [-7.0, 6.0]), (1.0, [0.0, 0.0])] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        for (key, value) in ["parabolax", "parabolaz"].into_iter().zip(expected) {
            assert_eq!(runtime_mod_value(&runtime, 0, key), Some(value));
            assert_eq!(runtime_mod_value(&runtime, 1, key), Some(0.0));
        }
    }
}

#[test]
fn sampled_modifiers_keep_final_partial_frame() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Final modifier frame");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 1.137;
    let compiled = compile_song_lua_layers(
        &[directory.join("runtime-mod-reader.lua").as_path()],
        0,
        &context,
    )
    .expect("compile final modifier frame");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for second in [1.137, 1.5] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        let tipsy = runtime_mod_value(&runtime, 0, "tipsy").expect("supported modifier");
        // The reader evaluates ((beat - 1) / 2)^2 + 0.05 at the exact song end.
        assert!((tipsy - 0.455769).abs() < 0.000001, "{second}: {tipsy}");
    }
}

#[test]
fn sampled_speed_modes_reactivate_previous_values() {
    use deadsync_rules::scroll::ScrollSpeedSetting::{CMod, MMod, XMod};
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Speed modes");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 3.0;
    let compiled = compile_song_lua_layers(
        &[directory.join("speed-mode-switch.lua").as_path()],
        0,
        &context,
    )
    .expect("compile speed switches");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.25, XMod(2.0)),
        (0.5, CMod(300.0)),
        (0.75, CMod(300.0)),
        (1.0, XMod(2.0)),
        (1.5, MMod(600.0)),
        (2.0, XMod(2.0)),
    ] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(
            runtime.scroll_speed[0],
            Some(expected),
            "speed mode at {second}"
        );
    }
}

#[test]
fn prefix_reader_writes_override_raw_ease_endpoints() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Prefix reader");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 1.0;
    let compiled = compile_song_lua_layers(
        &[directory.join("prefix-mod-reader.lua").as_path()],
        0,
        &context,
    )
    .expect("compile prefix reader");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [(0.25, 0.025), (0.5, 0.0), (0.75, 0.0)] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(runtime_mod_value(&runtime, 0, "centered"), Some(expected));
    }
}

#[test]
fn sampled_modifiers_change_on_the_recorded_frame() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    for (change_bpm, rate, offset) in [
        (false, 1.0, 0.0),
        (true, 1.0, 0.0),
        (false, 1.5, 0.0),
        (false, 1.0, -0.125),
        (true, 2.0, 0.125),
    ] {
        let mut context = SongLuaCompileContext::new(&directory, "Sample clock");
        context.song_timing_bpms = vec![(0.0, 140.0)];
        if change_bpm {
            context.song_timing_bpms.push((7.0, 180.0));
        }
        context.music_length_seconds = 61.0;
        context.song_music_rate = rate;
        let timing = deadsync_rules::timing::TimingData::from_segments(
            offset,
            0.0,
            &deadsync_rules::timing::TimingSegments {
                bpms: context.song_timing_bpms.clone(),
                ..Default::default()
            },
            &[],
        );
        context.player_timing[0] = Some(timing.clone());
        let compiled =
            compile_song_lua_layers(&[directory.join("sample-clock.lua").as_path()], 0, &context)
                .expect("compile sample clock");
        let (windows, unsupported) =
            deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
                &compiled[0],
                &timing,
                0,
                0.0,
                &[],
            );
        let mut runtime =
            GameplayAttackRuntimeState::new([Vec::new(), Vec::new()], [windows, Vec::new()]);
        assert_eq!(unsupported, 0);
        for frame in [1, 115, 116, 179, 180, 181, 3600] {
            let second = (f64::from(frame) / 60.0 * f64::from(rate)) as f32 + offset;
            for (probe, sampled_frame) in [(second.next_down(), frame - 1), (second, frame)] {
                let seconds = f64::from(sampled_frame) / 60.0 * f64::from(rate);
                let beat = if change_bpm && seconds > 3.0 {
                    7.0 + (seconds - 3.0) * 3.0
                } else {
                    seconds * 140.0 / 60.0
                };
                let _ = runtime.refresh_player(
                    0,
                    probe,
                    1_000_000.0,
                    deadsync_gameplay::AppearanceEffects::default(),
                    AttackBaseEffects::default,
                    SongLuaPlayerTransform::default(),
                );
                let actual = runtime_mod_value(&runtime, 0, "tiny").expect("tiny value");
                assert!(
                    (actual - (beat / 100.0) as f32).abs() < 0.000001,
                    "BPM change={change_bpm}, rate={rate}, offset={offset}, frame={sampled_frame}: {actual}, beat={beat}, probe={probe}"
                );
                assert_eq!(
                    runtime_mod_value(&runtime, 0, "flip"),
                    Some(if beat >= 140.0 { -0.25 } else { 0.0 })
                );
            }
        }
    }
}

#[test]
fn sampled_modifiers_keep_stops_and_split_chart_timing() {
    use deadsync_rules::timing::{StopSegment, TimingData, TimingSegments};
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    for (chart_bpm, stops, second, expected) in [
        (
            120.0,
            vec![StopSegment {
                beat: 2.0,
                duration: 0.5,
            }],
            1.25,
            0.02,
        ),
        (180.0, Vec::new(), 2.0, 0.06),
    ] {
        let mut context = SongLuaCompileContext::new(&directory, "Chart clock");
        context.song_timing_bpms = vec![(0.0, 120.0)];
        context.music_length_seconds = 4.0;
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, chart_bpm)],
                stops,
                ..Default::default()
            },
            &[],
        );
        context.player_timing[0] = Some(timing.clone());
        let compiled =
            compile_song_lua_layers(&[directory.join("sample-clock.lua").as_path()], 0, &context)
                .expect("compile chart clock");
        let (windows, unsupported) =
            deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
                &compiled[0],
                &timing,
                0,
                0.0,
                &[],
            );
        assert_eq!(unsupported, 0);
        let mut runtime =
            GameplayAttackRuntimeState::new([Vec::new(), Vec::new()], [windows, Vec::new()]);
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert!(
            (runtime_mod_value(&runtime, 0, "tiny").expect("tiny value") - expected).abs()
                < 0.000001
        );
    }
}

#[test]
fn sampled_dark_columns_keep_method_and_string_values() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Column dark");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 3.0;
    let compiled =
        compile_song_lua_layers(&[directory.join("column-dark.lua").as_path()], 0, &context)
            .unwrap();
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0, "unsupported runtime ease target");
    for (second, expected) in [
        (0.25, [1.0, -0.5, 0.25, 0.0]),
        (1.25, [0.0, 0.5, -0.25, 1.5]),
        (2.25, [1.0, -0.5, 0.25, 0.0]),
    ] {
        for player in 0..2 {
            let _ = runtime.refresh_player(
                player,
                second,
                0.25,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 0, "dark"), Some(0.25));
        for (key, expected) in ["dark1", "dark2", "dark3", "dark4"]
            .into_iter()
            .zip(expected)
        {
            let actual = runtime_mod_value(&runtime, 0, key).unwrap();
            assert!(
                (actual - expected).abs() < 0.00001,
                "{key} at {second}s: {actual}"
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 1, "dark1"), Some(0.75));
        assert_eq!(runtime_mod_value(&runtime, 1, "dark2"), Some(0.0));
        assert_eq!(runtime_mod_value(&runtime, 1, "dark"), Some(0.0));
    }
}

#[test]
fn sampled_mini_and_xmod_pulse_preserves_note_spacing() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song_lua");
    let mut context = SongLuaCompileContext::new(&directory, "Mini and XMod pulse");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 3.0;
    let compiled = compile_song_lua_layers(
        &[directory.join("mini-speed-pulse.lua").as_path()],
        0,
        &context,
    )
    .unwrap();
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0, "unsupported runtime ease target");
    let mut transform = SongLuaPlayerTransform::default();
    // Probe between the compiler's 60 Hz samples as well as on their edges.
    // ITGmania Player::Update uses zoom = 1 - Mini / 2; ArrowEffects::GetYOffset
    // multiplies travel by XMod. These paired writes must cancel at every frame.
    for frame in 0..540 {
        let second = frame as f32 / 180.0;
        if let Some(next) = runtime.refresh_player(
            0,
            second,
            1.0 / 180.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
        let mini = runtime_mod_value(&runtime, 0, "mini").unwrap();
        let speed = runtime_mod_value(&runtime, 0, "xmod").unwrap();
        let expected_mini = match frame {
            135 | 315 => Some(-0.4), // inside each method/string-authored pulse
            45 | 225 | 405 => Some(0.0),
            _ => None,
        };
        if let Some(expected) = expected_mini {
            assert!(
                (mini - expected).abs() < 0.00001,
                "pulse missing at {second}s"
            );
        }
        let spacing = (1.0 - mini * 0.5) * speed;
        assert!(
            (spacing - 1.0).abs() < 0.00001,
            "note spacing jumped at {second}s: Mini={mini}, XMod={speed}, spacing={spacing}"
        );
    }
}

#[test]
#[ignore = "requires the 7th Gear song files; set ITGMANIA_SONG_LUA_SIMFILE"]
fn seventh_gear_opening_pulse_keeps_size_and_speed_synchronized() {
    crate::paths::init();
    let mut trace = read_trace_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/itgmania-song-lua/7th Gear/7th Gear.ssc.semantic.json"),
    );
    // The native trace records the same outQuad factor for Mini and XMod.
    // Check that relationship first, then probe DeadSync between its samples.
    let (writes, unsupported) = option_writes(&trace);
    assert!(
        unsupported.is_empty(),
        "uncovered FromString: {unsupported:?}"
    );
    let mut native_samples = 0;
    for mini in writes.iter().filter(|write| {
        write.player == 0 && write.key == "mini" && (16.0..20.0).contains(&write.beat)
    }) {
        if writes.iter().any(|later| {
            later.player == mini.player
                && later.key == mini.key
                && later.second == mini.second
                && later.sequence > mini.sequence
        }) {
            continue;
        }
        let speed = writes
            .iter()
            .rev()
            .find(|write| write.player == 0 && write.key == "xmod" && write.second == mini.second)
            .unwrap();
        assert!((speed.value - (1.0 + mini.value * (5.0 / 12.0))).abs() < 0.00001);
        native_samples += 1;
    }
    assert!(native_samples >= 16);
    let first_pulse = writes.iter().find(|write| write.beat == 16.0).unwrap();
    let seconds_per_beat = first_pulse.second / first_pulse.beat;
    trace.end_position.seconds = 21.0 * seconds_per_beat;
    let (compiled, _, context) = compile_trace_song(&trace);
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0, "unsupported runtime ease target");
    let mut transform = SongLuaPlayerTransform::default();
    let mut pulsed = [false; 4];
    for frame in 0..(trace.end_position.seconds * 180.0) as usize {
        let second = frame as f32 / 180.0;
        if let Some(next) = runtime.refresh_player(
            0,
            second,
            1.0 / 180.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
        let beat = second / seconds_per_beat;
        if !(15.0..20.0).contains(&beat) {
            continue;
        }
        let mini = runtime_mod_value(&runtime, 0, "mini").unwrap();
        let speed = runtime_mod_value(&runtime, 0, "xmod").unwrap();
        if beat >= 16.0 && mini < -0.3 {
            pulsed[beat as usize - 16] = true;
        }
        assert!(
            (speed - (1.0 + mini * (5.0 / 12.0))).abs() < 0.00001,
            "pulse out of sync at beat {beat}: Mini={mini}, XMod={speed}"
        );
    }
    assert!(
        pulsed.into_iter().all(|seen| seen),
        "opening pulses missing"
    );
}

/// One check per recorded player/option target at each native timestamp,
/// reported per player/option pair.
pub(super) fn compare_runtime_modifiers(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("runtime modifiers");
    let (writes, unsupported) = option_writes(trace);
    for (part, count) in unsupported {
        parity.tally(count, count);
        parity
            .gaps
            .push(format!("uncovered option write `{part}`: {count} writes"));
    }
    if writes.is_empty() {
        return;
    }
    let (mut runtime, unsupported) = modifier_runtime(compiled, context);
    parity.tally(unsupported, unsupported);
    if unsupported > 0 {
        parity.gaps.push(format!(
            "{unsupported} runtime ease targets are unsupported by DeadSync"
        ));
    }
    let mut transforms = [SongLuaPlayerTransform::default(); 2];
    let mut stats = BTreeMap::<(usize, &str), ModStats>::new();
    let mut uncovered = BTreeMap::<&str, usize>::new();
    let mut nonfinite = Vec::new();
    let mut cursor = 0;
    while cursor < writes.len() {
        let second = writes[cursor].second;
        let mut last_writes = BTreeMap::new();
        let mut last_speed = [None; 2];
        while cursor < writes.len() && writes[cursor].second == second {
            let write = &writes[cursor];
            if matches!(write.key.as_str(), "xmod" | "cmod" | "mmod") {
                last_speed[write.player] = Some(write);
            }
            last_writes.insert((write.player, write.key.as_str()), write);
            cursor += 1;
        }
        // The oracle records requested targets. Settle approach rather than
        // treating a correct gradual approach as a target mismatch. This does
        // not advance ease time and does not measure native approach parity.
        for (player, transform) in transforms.iter_mut().enumerate() {
            if let Some(next) = runtime.refresh_player(
                player,
                second,
                1_000_000.0,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                *transform,
            ) {
                *transform = next;
            }
        }
        for (key, write) in last_writes {
            // Native XMod/CMod/MMod setters select one shared speed mode.
            // Observe each recorded option after the frame's final setter,
            // including nil getters for modes superseded in that same frame.
            let expected = if matches!(write.key.as_str(), "xmod" | "cmod" | "mmod") {
                let last = last_speed[write.player].expect("recorded speed write");
                if write.key == last.key {
                    Some(last.value)
                } else if write.key == "xmod" && last.key == "mmod" {
                    Some(1.0)
                } else {
                    None
                }
            } else {
                Some(write.value)
            };
            let Some(expected) = expected else {
                parity.check(
                    runtime_mod_value(&runtime, write.player, &write.key).is_none(),
                    || {
                        format!(
                            "P{} {} remains active after a different speed setter at beat {}",
                            write.player + 1,
                            write.key,
                            write.beat
                        )
                    },
                );
                continue;
            };
            if !write.value.is_finite() {
                nonfinite.push((write.player + 1, write.key.as_str(), write.beat));
                continue;
            }
            let Some(actual) = runtime_mod_value(&runtime, write.player, &write.key) else {
                *uncovered.entry(&write.key).or_default() += 1;
                continue;
            };
            let entry = stats.entry(key).or_default();
            entry.samples += 1;
            let difference = (actual - expected).abs();
            if difference > (entry.worst.1 - entry.worst.2).abs() {
                entry.worst = (write.beat, expected, actual);
            }
            if difference > EPSILON || !actual.is_finite() {
                entry.failures += 1;
                entry.first.get_or_insert((write.beat, expected, actual));
            }
        }
    }
    for ((player, key), entry) in &stats {
        parity.tally(entry.samples, entry.failures);
        if entry.failures > 0 {
            parity.gaps.push(format!(
                "P{} {key}: {}/{} modifier values outside {EPSILON}; first (beat, ITGmania, DeadSync)={:?}; worst={:?}",
                player + 1,
                entry.failures,
                entry.samples,
                entry.first,
                entry.worst
            ));
        }
    }
    for (key, count) in uncovered {
        parity.tally(count, count);
        parity.gaps.push(format!(
            "{key}: {count} modifier writes have no DeadSync runtime value"
        ));
    }
    for (player, key, beat) in nonfinite {
        parity.check(false, || {
            format!("P{player} {key}: non-finite reference target at beat {beat:.3}")
        });
    }
}

#[test]
#[ignore = "full-song runtime modifier audit; select CO5M1C or Riddle DX with ITGMANIA_SONG_LUA_TRACE"]
fn native_modifier_values_match_deadsync() {
    crate::paths::init();
    let trace = read_trace();
    let (compiled, _, context) = compile_trace_song(&trace);
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert!(parity.checks() > 0, "fixture contains no modifier writes");
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("runtime modifier");
}
