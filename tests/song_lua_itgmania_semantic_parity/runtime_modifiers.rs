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
                let value = word.strip_suffix('%').unwrap_or(word).parse::<f32>().ok()?;
                if value.is_finite() {
                    value / 100.0
                } else {
                    0.0
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
                for part in raw.split(',').filter(|part| !part.trim().is_empty()) {
                    let part = part.trim().to_ascii_lowercase();
                    let words = part.split_whitespace().collect::<Vec<_>>();
                    let Some((value, key)) = mod_string_level(&words).zip(words.last()) else {
                        *unsupported.entry(part.clone()).or_default() += 1;
                        continue;
                    };
                    let key = key.to_string();
                    match key.as_str() {
                        "hallway" | "distant" => {
                            push("tilt".into(), if key == "hallway" { -value } else { value });
                            push("skew".into(), 0.0);
                        }
                        _ => push(key, value),
                    }
                }
            } else {
                match value_f32(args.first()) {
                    Some(value) => push(operation.to_ascii_lowercase(), value),
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
        "tipsy" => visual.tipsy.unwrap_or(0.0),
        "dizzy" => visual.dizzy.unwrap_or(0.0),
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
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
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
