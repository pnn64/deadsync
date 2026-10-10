//! Compare recorded modifier writes with the production gameplay evaluator.
//! The headless oracle records Song-level targets, not native Current-level
//! approach state. Each probe evaluates the requested targets through the
//! production ease evaluator, leaving authored time and clamping intact.

use super::*;
use deadsync_gameplay::{
    ActiveAttackMaskValues, AttackBaseEffects, GameplayAttackRuntimeState, SongLuaPlayerTransform,
    SongLuaPlayerTransformValues,
};
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
                // Native strtof accepts the numeric prefix of "30+0%".
                let number = (1..=text.len())
                    .rev()
                    .find_map(|end| text.get(..end)?.parse::<f32>().ok());
                match number {
                    Some(value) if value.is_finite() => value / 100.0,
                    Some(_) => 0.0,
                    // strtof also reads C spellings of NaN such as "-nan(ind)".
                    None if text.contains("nan") => 0.0,
                    None => return None,
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
    let mut rejected = Vec::new();
    let mut assignments = Vec::new();
    let mut speed_resets = std::collections::BTreeSet::new();
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
        for (sequence, beat, second, args, detail) in &track.samples {
            if state_setter && args.first().and_then(Value::as_str) != Some("ModsLevel_Song") {
                continue;
            }
            let (Some(beat), Some(second)) = (beat, second) else {
                continue;
            };
            // Direct Song-level turns/transforms retain API state without
            // reprocessing loaded NoteData (Player.cpp). Only omit their
            // numeric-render audit when native getter evidence is present;
            // compare_boolean_options checks every such call separately.
            if detail
                .as_ref()
                .and_then(|value| value.get("boolean_option"))
                .is_some()
                && !matches!(
                    operation,
                    "StealthType" | "StealthPastReceptors" | "Cosecant" | "DizzyHolds" | "ZBuffer"
                )
            {
                continue;
            }
            let speed_observed = detail.as_ref().is_some_and(|detail| detail["speed_option"].is_object());
            if speed_observed && matches!(operation, "TimeSpacing" | "ScrollSpeed" | "ScrollBPM" | "MaxScrollBPM" | "XMod" | "CMod" | "MMod") {
                // All shared fields and every call are checked against native
                // getters below, followed by the actual final playback mode.
                continue;
            }
            if speed_observed { speed_resets.insert(*sequence); }
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
            if state_setter {
                // PlayerState.cpp constructs fresh PlayerOptions, then Assigns
                // them. Omitted numeric fields return to PlayerOptions::Init
                // defaults before the replacement string's targets are applied.
                push("__assignment_reset".into(), 0.0);
                if let Some(snapshot) = detail.as_ref()
                    .and_then(|detail| detail["numeric_options"].as_array())
                {
                    assignments.push((*sequence, *beat, *second, player, snapshot));
                }
            }
            // FromString Overhead always resets perspective, even at level zero.
            let mut set_option = |key: String, value: f32| match key.as_str() {
                "xmod" | "cmod" | "mmod" if speed_observed => {},
                "modtimergame" => push("modtimersetting".into(), 0.0),
                "modtimerbeat" => push("modtimersetting".into(), 1.0),
                "modtimersong" => push("modtimersetting".into(), 2.0),
                "modtimerdefault" => push("modtimersetting".into(), 3.0),
                "overhead" => {
                    push("tilt".into(), 0.0);
                    push("skew".into(), 0.0);
                }
                "hallway" | "distant" => {
                    push("tilt".into(), if key == "hallway" { -value } else { value });
                    push("skew".into(), 0.0);
                }
                "stealthtype" | "stealthpastreceptors" | "cosecant" | "dizzyholds" | "zbuffer" => {
                    push(key, f32::from(value > 0.5))
                }
                _ => push(key, value),
            };
            if operation == "NoteSkin" && detail.as_ref()
                .and_then(|detail| detail.get("noteskin_option"))
                .is_some_and(|state| state["previous"].is_string() && state["current"].is_string())
            {
                // A string setting has its own native getter/sequence audit.
                continue;
            }
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
                    if *key != "clearall" && detail.as_ref()
                        .and_then(|detail| detail.get("noteskin_option"))
                        .filter(|state| state["previous"].is_string() && state["current"].is_string())
                        .and_then(|state| state["parts"].as_array())
                        .is_some_and(|parts| parts.iter().any(|skin|
                            skin["part"].as_str().is_some_and(|name| name.eq_ignore_ascii_case(&part))
                                && skin["target"].is_string()))
                    {
                        // The linked native parser identified this exact part.
                        // compare_noteskin_options must also pass its API state;
                        // clearall still needs the numeric reset audit below.
                        continue;
                    }
                    if let Some(noop) = detail.as_ref()
                        .and_then(|detail| detail["rejected_parts"].as_array())
                        .and_then(|parts| parts.iter().find(|noop|
                            noop["part"].as_str().is_some_and(|raw|
                                raw.eq_ignore_ascii_case(&part))))
                    {
                        // The linked FromOneModString rejected this exact part.
                        // Keep its observation and audit native live fields at
                        // this timestamp instead of inventing a numeric target.
                        if noop["accepted"] != false || noop["unchanged"] != true {
                            *unsupported.entry(format!("native rejected part changed {part}"))
                                .or_default() += 1;
                        } else {
                            rejected.push((*sequence, *beat, *second, player, noop));
                        }
                        continue;
                    }
                    if let Some(noop) = detail
                        .as_ref()
                        .and_then(|detail| detail.get("indexed_noops"))
                        .and_then(Value::as_array)
                        .and_then(|noops| {
                            noops.iter().find(|noop| noop["key"].as_str() == Some(*key))
                        })
                    {
                        // Native PlayerOptions ignores invalid column indices.
                        // Compare its queried live fields instead of inventing
                        // a target from the invalid modifier's requested level.
                        if noop["unchanged"] != true {
                            *unsupported
                                .entry(format!("native no-op changed {key}"))
                                .or_default() += 1;
                        }
                        if let Some(values) = noop["values"]
                            .as_array()
                            .filter(|values| !values.is_empty())
                        {
                            for value in values {
                                if let Some((key, amount)) =
                                    value[0].as_str().zip(value_f32(value.get(1)))
                                {
                                    set_option(key.to_owned(), amount);
                                } else {
                                    *unsupported
                                        .entry(format!("invalid native no-op snapshot {key}"))
                                        .or_default() += 1;
                                }
                            }
                        } else {
                            *unsupported
                                .entry(format!("missing native no-op snapshot {key}"))
                                .or_default() += 1;
                        }
                        continue;
                    }
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
                // Overhead(false) changes approach speeds without writing an
                // angle target. This target audit does not measure approach.
                if operation == "Overhead"
                    && args
                        .first()
                        .is_some_and(|v| v.is_null() || v == &Value::Bool(false))
                {
                    continue;
                }
                let value = if operation == "ModTimerSetting" {
                    let Some(target) = args.first().filter(|v| !v.is_null()) else {
                        continue;
                    };
                    if let Some(raw) = target.as_str() {
                        ["Game", "Beat", "Song", "Default"]
                            .iter()
                            .position(|label| {
                                raw == format!("ModTimerType_{label}")
                                    || raw.eq_ignore_ascii_case(label)
                            })
                            .map(|mode| mode as f32)
                    } else {
                        value_f32(Some(target))
                            .filter(|v| (0.0..=3.0).contains(v) && v.fract() == 0.0)
                    }
                } else if operation == "Overhead" {
                    args.first().map(|_| 1.0)
                } else if matches!(
                    operation,
                    "StealthType" | "StealthPastReceptors" | "Cosecant" | "DizzyHolds" | "ZBuffer"
                ) {
                    // BOOL_INTERFACE treats a non-boolean first argument as a
                    // query. A chaining argument does not turn it into a write.
                    let Some(value) = args.first().and_then(Value::as_bool) else {
                        continue;
                    };
                    Some(f32::from(value))
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
    // clearall invokes PlayerOptions::Init; it has no numeric getter. Observe
    // every numeric option used by this trace at its native reset value, plus
    // the shared speed mode, perspective and timer defaults. Later writes in
    // the same native call/frame retain their original order and supersede it.
    let keys: [std::collections::BTreeSet<String>; 2] = std::array::from_fn(|player| {
        writes
            .iter()
            .filter(|write| write.player == player
                && !matches!(write.key.as_str(), "clearall" | "__assignment_reset"))
            .map(|write| write.key.clone())
            .chain(
                ["cmod", "mmod", "xmod", "tilt", "skew", "modtimersetting"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .collect()
    });
    for (sequence, beat, second, player, noop) in rejected {
        let mut observed = 0;
        if let Some(values) = noop["values"].as_array() {
            for value in values {
                let Some(key) = value[0].as_str().filter(|key| keys[player].contains(*key)) else {
                    continue;
                };
                if let Some(amount) = value_f32(value.get(1)) {
                    writes.push(ModWrite {
                        sequence, second, beat, player, key: key.to_owned(), value: amount,
                    });
                    observed += 1;
                } else {
                    *unsupported.entry(format!("invalid native rejected-part field {key}"))
                        .or_default() += 1;
                }
            }
        }
        if observed == 0 {
            *unsupported.entry(format!("missing native rejected-part fields {}", noop["part"]))
                .or_default() += 1;
        }
    }
    // New captures also record compiled native getters after assignment. Keep
    // those final fields after the text audit, including resets omitted by text.
    for (sequence, beat, second, player, snapshot) in assignments {
        for field in snapshot {
            let Some(key) = field[0].as_str().filter(|key| keys[player].contains(*key)
                && !matches!(*key, "xmod" | "cmod" | "mmod")) else { continue; };
            if let Some(value) = value_f32(field.get(1)) {
                writes.push(ModWrite { sequence, second, beat, player, key: key.into(), value });
            } else {
                *unsupported.entry(format!("invalid native assignment field {key}"))
                    .or_default() += 1;
            }
        }
    }
    writes.sort_by(|a, b| a.second.total_cmp(&b.second).then(a.sequence.cmp(&b.sequence)));
    let writes = writes
        .into_iter()
        .flat_map(|write| {
            if !matches!(write.key.as_str(), "clearall" | "__assignment_reset") {
                return vec![write];
            }
            keys[write.player]
                .iter()
                // Native reset getter snapshots replace synthetic alias defaults;
                // the reset call and its fields remain in the speed API audit.
                .filter(|key| !speed_resets.contains(&write.sequence)
                    || !matches!(key.as_str(), "xmod" | "cmod" | "mmod"))
                .map(|key| ModWrite {
                    sequence: write.sequence,
                    second: write.second,
                    beat: write.beat,
                    player: write.player,
                    key: key.clone(),
                    // PlayerOptions.cpp Init zeros numeric fields, except 1x/200 CMod
                    // and ModTimerType_Default. CMod/MMod getters are inactive at 1x.
                    value: match key.as_str() {
                        "xmod" => 1.0,
                        "cmod" => 200.0,
                        "modtimersetting" => 3.0,
                        _ => 0.0,
                    },
                })
                .collect()
        })
        .collect();
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
        "roll" => visual.roll.unwrap_or(0.0),
        "modtimersetting" => visual.mod_timer_type.unwrap_or_default() as u8 as f32,
        "modtimermult" => visual.mod_timer_mult.unwrap_or(0.0),
        "modtimeroffset" => visual.mod_timer_offset.unwrap_or(0.0),
        "parabolax" => visual.parabola_x.unwrap_or(0.0),
        "bumpyx" => visual.bumpy_x.unwrap_or(0.0),
        "bumpyxoffset" => visual.bumpy_x_offset.unwrap_or(0.0),
        "bumpyxperiod" => visual.bumpy_x_period.unwrap_or(0.0),
        "tanbumpy" => visual.tan_bumpy.unwrap_or(0.0),
        "tanbumpyoffset" => visual.tan_bumpy_offset.unwrap_or(0.0),
        "tanbumpyperiod" => visual.tan_bumpy_period.unwrap_or(0.0),
        "tanbumpyx" => visual.tan_bumpy_x.unwrap_or(0.0),
        "tanbumpyxoffset" => visual.tan_bumpy_x_offset.unwrap_or(0.0),
        "tanbumpyxperiod" => visual.tan_bumpy_x_period.unwrap_or(0.0),
        "drunkz" => visual.drunk_z.unwrap_or(0.0),
        "drunkzoffset" => visual.drunk_z_offset.unwrap_or(0.0),
        "drunkzspeed" => visual.drunk_z_speed.unwrap_or(0.0),
        "drunkzperiod" => visual.drunk_z_period.unwrap_or(0.0),
        "tandrunk" => visual.tan_drunk.unwrap_or(0.0),
        "tandrunkoffset" => visual.tan_drunk_offset.unwrap_or(0.0),
        "tandrunkspeed" => visual.tan_drunk_speed.unwrap_or(0.0),
        "tandrunkperiod" => visual.tan_drunk_period.unwrap_or(0.0),
        "tandrunkz" => visual.tan_drunk_z.unwrap_or(0.0),
        "tandrunkzoffset" => visual.tan_drunk_z_offset.unwrap_or(0.0),
        "tandrunkzspeed" => visual.tan_drunk_z_speed.unwrap_or(0.0),
        "tandrunkzperiod" => visual.tan_drunk_z_period.unwrap_or(0.0),
        "dizzyholds" => f32::from(visual.dizzy_holds.unwrap_or(false)),
        "zbuffer" => f32::from(visual.z_buffer.unwrap_or(false)),
        "cosecant" => f32::from(visual.cosecant.unwrap_or(false)),
        "drawsize" => visual.draw_size.unwrap_or(0.0),
        "drawsizeback" => visual.draw_size_back.unwrap_or(0.0),
        "digital" => visual.digital.unwrap_or(0.0),
        "digitalsteps" => visual.digital_steps.unwrap_or(0.0),
        "digitaloffset" => visual.digital_offset.unwrap_or(0.0),
        "digitalperiod" => visual.digital_period.unwrap_or(0.0),
        "zigzag" => visual.zigzag.unwrap_or(0.0),
        "zigzagz" => visual.zigzag_z.unwrap_or(0.0),
        "zigzagoffset" => visual.zigzag_offset.unwrap_or(0.0),
        "zigzagzoffset" => visual.zigzag_z_offset.unwrap_or(0.0),
        "zigzagperiod" => visual.zigzag_period.unwrap_or(0.0),
        "zigzagzperiod" => visual.zigzag_z_period.unwrap_or(0.0),
        "square" => visual.square.unwrap_or(0.0),
        "squareoffset" => visual.square_offset.unwrap_or(0.0),
        "squareperiod" => visual.square_period.unwrap_or(0.0),
        "squarez" => visual.square_z.unwrap_or(0.0),
        "squarezoffset" => visual.square_z_offset.unwrap_or(0.0),
        "squarezperiod" => visual.square_z_period.unwrap_or(0.0),
        "xmode" => visual.xmode.unwrap_or(0.0),
        "bounce" => visual.bounce.unwrap_or(0.0),
        "bounceperiod" => visual.bounce_period.unwrap_or(0.0),
        "bounceoffset" => visual.bounce_offset.unwrap_or(0.0),
        "tornadoperiod" => visual.tornado_period.unwrap_or(0.0),
        "tornadooffset" => visual.tornado_offset.unwrap_or(0.0),
        "parabolaz" => visual.parabola_z.unwrap_or(0.0),
        "confusion" => visual.confusion.unwrap_or(0.0),
        "confusionoffset" => visual.confusion_offset.unwrap_or(0.0),
        "confusionxoffset" => visual.confusion_x_offset.unwrap_or(0.0),
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
        "beatperiod" => visual.beat_period.unwrap_or(0.0),
        "pulseoffset" => visual.pulse_offset.unwrap_or(0.0),
        "randomspeed" => visual.random_speed.unwrap_or(0.0),
        "brake" => runtime.accel[player].brake.unwrap_or(0.0),
        "boost" => runtime.accel[player].boost.unwrap_or(0.0),
        "wave" => runtime.accel[player].wave.unwrap_or(0.0),
        "waveperiod" => runtime.accel[player].wave_period.unwrap_or(0.0),
        "expand" => runtime.accel[player].expand.unwrap_or(0.0),
        "boomerang" => runtime.accel[player].boomerang.unwrap_or(0.0),
        "hidden" => appearance.hidden,
        "hiddenoffset" => appearance.hidden_offset,
        "stealth" => appearance.stealth,
        "stealthtype" => f32::from(appearance.stealth_type),
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
        let timing = if context.song_timing.is_some() {
            context.player_timing[player].as_ref().unwrap_or(&timing)
        } else {
            &timing
        };
        compiled
            .iter()
            .flat_map(|layer| {
                deadsync_song_lua::gameplay::build_song_lua_constant_windows_for_player(
                    layer, timing, player, 0.0,
                )
            })
            .collect::<Vec<_>>()
    });
    let eases = std::array::from_fn(|player| {
        let timing = if context.song_timing.is_some() {
            context.player_timing[player].as_ref().unwrap_or(&timing)
        } else {
            &timing
        };
        compiled
            .iter()
            .flat_map(|layer| {
                let (eases, unsupported) =
                    deadsync_song_lua::gameplay::build_song_lua_ease_windows_for_player(
                        layer,
                        timing,
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
fn zero_approach_keeps_current_and_audits_song_target() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace =
        read_trace_file(&root.join("tests/fixtures/itgmania-song-lua-micro/zero-approach.json"));
    let simfile = root.join("tests/fixtures/song-lua/zero-approach.sm");
    let (compiled, primary, context) = compile_trace_song_at(&trace, &simfile);
    let mut parity = compare_semantics(&trace, &compiled, primary, &context);
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    parity.assert_complete("zero approach");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for second in [0.5, 1.5, 2.5] {
        runtime.refresh_player(
            0,
            second,
            1.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(runtime_mod_value(&runtime, 0, "drunk"), Some(1.0));
    }
    let mut missing = compiled.clone();
    for layer in &mut missing {
        layer
            .eases
            .retain(|window| window.approach_speed != Some(0.0));
    }
    let mut missing_parity = Parity::default();
    compare_runtime_modifiers(&trace, &missing, &context, &mut missing_parity);
    assert_eq!(missing_parity.checks() - missing_parity.passed(), 2);
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
fn hidden_actor_tweens_drive_modifiers_without_probe_state() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let mut context = SongLuaCompileContext::new(&directory, "Signal tween");
    context.song_timing_bpms = vec![(0.0, 60.0)];
    context.music_length_seconds = 3.0;
    let compiled =
        compile_song_lua_layers(&[directory.join("signal-tween.lua").as_path()], 0, &context)
            .expect("compile hidden signal actor");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.5, 0.0),
        (1.0, 0.0),
        (1.25, 0.25),
        (1.5, 1.0 / 3.0),
        (1.75, 1.0 / 3.0),
        (2.0, 7.0 / 12.0),
        (2.25, 2.0 / 3.0),
    ] {
        let _ = runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        for (key, expected) in [("invert", expected), ("drunk", 0.0), ("wave", 0.0)] {
            let actual = runtime_mod_value(&runtime, 0, key).unwrap();
            assert!(
                (actual - expected).abs() <= 1e-6,
                "{key} at {second}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn song_clock_uses_global_pauses() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let song = parse_song(&directory.join("song-clock.ssc"));
    let mut context = SongLuaCompileContext::new(&directory, "Song clock");
    context.song_timing_bpms = parse_song_timing_bpms(&song.normalized_bpms);
    context.song_timing = song.song_timing.clone();
    assert!(
        context.song_timing.is_some(),
        "retain global timing at song load"
    );
    // The fixture's selected Steps has its own 180 BPM timing. Global song
    // position and sampled modifier timestamps must remain independent of it.
    context.player_timing[0] = Some(deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 180.0)],
            ..Default::default()
        },
        &[],
    ));
    context.music_length_seconds = 6.0;
    for rate in [1.0, 2.0] {
        context.song_music_rate = rate;
        let compiled =
            compile_song_lua_layers(&[directory.join("song-clock.lua").as_path()], 0, &context)
                .unwrap();
        let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
        assert_eq!(unsupported, 0);
        for (second, invert, freeze, delay, speed, x) in [
            (1.2, 0.12, 0.0, 0.0, 1.0, 0.0),
            (2.3, 0.2, 1.0, 0.0, 1.0, 0.375),
            (2.4, 0.2, 1.0, 0.0, 1.0, 0.5),
            (3.6, 0.3, 0.0, 1.0, 1.0, 0.5),
            (4.0, 0.325, 0.0, 0.0, 1.0, 0.5),
            (4.8, 0.41, 0.0, 0.0, 2.0, 0.5),
            (5.0, 0.45, 0.0, 0.0, 2.0, 0.5),
            (5.5, 0.65, 0.0, 0.0, 2.0, 0.5),
        ] {
            let x = if rate == 2.0 && (second == 2.3 || second == 2.4) {
                0.25
            } else {
                x
            };
            let _ = runtime.refresh_player(
                0,
                second,
                1_000_000.0,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
            for (key, expected) in [
                ("invert", invert),
                ("drunk", freeze),
                ("wave", delay),
                ("xmod", speed),
                ("tornado", x),
            ] {
                let actual = runtime_mod_value(&runtime, 0, key).unwrap();
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "{key} at {second}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn song_clock_retains_native_float_rounding() {
    crate::paths::init();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/song-lua");
    let song = parse_song(&directory.join("song-clock-float.ssc"));
    let mut context = SongLuaCompileContext::new(&directory, "Float song clock");
    context.song_timing_bpms = parse_song_timing_bpms(&song.normalized_bpms);
    context.song_timing = song.song_timing.clone();
    context.music_length_seconds = 34.0;
    let compiled =
        compile_song_lua_layers(&[directory.join("song-clock.lua").as_path()], 0, &context)
            .expect("compile float song clock");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (seconds, beat, expected) in [
        (32.5, 74.58333587646484_f32, 7.458333492279053_f32),
        (32.75, 74.95417022705078, 7.49541711807251),
        (33.0, 75.32500457763672, 7.532500267028809),
    ] {
        assert_eq!(
            context
                .song_timing
                .as_ref()
                .expect("float clock")
                .get_song_position(seconds)
                .beat,
            beat
        );
        let _ = runtime.refresh_player(
            0,
            seconds,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        // Runtime modifier composition can round by one ULP; the native song
        // position itself is checked bit for bit above.
        assert!((runtime_mod_value(&runtime, 0, "invert").unwrap() - expected).abs() < 1e-6);
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
    assert!(parity.checks() >= 5, "audit replacement strings and omitted numeric resets");
    parity.assert_complete("state option string targets");
    let mut stale = compiled.clone();
    stale[0].eases.retain(|window| !matches!(&window.target,
        deadsync_song_lua::SongLuaEaseTarget::Mod(key) if key == "drunk" && window.to == 0.0));
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &stale, &context, &mut parity);
    assert!(!parity.gaps.is_empty(), "the audit rejects an omitted replacement reset");
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
fn clearall_audit_checks_reset_and_later_writes() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create reset fixture");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('3x, 125% drunk, modtimerbeat')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:FromString('clearall')
            phase = 2
        elseif phase == 2 and beat >= 1.5 then
            options:FromString('clearall, C500, 25% drunk')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write reset fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "ClearAll");
    context.song_timing_bpms = vec![(0.0, 60.0)];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile production reset targets");
    let mut trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    trace.enabled_players = Some([true, false]);
    trace.timeline_tracks = vec![NativeTimelineTrack {
        kind: "modifier".into(),
        actor: Some("player-state:PLAYER_1/options:ModsLevel_Song".into()),
        operation: "PlayerOptions.FromString".into(),
        samples: [
            (0.0, "3x, 125% drunk, modtimerbeat"),
            (1.0, "clearall"),
            (1.5, "clearall, C500, 25% drunk"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (beat, mods))| {
            (
                i as u64,
                Some(beat),
                Some(beat),
                vec![serde_json::json!(mods)],
                None,
            )
        })
        .collect(),
    }];
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(
        parity.checks(),
        17,
        "retain resets and final same-frame speed mode"
    );
    parity.assert_complete("native Init defaults and subsequent FromString writes");
    let mut wrong = compiled;
    let mut changed = 0;
    for ease in &mut wrong[0].eases {
        if matches!(&ease.target, SongLuaEaseTarget::Mod(key) if key == "drunk")
            && ease.start >= 1.0
            && ease.to == 0.0
        {
            ease.from = 0.25;
            ease.to = 0.25;
            changed += 1;
        }
    }
    assert!(changed > 0, "mutate the sampled clearall reset");
    let mut rejected = Parity::default();
    compare_runtime_modifiers(&trace, &wrong, &context, &mut rejected);
    assert!(
        !rejected.gaps.is_empty(),
        "incorrect reset values must fail the audit"
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
    options:FromString('*2 250% drunkoffset, *4 -50% drunkspeed, *3 -99% drunkperiod, *5 250% tipsyoffset, *6 -50% tipsyspeed, *7 150% hiddenoffset, *8 400% beatperiod')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:DrunkOffset(200):DrunkSpeed(1.5):DrunkPeriod(0.75)
            options:TipsyOffset(-1):TipsySpeed(-1)
            options:HiddenOffset(-0.5)
            options:BeatPeriod(-0.5)
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
        (0.25, [2.5, -0.5, -0.99, 2.5, -0.5, 1.5, 4.0]),
        (0.5, [200.0, 1.5, 0.75, -1.0, -1.0, -0.5, -0.5]),
        (1.0, [0.0; 7]),
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
            "beatperiod",
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
fn roll_survives_lua_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Roll fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 -250% roll')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:Roll(7, 4)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Roll fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Roll");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Roll fixture");
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
        assert_eq!(runtime_mod_value(&runtime, 0, "roll"), Some(expected));
        assert_eq!(runtime_mod_value(&runtime, 1, "roll"), Some(0.0));
    }
}

#[test]
fn perspective_aliases_compile_into_shared_gameplay_targets() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("perspective fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local o = player:GetPlayerOptions('ModsLevel_Song')
local p2 = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    assert(o:Incoming() == 0.5 and o:Tilt() == -0.25)
    assert(p2:Space() == -0.75)
    o:FromString('*3 50% incoming')
    p2:Space(-0.25, 4)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            o:Space(0.75, 5, true):Overhead(false, 9, true)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            o:Hallway(0.5, 4)
            phase = 3
        elseif phase == 3 and beat >= 3 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 4
        end
    end)
end}
"#,
    )
    .expect("write perspective fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Perspective");
    context.players[0].perspective = deadsync_gameplay::PerspectiveEffects {
        tilt: -0.25,
        skew: 0.5,
    };
    context.players[1].perspective = deadsync_gameplay::PerspectiveEffects {
        tilt: -0.75,
        skew: -0.75,
    };
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile perspective fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.25, [-0.5, 0.5]),
        (0.5, [0.75, 0.75]),
        (1.0, [-0.5, 0.0]),
        (1.5, [0.0, 0.0]),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                1_000_000.0,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (key, value) in ["tilt", "skew"].into_iter().zip(expected) {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(value),
                "{key} at {second}"
            );
            assert_eq!(runtime_mod_value(&runtime, 1, key), Some(-0.25));
        }
    }
}

#[test]
fn xmode_survives_lua_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Xmode fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:FromString('*2 -250% xmode')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:Xmode(7, 4)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Xmode fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Xmode");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Xmode fixture");
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
        assert_eq!(runtime_mod_value(&runtime, 0, "xmode"), Some(expected));
        assert_eq!(runtime_mod_value(&runtime, 1, "xmode"), Some(0.0));
    }
}

