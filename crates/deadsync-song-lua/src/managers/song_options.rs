//! SongOptions values and the song half of ModsGroup operations.
//! GAMESTATE owns frame-clock validation and the ordering with player updates.
use super::catalog::{arg, float, raw, text};
use super::fields::{object, set};
use super::*;

pub(super) const SWITCHES: [&str; 8] = [
    "__songlua_assistclap",
    "__songlua_assistmetronome",
    "__songlua_staticbackground",
    "__songlua_randombgonly",
    "__songlua_savescore",
    "__songlua_savereplay",
    "__songlua_autosync",
    "__songlua_soundeffect",
];
const FLOATS: [(&str, &str); 2] = [
    ("__songlua_music_rate", "__songlua_music_rate_speed"),
    ("__songlua_haste", "__songlua_haste_speed"),
];
const AUTOSYNC: [&str; 4] = [
    "AutosyncType_Off",
    "AutosyncType_Song",
    "AutosyncType_Machine",
    "AutosyncType_Tempo",
];
pub(super) const LEVELS: [&str; 4] = [
    "ModsLevel_Preferred",
    "ModsLevel_Stage",
    "ModsLevel_Song",
    "ModsLevel_Current",
];

#[derive(Clone, Copy)]
enum Method {
    Float(usize),
    Boolean(usize),
    Autosync,
}

fn defaults(call: &mut HostCall<'_>, owner: TableId, rate: f32) -> Result<(), Error> {
    for (key, value) in [
        (FLOATS[0].0, rate),
        (FLOATS[0].1, 1.0),
        (FLOATS[1].0, 0.0),
        (FLOATS[1].1, 1.0),
    ] {
        set(owner, key, value, call)?;
    }
    for key in &SWITCHES[..6] {
        set(owner, *key, *key == "__songlua_savescore", call)?;
    }
    for key in &SWITCHES[6..] {
        set(owner, *key, 0_i32, call)?;
    }
    Ok(())
}
fn initialize(
    call: &mut HostCall<'_>,
    owner: TableId,
    rate: f32,
    tonumber: Value,
) -> Result<(), Error> {
    defaults(call, owner, rate)?;
    for (name, method) in [
        ("MusicRate", Method::Float(0)),
        ("Haste", Method::Float(1)),
        ("AssistClap", Method::Boolean(0)),
        ("AssistMetronome", Method::Boolean(1)),
        ("StaticBackground", Method::Boolean(2)),
        ("RandomBGOnly", Method::Boolean(3)),
        ("SaveScore", Method::Boolean(4)),
        ("SaveReplay", Method::Boolean(5)),
        ("AutosyncSetting", Method::Autosync),
    ] {
        let callback =
            call.new_host_function_with_data(binding, &[Value::Table(owner), tonumber], method)?;
        set(owner, name, callback, call)?;
    }
    Ok(())
}

pub(super) fn install(
    call: &mut HostCall<'_>,
    catalog: TableId,
    current: TableId,
    rate: f32,
) -> Result<(), Error> {
    let tonumber = call.captures()[1];
    let levels = object(call, |call, levels| {
        for index in 0..4 {
            let owner = if index == 3 {
                current
            } else {
                call.try_new_table()?
            };
            set(levels, index + 1, owner, call)?;
            initialize(call, owner, rate, tonumber)?;
        }
        Ok(())
    })?;
    set(catalog, "song_options", levels, call)?;
    // Keep native level references independently of the exposed array. The
    // operations are private host plumbing until GAMESTATE is installed.
    let mut captures = vec![tonumber];
    for index in 1..=4 {
        captures.push(call.raw_table_get(levels, Value::Number(index as f64))?);
    }
    let operations = object(call, |call, operations| {
        for (name, operation) in [
            ("Select", Operation::Select),
            ("Text", Operation::Text),
            ("Assign", Operation::Assign),
            ("Apply", Operation::Apply),
            ("Restart", Operation::Restart),
            ("ClearAttacks", Operation::ClearAttacks),
            ("Advance", Operation::Advance),
        ] {
            let callback = call.new_host_function_with_data(group, &captures, operation)?;
            set(operations, name, callback, call)?;
        }
        Ok(())
    })?;
    set(catalog, "song_option_operations", operations, call)
}

fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
pub(super) fn number(call: &mut HostCall<'_>, value: Value, tonumber: Value) -> Result<f64, Error> {
    let number = if matches!(value, Value::Number(_)) {
        value
    } else {
        call.call(tonumber, &[value])?
            .first()
            .copied()
            .unwrap_or(Value::Nil)
    };
    if let Value::Number(number) = number {
        Ok(number)
    } else {
        Err(call.error(b"number expected"))
    }
}
fn raw_float(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    tonumber: Value,
) -> Result<f32, Error> {
    let value = raw(call, owner, key)?;
    Ok(number(call, value, tonumber)? as f32)
}
fn raw_index(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    tonumber: Value,
) -> Result<usize, Error> {
    let value = raw(call, owner, key)?;
    let value = number(call, value, tonumber)?;
    // Legacy Lua 5.1 integer extraction truncates finite fractional values.
    // Values in (-1, 0) truncate to zero, including strings coerced to numbers.
    if value.is_finite() && value > -1.0 && value < usize::MAX as f64 + 1.0 {
        Ok(value as usize)
    } else {
        Err(call.error(b"unsigned option value out of range"))
    }
}
pub(super) fn enum_index(
    call: &mut HostCall<'_>,
    value: Value,
    names: &[&str],
    kind: &str,
) -> Result<usize, Error> {
    let result = match value {
        Value::Number(n) if n >= 0.0 && n < names.len() as f64 && n.fract() == 0.0 => {
            Some(n as usize)
        }
        Value::String(_) => text(call, value).and_then(|name| {
            names.iter().position(|full| {
                name == *full
                    || full
                        .split_once('_')
                        .is_some_and(|(_, suffix)| name.eq_ignore_ascii_case(suffix))
            })
        }),
        _ => None,
    };
    result.ok_or_else(|| call.error(format!("Expected {kind}").as_bytes()))
}
fn chaining(call: &HostCall<'_>) -> bool {
    call.arguments().len() > 1 && call.arguments().last() == Some(&Value::Boolean(true))
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let method = *call.function_data::<Method>().expect("song option method");
    let owner = table(call, call.captures()[0])?;
    let tonumber = call.captures()[1];
    match method {
        Method::Float(index) => {
            let (field, speed) = FLOATS[index];
            let previous = raw_float(call, owner, field, tonumber)?;
            let previous_speed = raw_float(call, owner, speed, tonumber)?;
            if let Some(value) = float(call, arg(call, 0)) {
                let valid = if index == 0 {
                    value > 0.0 && value <= 3.0
                } else {
                    (-1.0..=1.0).contains(&value)
                };
                if !valid {
                    return Err(call.error(if index == 0 {
                        b"Invalid MusicRate"
                    } else {
                        b"Invalid Haste"
                    }));
                }
                set(owner, field, value, call)?;
            }
            if let Some(value) = float(call, arg(call, 1)) {
                if value < 0.0 {
                    return Err(call.error(b"negative option speed"));
                }
                set(owner, speed, value, call)?;
            }
            if chaining(call) {
                call.push(Value::Table(owner))
            } else {
                call.push(Value::Number(previous as f64))?;
                call.push(Value::Number(previous_speed as f64))
            }
        }
        Method::Boolean(index) => {
            let key = SWITCHES[index];
            let previous = truthy(raw(call, owner, key)?);
            if let Value::Boolean(value) = arg(call, 0) {
                set(owner, key, value, call)?;
            }
            call.push(if matches!(arg(call, 1), Value::Boolean(_)) {
                Value::Table(owner)
            } else {
                Value::Boolean(previous)
            })
        }
        Method::Autosync => {
            let previous = raw_index(call, owner, SWITCHES[6], tonumber)?;
            let value = arg(call, 0);
            if value != Value::Nil {
                let next = enum_index(call, value, &AUTOSYNC, "AutosyncType")?;
                set(owner, SWITCHES[6], next, call)?;
            }
            let value = if chaining(call) {
                Value::Table(owner)
            } else {
                let label = AUTOSYNC
                    .get(previous)
                    .ok_or_else(|| call.error(b"Invalid AutosyncType"))?;
                call.try_intern(label.as_bytes())?
            };
            call.push(value)
        }
    }
}

