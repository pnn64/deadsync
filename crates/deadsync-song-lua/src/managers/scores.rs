//! Existing high-score list defaults and dynamic name lookup.
use super::catalog::{arg, text};
use super::fields::{Field, object, set};
use super::*;

pub(super) fn method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    value: impl Field,
) -> Result<(), Error> {
    let value = value.value(call)?;
    let function = call.new_host_function(constant, &[value])?;
    set(owner, name, function, call)
}
pub(super) fn empty(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = call.try_new_table()?;
    call.push(Value::Table(result))
}
pub(super) fn radar(call: &mut HostCall<'_>, fallback: f32) -> Result<TableId, Error> {
    object(call, |call, table| {
        method(call, table, "GetValue", fallback)
    })
}
pub(super) fn score(call: &mut HostCall<'_>, name: &str) -> Result<TableId, Error> {
    object(call, |call, owner| {
        for (method_name, value) in [
            ("GetName", name),
            ("GetGrade", "Grade_Tier07"),
            ("GetDate", ""),
            ("GetModifiers", ""),
            ("GetStageAward", "StageAward_None"),
            ("GetPeakComboAward", "PeakComboAward_None"),
        ] {
            method(call, owner, method_name, value)?;
        }
        for name in [
            "GetScore",
            "GetPercentDP",
            "GetPercentDancePoints",
            "GetTapNoteScore",
            "GetHoldNoteScore",
            "GetMaxCombo",
            "GetSurvivalSeconds",
        ] {
            method(call, owner, name, 0_i32)?;
        }
        method(call, owner, "IsFillInMarker", false)?;
        let function = call.new_host_function(score_radar, &[])?;
        set(owner, "GetRadarValues", function, call)
    })
}
fn score_radar(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = radar(call, 0.)?;
    call.push(Value::Table(value))
}
pub(super) fn list(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = object(call, |call, owner| {
        let values = object(call, |call, values| {
            let machine = score(call, "Machine")?;
            set(values, 1_i32, machine, call)
        })?;
        call.with_roots(&[Value::Table(values)], |call| {
            method(call, owner, "GetHighScores", values)?;
            for (name, rank) in [("GetHighestScoreOfName", false), ("GetRankOfName", true)] {
                let function =
                    call.new_host_function_with_data(lookup, &[Value::Table(values)], rank)?;
                set(owner, name, function, call)?;
            }
            Ok(())
        })
    })?;
    call.push(Value::Table(result))
}

fn lookup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let rank = *call.function_data::<bool>().unwrap();
    let Some(name) = text(call, arg(call, 0)) else {
        return call.push(if rank { Value::Number(0.) } else { Value::Nil });
    };
    let values = table(call, call.captures()[0])?;
    let mut index = 1;
    loop {
        // Native sequence_values uses raw successive slots, stopping at nil.
        let score = call.raw_table_get(values, Value::Number(index as f64))?;
        if score == Value::Nil {
            break;
        }
        table(call, score)?;
        let matches = call.with_roots(&[score], |call| {
            let function = super::super::field(call, score, "GetName")?;
            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"GetName: function expected"));
            }
            let value = call
                .call(function, &[score])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            let id = call
                .coerce_string(value)?
                .ok_or_else(|| call.error(b"GetName: string expected"))?;
            let bytes = call.string_bytes(id).unwrap();
            let value =
                std::str::from_utf8(bytes).map_err(|e| call.error(e.to_string().as_bytes()))?;
            if value == name {
                // Keep the selected score rooted after callbacks remove its list slot.
                call.push(if rank {
                    Value::Number(index as f64)
                } else {
                    score
                })?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;
        if matches {
            return Ok(());
        }
        index += 1;
    }
    call.push(if rank { Value::Number(0.) } else { Value::Nil })
}