#[test]
fn bumpy_variants_survive_lua_methods_strings_approach_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Bumpy fixture");
    let entry = directory.path().join("default.lua");
    fs::write(&entry, r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local names = {"BumpyX","BumpyXOffset","BumpyXPeriod","TanBumpy","TanBumpyOffset","TanBumpyPeriod","TanBumpyX","TanBumpyXOffset","TanBumpyXPeriod"}
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    for i, name in ipairs(names) do
        local amount, speed = options[name](options)
        assert(amount == 0 and speed == 1 and select('#', options[name](options)) == 2)
        assert(options[name](options, -i/4, i/2, true) == options)
        amount, speed = options[name](options)
        assert(amount == -i/4 and speed == i/2)
        other:FromString('*9999 '..i * 12.5 ..'% '..string.lower(name))
    end
    assert(options:Cosecant(true) == false and options:Cosecant() == true)
    assert(options:Cosecant(0) == true and options:Cosecant() == true)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            for i, name in ipairs(names) do options:FromString('*9999 '..i * 25 ..'% '..string.lower(name)) end
            assert(options:Cosecant(false, false) == options and not options:Cosecant())
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#).expect("write Bumpy fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Bumpy variants");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Bumpy fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, dt, multiplier, csc) in [
        (0.25, 0.25, -0.125, 1.0),
        (0.5, 1_000_000.0, 0.25, 0.0),
        (1.0, 1_000_000.0, 0.0, 0.0),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                dt,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (i, key) in [
            "bumpyx",
            "bumpyxoffset",
            "bumpyxperiod",
            "tanbumpy",
            "tanbumpyoffset",
            "tanbumpyperiod",
            "tanbumpyx",
            "tanbumpyxoffset",
            "tanbumpyxperiod",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some((i + 1) as f32 * multiplier),
                "{key} at {second}"
            );
            assert_eq!(
                runtime_mod_value(&runtime, 1, key),
                Some((i + 1) as f32 / 8.0)
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 0, "cosecant"), Some(csc));
    }
}

