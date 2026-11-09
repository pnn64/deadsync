//! Existing theme definitions, synthetic resource paths and dynamic SL metrics.
use super::*;

#[derive(Clone, Copy)]
enum Operation {
    Exists(bool),
    Names,
    SideEffect,
    Metric,
    Float,
    Integer,
    Boolean,
    HasMetric,
    MetricNames,
    StringNames,
    Path(&'static str),
    PathInfo,
    HasString,
    String,
}

struct Binding {
    operation: Operation,
    human_players: usize,
    width: f32,
    height: f32,
}

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let theme = vm.try_new_table()?;
    let root = vm.pin(Value::Table(theme))?;
    let result = (|| {
        for (name, text) in [
            ("GetCurThemeName", crate::SONG_LUA_THEME_NAME),
            ("GetThemeDisplayName", crate::SONG_LUA_THEME_NAME),
            ("GetCurLanguage", "en"),
            (
                "GetCurrentThemeDirectory",
                crate::SONG_LUA_THEME_PATH_PREFIX,
            ),
            ("GetThemeAuthor", ""),
        ] {
            let value = vm.try_intern(text.as_bytes())?;
            constant_method(vm, theme, name, value)?;
        }
        constant_method(vm, theme, "GetNumSelectableThemes", Value::Number(1.))?;
        let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
        for (name, operation) in [
            ("DoesThemeExist", Operation::Exists(false)),
            ("IsThemeSelectable", Operation::Exists(false)),
            ("DoesLanguageExist", Operation::Exists(true)),
            ("get_theme_fallback_list", Operation::Names),
            ("GetSelectableThemeNames", Operation::Names),
            ("RunLuaScripts", Operation::SideEffect),
            ("ReloadMetrics", Operation::SideEffect),
            ("SetTheme", Operation::SideEffect),
            ("GetMetric", Operation::Metric),
            ("GetMetricF", Operation::Float),
            ("GetMetricI", Operation::Integer),
            ("GetMetricB", Operation::Boolean),
            ("HasMetric", Operation::HasMetric),
            ("GetMetricNamesInGroup", Operation::MetricNames),
            ("GetStringNamesInGroup", Operation::StringNames),
            ("GetPathInfoB", Operation::PathInfo),
            ("GetPathB", Operation::Path("B")),
            ("GetPathF", Operation::Path("F")),
            ("GetPathG", Operation::Path("G")),
            ("GetPathO", Operation::Path("O")),
            ("GetPathS", Operation::Path("S")),
            ("HasString", Operation::HasString),
            ("GetString", Operation::String),
        ] {
            let function = vm.new_host_function_with_data(
                callback,
                &captures,
                Binding {
                    operation,
                    human_players: crate::song_lua_human_player_count(context),
                    width: context.screen_width.max(1.),
                    height: context.screen_height.max(1.),
                },
            )?;
            let pin = vm.pin(function)?;
            let result = super::super::put(vm, theme, name, function);
            vm.unpin(pin)?;
            result?;
        }
        vm.set_global(b"THEME", Value::Table(theme))
    })();
    vm.unpin(root)?;
    result
}

fn argument(call: &HostCall<'_>, index: usize) -> Option<String> {
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    let Value::String(id) = *call.arguments().get(offset + index)? else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?)
        .ok()
        .map(str::to_owned)
}

fn string(call: &mut HostCall<'_>, text: &str) -> Result<(), Error> {
    let value = call.try_intern(text.as_bytes())?;
    call.push(value)
}

