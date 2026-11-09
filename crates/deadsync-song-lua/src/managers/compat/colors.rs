//! Existing theme palette and color convenience functions.
use super::*;
use crate::theme_colors as colors;

#[derive(Clone, Copy)]
enum Operation {
    Palette,
    Score,
    Dark,
    DifficultyDark,
    DifficultyLight,
    Steps,
    Named(fn(&str) -> Option<[f32; 4]>, f32),
    Tone(f32),
    Hex,
    Boost,
    Blend,
}

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    for (name, op) in [
        ("GetHexColor", Operation::Palette),
        ("PlayerScoreColor", Operation::Score),
        ("PlayerDarkColor", Operation::Dark),
        ("CustomDifficultyToDarkColor", Operation::DifficultyDark),
        ("CustomDifficultyToLightColor", Operation::DifficultyLight),
        ("StepsOrTrailToColor", Operation::Steps),
        ("StageToColor", Operation::Named(colors::stage_color, 1.)),
        (
            "StageToStrokeColor",
            Operation::Named(colors::stage_color, 0.5),
        ),
        (
            "JudgmentLineToColor",
            Operation::Named(colors::judgment_line_color, 1.),
        ),
        (
            "JudgmentLineToStrokeColor",
            Operation::Named(colors::judgment_line_color, 0.5),
        ),
        ("LightenColor", Operation::Tone(1.25)),
        ("ColorLightTone", Operation::Tone(1.5)),
        ("ColorMidTone", Operation::Tone(1. / 1.5)),
        ("ColorDarkTone", Operation::Tone(0.5)),
        ("ColorToHex", Operation::Hex),
        ("BoostColor", Operation::Boost),
        ("BlendColors", Operation::Blend),
    ] {
        let number = vm.get_global(b"tonumber")?;
        let function = vm.new_host_function_with_data(dispatch, &[number], op)?;
        bind(vm, name, function)?;
    }
    Ok(())
}

fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call.function_data::<Operation>().unwrap();
    let input = arg(call, 0);
    let color = match op {
        Operation::Palette => match float(call, input) {
            None => [1.; 4],
            Some(index) => {
                let decorative = super::super::gameplay_helpers::boolish(call, arg(call, 1));
                let palette = text(call, arg(call, 2));
                let color = colors::palette_color(
                    index.trunc() as i64,
                    colors::song_lua_palette(palette.as_deref(), decorative),
                );
                if !decorative && palette.is_some_and(|p| p.eq_ignore_ascii_case("ITG")) {
                    colors::tone_color(color, 1.25)
                } else {
                    color
                }
            }
        },
        Operation::Score | Operation::Dark => {
            super::super::game_state::player(call, input).map_or([1.; 4], |player| {
                if matches!(op, Operation::Score) {
                    colors::song_lua_player_score_color(player)
                } else {
                    colors::song_lua_player_dark_color(player)
                }
            })
        }
        Operation::DifficultyDark => text(call, input)
            .as_deref()
            .and_then(colors::custom_difficulty_color)
            .map(|c| colors::tone_color(c, 0.5))
            .unwrap_or([0.5, 0.5, 0.5, 1.]),
        Operation::DifficultyLight => text(call, input)
            .as_deref()
            .and_then(colors::custom_difficulty_color)
            .map(colors::light_color)
            .unwrap_or([1.; 4]),
        Operation::Steps => steps_color(call, input).unwrap_or([1.; 4]),
        Operation::Named(read, factor) => text(call, input)
            .as_deref()
            .and_then(read)
            .map(|c| colors::tone_color(c, factor))
            .unwrap_or([0., 0., 0., 1.]),
        Operation::Tone(factor) => {
            let color = misc::color(call, input)?.unwrap_or([1.; 4]);
            colors::tone_color(color, factor)
        }
        Operation::Hex => {
            let color = misc::color(call, input)?.unwrap_or([1.; 4]);
            let text = colors::color_to_hex(color);
            let value = call.try_intern(text.as_bytes())?;
            return call.push(value);
        }
        Operation::Boost => {
            let color = misc::color(call, input)?.unwrap_or([1.; 4]);
            colors::tone_color(color, float(call, arg(call, 1)).unwrap_or(1.))
        }
        Operation::Blend => {
            let a = misc::color(call, input)?.unwrap_or([1.; 4]);
            let b = misc::color(call, arg(call, 1))?.unwrap_or([1.; 4]);
            colors::blend_color(a, b)
        }
    };
    let out = array(call, color)?;
    call.push(Value::Table(out))
}

fn steps_color(call: &mut HostCall<'_>, value: Value) -> Option<[f32; 4]> {
    if let Some(color) = text(call, value)
        .as_deref()
        .and_then(colors::custom_difficulty_color)
    {
        return Some(color);
    }
    let Value::Table(_) = value else { return None };
    let key = call.try_intern(b"GetDifficulty").ok()?;
    let method = call.table_get(value, key).ok()?;
    if !matches!(method, Value::Function(_) | Value::Native(_)) {
        return None;
    }
    let result = call.call(method, &[value]).ok()?.first().copied()?;
    text(call, result)
        .as_deref()
        .and_then(colors::custom_difficulty_color)
}