#[test]
fn drunk_variants_survive_lua_methods_strings_approach_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Drunk fixture");
    let entry = directory.path().join("default.lua");
    fs::write(&entry, r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local names = {"DrunkZ","DrunkZOffset","DrunkZSpeed","DrunkZPeriod","TanDrunk","TanDrunkOffset","TanDrunkSpeed","TanDrunkPeriod","TanDrunkZ","TanDrunkZOffset","TanDrunkZSpeed","TanDrunkZPeriod"}
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    for i, name in ipairs(names) do
        local amount, speed = options[name](options)
        assert(amount == 0 and speed == 1 and select('#', options[name](options)) == 2)
        assert(options[name](options, -i/4, i/2, true) == options)
        amount, speed = options[name](options)
        assert(amount == -i/4 and speed == i/2)
        other:FromString('*9999 '..i * 12.5 ..'% '..string.lower(name))
    end
    assert(options:Cosecant(true) == false and options:Cosecant() == true)
    assert(options:Cosecant(0) == true and options:Cosecant() == true)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            for i, name in ipairs(names) do options:FromString('*9999 '..i * 25 ..'% '..string.lower(name)) end
            assert(options:Cosecant(false, false) == options and not options:Cosecant())
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#).expect("write Drunk fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Drunk variants");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Drunk fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, dt, multiplier, csc) in [
        (0.25, 0.25, -0.125, 1.0),
        (0.5, 1_000_000.0, 0.25, 0.0),
        (1.0, 1_000_000.0, 0.0, 0.0),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                dt,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (i, key) in [
            "drunkz",
            "drunkzoffset",
            "drunkzspeed",
            "drunkzperiod",
            "tandrunk",
            "tandrunkoffset",
            "tandrunkspeed",
            "tandrunkperiod",
            "tandrunkz",
            "tandrunkzoffset",
            "tandrunkzspeed",
            "tandrunkzperiod",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some((i + 1) as f32 * multiplier),
                "{key} at {second}"
            );
            assert_eq!(
                runtime_mod_value(&runtime, 1, key),
                Some((i + 1) as f32 / 8.0)
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 0, "cosecant"), Some(csc));
    }
}