fn describe(call: &mut HostCall<'_>, owner: TableId, tonumber: Value) -> Result<String, Error> {
    let mut parts = Vec::new();
    let rate = raw_float(call, owner, FLOATS[0].0, tonumber)?;
    if rate != 1.0 {
        parts.push(crate::format_song_options_text(rate));
    }
    let haste = raw_float(call, owner, FLOATS[1].0, tonumber)?;
    if haste != 0.0 && haste.is_finite() {
        parts.push(if haste == 1.0 {
            "Haste".into()
        } else {
            format!("{:.0}% Haste", (haste * 100.0).round_ties_even())
        });
    }
    for (key, labels) in [
        (
            SWITCHES[6],
            &["", "AutosyncSong", "AutosyncMachine", "AutosyncTempo"][..],
        ),
        (SWITCHES[7], &["", "EffectSpeed", "EffectPitch"][..]),
    ] {
        let index = raw_index(call, owner, key, tonumber)?;
        if let Some(name) = labels.get(index).filter(|s| !s.is_empty()) {
            parts.push((*name).to_owned());
        }
    }
    for (key, label) in SWITCHES[..4]
        .iter()
        .zip(["Clap", "Metronome", "StaticBG", "RandomBG"])
    {
        if truthy(raw(call, owner, key)?) {
            parts.push(label.to_owned());
        }
    }
    Ok(parts.join(", "))
}

