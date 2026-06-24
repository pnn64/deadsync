use mlua::{Lua, Value};

use crate::{SongLuaSpeedMod, read_boolish, read_f32};

pub const SONG_LUA_PLAYER_OPTION_CAPABILITIES: &[&str] = &[
    "FromString",
    "IsEasierForSongAndSteps",
    "IsEasierForCourseAndTrail",
    "LifeSetting",
    "DrainSetting",
    "HideLightSetting",
    "ModTimerSetting",
    "BatteryLives",
    "XMod",
    "CMod",
    "MMod",
    "AMod",
    "CAMod",
    "DrawSize",
    "DrawSizeBack",
    "ModTimerMult",
    "ModTimerOffset",
    "TimeSpacing",
    "MaxScrollBPM",
    "ScrollSpeed",
    "ScrollBPM",
    "Boost",
    "Brake",
    "Wave",
    "WavePeriod",
    "Expand",
    "ExpandPeriod",
    "TanExpand",
    "TanExpandPeriod",
    "Boomerang",
    "Drunk",
    "DrunkSpeed",
    "DrunkOffset",
    "DrunkPeriod",
    "TanDrunk",
    "TanDrunkSpeed",
    "TanDrunkOffset",
    "TanDrunkPeriod",
    "DrunkZ",
    "DrunkZSpeed",
    "DrunkZOffset",
    "DrunkZPeriod",
    "TanDrunkZ",
    "TanDrunkZSpeed",
    "TanDrunkZOffset",
    "TanDrunkZPeriod",
    "Dizzy",
    "AttenuateX",
    "AttenuateY",
    "AttenuateZ",
    "ShrinkLinear",
    "ShrinkMult",
    "PulseInner",
    "PulseOuter",
    "PulsePeriod",
    "PulseOffset",
    "Confusion",
    "ConfusionOffset",
    "ConfusionX",
    "ConfusionXOffset",
    "ConfusionY",
    "ConfusionYOffset",
    "Bounce",
    "BouncePeriod",
    "BounceOffset",
    "BounceZ",
    "BounceZPeriod",
    "BounceZOffset",
    "Mini",
    "Tiny",
    "Flip",
    "Invert",
    "Tornado",
    "TornadoPeriod",
    "TornadoOffset",
    "TanTornado",
    "TanTornadoPeriod",
    "TanTornadoOffset",
    "TornadoZ",
    "TornadoZPeriod",
    "TornadoZOffset",
    "TanTornadoZ",
    "TanTornadoZPeriod",
    "TanTornadoZOffset",
    "Tipsy",
    "TipsySpeed",
    "TipsyOffset",
    "TanTipsy",
    "TanTipsySpeed",
    "TanTipsyOffset",
    "Bumpy",
    "BumpyOffset",
    "BumpyPeriod",
    "TanBumpy",
    "TanBumpyOffset",
    "TanBumpyPeriod",
    "BumpyX",
    "BumpyXOffset",
    "BumpyXPeriod",
    "TanBumpyX",
    "TanBumpyXOffset",
    "TanBumpyXPeriod",
    "Beat",
    "BeatOffset",
    "BeatPeriod",
    "BeatMult",
    "BeatY",
    "BeatYOffset",
    "BeatYPeriod",
    "BeatYMult",
    "BeatZ",
    "BeatZOffset",
    "BeatZPeriod",
    "BeatZMult",
    "Zigzag",
    "ZigzagPeriod",
    "ZigzagOffset",
    "ZigzagZ",
    "ZigzagZPeriod",
    "ZigzagZOffset",
    "Sawtooth",
    "SawtoothPeriod",
    "SawtoothZ",
    "SawtoothZPeriod",
    "Square",
    "SquareOffset",
    "SquarePeriod",
    "SquareZ",
    "SquareZOffset",
    "SquareZPeriod",
    "Digital",
    "DigitalSteps",
    "DigitalPeriod",
    "DigitalOffset",
    "TanDigital",
    "TanDigitalSteps",
    "TanDigitalPeriod",
    "TanDigitalOffset",
    "DigitalZ",
    "DigitalZSteps",
    "DigitalZPeriod",
    "DigitalZOffset",
    "TanDigitalZ",
    "TanDigitalZSteps",
    "TanDigitalZPeriod",
    "TanDigitalZOffset",
    "ParabolaX",
    "ParabolaY",
    "ParabolaZ",
    "Xmode",
    "Twirl",
    "Roll",
    "Hidden",
    "HiddenOffset",
    "Sudden",
    "SuddenOffset",
    "Stealth",
    "Blink",
    "RandomVanish",
    "Reverse",
    "Split",
    "Alternate",
    "Cross",
    "Centered",
    "Dark",
    "Blind",
    "Cover",
    "StealthType",
    "StealthPastReceptors",
    "DizzyHolds",
    "ZBuffer",
    "Cosecant",
    "RandAttack",
    "NoAttack",
    "PlayerAutoPlay",
    "Tilt",
    "Skew",
    "Passmark",
    "RandomSpeed",
    "TurnNone",
    "Mirror",
    "LRMirror",
    "UDMirror",
    "Backwards",
    "Left",
    "Right",
    "Shuffle",
    "SoftShuffle",
    "SuperShuffle",
    "HyperShuffle",
    "NoHolds",
    "NoRolls",
    "NoMines",
    "Little",
    "Wide",
    "Big",
    "Quick",
    "BMRize",
    "Skippy",
    "Mines",
    "AttackMines",
    "Echo",
    "Stomp",
    "Planted",
    "Floored",
    "Twister",
    "HoldRolls",
    "NoJumps",
    "NoHands",
    "NoLifts",
    "NoFakes",
    "NoQuads",
    "NoStretch",
    "MuteOnError",
    "Overhead",
    "Incoming",
    "Space",
    "Hallway",
    "Distant",
    "NoteSkin",
    "FailSetting",
    "MinTNSToHideNotes",
    "VisualDelay",
    "DisableTimingWindow",
    "ResetDisabledTimingWindows",
    "GetDisabledTimingWindows",
    "UsingReverse",
    "GetReversePercentForColumn",
    "GetStepAttacks",
];