#[test]
fn dizzy_holds_survives_lua_boolean_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create DizzyHolds fixture");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    assert(options:DizzyHolds() == false and select('#', options:DizzyHolds()) == 1)
    assert(options:DizzyHolds(true) == false and options:DizzyHolds() == true)
    for _, invalid in ipairs({0, 1, 'false', 'true'}) do
        assert(options:DizzyHolds(invalid) == true and options:DizzyHolds() == true)
    end
    assert(options:DizzyHolds(nil, false) == options)
    assert(options:DizzyHolds(false, 0, true) == true and options:DizzyHolds() == false)
    assert(options:DizzyHolds(true, true) == options)
    other:FromString('*0 51% dizzyholds')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            assert(options:DizzyHolds(false, false) == options)
            phase = 2
        elseif phase == 2 and beat >= 1.5 then
            options:FromString('*0 50% dizzyholds')
            assert(options:DizzyHolds() == false)
            options:FromString('*0 51% dizzyholds')
            assert(options:DizzyHolds() == true)
            phase = 3
        elseif phase == 3 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            assert(options:DizzyHolds() == false)
            phase = 4
        end
    end)
end}
"#,
    )
    .expect("write DizzyHolds fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "DizzyHolds");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile DizzyHolds fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.25, 1.0),
        (f32::from_bits(0.5f32.to_bits() - 1), 1.0),
        (0.5, 0.0),
        (0.75, 1.0),
        (1.0, 0.0),
        (1.25, 0.0),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                0.0,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        assert_eq!(
            runtime_mod_value(&runtime, 0, "dizzyholds"),
            Some(expected),
            "{second}"
        );
        assert_eq!(
            runtime_mod_value(&runtime, 1, "dizzyholds"),
            Some(1.0),
            "independent P2 at {second}"
        );
    }
}

#[test]
fn boolean_options_survive_methods_strings_and_fresh_options() {
    crate::paths::init();
    for (method, key) in [("StealthType", "stealthtype"), ("ZBuffer", "zbuffer")] {
        let directory = tempfile::tempdir().expect("create StealthType fixture");
        let entry = directory.path().join("default.lua");
        fs::write(
            &entry,
            r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    assert(options:StealthType() == false and select('#', options:StealthType()) == 1)
    assert(options:StealthType(true) == false and options:StealthType() == true)
    for _, invalid in ipairs({0, 1, 'false', 'true'}) do
        assert(options:StealthType(invalid) == true and options:StealthType() == true)
    end
    assert(options:StealthType(nil, false) == options)
    assert(options:StealthType(false, 0, true) == true and options:StealthType() == false)
    assert(options:StealthType(true, true) == options)
    other:FromString('*0 51% stealthtype')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            assert(options:StealthType(false, false) == options)
            phase = 2
        elseif phase == 2 and beat >= 1.5 then
            options:FromString('*0 50% stealthtype')
            assert(options:StealthType() == false)
            options:FromString('*0 51% stealthtype')
            assert(options:StealthType() == true)
            phase = 3
        elseif phase == 3 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            assert(options:StealthType() == false)
            phase = 4
        elseif phase == 4 and beat >= 2.5 then
            options:StealthType(true)
            phase = 5
        elseif phase == 5 and beat >= 3 then
            options:FromString('clearall')
            assert(options:StealthType() == false)
            phase = 6
        end
    end)
end}
"#
            .replace("StealthType", method)
            .replace("stealthtype", key),
        )
        .expect("write StealthType fixture");
        let mut context = SongLuaCompileContext::new(directory.path(), method);
        context.song_timing_bpms = vec![(0.0, 120.0)];
        context.music_length_seconds = 2.0;
        let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
            .expect("compile StealthType fixture");
        let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
        assert_eq!(unsupported, 0);
        for (second, expected) in [
            (0.25, 1.0),
            (f32::from_bits(0.5f32.to_bits() - 1), 1.0),
            (0.5, 0.0),
            (0.75, 1.0),
            (1.0, 0.0),
            (1.25, 1.0),
            (1.5, 0.0),
            (1.75, 0.0),
        ] {
            for player in 0..2 {
                runtime.refresh_player(
                    player,
                    second,
                    0.0,
                    deadsync_gameplay::AppearanceEffects::default(),
                    AttackBaseEffects::default,
                    SongLuaPlayerTransform::default(),
                );
            }
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(expected),
                "{second}"
            );
            assert_eq!(
                runtime_mod_value(&runtime, 1, key),
                Some(1.0),
                "independent P2 at {second}"
            );
        }
    }
}

#[test]
fn rejected_strings_preserve_native_runtime_fields() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let trace_path = root.join("tests/fixtures/itgmania-song-lua-micro/rejected-option-parts-native.json");
    let trace = read_trace_file(&trace_path);
    let song_dir = root.join("tests/fixtures/song-lua");
    let entry = song_dir.join("rejected-option-parts.lua");
    let mut context = SongLuaCompileContext::new(&song_dir, "Rejected modifier parts");
    context.players[0].noteskin_name = "cyber".into();
    context.song_timing_bpms = vec![(0.0, 60.0)];
    context.music_length_seconds = 1.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile native rejected-part control");
    let (writes, unsupported) = option_writes(&trace);
    assert!(unsupported.is_empty());
    assert!(writes.iter().all(|write| write.key != "bumpperiod" && write.key != "completely_unknown"));
    assert!(writes.iter().any(|write| write.beat >= 0.5 && write.key == "bumpyperiod"
        && (write.value + 0.66).abs() < EPSILON), "retain native live fields after rejected text");
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert!(parity.checks() > 6);
    parity.assert_complete("native parser rejection and unchanged runtime fields");

    // An implementation that aliases the typo must fail at the rejected write,
    // while the earlier, correctly spelled native setter still matches.
    let directory = tempfile::tempdir().expect("changed rejected-part control");
    let wrong_entry = directory.path().join("control.lua");
    let source = fs::read_to_string(&entry).expect("native control Lua");
    let wrong_source = source.replace("BumpPeriod", "BumpyPeriod").lines()
        .filter(|line| !line.trim_start().starts_with("assert("))
        .collect::<Vec<_>>().join("\n");
    fs::write(&wrong_entry, wrong_source).expect("write deliberate invalid alias");
    let wrong = compile_song_lua_layers(&[wrong_entry.as_path()], 0, &context)
        .expect("compile deliberately changed option");
    let mut rejected = Parity::default();
    compare_runtime_modifiers(&trace, &wrong, &context, &mut rejected);
    assert!(rejected.gaps.iter().any(|gap| gap.contains("bumpyperiod")),
        "changed BumpyPeriod at the native no-op timestamp must fail");
    for mutation in 0..4 {
        let mut altered = read_trace_file(&trace_path);
        let detail = altered.timeline_tracks.iter_mut().flat_map(|track| &mut track.samples)
            .find_map(|sample| sample.4.as_mut().filter(|detail| detail["rejected_parts"].is_array()))
            .expect("native rejected-part detail");
        match mutation {
            0 => { detail.as_object_mut().expect("native detail").remove("rejected_parts"); }
            1 => detail["rejected_parts"][0]["unchanged"] = serde_json::json!(false),
            2 => detail["rejected_parts"][0]["values"] = serde_json::json!([]),
            _ => detail["rejected_parts"][0]["part"] = serde_json::json!("unrelated text"),
        }
        let mut rejected = Parity::default();
        compare_runtime_modifiers(&altered, &compiled, &context, &mut rejected);
        assert!(!rejected.gaps.is_empty(), "missing or altered native rejection evidence {mutation}");
    }
}

