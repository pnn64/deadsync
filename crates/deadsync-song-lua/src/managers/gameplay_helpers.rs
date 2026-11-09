//! Existing theme helpers used by the gameplay screen's constructor and updates.
use super::fields::{array, object, set};
use super::*;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let function = vm.new_host_function(player_tables, &[globals])?;
    vm.call(function, &[])?;
    for name in [
        "left",
        "center",
        "middle",
        "right",
        "top",
        "bottom",
        "HorizAlign_Left",
        "HorizAlign_Center",
        "HorizAlign_Right",
        "VertAlign_Top",
        "VertAlign_Middle",
        "VertAlign_Bottom",
    ] {
        let value = vm.try_intern(name.as_bytes())?;
        vm.set_global(name.as_bytes(), value)?;
    }
    for (name, callback) in [
        (
            "BackgroundFilterValues",
            filters as deadlib_lua::HostFunction,
        ),
        ("PlayerColor", player_color),
        ("GetCurrentColor", current_color),
        ("DifficultyColor", difficulty_color),
        ("CustomDifficultyToColor", custom_difficulty_color),
        ("round", round),
        ("clamp", clamp),
        ("range", range),
        ("scale", scale),
        ("GetTimingWindow", timing_window),
        ("CalculateExScore", ex_score),
    ] {
        let function = vm.new_host_function(callback, &[])?;
        vm.set_global(name.as_bytes(), function)?;
    }
    let service = vm.new_host_function(super::constant, &[Value::Boolean(false)])?;
    vm.set_global(b"IsServiceAllowed", service)?;
    let dark = vm.new_host_function(super::constant, &[Value::Boolean(false)])?;
    vm.set_global(b"DarkUI", dark)?;
    let judgments = vm.new_host_function(super::constant, &[Value::Number(5.)])?;
    vm.set_global(b"NumJudgmentsAvailable", judgments)?;
    for (name, formatter) in [
        (
            "SecondsToHMMSS",
            crate::seconds_to_hhmmss as fn(f64) -> String,
        ),
        ("SecondsToHHMMSS", crate::seconds_to_hhmmss),
        ("SecondsToMSS", crate::seconds_to_mss),
        ("SecondsToMMSS", crate::seconds_to_mmss),
    ] {
        let function = vm.new_host_function_with_data(format_seconds, &[], formatter)?;
        vm.set_global(name.as_bytes(), function)?;
    }
    let month = crate::song_lua_local_date_globals().month_of_year;
    let function = vm.new_host_function(super::constant, &[Value::Number(month as f64)])?;
    vm.set_global(b"MonthOfYear", function)?;
    for (name, source, chunk) in [
        (
            "GetColumnMapping",
            include_bytes!("get_column_mapping.lua").as_slice(),
            "@theme:/Scripts/SL-Helpers.lua",
        ),
        (
            "IsRoutine",
            include_bytes!("is_routine.lua").as_slice(),
            "@theme:/Scripts/02 Utilities.lua:IsRoutine",
        ),
        (
            "IsSpooky",
            include_bytes!("is_spooky.lua").as_slice(),
            "@theme:/Scripts/SL-Helpers.lua:IsSpooky",
        ),
    ] {
        let code = deadlib_lua::compile_named(source, chunk.as_bytes())?;
        let function = vm.execute(&code)?[0];
        vm.set_global(name.as_bytes(), function)?;
    }
    Ok(())
}

fn arg(call: &HostCall<'_>, index: usize) -> Value {
    call.arguments().get(index).copied().unwrap_or(Value::Nil)
}

pub(super) fn boolish(call: &HostCall<'_>, value: Value) -> bool {
    match value {
        Value::Boolean(value) => value,
        Value::Number(value) => value != 0.,
        Value::String(_) => super::catalog::text(call, value).is_some_and(|value| {
            let value = value.trim();
            if ["true", "yes", "on"]
                .iter()
                .any(|s| value.eq_ignore_ascii_case(s))
            {
                true
            } else if ["false", "no", "off"]
                .iter()
                .any(|s| value.eq_ignore_ascii_case(s))
            {
                false
            } else {
                value.parse::<f32>().is_ok_and(|value| value != 0.)
            }
        }),
        _ => false,
    }
}

fn filters(call: &mut HostCall<'_>) -> Result<(), Error> {
    let values = object(call, |call, values| {
        for (name, value) in [("Off", 0), ("Dark", 50), ("Darker", 75), ("Darkest", 95)] {
            set(values, name, value, call)?;
        }
        for value in 0..=100 {
            set(values, value, value, call)?;
        }
        Ok(())
    })?;
    call.push(Value::Table(values))
}

fn player_color(call: &mut HostCall<'_>) -> Result<(), Error> {
    let color = super::game_state::player(call, arg(call, 0))
        .map(|player| crate::song_lua_player_color(player, boolish(call, arg(call, 1))))
        .unwrap_or([1.; 4]);
    let color = array(call, color)?;
    call.push(Value::Table(color))
}

fn difficulty_color(call: &mut HostCall<'_>) -> Result<(), Error> {
    let difficulty = match arg(call, 0) {
        Value::Number(value) if value.is_finite() => Some(value.trunc() as i64),
        value @ Value::String(_) => super::catalog::text(call, value)
            .and_then(|value| crate::song_lua_difficulty_index(&value)),
        _ => None,
    };
    let color = difficulty
        .map(|difficulty| crate::song_lua_difficulty_color(difficulty, boolish(call, arg(call, 1))))
        .unwrap_or_else(|| crate::parse_color_text("#B4B7BA").unwrap());
    let color = array(call, color)?;
    call.push(Value::Table(color))
}