pub const SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES: &[&str] = &[
    "MoveX",
    "MoveY",
    "MoveZ",
    "ConfusionOffset",
    "ConfusionXOffset",
    "ConfusionYOffset",
    "Dark",
    "Stealth",
    "Tiny",
    "Bumpy",
    "Reverse",
];

pub fn is_player_option_method_name(name: &str) -> bool {
    SONG_LUA_PLAYER_OPTION_CAPABILITIES.contains(&name)
        || SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES
            .iter()
            .any(|prefix| {
                name.strip_prefix(prefix)
                    .and_then(|suffix| suffix.parse::<usize>().ok())
                    .is_some_and(|column| (1..=16).contains(&column))
            })
}

pub fn strip_player_option_prefix(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let Some(rest) = trimmed.strip_prefix('*') else {
            return trimmed;
        };
        let prefix_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if prefix_len == 0 {
            return trimmed;
        }
        text = &rest[prefix_len..];
    }
}

pub fn split_first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(index) => (&text[..index], text[index..].trim_start()),
        None => (text, ""),
    }
}

pub fn parse_player_option_amount(text: &str) -> Option<f32> {
    let text = text.trim();
    let percent = text.ends_with('%');
    let raw = text.trim_end_matches('%');
    let value = raw.parse::<f32>().ok()?;
    Some(if percent { value / 100.0 } else { value })
}

pub fn normalize_player_option_key(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

pub fn parse_player_speed_option(text: &str) -> Option<(&'static str, f32)> {
    let compact: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    if let Some(value) = compact
        .strip_suffix('x')
        .and_then(|raw| raw.parse::<f32>().ok())
    {
        return Some(("xmod", value));
    }
    for (prefix, key) in [("ca", "camod"), ("c", "cmod"), ("m", "mmod"), ("a", "amod")] {
        if let Some(value) = compact
            .strip_prefix(prefix)
            .and_then(|raw| raw.parse::<f32>().ok())
            .or_else(|| {
                compact
                    .strip_suffix(prefix)
                    .and_then(|raw| raw.parse::<f32>().ok())
            })
        {
            return Some((key, value));
        }
    }
    None
}

#[inline(always)]
pub fn normalize_player_option_value(lua: &Lua, name: &str, value: Value) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(read_boolish(value).unwrap_or(false)));
    }
    if player_option_default_string(name).is_some() {
        return Ok(match value {
            Value::String(_) => value,
            _ => default_player_option_value(lua, name)?,
        });
    }
    Ok(Value::Number(read_f32(value).unwrap_or(0.0) as f64))
}

#[inline(always)]
pub fn default_player_option_value(lua: &Lua, name: &str) -> mlua::Result<Value> {
    if player_option_uses_bool(name) {
        return Ok(Value::Boolean(false));
    }
    if let Some(value) = player_option_default_string(name) {
        return Ok(Value::String(lua.create_string(value)?));
    }
    Ok(Value::Number(0.0))
}

pub fn song_lua_speedmod_value(
    speedmod: SongLuaSpeedMod,
    ctor: fn(f32) -> SongLuaSpeedMod,
) -> Value {
    match (speedmod, ctor(0.0)) {
        (SongLuaSpeedMod::X(value), SongLuaSpeedMod::X(_))
        | (SongLuaSpeedMod::C(value), SongLuaSpeedMod::C(_))
        | (SongLuaSpeedMod::M(value), SongLuaSpeedMod::M(_))
        | (SongLuaSpeedMod::A(value), SongLuaSpeedMod::A(_)) => Value::Number(value as f64),
        _ => Value::Nil,
    }
}