#[test]
fn noteskin_audit_rejects_missing_and_altered_writes() {
    let mut trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    trace.enabled_players = Some([true, false]);
    trace.timeline_tracks = vec![NativeTimelineTrack {
        kind: "modifier".into(), actor: Some("player-state:PLAYER_1/options:ModsLevel_Song".into()),
        operation: "PlayerOptions.FromString".into(),
        samples: [0.1_f32, 0.15].into_iter().enumerate().map(|(index, second)| (
            index as u64 + 1, Some(second * 2.0), Some(second), vec![serde_json::json!("CyBeR")],
            Some(serde_json::json!({ "noteskin_option": {
                "previous": if index == 0 { "cel" } else { "cyber" }, "current": "cyber",
                "parts": [{ "part": "CyBeR", "target": "cyber" }],
            } }))
        )).collect(),
    }];
    let mut compiled = vec![CompiledSongLua {
        noteskin_writes: [0.1_f32, 0.15].into_iter().enumerate().map(|(index, second)|
            deadsync_song_lua::SongLuaSkinWrite {
                player: 0, key: "fromstring".into(), beat: f64::from(second * 2.0),
                second: f64::from(second), previous: if index == 0 { "cel" } else { "cyber" }.into(),
                current: "cyber".into(),
            }).collect(),
        ..CompiledSongLua::default()
    }];
    let mut parity = Parity::default();
    compare_noteskin_options(&trace, &compiled, &mut parity);
    parity.assert_complete("complete string-option audit");
    let (writes, unsupported) = option_writes(&trace);
    assert!(writes.is_empty() && unsupported.is_empty(), "native string evidence replaces numeric guessing");
    let correct = compiled[0].noteskin_writes.clone();
    for mutation in 0..5 {
        compiled[0].noteskin_writes.clone_from(&correct);
        match mutation {
            0 => { compiled[0].noteskin_writes.pop(); },
            1 => compiled[0].noteskin_writes[0].previous = "default".into(),
            2 => compiled[0].noteskin_writes[1].current = "CEL".into(),
            3 => compiled[0].noteskin_writes[0].second = 1.0,
            _ => compiled[0].noteskin_writes.swap(0, 1),
        }
        let mut rejected = Parity::default();
        compare_noteskin_options(&trace, &compiled, &mut rejected);
        assert!(!rejected.gaps.is_empty(), "reject changed noteskin sequence {mutation}");
    }
    compiled[0].noteskin_writes.clone_from(&correct);
    trace.timeline_tracks[0].samples[0].4.as_mut().expect("native detail")
        ["noteskin_option"]["parts"][0]["target"] = serde_json::json!("default");
    let mut rejected = Parity::default();
    compare_noteskin_options(&trace, &compiled, &mut rejected);
    assert!(!rejected.gaps.is_empty(), "reject inconsistent native classification");
    for track in &mut trace.timeline_tracks {
        for sample in &mut track.samples { sample.4 = None; }
    }
    let (writes, _) = option_writes(&trace);
    assert_eq!(writes.len(), 2, "old captures cannot silently excuse Cyber");
}

#[test]
fn boolean_audit_keeps_repeated_writes() {
    let mut trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    trace.enabled_players = Some([true, false]);
    trace.timeline_tracks = vec![NativeTimelineTrack {
        kind: "modifier".into(),
        actor: Some("player-state:PLAYER_1/options:ModsLevel_Song".into()),
        operation: "PlayerOptions.StealthPastReceptors".into(),
        samples: [0.1_f32, 0.15].into_iter().enumerate().map(|(index, second)| (
            index as u64 + 1, Some(second * 2.0), Some(second),
            vec![serde_json::json!(true)], Some(serde_json::json!({
                "boolean_option": { "previous": index > 0, "current": true, "chained": false },
            })),
        )).collect(),
    }];
    let mut compiled = vec![CompiledSongLua {
        boolean_writes: [0.1_f32, 0.15]
            .into_iter()
            .enumerate()
            .map(|(index, second)| deadsync_song_lua::SongLuaBoolWrite {
                player: 0,
                key: "stealthpastreceptors".into(),
                beat: f64::from(second * 2.0),
                second: f64::from(second),
                previous: index > 0,
                current: true,
                chained: false,
            })
            .collect(),
        ..CompiledSongLua::default()
    }];
    let mut parity = Parity::default();
    compare_boolean_options(&trace, &compiled, &mut parity);
    assert_eq!(parity.checks(), 3, "count and both repeated writes");
    parity.assert_complete("complete boolean writes");
    let correct = compiled[0].boolean_writes.clone();
    for mutation in 0..4 {
        compiled[0].boolean_writes.clone_from(&correct);
        match mutation {
            0 => {
                compiled[0].boolean_writes.pop();
            }
            1 => compiled[0].boolean_writes[1].previous = false,
            2 => compiled[0].boolean_writes[1].chained = true,
            _ => compiled[0].boolean_writes.swap(0, 1),
        }
        let mut parity = Parity::default();
        compare_boolean_options(&trace, &compiled, &mut parity);
        assert!(
            !parity.gaps.is_empty(),
            "reject altered boolean sequence {mutation}"
        );
    }
}

#[test]
fn boolean_option_queries_are_not_modifier_targets() {
    let mut trace = read_trace_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_TRACE));
    trace.timeline_tracks = [
        "DizzyHolds",
        "Cosecant",
        "StealthPastReceptors",
        "StealthType",
        "ZBuffer",
    ]
    .into_iter()
    .map(|name| NativeTimelineTrack {
        kind: "modifier".into(),
        actor: Some("player-state:PLAYER_1/options:ModsLevel_Song".into()),
        operation: format!("PlayerOptions.{name}"),
        samples: [
            serde_json::json!([true]),
            serde_json::json!([false, false]),
            serde_json::json!([0]),
            serde_json::json!([1]),
            serde_json::json!(["true"]),
            serde_json::json!([null, false]),
        ]
        .into_iter()
        .enumerate()
        .map(|(seq, args)| {
            (
                seq as u64 + 1,
                Some(0.0),
                Some(0.0),
                args.as_array().expect("args").clone(),
                None,
            )
        })
        .collect(),
    })
    .collect();
    let (writes, unsupported) = option_writes(&trace);
    assert!(unsupported.is_empty());
    assert_eq!(writes.len(), 10);
    for key in [
        "dizzyholds",
        "cosecant",
        "stealthpastreceptors",
        "stealthtype",
        "zbuffer",
    ] {
        let values = writes
            .iter()
            .filter(|write| write.key == key)
            .map(|write| (write.sequence, write.value))
            .collect::<Vec<_>>();
        assert_eq!(values, [(1, 1.0), (2, 0.0)], "{key}");
    }
}

#[test]
fn draw_size_survives_lua_methods_strings_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create DrawSize fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    local value, speed = options:DrawSize()
    assert(value == 0 and speed == 1 and select('#', options:DrawSize()) == 2)
    local old, old_speed = options:DrawSize(3, 2)
    assert(old == 0 and old_speed == 1)
    local back, back_speed = options:DrawSizeBack(-1.5, 4)
    assert(back == 0 and back_speed == 1)
    assert(options:DrawSize() == 3 and select(2, options:DrawSize()) == 2)
    assert(options:DrawSizeBack() == -1.5 and select(2, options:DrawSizeBack()) == 4)
    other:DrawSize(-0.5, 2, true):DrawSizeBack(0.75, 3, true)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:FromString('*2 -150% drawsize,*4 300% drawsizeback')
            assert(options:DrawSize() == -1.5 and select(2, options:DrawSize()) == 2)
            assert(options:DrawSizeBack() == 3 and select(2, options:DrawSizeBack()) == 4)
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write DrawSize fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "DrawSize");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile DrawSize fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, dt, expected) in [
        (0.25, 0.25, [0.5, -1.0]),
        (0.5, 1_000_000.0, [-1.5, 3.0]),
        (1.0, 1_000_000.0, [0.0; 2]),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                dt,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (key, value) in ["drawsize", "drawsizeback"].into_iter().zip(expected) {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(value),
                "{key} at {second}"
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 1, "drawsize"), Some(-0.5));
        assert_eq!(runtime_mod_value(&runtime, 1, "drawsizeback"), Some(0.75));
    }
}

#[test]
fn square_family_survives_lua_strings_approach_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Square fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(&entry, r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    options:Square(-2.5, 2)
    options:SquareOffset(32, 4)
    options:SquarePeriod(-1, 6)
    options:SquareZ(6, 8)
    options:SquareZOffset(-64, 10)
    options:SquareZPeriod(1, 12)
    other:Square(0.75, 3)
    other:SquareZ(-0.5, 2)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:FromString('*2 700% square,*4 -1600% squareoffset,*6 200% squareperiod,*8 -300% squarez,*10 6400% squarezoffset,*12 -100% squarezperiod')
            phase = 2
        elseif phase == 2 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 3
        end
    end)
end}
"#).expect("write Square fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Square");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile Square fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, dt, expected) in [
        (0.25, 0.25, [-0.5, 1.0, -1.0, 2.0, -2.5, 1.0]),
        (0.5, 1_000_000.0, [7.0, -16.0, 2.0, -3.0, 64.0, -1.0]),
        (1.0, 1_000_000.0, [0.0; 6]),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                dt,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (key, value) in [
            "square",
            "squareoffset",
            "squareperiod",
            "squarez",
            "squarezoffset",
            "squarezperiod",
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(value),
                "{key} at {second}"
            );
        }
        assert_eq!(runtime_mod_value(&runtime, 1, "square"), Some(0.75));
        assert_eq!(runtime_mod_value(&runtime, 1, "squarez"), Some(-0.5));
    }
}

#[test]
fn square_fresh_options_start_on_native_frame() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create Square frame fixture directory");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 257 then
            player:SetPlayerOptions('ModsLevel_Song', '*1 20% square,*0.25 50% squarez,*1 20% TanDrunk')
            phase = 2
        elseif phase == 2 and beat >= 260 then
            player:SetPlayerOptions('ModsLevel_Song', 'no square,no squarez,no TanDrunk')
            phase = 3
        end
    end)
