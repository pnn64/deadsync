use super::*;

#[derive(Clone, Copy)]
enum Operation {
    Player,
    Percent,
    Suffix,
    Seconds(fn(f64) -> String),
    ForceRange,
    WrappedIndex,
    TimingWindow,
    EmptyTable,
    Names,
    Credits,
    Judgments,
    Effect,
}

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    for (name, value) in [
        ("IsITGmania", Value::Boolean(true)),
        ("StepManiaVersionIsSupported", Value::Boolean(true)),
        ("CurrentGameIsSupported", Value::Boolean(true)),
        ("SupportsRenderToTexture", Value::Boolean(true)),
        ("IsGameAndMenuButton", Value::Boolean(false)),
        ("TotalCourseLength", Value::Number(0.)),
        ("TotalCourseLengthPlayed", Value::Number(0.)),
    ] {
        if missing(vm, name)? {
            constant_global(vm, name, value)?;
        }
    }
    for (name, text) in [
        ("MinimumVersionString", "1.2.0"),
        ("GetThemeVersion", crate::SONG_LUA_PRODUCT_VERSION),
        ("GetAuthor", "DeadSync song-Lua compat"),
        (
            "GetFallbackBanner",
            crate::SONG_LUA_THEME_PATH_PREFIX.trim_end_matches('/'),
        ),
        ("GetPlayerOptionsString", ""),
    ] {
        if missing(vm, name)? {
            let value = vm.try_intern(text.as_bytes())?;
            constant_global(vm, name, value)?;
        }
    }
    let globals = vm.get_global(b"_G")?;
    for (name, operation) in [
        ("PlayerNumberToString", Operation::Player),
        ("FormatPercentScore", Operation::Percent),
        ("FormatNumberAndSuffix", Operation::Suffix),
        (
            "SecondsToMSSMsMs",
            Operation::Seconds(crate::seconds_to_mss_ms_ms),
        ),
        (
            "SecondsToMMSSMsMs",
            Operation::Seconds(crate::seconds_to_mmss_ms_ms),
        ),
        ("force_to_range", Operation::ForceRange),
        ("wrapped_index", Operation::WrappedIndex),
        ("DetermineTimingWindow", Operation::TimingWindow),
        ("GetStepsCredit", Operation::EmptyTable),
        ("courseLengthBySong", Operation::EmptyTable),
        ("GetJudgmentGraphics", Operation::Names),
        ("GetHoldJudgments", Operation::Names),
        ("GetHeldMissGraphics", Operation::Names),
        ("GetCredits", Operation::Credits),
        ("GetExJudgmentCounts", Operation::Judgments),
        ("LoadGuest", Operation::Effect),
        ("LoadProfileCustom", Operation::Effect),
        ("SaveProfileCustom", Operation::Effect),
    ] {
        if missing(vm, name)? {
            let function = vm.new_host_function_with_data(dispatch, &[globals], operation)?;
            bind(vm, name, function)?;
        }
    }
    for name in ["GetAvatarPath", "GetPlayerAvatarPath"] {
        if missing(vm, name)? {
            constant_global(vm, name, Value::Nil)?;
        }
    }
    Ok(())
}

fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let operation = *call
        .function_data::<Operation>()
        .expect("compatibility helper");
    let result = match operation {
        Operation::Player => {
            let player = super::super::game_state::player(call, arg(call, 0));
            let text = player.map(|p| format!("P{}", p + 1)).unwrap_or_default();
            call.try_intern(text.as_bytes())?
        }
        Operation::Percent => {
            let value = float(call, arg(call, 0)).unwrap_or(0.);
            call.try_intern(format!("{:.2}%", value * 100.).as_bytes())?
        }
        Operation::Suffix => {
            let value = integer(call, 0)?;
            call.try_intern(crate::format_number_and_suffix(value).as_bytes())?
        }
        Operation::Seconds(format) => {
            let value = call.number(0)?;
            // mlua's default callback wrapper turns a Rust formatter panic into
            // a Lua error that pcall can catch. Keep that boundary here: a Rust
            // host-API panic would otherwise invalidate the whole VM.
            let text = std::panic::catch_unwind(|| format(value)).map_err(|payload| {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("Rust formatter panic");
                call.error(message.as_bytes())
            })?;
            call.try_intern(text.as_bytes())?
        }
        Operation::ForceRange => {
            let min = call.number(0)?;
            let value = call.number(1)?;
            let max = call.number(2)?;
            if min.is_nan() || max.is_nan() || min > max {
                return Err(call.error(b"invalid clamp bounds"));
            }
            Value::Number(value.clamp(min, max))
        }
        Operation::WrappedIndex => {
            let start = integer(call, 0)?;
            let offset = integer(call, 1)?;
            let len = integer(call, 2)?;
            Value::Number(if len <= 0 {
                0.
            } else {
                (start.wrapping_add(offset).wrapping_sub(1).rem_euclid(len) + 1) as f64
            })
        }
        Operation::TimingWindow => {
            let offset = float(call, arg(call, 0)).unwrap_or(0.).abs();
            Value::Number(
                (1..=5)
                    .find(|index| offset <= crate::timing_window_seconds(*index, "", false))
                    .unwrap_or(5) as f64,
            )
        }
        Operation::EmptyTable => Value::Table(object(call, |_, _| Ok(()))?),
        Operation::Names => Value::Table(array(call, ["None"])?),
        Operation::Credits => Value::Table(object(call, |call, table| {
            for (name, value) in [("Credits", 0), ("Remainder", 0), ("CoinsPerCredit", 1)] {
                set(table, name, value, call)?;
            }
            Ok(())
        })?),
        Operation::Judgments => Value::Table(object(call, |call, table| {
            for name in [
                "W0",
                "W1",
                "W2",
                "W3",
                "W4",
                "W5",
                "Miss",
                "Hands",
                "Holds",
                "Mines",
                "Rolls",
                "totalHands",
                "totalHolds",
                "totalMines",
                "totalRolls",
            ] {
                set(table, name, 0, call)?;
            }
            Ok(())
        })?),
        Operation::Effect => {
            super::super::note_side_effect(call, call.captures()[0])?;
            Value::Boolean(true)
        }
    };
    call.push(result)
}
