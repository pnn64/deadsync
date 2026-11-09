use super::*;
fn optional_method(call: &mut HostCall<'_>, owner: Value, name: &str) -> Result<Value, Error> {
    let method = get(call, owner, name)?;
    if method == Value::Nil {
        return Ok(Value::Nil);
    }
    let method = function(call, method)?;
    Ok(call
        .call(method, &[owner])?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}
fn bpms(call: &mut HostCall<'_>, value: Value) -> Result<Option<[f32; 2]>, Error> {
    let Value::Table(table) = value else {
        return Ok(None);
    };
    let first = call.raw_table_get(table, Value::Number(1.0))?;
    let second = call.raw_table_get(table, Value::Number(2.0))?;
    Ok(float(call, first)
        .zip(float(call, second))
        .map(|(a, b)| [a, b]))
}
fn display_bpms(call: &mut HostCall<'_>, value: Value) -> Result<[f32; 2], Error> {
    let result = optional_method(call, value, "GetDisplayBpms")?;
    if let Some(bpms) = bpms(call, result)?
        && bpms[0] > 0.0
        && bpms[1] > 0.0
    {
        return Ok(bpms);
    }
    let timing = optional_method(call, value, "GetTimingData")?;
    if !matches!(timing, Value::Table(_)) {
        return Ok([0.0; 2]);
    }
    call.with_roots(&[timing], |call| {
        let value = optional_method(call, timing, "GetActualBPM")?;
        Ok(bpms(call, value)?.unwrap_or([0.0; 2]))
    })
}
pub(super) fn binding(call: &mut HostCall<'_>, name: &str, meta: &Metadata) -> Result<(), Error> {
    match name {
        "GetDisplayBPMs" | "StringifyDisplayBPMs" => {
            let rate = call
                .arguments()
                .get(2)
                .copied()
                .and_then(|v| float(call, v))
                .unwrap_or(meta.rate)
                .max(f32::EPSILON);
            let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
            let bpms = if matches!(value, Value::Table(_)) {
                display_bpms(call, value)?
            } else {
                meta.display_bpms
            };
            let bpms = [bpms[0] * rate, bpms[1] * rate];
            let result = if name == "GetDisplayBPMs" {
                Value::Table(array(call, bpms)?)
            } else {
                call.try_intern(crate::display_bpms_text(bpms, rate).as_bytes())?
            };
            call.push(result)
        }
        "GetNotefieldX" => call.push(
            player(call, arg(call, 0))
                .map(|i| Value::Number(meta.screen_x[i] as f64))
                .unwrap_or(Value::Nil),
        ),
        "GetNotefieldWidth" => {
            let style = super::super::player_options::current_style(call);
            call.push(Value::Number(
                crate::song_lua_style_info(&style).width as f64,
            ))
        }
        "GetGameplayLayout" => {
            let reverse = truthy(arg(call, 1));
            let sign = if reverse { -1.0 } else { 1.0 };
            let combo = meta.center_y + sign * 30.0;
            let judgment = meta.center_y - sign * 30.0;
            let result = object(call, |call, result| {
                for (name, y, height) in [
                    ("Combo", combo, None),
                    ("ErrorBar", judgment, Some(30.0)),
                    ("MeasureCounter", judgment + sign * 28.0, None),
                    ("SubtractiveScoring", judgment - sign * 28.0, None),
                ] {
                    let slot = object(call, |call, slot| {
                        set(slot, "y", y, call)?;
                        if let Some(height) = height {
                            set(slot, "maxHeight", height, call)?;
                        }
                        Ok(())
                    })?;
                    set(result, name, slot, call)?;
                }
                Ok(())
            })?;
            call.push(Value::Table(result))
        }
        _ => unreachable!("installed GameState helper"),
    }
}