end}
"#,
    )
    .expect("write Square frame fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "Square frame");
    context.song_timing_bpms = vec![(0.0, 170.0)];
    context.music_length_seconds = 93.0;
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile Square frame fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    // ITGmania's 60 Hz frame 5443 is the first to reach beat 257;
    // frame 5506 reaches 260. Probe each boundary without a tolerance.
    for (second, expected) in [
        (((5443.0_f64 / 60.0) as f32).next_down(), [0.0, 0.0, 0.0]),
        ((5443.0_f64 / 60.0) as f32, [0.2, 0.5, 0.2]),
        (((5506.0_f64 / 60.0) as f32).next_down(), [0.2, 0.5, 0.2]),
        ((5506.0_f64 / 60.0) as f32, [0.0, 0.0, 0.0]),
    ] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        for (key, value) in ["square", "squarez", "tandrunk"].into_iter().zip(expected) {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(value),
                "{key} at {second}"
            );
        }
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
fn opening_warp_keeps_zero_delta_options() {
    use deadsync_rules::timing::{TimingData, TimingSegments, WarpSegment};
    crate::paths::init();
    let directory = tempfile::tempdir().expect("opening warp fixture");
    let entry = directory.path().join("default.lua");
    fs::write(
        &entry,
        r#"
local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{OnCommand=function(self)
    assert(GAMESTATE:GetSongBeat() == 0, 'startup clock must precede playback')
    self:SetUpdateFunction(function()
        if GAMESTATE:GetSongBeat() >= 12 then options:Dark(1) options:Stealth(0.5) end
    end)
end}
"#,
    )
    .expect("write opening warp Lua");
    let mut context = SongLuaCompileContext::new(directory.path(), "Opening warp");
    context.music_length_seconds = 1.0;
    context.song_timing_bpms = vec![(0.0, 190.0)];
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: context.song_timing_bpms.clone(),
            warps: vec![WarpSegment {
                beat: 0.0,
                length: 12.0,
            }],
            ..Default::default()
        },
        &[],
    );
    context.song_timing = Some(timing.clone());
    context.player_timing[0] = Some(timing);
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile opening warp");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for second in [0.0, 1.0 / 60.0, 0.5] {
        runtime.refresh_player(
            0,
            second,
            1_000_000.0,
            deadsync_gameplay::AppearanceEffects::default(),
            AttackBaseEffects::default,
            SongLuaPlayerTransform::default(),
        );
        assert_eq!(
            runtime_mod_value(&runtime, 0, "dark"),
            Some(1.0),
            "first update at {second}"
        );
        assert_eq!(runtime_mod_value(&runtime, 0, "stealth"), Some(0.5));
        assert_eq!(runtime_mod_value(&runtime, 1, "dark"), Some(0.0));
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
        // Music length is measured in song seconds. Cover the 60 seconds of
        // real-time updates below even at accelerated playback rates.
        context.music_length_seconds = 61.0 * rate;
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

/// Compare every update frame, carrying sparse native state through quiet frames.
pub(super) fn compare_player_frames(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    parity.section("player transform frames");
    let (mut runtime, unsupported) = modifier_runtime(compiled, context);
    parity.check(unsupported == 0, || {
        format!("{unsupported} unsupported player frame targets")
    });
    let origin = context
        .song_timing
        .as_ref()
        .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0));
    for track in &trace.player_render_tracks {
        let player = track.player - 1;
        if player >= 2 || !trace.enabled_players.unwrap_or([true; 2])[player] {
            continue;
        }
        let mut transform = SongLuaPlayerTransform::default();
        let mut prior = 0.0;
        let mut cursor = 0;
        for (frame, &(beat, seconds)) in trace.update_frames.iter().enumerate() {
            while track
                .transform_samples
                .get(cursor + 1)
                .is_some_and(|sample| sample[0].is_some_and(|index| index as usize <= frame))
            {
                cursor += 1;
            }
            let sample = track
                .transform_samples
                .get(cursor)
                .expect("native player state");
            let now = seconds as f32 + origin;
            if let Some(next) = runtime.refresh_player(
                player,
                now,
                now - prior,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                transform,
            ) {
                transform = next;
            }
            prior = now;
            let actual = [
                Some(
                    transform
                        .x
                        .unwrap_or(context.players[player].screen_x),
                ),
                Some(transform.y.unwrap_or(context.players[player].screen_y)),
                Some(transform.z),
                Some(transform.rotation_x),
                Some(transform.rotation_z),
                Some(transform.rotation_y),
                Some(transform.zoom_x),
                Some(transform.zoom_y),
                Some(transform.zoom_z),
                Some(transform.skew_x),
                Some(transform.skew_y),
            ];
            for (axis, (expected, actual)) in sample[1..].iter().zip(actual).enumerate() {
                parity.check(expected.zip(actual).is_some_and(|(expected, actual)|
                    (expected - actual).abs() <= EPSILON), || format!(
                    "P{} frame {frame} beat {beat} transform axis {axis}: native {expected:?}, DeadSync {actual:?}", player + 1));
            }
        }
    }
}

fn compare_boolean_options(trace: &NativeTrace, compiled: &[CompiledSongLua], parity: &mut Parity) {
    let mut expected = BTreeMap::<(usize, String), Vec<(f32, f32, &Value)>>::new();
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
        let Some(key) = track.operation.strip_prefix("PlayerOptions.") else {
            continue;
        };
        for (_, beat, second, _, detail) in &track.samples {
            if let (Some(beat), Some(second), Some(state)) = (
                beat,
                second,
                detail
                    .as_ref()
                    .and_then(|value| value.get("boolean_option")),
            ) {
                expected
                    .entry((player, key.to_ascii_lowercase()))
                    .or_default()
                    .push((*beat, *second, state));
            }
        }
    }
    if expected.is_empty() {
        return;
    }
    parity.section("boolean option API");
    for ((player, key), expected) in expected {
        let actual = compiled
            .iter()
            .flat_map(|layer| &layer.boolean_writes)
            .filter(|write| write.player == player && write.key == key)
            .collect::<Vec<_>>();
        parity.check(actual.len() == expected.len(), || {
            format!(
                "P{} {key} boolean write count: native {}, DeadSync {}",
                player + 1,
                expected.len(),
                actual.len()
            )
        });
        for (index, (beat, second, state)) in expected.iter().enumerate() {
            parity.check(actual.get(index).is_some_and(|write| {
                (write.beat - f64::from(*beat)).abs() < f64::from(EPSILON)
                    && (write.second - f64::from(*second)).abs() < f64::from(EPSILON)
                    && state.get("previous").and_then(Value::as_bool) == Some(write.previous)
                    && state.get("current").and_then(Value::as_bool) == Some(write.current)
                    && state.get("chained").and_then(Value::as_bool) == Some(write.chained)
            }), || format!("P{} {key} boolean getter/return state differs at beat {beat}: native {state}, DeadSync {:?}",
                player + 1, actual.get(index)));
        }
    }
}

fn compare_noteskin_options(trace: &NativeTrace, compiled: &[CompiledSongLua], parity: &mut Parity) {
    parity.section("noteskin option API");
    let mut expected = BTreeMap::<(usize, String), Vec<(u64, f32, f32, &Value)>>::new();
    for track in &trace.timeline_tracks {
        let state_setter = track.operation == "PlayerState.SetPlayerOptions";
        let Some(player) = (0..2).find(|player| track.actor.as_deref() == Some(
            if state_setter { format!("player-state:PLAYER_{}", player + 1) }
            else { format!("player-state:PLAYER_{}/options:ModsLevel_Song", player + 1) }.as_str()
        )) else { continue };
        if !trace.enabled_players.unwrap_or([true; 2])[player] { continue; }
        let key = if state_setter { "setplayeroptions".to_owned() }
            else if let Some(key) = track.operation.strip_prefix("PlayerOptions.") {
                key.to_ascii_lowercase()
            } else { continue };
        for (sequence, beat, second, args, detail) in &track.samples {
            if state_setter && args.first().and_then(Value::as_str) != Some("ModsLevel_Song") {
                continue;
            }
            if let Some(state) = detail.as_ref().and_then(|detail| detail.get("noteskin_option")) {
                if matches!(key.as_str(), "fromstring" | "setplayeroptions") {
                    let raw = args.get(usize::from(state_setter)).and_then(Value::as_str);
                    let parts = state["parts"].as_array();
                    // Require the native classifier and live getter to agree.
                    // Simply Love's Common default is cel when a raw skin is empty.
                    let target = parts.and_then(|parts| parts.last())
                        .and_then(|part| part["target"].as_str())
                        .map(|target| if target.is_empty() { "cel" } else { target })
                        // Native assignment preserves the prior skin when the
                        // parsed replacement omits a valid noteskin.
                        .or_else(|| state["previous"].as_str());
                    parity.check(parts.is_some_and(|parts| parts.iter().all(|part|
                        part["target"].is_string() && part["part"].as_str().is_some_and(|part|
                            raw.is_some_and(|raw| raw.split(',').any(|token| token.trim() == part)))))
                        && target.is_some() && target == state["current"].as_str(), || format!(
                            "P{} {key} native noteskin classification disagrees with its getter: {state}", player + 1));
                }
                parity.check(beat.is_some() && second.is_some(), ||
                    format!("P{} {key} noteskin call lacks a native clock", player + 1));
                if let (Some(beat), Some(second)) = (beat, second) {
                    expected.entry((player, key.clone())).or_default()
                        .push((*sequence, *beat, *second, state));
                }
            }
        }
    }
    if expected.is_empty() { return; }
    for ((player, key), mut expected) in expected {
        expected.sort_by_key(|sample| sample.0);
        let actual = compiled.iter().flat_map(|layer| &layer.noteskin_writes)
            .filter(|write| write.player == player && write.key == key).collect::<Vec<_>>();
        parity.check(actual.len() == expected.len(), || format!(
            "P{} {key} noteskin write count: native {}, DeadSync {}",
            player + 1, expected.len(), actual.len()));
        for (index, (_, beat, second, state)) in expected.iter().enumerate() {
            parity.check(actual.get(index).is_some_and(|write|
                (write.beat - f64::from(*beat)).abs() < f64::from(EPSILON)
                    && (write.second - f64::from(*second)).abs() < f64::from(EPSILON)
                    && state["previous"].as_str() == Some(write.previous.as_str())
                    && state["current"].as_str() == Some(write.current.as_str())
            ), || format!("P{} {key} noteskin state differs at beat {beat}: native {state}, DeadSync {:?}",
                player + 1, actual.get(index)));
        }
    }
}

