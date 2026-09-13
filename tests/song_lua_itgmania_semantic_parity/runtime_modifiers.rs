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

fn option_writes(trace: &NativeTrace) -> Vec<ModWrite> {
    let mut writes = Vec::new();
    for track in &trace.timeline_tracks {
        let Some(player) = (0..2).find(|player| {
            track.actor.as_deref()
                == Some(
                    format!("player-state:PLAYER_{}/options:ModsLevel_Song", player + 1).as_str(),
                )
        }) else {
            continue;
        };
        if !trace.enabled_players.unwrap_or([true; 2])[player] {
            continue;
        }
        let Some(operation) = track.operation.strip_prefix("PlayerOptions.") else {
            continue;
        };
        for (sequence, beat, second, args, _) in &track.samples {
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
            if operation == "FromString" {
                let raw = args
                    .first()
                    .and_then(Value::as_str)
                    .expect("modifier string");
                // Independent subset of PlayerOptions::FromOneModString: numeric
                // levels are percentages; a leading * token is approach speed.
                // Fail on other syntax rather than silently claiming coverage.
                for part in raw.split(',').filter(|part| !part.trim().is_empty()) {
                    let words = part.split_whitespace().collect::<Vec<_>>();
                    assert!(
                        words.len() == 3 && words[0] == "*10000",
                        "uncovered FromString: {part}"
                    );
                    let value = if words[1] == "inf" {
                        // FromOneModString only parses a level starting with a
                        // digit or '-'. Lua's positive infinity leaves level=1.
                        1.0
                    } else {
                        words[1].parse::<f32>().expect("numeric modifier level") / 100.0
                    };
                    let key = words[2].to_ascii_lowercase();
                    match key.as_str() {
                        "hallway" | "distant" => {
                            push("tilt".into(), if key == "hallway" { -value } else { value });
                            push("skew".into(), 0.0);
                        }
                        _ => push(key, value),
                    }
                }
            } else {
                let value = value_f32(args.first()).expect("numeric PlayerOptions target");
                push(operation.to_ascii_lowercase(), value);
            }
        }
    }
    writes.sort_by(|a, b| {
        a.second
            .total_cmp(&b.second)
            .then(a.sequence.cmp(&b.sequence))
    });
    writes
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
    for (prefix, values) in [
        ("confusionoffset", visual.confusion_offset_cols),
        ("movex", visual.move_x_cols),
        ("movey", visual.move_y_cols),
        ("tiny", visual.tiny_cols),
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
        "tipsy" => visual.tipsy.unwrap_or(0.0),
        "dizzy" => visual.dizzy.unwrap_or(0.0),
        "confusionoffset" => visual.confusion_offset.unwrap_or(0.0),
        "tiny" => visual.tiny.unwrap_or(0.0),
        "flip" => visual.flip.unwrap_or(0.0),
        "invert" => visual.invert.unwrap_or(0.0),
        "tornado" => visual.tornado.unwrap_or(0.0),
        "brake" => runtime.accel[player].brake.unwrap_or(0.0),
        "boost" => runtime.accel[player].boost.unwrap_or(0.0),
        "stealth" => appearance.stealth,
        "sudden" => appearance.sudden,
        "suddenoffset" => appearance.sudden_offset,
        "dark" => runtime.visibility[player].dark.unwrap_or(0.0),
        "blind" => runtime.visibility[player].blind.unwrap_or(0.0),
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
            None => 1.0,
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

fn modifier_runtime(
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
) -> GameplayAttackRuntimeState {
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
                deadsync_profile_gameplay::build_song_lua_constant_windows_for_player(
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
                    deadsync_profile_gameplay::build_song_lua_ease_windows_for_player(
                        layer,
                        &timing,
                        player,
                        0.0,
                        &constants[player],
                    );
                assert_eq!(unsupported, 0, "unsupported runtime ease target");
                eases
            })
            .collect()
    });
    GameplayAttackRuntimeState::new(constants, eases)
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
    let mut runtime = modifier_runtime(&compiled, &context);
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
            AttackBaseEffects::default(),
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
    let writes = option_writes(&trace);
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
    let mut runtime = modifier_runtime(&compiled, &context);
    let mut transform = SongLuaPlayerTransform::default();
    let mut pulsed = [false; 4];
    for frame in 0..(trace.end_position.seconds * 180.0) as usize {
        let second = frame as f32 / 180.0;
        if let Some(next) = runtime.refresh_player(
            0,
            second,
            1.0 / 180.0,
            AttackBaseEffects::default(),
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

#[test]
#[ignore = "full-song runtime modifier audit; select CO5M1C or Riddle DX with ITGMANIA_SONG_LUA_TRACE"]
fn native_modifier_values_match_deadsync() {
    crate::paths::init();
    let trace = read_trace();
    let (compiled, _, context) = compile_trace_song(&trace);
    let mut runtime = modifier_runtime(&compiled, &context);
    let mut transforms = [SongLuaPlayerTransform::default(); 2];
    let writes = option_writes(&trace);
    assert!(!writes.is_empty(), "fixture contains no modifier writes");
    let mut stats = BTreeMap::<(usize, &str), ModStats>::new();
    let mut uncovered = BTreeMap::<&str, usize>::new();
    let mut nonfinite = Vec::new();
    let mut cursor = 0;
    while cursor < writes.len() {
        let second = writes[cursor].second;
        let mut last_writes = BTreeMap::new();
        while cursor < writes.len() && writes[cursor].second == second {
            let write = &writes[cursor];
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
                AttackBaseEffects::default(),
                *transform,
            ) {
                *transform = next;
            }
        }
        for (key, write) in last_writes {
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
            let difference = (actual - write.value).abs();
            if difference > (entry.worst.1 - entry.worst.2).abs() {
                entry.worst = (write.beat, write.value, actual);
            }
            if difference > EPSILON || !actual.is_finite() {
                entry.failures += 1;
                entry.first.get_or_insert((write.beat, write.value, actual));
            }
        }
    }
    let mut failures = 0;
    for ((player, key), entry) in &stats {
        failures += entry.failures;
        eprintln!(
            "P{} {key}: {}/{} outside {EPSILON}; first={:?}; worst={:?}",
            player + 1,
            entry.failures,
            entry.samples,
            entry.first,
            entry.worst
        );
    }
    eprintln!("Uncovered option writes: {uncovered:?}");
    eprintln!("Non-finite reference targets (player, option, beat): {nonfinite:?}");
    eprintln!(
        "{}: {failures}/{} sampled modifier values differ across {} player/option pairs",
        trace.title,
        stats.values().map(|entry| entry.samples).sum::<usize>(),
        stats.len()
    );
    assert_eq!(
        failures, 0,
        "runtime modifier differences (see per-option results)"
    );
    assert!(uncovered.is_empty(), "runtime option coverage incomplete");
    assert!(nonfinite.is_empty(), "non-finite reference targets");
}