#[inline(always)]
pub fn player_option_default_string(name: &str) -> Option<&'static str> {
    Some(match name {
        "drainsetting" => "DrainType_Normal",
        "failsetting" => "FailType_Immediate",
        "hidelightsetting" => "HideLightType_NoHideLights",
        "lifesetting" => "LifeType_Bar",
        "mintnstohidenotes" => "TapNoteScore_None",
        "modtimersetting" => "ModTimerType_Default",
        _ => return None,
    })
}

#[inline(always)]
pub fn player_option_uses_bool(name: &str) -> bool {
    matches!(
        name,
        "attackmines"
            | "backwards"
            | "big"
            | "bmrize"
            | "cosecant"
            | "dizzyholds"
            | "echo"
            | "floored"
            | "holdrolls"
            | "hypershuffle"
            | "left"
            | "little"
            | "lrmirror"
            | "mirror"
            | "mines"
            | "muteonerror"
            | "nohands"
            | "noholds"
            | "nojumps"
            | "nolifts"
            | "nomines"
            | "noquads"
            | "norolls"
            | "nostretch"
            | "nofakes"
            | "overhead"
            | "planted"
            | "quick"
            | "right"
            | "shuffle"
            | "skippy"
            | "softshuffle"
            | "stealthpastreceptors"
            | "stealthtype"
            | "stomp"
            | "supershuffle"
            | "turnnone"
            | "twister"
            | "udmirror"
            | "wide"
            | "zbuffer"
    )
}

#[cfg(test)]
mod tests {
    use mlua::{Lua, Value};

    use crate::SongLuaSpeedMod;

    use super::{
        default_player_option_value, is_player_option_method_name, normalize_player_option_key,
        normalize_player_option_value, parse_player_option_amount, parse_player_speed_option,
        player_option_default_string, player_option_uses_bool, song_lua_speedmod_value,
        split_first_word, strip_player_option_prefix,
    };

    #[test]
    fn strips_stepmania_player_option_prefixes() {
        assert_eq!(strip_player_option_prefix("*2 50% Reverse"), "50% Reverse");
        assert_eq!(strip_player_option_prefix("  *-1.5   C400"), "C400");
        assert_eq!(strip_player_option_prefix("Mini"), "Mini");
    }

    #[test]
    fn parses_player_option_amounts() {
        assert_eq!(parse_player_option_amount("50%"), Some(0.5));
        assert_eq!(parse_player_option_amount("-25%"), Some(-0.25));
        assert_eq!(parse_player_option_amount("1.5"), Some(1.5));
        assert_eq!(parse_player_option_amount("Mini"), None);
    }

    #[test]
    fn normalizes_player_option_keys() {
        assert_eq!(normalize_player_option_key("No Mines"), "nomines");
        assert_eq!(normalize_player_option_key("C-Mod!"), "cmod");
        assert_eq!(split_first_word("  50% Reverse"), ("50%", "Reverse"));
    }

    #[test]
    fn classifies_player_option_storage() {
        assert!(player_option_uses_bool("nomines"));
        assert!(!player_option_uses_bool("reverse"));
        assert_eq!(
            player_option_default_string("failsetting"),
            Some("FailType_Immediate")
        );
        assert_eq!(player_option_default_string("reverse"), None);
    }

    #[test]
    fn detects_player_option_method_names() {
        assert!(is_player_option_method_name("Reverse"));
        assert!(is_player_option_method_name("MoveX16"));
        assert!(!is_player_option_method_name("MoveX17"));
        assert!(!is_player_option_method_name("NotAPlayerOption"));
    }

    #[test]
    fn parses_player_speed_options() {
        assert_eq!(parse_player_speed_option("1.5x"), Some(("xmod", 1.5)));
        assert_eq!(parse_player_speed_option("C400"), Some(("cmod", 400.0)));
        assert_eq!(parse_player_speed_option("650m"), Some(("mmod", 650.0)));
        assert_eq!(parse_player_speed_option("CA250"), Some(("camod", 250.0)));
        assert_eq!(parse_player_speed_option("Reverse"), None);
    }

    #[test]
    fn normalizes_player_option_values() {
        let lua = Lua::new();
        assert_eq!(
            normalize_player_option_value(&lua, "nomines", Value::Integer(1)).unwrap(),
            Value::Boolean(true)
        );
        assert_eq!(
            default_player_option_value(&lua, "failsetting")
                .unwrap()
                .as_string()
                .unwrap()
                .to_string_lossy(),
            "FailType_Immediate"
        );
        assert_eq!(
            normalize_player_option_value(&lua, "reverse", Value::Number(0.5)).unwrap(),
            Value::Number(0.5)
        );
    }

    #[test]
    fn speedmod_value_matches_requested_variant() {
        assert_eq!(
            song_lua_speedmod_value(SongLuaSpeedMod::C(400.0), SongLuaSpeedMod::C),
            Value::Number(400.0)
        );
        assert_eq!(
            song_lua_speedmod_value(SongLuaSpeedMod::X(1.5), SongLuaSpeedMod::C),
            Value::Nil
        );
    }
}