fn native_speed_fields(value: &Value) -> Option<[[f32; 2]; 4]> {
    let rows = value.as_array().filter(|rows| rows.len() == 4)?;
    let mut out = [[0.0; 2]; 4];
    for (row, values) in rows.iter().zip(&mut out) {
        let row = row.as_array().filter(|row| row.len() == 2)?;
        for (field, value) in row.iter().zip(values) {
            *value = value_f32(Some(field)).filter(|value| value.is_finite())?;
        }
    }
    Some(out)
}

fn compare_speed_options(trace: &NativeTrace, compiled: &[CompiledSongLua], context: &SongLuaCompileContext, parity: &mut Parity) {
    let mut expected = BTreeMap::<(usize, String), Vec<(u64, f32, f32, &Value)>>::new();
    let mut playback = Vec::new();
    for track in &trace.timeline_tracks {
        let Some(player) = (0..2).find(|player| track.actor.as_deref()
            == Some(format!("player-state:PLAYER_{}/options:ModsLevel_Song", player + 1).as_str())) else { continue; };
        if !trace.enabled_players.unwrap_or([true; 2])[player] { continue; }
        let Some(key) = track.operation.strip_prefix("PlayerOptions.") else { continue; };
        for (sequence, beat, second, _, detail) in &track.samples {
            let Some(state) = detail.as_ref().and_then(|detail| detail.get("speed_option")) else { continue; };
            parity.check(beat.is_some() && second.is_some(), || format!("P{} {key} speed call lacks its native clock", player + 1));
            if let (Some(beat), Some(second)) = (beat, second) {
                expected.entry((player, key.to_ascii_lowercase())).or_default().push((*sequence, *beat, *second, state));
                playback.push((*sequence, *second, player, state));
            }
        }
    }
    if expected.is_empty() { return; }
    parity.section("speed option API");
    for ((player, key), mut expected) in expected {
        expected.sort_by_key(|sample| sample.0);
        let actual = compiled.iter().flat_map(|layer| &layer.speed_writes)
            .filter(|write| write.player == player && write.key == key).collect::<Vec<_>>();
        parity.check(actual.len() == expected.len(), || format!("P{} {key} speed call count: native {}, DeadSync {}", player + 1, expected.len(), actual.len()));
        for (index, (_, beat, second, state)) in expected.iter().enumerate() {
            let before = native_speed_fields(&state["previous"]);
            let after = native_speed_fields(&state["current"]);
            parity.check(actual.get(index).is_some_and(|write| {
                let fields_match = |native: Option<[[f32; 2]; 4]>, actual: [[f32; 2]; 4]| native.is_some_and(|native|
                    native.iter().flatten().zip(actual.iter().flatten()).all(|(expected, actual)| (expected - actual).abs() <= EPSILON));
                (write.beat - f64::from(*beat)).abs() <= f64::from(EPSILON)
                    && (write.second - f64::from(*second)).abs() <= f64::from(EPSILON)
                    && fields_match(before, write.previous) && fields_match(after, write.current)
                    && state["failed"].as_bool() == Some(write.failed)
                    && state["chained"].as_bool() == Some(write.chained)
            }), || format!("P{} {key} speed fields at beat {beat}: native {state}, DeadSync {:?}", player + 1, actual.get(index)));
        }
    }
    compare_speed_playback(compiled, context, playback, parity);
}

fn compare_speed_playback(compiled: &[CompiledSongLua], context: &SongLuaCompileContext, mut observations: Vec<(u64, f32, usize, &Value)>, parity: &mut Parity) {
    use deadsync_rules::scroll::ScrollSpeedSetting;
    parity.section("speed playback targets");
    observations.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let (mut runtime, unsupported) = modifier_runtime(compiled, context);
    parity.check(unsupported == 0, || format!("{unsupported} unsupported speed playback ease targets"));
    let origin = context.song_timing.as_ref().map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0));
    let mut cursor = 0;
    while cursor < observations.len() {
        let second = observations[cursor].1;
        let mut final_fields = [None; 2];
        while cursor < observations.len() && observations[cursor].1 == second {
            let (_, _, player, state) = observations[cursor];
            final_fields[player] = Some(&state["current"]);
            cursor += 1;
        }
        for (player, fields) in final_fields.into_iter().enumerate() {
            let Some(fields) = fields else { continue; };
            runtime.refresh_player(player, second + origin, 1_000_000.0, Default::default(), AttackBaseEffects::default, Default::default());
            let mut targets = ActiveAttackMaskValues { scroll_speed: runtime.scroll_speed[player], ..ActiveAttackMaskValues::new(Default::default()) };
            deadsync_gameplay::apply_song_lua_attack_eases(&mut targets, &mut Default::default(), &mut Default::default(), &runtime.song_lua_ease_windows[player], second + origin, 0.0);
            let expected = native_speed_fields(fields).and_then(|fields| match fields[0][0] {
                0.0 if fields[3][0] == 0.0 => Some(ScrollSpeedSetting::XMod(fields[1][0])),
                0.0 => Some(ScrollSpeedSetting::MMod(fields[3][0])),
                1.0 if fields[1][0] == 1.0 && fields[3][0] == 0.0 => Some(ScrollSpeedSetting::CMod(fields[2][0])),
                _ => None,
            });
            // Fractional spacing and mixed CMod/raw multipliers require actual
            // note-travel support. Keep them failing rather than forcing a mode.
            parity.check(expected.is_some() && targets.scroll_speed.is_some_and(|actual| expected.is_some_and(|expected| match (actual, expected) {
                (ScrollSpeedSetting::XMod(a), ScrollSpeedSetting::XMod(b))
                | (ScrollSpeedSetting::CMod(a), ScrollSpeedSetting::CMod(b))
                | (ScrollSpeedSetting::MMod(a), ScrollSpeedSetting::MMod(b)) => (a - b).abs() <= EPSILON,
                _ => false,
            })), || format!("P{} native speed fields {fields} at {second}s: playback {:?}, expected {expected:?}", player + 1, targets.scroll_speed));
        }
    }
}

/// One check per recorded player/option target at each native timestamp.
pub(super) fn compare_runtime_modifiers(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    compare_boolean_options(trace, compiled, parity);
    compare_noteskin_options(trace, compiled, parity);
    compare_speed_options(trace, compiled, context, parity);
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
    let mut perspective = [[0.0; 2]; 2];
    while cursor < writes.len() {
        let trace_second = writes[cursor].second;
        let second = trace_second
            + context
                .song_timing
                .as_ref()
                .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0));
        let mut last_writes = BTreeMap::new();
        let mut last_speed = [None; 2];
        while cursor < writes.len() && writes[cursor].second == trace_second {
            let write = &writes[cursor];
            if matches!(write.key.as_str(), "xmod" | "cmod" | "mmod") {
                last_speed[write.player] = Some(write);
            }
            match write.key.as_str() {
                "incoming" => perspective[write.player] = [-write.value, write.value],
                "space" => perspective[write.player] = [write.value; 2],
                "tilt" => perspective[write.player][0] = write.value,
                "skew" => perspective[write.player][1] = write.value,
                _ => {}
            }
            last_writes.insert((write.player, write.key.as_str()), write);
            cursor += 1;
        }
        // The oracle records Song targets. Current cannot settle to a target
        // with zero approach speed, even with an arbitrarily large delta.
        // Evaluate the authored targets directly through the production API;
        // actual Current progression is checked separately with real deltas.
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
            let mut targets = ActiveAttackMaskValues {
                accel: runtime.accel[player],
                visual: runtime.visual[player],
                visibility: runtime.visibility[player],
                scroll: runtime.scroll[player],
                perspective: runtime.perspective[player],
                scroll_speed: runtime.scroll_speed[player],
                mini_percent: runtime.mini_percent[player],
                ..ActiveAttackMaskValues::new(runtime.appearance[player])
            };
            let mut player_targets = SongLuaPlayerTransformValues::default();
            deadsync_gameplay::apply_song_lua_attack_eases(
                &mut targets,
                &mut runtime.appearance[player],
                &mut player_targets,
                &runtime.song_lua_ease_windows[player],
                second,
                0.0,
            );
            *transform = player_targets.resolve();
            runtime.accel[player] = targets.accel;
            runtime.visual[player] = targets.visual;
            runtime.visibility[player] = targets.visibility;
            runtime.scroll[player] = targets.scroll;
            runtime.perspective[player] = targets.perspective;
            runtime.scroll_speed[player] = targets.scroll_speed;
            runtime.mini_percent[player] = targets.mini_percent;
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
            } else if write.key == "tilt" {
                Some(perspective[write.player][0])
            } else if write.key == "skew" {
                Some(perspective[write.player][1])
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
            // Incoming and Space each count as one native call, but validate
            // both shared angles after the frame's final perspective setter.
            let (expected, actual) = if matches!(write.key.as_str(), "incoming" | "space") {
                let angles = perspective[write.player];
                let pairs = [
                    (angles[0], runtime_mod_value(&runtime, write.player, "tilt")),
                    (angles[1], runtime_mod_value(&runtime, write.player, "skew")),
                ];
                pairs
                    .into_iter()
                    .max_by(|a, b| {
                        (a.0 - a.1.unwrap_or(f32::NAN))
                            .abs()
                            .total_cmp(&(b.0 - b.1.unwrap_or(f32::NAN)).abs())
                    })
                    .expect("two perspective angles")
            } else if write.key == "confusionyoffset" {
                // PlayerOptions stores radians; the notefield consumes degrees.
                (
                    expected,
                    Some(transforms[write.player].confusion_y_offset.to_radians()),
                )
            } else {
                (
                    expected,
                    runtime_mod_value(&runtime, write.player, &write.key),
                )
            };
            let Some(actual) = actual else {
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

#[test]
fn confusion_y_matches_native_targets() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let entry = song_dir.join("confusion-y.lua");
    let trace: NativeTrace = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/confusion-y-native.json"))
            .expect("native confusion fixture"),
    )
    .expect("valid native confusion fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Confusion Y");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile confusion fixture");
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(parity.checks(), 260);
    parity.assert_complete("confusion Y");
}

