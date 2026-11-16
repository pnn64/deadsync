//! Existing file listing, author, judgment and color helper contracts.
use super::*;
use std::path::PathBuf;
#[derive(Clone, Copy)]
enum Operation {
    Files,
    Author,
    Worst,
    Lerp,
}
struct Binding {
    op: Operation,
    song_dir: PathBuf,
}
pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    for (name, op) in [
        ("findFiles", Operation::Files),
        ("getAuthorTable", Operation::Author),
        ("GetWorstJudgment", Operation::Worst),
        ("lerp_color", Operation::Lerp),
    ] {
        let number = vm.get_global(b"tonumber")?;
        let value = vm.new_host_function_with_data(
            dispatch,
            &[number],
            Binding {
                op,
                song_dir: context.song_dir.clone(),
            },
        )?;
        bind(vm, name, value)?;
    }
    Ok(())
}
fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap();
    let op = binding.op;
    let value = match op {
        Operation::Files => {
            let song_dir = binding.song_dir.clone();
            let directory = text(call, arg(call, 0)).unwrap_or_default();
            let extension = text(call, arg(call, 1)).unwrap_or_else(|| "ogg".to_owned());
            let files = crate::find_compat_files(&song_dir, &directory, &extension)
                .map_err(|e| call.error(e.to_string().as_bytes()))?;
            Value::Table(array(call, files)?)
        }
        Operation::Author => {
            let mut values = Vec::new();
            if let Value::Table(input) = arg(call, 0) {
                for name in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
                    let key = call.try_intern(name.as_bytes())?;
                    let function = call.table_get(Value::Table(input), key)?;
                    if function == Value::Nil {
                        continue;
                    }
                    if !matches!(function, Value::Function(_) | Value::Native(_)) {
                        return Err(call.error(b"author method must be a function"));
                    }
                    let value = call
                        .call(function, &[Value::Table(input)])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    let value = value_text(call, value, false)?.unwrap_or_default();
                    if !value.is_empty() && !values.contains(&value) {
                        values.push(value);
                    }
                }
            }
            Value::Table(array(call, values)?)
        }
        Operation::Worst => {
            let mut worst = 1;
            if let Value::Table(input) = arg(call, 0) {
                let mut index = 1;
                loop {
                    let value = call.raw_table_get(input, Value::Number(index as f64))?;
                    if value == Value::Nil {
                        break;
                    }
                    let mut offsets = [None, None];
                    if let Value::Table(row) = value {
                        let v = call.raw_table_get(row, Value::Number(2.))?;
                        offsets[0] = float(call, v);
                        let enabled = call.raw_table_get(row, Value::Number(6.))?;
                        if !matches!(enabled, Value::Nil | Value::Boolean(false)) {
                            let v = call.raw_table_get(row, Value::Number(7.))?;
                            offsets[1] = float(call, v);
                        }
                    } else {
                        offsets[0] = float(call, value);
                    }
                    for offset in offsets.into_iter().flatten() {
                        let window = (1..=5)
                            .find(|i| offset.abs() <= crate::timing_window_seconds(*i, "", false))
                            .unwrap_or(5);
                        worst = worst.max(window);
                    }
                    index += 1;
                }
            }
            Value::Number(worst as f64)
        }
        Operation::Lerp => {
            let Some(percent) = float(call, arg(call, 0)) else {
                return call.push(Value::Nil);
            };
            let Some(a) = color(call, arg(call, 1))? else {
                return call.push(Value::Nil);
            };
            let Some(b) = color(call, arg(call, 2))? else {
                return call.push(Value::Nil);
            };
            Value::Table(array(
                call,
                std::array::from_fn::<_, 4, _>(|i| (b[i] - a[i]).mul_add(percent, a[i])),
            )?)
        }
    };
    call.push(value)
}
fn number(call: &mut HostCall<'_>, value: Value) -> Result<Option<f32>, Error> {
    let function = call.captures()[0];
    Ok(match call.call(function, &[value])?.first() {
        Some(Value::Number(value)) => Some(*value as f32),
        _ => None,
    })
}

pub(super) fn color(call: &mut HostCall<'_>, value: Value) -> Result<Option<[f32; 4]>, Error> {
    match value {
        Value::String(id) => Ok(std::str::from_utf8(call.string_bytes(id).unwrap())
            .ok()
            .map(|s| crate::parse_color_text(s).unwrap_or([1.; 4]))),
        Value::Table(table) => {
            let mut out = [1.; 4];
            for (i, component) in out.iter_mut().enumerate() {
                let value = call.raw_table_get(table, Value::Number((i + 1) as f64))?;
                if i == 3 && value == Value::Nil {
                    continue;
                }
                let Some(value) = number(call, value)? else {
                    return Ok(None);
                };
                *component = value;
            }
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}