fn strings(call: &mut HostCall<'_>, names: &[String]) -> Result<(), Error> {
    let array = call.try_new_table()?;
    call.with_roots(&[Value::Table(array)], |call| {
        for (i, name) in names.iter().enumerate() {
            let value = call.try_intern(name.as_bytes())?;
            call.raw_table_set(array, Value::Number((i + 1) as f64), value)?;
        }
        call.push(Value::Table(array))
    })
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().expect("theme binding");
    let (op, human, width, height) = (
        binding.operation,
        binding.human_players,
        binding.width,
        binding.height,
    );
    if matches!(op, Operation::SideEffect) {
        return side_effect(call, call.captures()[0], call.captures()[1]);
    }
    if matches!(op, Operation::Names) {
        return strings(call, &[crate::SONG_LUA_THEME_NAME.to_owned()]);
    }
    let group = argument(call, 0);
    let name = argument(call, 1);
    if let Operation::Exists(language) = op {
        let exists = group.is_some_and(|name| {
            if language {
                name.eq_ignore_ascii_case("en") || name.eq_ignore_ascii_case("english")
            } else {
                name.eq_ignore_ascii_case(crate::SONG_LUA_THEME_NAME)
            }
        });
        return call.push(Value::Boolean(exists));
    }
    if matches!(op, Operation::MetricNames | Operation::StringNames) {
        let names = group
            .map(|group| {
                if matches!(op, Operation::MetricNames) {
                    crate::theme_metric_names(&group)
                } else {
                    crate::theme_string_names(&group)
                }
            })
            .unwrap_or_default();
        return if names.is_empty() {
            call.push(Value::Nil)
        } else {
            strings(call, &names)
        };
    }
    if let Operation::Path(kind) = op {
        return string(
            call,
            &crate::theme_path(kind, &group.unwrap_or_default(), &name.unwrap_or_default()),
        );
    }
    if matches!(op, Operation::PathInfo) {
        let group = group.unwrap_or_default();
        let name = name.unwrap_or_default();
        string(call, &crate::theme_path("B", &group, &name))?;
        string(call, &group)?;
        return string(call, &name);
    }
    let (Some(group), Some(name)) = (group, name) else {
        return match op {
            Operation::String => string(call, ""),
            Operation::HasString | Operation::HasMetric | Operation::Boolean => {
                call.push(Value::Boolean(false))
            }
            _ => call.push(Value::Nil),
        };
    };
    let value = match op {
        Operation::String => return string(call, &crate::theme_string(&group, &name)),
        Operation::HasString => Value::Boolean(crate::theme_has_string(&group, &name)),
        Operation::Float | Operation::Integer => crate::theme_metric_number_for_screen(
            &group, &name, human, width, height,
        )
        .map_or(Value::Nil, |n| {
            let n = if matches!(op, Operation::Integer) {
                n.round()
            } else {
                n
            };
            // The production integer result canonicalizes a rounded -0.
            Value::Number(if matches!(op, Operation::Integer) && n == 0. {
                0.
            } else {
                n as f64
            })
        }),
        Operation::HasMetric if crate::sl::gameplay_lead_in_seconds(&group, &name).is_some() => {
            Value::Boolean(true)
        }
        Operation::Metric | Operation::Boolean | Operation::HasMetric => {
            let value = metric(call, &group, &name, human, width, height)?;
            match op {
                Operation::Boolean => Value::Boolean(match value {
                    Value::Boolean(v) => v,
                    Value::Number(v) => v != 0.,
                    Value::String(id) => !call
                        .string_bytes(id)
                        .expect("live metric string")
                        .is_empty(),
                    _ => false,
                }),
                Operation::HasMetric => Value::Boolean(value != Value::Nil),
                _ => value,
            }
        }
        _ => unreachable!(),
    };
    call.push(value)
}

fn metric(
    call: &mut HostCall<'_>,
    group: &str,
    name: &str,
    human: usize,
    width: f32,
    height: f32,
) -> Result<Value, Error> {
    if let Some(value) = sl_metric(call, group, name)? {
        return Ok(value);
    }
    if let Some(value) = crate::theme_metric_number_for_screen(group, name, human, width, height) {
        return Ok(Value::Number(value as f64));
    }
    if let Some(text) = crate::theme_metric_string(group, name) {
        return call.try_intern(text.as_bytes());
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("DefaultNoteSkinName") {
        return call.try_intern(crate::SONG_LUA_DEFAULT_NOTESKIN_NAME.as_bytes());
    }
    if name.eq_ignore_ascii_case("Class") {
        return call.try_intern(group.as_bytes());
    }
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("AutoSetStyle")
        || group.eq_ignore_ascii_case("ScreenHeartEntry")
            && name.eq_ignore_ascii_case("HeartEntryEnabled")
    {
        return Ok(Value::Boolean(false));
    }
    Ok(Value::Nil)
}

fn field(call: &mut HostCall<'_>, target: Value, name: &str) -> Result<Value, Error> {
    table(call, target)?;
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(target, key)
}

// Keep every intermediate table alive when __index constructs temporary values
// or replaces its ancestors while collecting or reentering the host.
fn path(call: &mut HostCall<'_>, target: Value, names: &[&str]) -> Result<Value, Error> {
    let Some((name, rest)) = names.split_first() else {
        return Ok(target);
    };
    call.with_roots(&[target], |call| {
        let next = field(call, target, name)?;
        path(call, next, rest)
    })
}

fn sl_metric(call: &mut HostCall<'_>, group: &str, name: &str) -> Result<Option<Value>, Error> {
    if let Some(seconds) = crate::sl::gameplay_lead_in_seconds(group, name) {
        let globals = call.captures()[0];
        let rate = path(
            call,
            globals,
            &["SL", "Global", "ActiveModifiers", "MusicRate"],
        )?;
        let value = call.call(call.captures()[1], &[rate])?[0];
        let Value::Number(rate) = value else {
            return Err(call.error(b"SL music rate must be a number"));
        };
        return Ok(Some(Value::Number(seconds * rate)));
    }
    let mapped = group.eq_ignore_ascii_case("ScoreKeeperNormal")
        && (name.starts_with("PercentScoreWeight") || name.starts_with("GradeWeight"))
        || group.eq_ignore_ascii_case("LifeMeterBar") && name.starts_with("LifePercentChange");
    if !mapped {
        return Ok(None);
    }
    let sl = field(call, call.captures()[0], "SL")?;
    if sl == Value::Nil {
        return Ok(None);
    }
    call.with_roots(&[sl], |call| {
        let mode = path(call, sl, &["Global", "GameMode"])?;
        let mode = Value::String(
            call.coerce_string(mode)?
                .ok_or_else(|| call.error(b"SL game mode must be a string"))?,
        );
        call.with_roots(&[mode], |call| {
            let metrics = field(call, sl, "Metrics")?;
            table(call, metrics)?;
            call.with_roots(&[metrics], |call| {
                let metrics = call.table_get(metrics, mode)?;
                call.with_roots(&[metrics], |call| {
                    let value = field(call, metrics, name)?;
                    Ok((value != Value::Nil).then_some(value))
                })
            })
        })
    })
}