#[test]
fn confusion_x_matches_native_targets() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let entry = song_dir.join("confusion-x.lua");
    let trace: NativeTrace = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/confusion-x-native.json"))
            .expect("native confusion fixture"),
    )
    .expect("valid native confusion fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Confusion X");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile confusion fixture");
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(parity.checks(), 36);
    parity.assert_complete("confusion X");
}

#[test]
fn bounce_tornado_match_native_targets() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let entry = song_dir.join("bounce-tornado.lua");
    let trace: NativeTrace = serde_json::from_slice(
        &fs::read(root.join("tests/fixtures/itgmania-song-lua-micro/bounce-tornado-native.json"))
            .expect("native motion fixture"),
    )
    .expect("valid native motion fixture");
    let mut context = SongLuaCompileContext::new(&song_dir, "Native player options");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let noteskin = trace.noteskin_reference.as_ref().expect("captured noteskin");
    for player in &mut context.players {
        player.noteskin_name = noteskin.skin.clone();
    }
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile motion fixture");
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert!(
        parity.checks() >= 48,
        "must compare setters, strings and resets"
    );
    parity.assert_complete("Bounce, Tornado suboptions and nonpositive XMod");
}

#[test]
fn startup_boolean_writes_match_native() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/startup-booleans-native.json"),
    );
    let mut context = SongLuaCompileContext::new(&song_dir, "Startup boolean options");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let noteskin = trace.noteskin_reference.as_ref().expect("captured noteskin");
    for player in &mut context.players {
        player.noteskin_name = noteskin.skin.clone();
    }
    let entry = song_dir.join("startup-booleans.lua");
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile startup boolean options");
    let writes = &compiled[0].boolean_writes;
    assert_eq!(writes.len(), 8, "retain Init, On, repeated false and queued writes");
    assert!(writes[..6].iter().all(|write| write.second == 0.0));
    assert!(writes[6..].iter().all(|write| write.second >= 0.5));
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    assert_eq!(parity.checks(), 12, "retain every native boolean call");
    parity.assert_complete("startup and queued boolean writes");
    let mut missing = compiled.clone();
    missing[0].boolean_writes.remove(0);
    let mut rejected = Parity::default();
    compare_runtime_modifiers(&trace, &missing, &context, &mut rejected);
    assert!(!rejected.gaps.is_empty(), "losing an Init write must fail");
}

#[test]
fn native_speed_fields_drive_playback() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/speed-fields-native.json"),
    );
    let mut context = SongLuaCompileContext::new(&song_dir, "Native speed-fields control");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let noteskin = trace.noteskin_reference.as_ref().expect("captured noteskin");
    for player in &mut context.players {
        player.noteskin_name = noteskin.skin.clone();
    }
    let entry = song_dir.join("speed-fields.lua");
    // These same Lua getter assertions passed in the linked native capture.
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("native shared speed fields, getters and approach speeds");
    let (runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, expected) in [
        (0.0, deadsync_rules::scroll::ScrollSpeedSetting::XMod(4.0)),
        (1.1, deadsync_rules::scroll::ScrollSpeedSetting::XMod(-0.5)),
        (2.1, deadsync_rules::scroll::ScrollSpeedSetting::CMod(480.0)),
        (3.1, deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0)),
    ] {
        for player in 0..2 {
            let mut targets = ActiveAttackMaskValues::new(Default::default());
            deadsync_gameplay::apply_song_lua_attack_eases(
                &mut targets, &mut Default::default(), &mut Default::default(),
                &runtime.song_lua_ease_windows[player], second, 0.0,
            );
            assert_eq!(targets.scroll_speed, Some(expected), "P{} at {second}s", player + 1);
        }
    }
}

#[test]
fn speed_field_audit_keeps_failed_and_startup_writes() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/speed-fields-audit-native.json"),
    );
    let mut context = SongLuaCompileContext::new(&song_dir, "Native speed-fields control");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let noteskin = trace.noteskin_reference.as_ref().expect("captured noteskin");
    for player in &mut context.players {
        player.noteskin_name = noteskin.skin.clone();
    }
    let entry = song_dir.join("speed-fields.lua");
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile native shared speed-field control");
    let writes = &compiled[0].speed_writes;
    assert_eq!(writes.len(), 20, "retain ten speed setters per player");
    assert_eq!(writes.iter().filter(|write| write.failed).count(), 2);
    assert_eq!(writes.iter().filter(|write| write.second == 0.0).count(), 6);
    for write in writes.iter().filter(|write| write.failed) {
        assert_eq!(write.current[1], [2.0, 0.0], "failed approach retains its amount write");
    }
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    parity.assert_complete("native shared speed-field API and playback targets");
    for field in 0..4 {
        let mut altered = compiled.clone();
        altered[0].speed_writes[0].current[field][0] += 1.0;
        let mut rejected = Parity::default();
        compare_runtime_modifiers(&trace, &altered, &context, &mut rejected);
        assert!(!rejected.gaps.is_empty(), "changing speed field {field} must fail");
    }
    let mut missing = compiled.clone();
    missing[0].speed_writes.remove(0);
    let mut rejected = Parity::default();
    compare_runtime_modifiers(&trace, &missing, &context, &mut rejected);
    assert!(!rejected.gaps.is_empty(), "losing a startup setter must fail");
}

#[test]
fn wave_period_matches_native_targets() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let song_dir = root.join("tests/fixtures/song-lua");
    let trace = read_trace_file(
        &root.join("tests/fixtures/itgmania-song-lua-micro/wave-period-native.json"),
    );
    let mut context = SongLuaCompileContext::new(&song_dir, "Native wave-period control");
    context.screen_width = 854.0;
    context.music_length_seconds = 4.0;
    context.song_timing_bpms = vec![(0.0, 60.0)];
    let noteskin = trace.noteskin_reference.as_ref().expect("captured noteskin");
    for player in &mut context.players {
        player.noteskin_name = noteskin.skin.clone();
    }
    let entry = song_dir.join("wave-period.lua");
    let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
        .expect("compile Wave period, signed strengths and resets");
    let mut parity = Parity::default();
    compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    parity.assert_complete("native Wave period and signed strengths");
}

#[test]
fn mod_timer_survives_lua_selectors_approach_and_fresh_options() {
    crate::paths::init();
    let directory = tempfile::tempdir().expect("create timer fixture");
    let entry = directory.path().join("default.lua");
    fs::write(&entry, r#"
local player = GAMESTATE:GetPlayerState(PLAYER_1)
local options = player:GetPlayerOptions('ModsLevel_Song')
local other = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song')
local phase = 1
return Def.ActorFrame{OnCommand=function(self)
    assert(ModTimerType:GetName() == 'ModTimerType')
    assert(ModTimerType:Reverse()['song'] == 2 and ModTimerType:Reverse()[0] == 0)
    assert(options:ModTimerSetting() == 'ModTimerType_Default')
    assert(options:ModTimerSetting(ModTimerType[3]) == 'ModTimerType_Default')
    assert(options:ModTimerSetting() == 'ModTimerType_Song')
    options:ModTimerMult(1, 2, true):ModTimerOffset(-2, 4, true)
    other:FromString('*0 no modtimerbeat, *9999 -50% modtimermult, *9999 150% modtimeroffset')
    assert(other:ModTimerSetting() == 'ModTimerType_Beat')
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        if phase == 1 and beat >= 1 then
            options:ModTimerSetting('GaMe', true):ModTimerMult(-1, 9999, true):ModTimerOffset(3, 9999, true)
            phase = 2
        elseif phase == 2 and beat >= 1.5 then
            options:FromString('clearall')
            phase = 3
        elseif phase == 3 and beat >= 2 then
            player:SetPlayerOptions('ModsLevel_Song', 'modtimerbeat')
            phase = 4
        elseif phase == 4 and beat >= 3 then
            player:SetPlayerOptions('ModsLevel_Song', '')
            phase = 5
        end
    end)
end}
"#).expect("write timer fixture");
    let mut context = SongLuaCompileContext::new(directory.path(), "ModTimer");
    context.song_timing_bpms = vec![(0.0, 120.0)];
    context.music_length_seconds = 2.0;
    let compiled =
        compile_song_lua_layers(&[entry.as_path()], 0, &context).expect("compile timer fixture");
    let (mut runtime, unsupported) = modifier_runtime(&compiled, &context);
    assert_eq!(unsupported, 0);
    for (second, dt, mode, mult, offset) in [
        (0.25, 0.25, 2.0, 0.5, -1.0),
        (0.5, 1_000_000.0, 0.0, -1.0, 3.0),
        (0.75, 1_000_000.0, 3.0, 0.0, 0.0),
        (1.0, 1_000_000.0, 1.0, 0.0, 0.0),
        (1.5, 1_000_000.0, 3.0, 0.0, 0.0),
    ] {
        for player in 0..2 {
            runtime.refresh_player(
                player,
                second,
                dt,
                deadsync_gameplay::AppearanceEffects::default(),
                AttackBaseEffects::default,
                SongLuaPlayerTransform::default(),
            );
        }
        for (key, expected) in [
            ("modtimersetting", mode),
            ("modtimermult", mult),
            ("modtimeroffset", offset),
        ] {
            assert_eq!(
                runtime_mod_value(&runtime, 0, key),
                Some(expected),
                "{key} at {second}"
            );
        }
        for (key, expected) in [
            ("modtimersetting", 1.0),
            ("modtimermult", -0.5),
            ("modtimeroffset", 1.5),
        ] {
            assert_eq!(
                runtime_mod_value(&runtime, 1, key),
                Some(expected),
                "P2 {key}"
            );
        }
    }
}