fn round(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = super::catalog::float(call, arg(call, 0)).unwrap_or(0.);
    let precision = super::catalog::float(call, arg(call, 1)).unwrap_or(0.);
    let factor = 10.0_f32.powf(precision.max(0.));
    call.push(Value::Number(((value * factor).round() / factor) as f64))
}

fn current_color(call: &mut HostCall<'_>) -> Result<(), Error> {
    let color = crate::theme_colors::palette_color(
        crate::theme_colors::SONG_LUA_ACTIVE_COLOR_INDEX,
        crate::theme_colors::song_lua_palette(None, boolish(call, arg(call, 0))),
    );
    let color = array(call, color)?;
    call.push(Value::Table(color))
}

fn custom_difficulty_color(call: &mut HostCall<'_>) -> Result<(), Error> {
    let color = match arg(call, 0) {
        value @ Value::String(_) => super::catalog::text(call, value)
            .and_then(|name| crate::theme_colors::custom_difficulty_color(&name)),
        _ => None,
    }
    .unwrap_or([1.; 4]);
    let color = array(call, color)?;
    call.push(Value::Table(color))
}

fn clamp(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.number(0)?;
    let minimum = call.number(1)?;
    let maximum = call.number(2)?;
    call.push(Value::Number(value.clamp(minimum, maximum)))
}

fn scale(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = crate::scale_value(
        call.number(0)? as f32,
        call.number(1)? as f32,
        call.number(2)? as f32,
        call.number(3)? as f32,
        call.number(4)? as f32,
    );
    call.push(Value::Number(value as f64))
}

fn format_seconds(call: &mut HostCall<'_>) -> Result<(), Error> {
    let formatter = *call.function_data::<fn(f64) -> String>().unwrap();
    let value = formatter(call.number(0)?);
    let value = call.try_intern(value.as_bytes())?;
    call.push(value)
}

fn timing_window(call: &mut HostCall<'_>) -> Result<(), Error> {
    let index = super::catalog::integer(call, arg(call, 0))
        .or_else(|| {
            let text = super::catalog::text(call, arg(call, 0))?;
            text.rsplit_once('W')?.1.parse::<i32>().ok()
        })
        .unwrap_or(1);
    let mode = super::catalog::text(call, arg(call, 1)).unwrap_or_default();
    let tenms = !matches!(arg(call, 2), Value::Nil | Value::Boolean(false));
    call.push(Value::Number(
        crate::timing_window_seconds(index, &mode, tenms) as f64,
    ))
}

fn ex_score(call: &mut HostCall<'_>) -> Result<(), Error> {
    // Preserve the currently supported production helper's three return values.
    for _ in 0..3 {
        call.push(Value::Number(0.))?;
    }
    Ok(())
}

fn range(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Some(mut start) = super::catalog::float(call, arg(call, 0)) else {
        return call.push(Value::Nil);
    };
    let stop = match super::catalog::float(call, arg(call, 1)) {
        Some(stop) => stop,
        None => {
            let stop = start;
            start = 1.;
            stop
        }
    };
    let mut step = super::catalog::float(call, arg(call, 2)).unwrap_or(1.);
    if step > 0. && start > stop {
        step = -step;
    }
    let empty = step.abs() <= f32::EPSILON || (step < 0. && start < stop);
    let capacity = if !empty && start.is_finite() && stop.is_finite() && step.is_finite() {
        (((f64::from(stop) - f64::from(start)) / f64::from(step)).floor() + 1.).clamp(0., 10_000.)
            as usize
    } else {
        0
    };
    let mut stack = call.native_stack()?;
    stack.create_table(capacity, 0)?;
    let output = stack.top() as isize;
    let mut index = 1;
    let mut value = start;
    while !empty
        && ((step > 0. && value <= stop + f32::EPSILON)
            || (step < 0. && value >= stop - f32::EPSILON))
    {
        let number = if value.is_finite()
            && value.fract().abs() <= f32::EPSILON
            && value >= i64::MIN as f32
            && value < -(i64::MIN as f32)
        {
            (value as i64) as f64
        } else {
            value as f64
        };
        stack.push(Value::Number(number))?;
        stack.raw_set_integer(output, index)?;
        index += 1;
        value += step;
        if index > 10_000 {
            break;
        }
    }
    stack.finish(1)
}

// Preserve the production enum aliases and the identity of its reverse table.
fn player_tables(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let players = object(call, |call, players| {
        let reverse = object(call, |call, reverse| {
            for player in 0..crate::LUA_PLAYERS {
                let name = crate::player_number_name(player);
                set(players, player + 1, name, call)?;
                set(players, name, name, call)?;
                set(reverse, name, player, call)?;
            }
            Ok(())
        })?;
        let function = call.new_host_function(super::constant, &[Value::Table(reverse)])?;
        call.with_roots(&[function], |call| set(players, "Reverse", function, call))
    })?;
    call.with_roots(&[Value::Table(players)], |call| {
        set(globals, "PlayerNumber", players, call)
    })?;
    let other = object(call, |call, other| {
        set(
            other,
            crate::player_number_name(0),
            crate::player_number_name(1),
            call,
        )?;
        set(
            other,
            crate::player_number_name(1),
            crate::player_number_name(0),
            call,
        )
    })?;
    call.with_roots(&[Value::Table(other)], |call| {
        set(globals, "OtherPlayer", other, call)
    })
}