pub(super) fn apply(call: &mut HostCall<'_>, owner: TableId, text: &str) -> Result<(), Error> {
    for part in text.split(',') {
        let lower = part.trim().to_ascii_lowercase();
        let rate = lower
            .strip_suffix("xmusic")
            .filter(|s| {
                let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
                s.split_once('.')
                    .map_or_else(|| digits(s), |(whole, frac)| digits(whole) && digits(frac))
            })
            .and_then(|s| s.parse::<f32>().ok());
        if let Some(rate) = rate {
            set(owner, FLOATS[0].0, rate, call)?;
            continue;
        }
        let mut words = lower.split(' ').filter(|s| !s.is_empty());
        let first = words.next();
        let on = first != Some("no");
        let name = words.next().or(first).unwrap_or_default();
        let key = match name {
            "clap" => SWITCHES[0],
            "metronome" => SWITCHES[1],
            "staticbg" => SWITCHES[2],
            "randombg" => SWITCHES[3],
            "savescore" => SWITCHES[4],
            "savereplay" => SWITCHES[5],
            "haste" => {
                set(owner, FLOATS[1].0, f32::from(on), call)?;
                continue;
            }
            "autosync" | "autosyncsong" | "autosyncmachine" | "autosynctempo" => {
                set(
                    owner,
                    SWITCHES[6],
                    if !on {
                        0
                    } else if name == "autosyncmachine" {
                        2
                    } else if name == "autosynctempo" {
                        3
                    } else {
                        1
                    },
                    call,
                )?;
                continue;
            }
            "effect" if !on => {
                set(owner, SWITCHES[7], 0_i32, call)?;
                continue;
            }
            "effectspeed" | "effectpitch" => {
                set(
                    owner,
                    SWITCHES[7],
                    if !on {
                        0
                    } else if name == "effectspeed" {
                        1
                    } else {
                        2
                    },
                    call,
                )?;
                continue;
            }
            _ => continue,
        };
        set(owner, key, on, call)?;
    }
    Ok(())
}
pub(super) fn assign_fields(
    call: &mut HostCall<'_>,
    source: TableId,
    target: TableId,
) -> Result<(), Error> {
    for key in SWITCHES
        .into_iter()
        .chain(FLOATS.into_iter().flat_map(|(field, speed)| [field, speed]))
    {
        let value = raw(call, source, key)?;
        set(target, key, value, call)?;
    }
    Ok(())
}
fn assign(
    call: &mut HostCall<'_>,
    levels: [TableId; 4],
    level: usize,
    text: &str,
) -> Result<(), Error> {
    let replacement = object(call, |call, replacement| {
        defaults(call, replacement, 1.0)?;
        apply(call, replacement, text)
    })?;
    for (index, owner) in levels.into_iter().enumerate() {
        if (index >= level && index < 3) || (index == 3 && level != 2) {
            assign_fields(call, replacement, owner)?;
        }
    }
    Ok(())
}
pub(super) fn approach(
    call: &mut HostCall<'_>,
    value: f32,
    target: f32,
    amount: f32,
) -> Result<f32, Error> {
    if amount.is_nan() || amount < 0.0 {
        return Err(call.error(b"negative native option approach"));
    }
    if value == target {
        return Ok(value);
    }
    let delta = target - value;
    let mut movement = (delta / delta.abs()) * amount;
    if movement.abs() > delta.abs() {
        movement = delta;
    }
    Ok(value + movement)
}
pub(super) fn advance(
    call: &mut HostCall<'_>,
    levels: [TableId; 4],
    delta: f32,
    tonumber: Value,
) -> Result<(), Error> {
    if !delta.is_finite() || delta < 0.0 {
        return Err(call.error(b"invalid native option frame delta"));
    }
    for (field, speed) in FLOATS {
        let value = raw_float(call, levels[3], field, tonumber)?;
        let target = raw_float(call, levels[2], field, tonumber)?;
        let amount = delta * raw_float(call, levels[2], speed, tonumber)?;
        let value = approach(call, value, target, amount)?;
        set(levels[3], field, value, call)?;
    }
    for key in SWITCHES {
        let value = raw(call, levels[2], key)?;
        set(levels[3], key, value, call)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Operation {
    Select,
    Text,
    Assign,
    Apply,
    Restart,
    ClearAttacks,
    Advance,
}
fn group(call: &mut HostCall<'_>) -> Result<(), Error> {
    let operation = *call
        .function_data::<Operation>()
        .expect("song option operation");
    let tonumber = call.captures()[0];
    let levels = [
        table(call, call.captures()[1])?,
        table(call, call.captures()[2])?,
        table(call, call.captures()[3])?,
        table(call, call.captures()[4])?,
    ];
    match operation {
        Operation::Select | Operation::Text | Operation::Assign | Operation::Apply => {
            let level = enum_index(call, arg(call, 0), &LEVELS, "ModsLevel")?;
            match operation {
                Operation::Select => call.push(Value::Table(levels[level])),
                Operation::Text => {
                    let text = describe(call, levels[level], tonumber)?;
                    let value = call.try_intern(text.as_bytes())?;
                    call.push(value)
                }
                _ => {
                    let text = text(call, arg(call, 1))
                        .ok_or_else(|| call.error(b"Expected song options string"))?;
                    if matches!(operation, Operation::Assign) {
                        assign(call, levels, level, &text)
                    } else {
                        for owner in &levels[level..] {
                            apply(call, *owner, &text)?;
                        }
                        Ok(())
                    }
                }
            }
        }
        Operation::Restart => {
            for owner in &levels[1..] {
                assign_fields(call, levels[0], *owner)?;
            }
            Ok(())
        }
        Operation::ClearAttacks => assign_fields(call, levels[1], levels[2]),
        Operation::Advance => {
            let delta = number(call, arg(call, 0), tonumber)? as f32;
            advance(call, levels, delta, tonumber)
        }
    }
}
