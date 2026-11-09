//! Existing authored Judgment normalization and retained note/result methods.
use super::*;

fn field(call: &mut HostCall<'_>, table: TableId, name: &[u8]) -> Result<Value, Error> {
    let name = call.try_intern(name)?;
    call.table_get(Value::Table(table), name)
}
pub(super) fn set(
    call: &mut HostCall<'_>,
    table: TableId,
    name: &[u8],
    value: Value,
) -> Result<(), Error> {
    let name = call.try_intern(name)?;
    call.table_set(Value::Table(table), name, value)
}
fn first<T>(
    call: &mut HostCall<'_>,
    table: TableId,
    names: &[&[u8]],
    convert: impl Fn(&HostCall<'_>, Value) -> Option<T>,
) -> Result<Option<T>, Error> {
    for name in names {
        let value = field(call, table, name)?;
        if let Some(value) = convert(call, value) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}
fn text_field(
    call: &mut HostCall<'_>,
    table: TableId,
    names: &[&[u8]],
) -> Result<Option<Vec<u8>>, Error> {
    first(call, table, names, command_name)
}
fn float_field(
    call: &mut HostCall<'_>,
    table: TableId,
    names: &[&[u8]],
) -> Result<Option<f32>, Error> {
    first(call, table, names, |call, value| {
        float_scalar(call, value, false)
    })
}
fn constant(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])
}
fn method(call: &mut HostCall<'_>, table: TableId, name: &[u8], value: Value) -> Result<(), Error> {
    let function = call.new_host_function(constant, &[value])?;
    set(call, table, name, function)
}
fn string_method(
    call: &mut HostCall<'_>,
    table: TableId,
    name: &[u8],
    value: &[u8],
) -> Result<(), Error> {
    let value = call.try_intern(value)?;
    method(call, table, name, value)
}

pub(super) fn normalize(call: &mut HostCall<'_>, params: TableId) -> Result<(), Error> {
    for (key, default) in [
        (b"Player".as_slice(), b"PlayerNumber_P1".as_slice()),
        (b"TapNoteScore", b"TapNoteScore_W3"),
        (b"HoldNoteScore", b"HoldNoteScore_None"),
    ] {
        if field(call, params, key)? == Value::Nil {
            let default = call.try_intern(default)?;
            set(call, params, key, default)?;
        }
    }
    if field(call, params, b"TapNoteOffset")? == Value::Nil {
        set(call, params, b"TapNoteOffset", Value::Number(0.0))?;
    }
    let notes = match field(call, params, b"Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = call.try_new_table()?;
            call.with_roots(&[Value::Table(notes)], |call| {
                let track = first(call, params, &[b"FirstTrack", b"Column"], integer_scalar)?
                    .unwrap_or(0)
                    .max(0);
                let note = create_note(call, params, None)?;
                call.raw_table_set(notes, Value::Number(track as f64 + 1.0), Value::Table(note))?;
                set(call, params, b"Notes", Value::Table(notes))?;
                Ok(notes)
            })?
        }
    };
    call.with_roots(&[Value::Table(notes)], |call| {
        let entries = call.call(helper(call, 7)?, &[Value::Table(notes)])?[0];
        call.with_roots(&[entries], |call| {
            let entries = table(call, entries)?;
            let count = call.raw_table_len(entries)?;
            if count == 0 {
                let note = create_note(call, params, None)?;
                call.raw_table_set(notes, Value::Number(1.0), Value::Table(note))?;
            }
            for index in 1..=count {
                let entry = table(
                    call,
                    call.raw_table_get(entries, Value::Number(index as f64))?,
                )?;
                let key = call.raw_table_get(entry, Value::Number(1.0))?;
                let value = call.raw_table_get(entry, Value::Number(2.0))?;
                let note = match value {
                    Value::Table(note) => {
                        normalize_note(call, params, note)?;
                        note
                    }
                    value => create_note(call, params, Some(value))?,
                };
                call.table_set(Value::Table(notes), key, Value::Table(note))?;
            }
            Ok(())
        })
    })
}

fn create_note(
    call: &mut HostCall<'_>,
    params: TableId,
    value: Option<Value>,
) -> Result<TableId, Error> {
    let note = call.try_new_table()?;
    call.with_roots(&[Value::Table(note)], |call| {
        if let Some(value) = value {
            set(call, note, b"TapNoteType", value)?;
        }
        normalize_note(call, params, note)?;
        Ok(note)
    })
}

pub(in crate::actors) fn normalize_note(
    call: &mut HostCall<'_>,
    params: TableId,
    note: TableId,
) -> Result<(), Error> {
    if field(call, note, b"GetTapNoteType")? != Value::Nil {
        return Ok(());
    }
    let result = match field(call, note, b"TapNoteResult")? {
        Value::Table(result) => result,
        _ => call.try_new_table()?,
    };
    call.with_roots(&[Value::Table(result)], |call| {
        if field(call, result, b"GetHeld")? == Value::Nil {
            result_methods(call, params, note, result)?;
        }
        set(call, note, b"TapNoteResult", Value::Table(result))?;
        set(call, note, b"HoldNoteResult", Value::Table(result))?;
        note_methods(call, params, note, result)
    })
}

fn result_methods(
    call: &mut HostCall<'_>,
    params: TableId,
    note: TableId,
    result: TableId,
) -> Result<(), Error> {
    let held = first(call, note, &[b"Held", b"held"], boolish)?.unwrap_or(false);
    let hidden = first(call, note, &[b"Hidden", b"hidden"], boolish)?.unwrap_or(false);
    let offset = float_field(call, note, &[b"TapNoteOffset", b"Offset"])?
        .or(float_field(call, params, &[b"TapNoteOffset", b"Offset"])?)
        .unwrap_or(0.0);
    let score = text_field(call, note, &[b"TapNoteScore", b"Score"])?
        .or(text_field(call, params, &[b"TapNoteScore", b"Score"])?)
        .unwrap_or_else(|| b"TapNoteScore_None".to_vec());
    method(call, result, b"GetHeld", Value::Boolean(held))?;
    method(call, result, b"GetHidden", Value::Boolean(hidden))?;
    method(
        call,
        result,
        b"GetTapNoteOffset",
        Value::Number(offset as f64),
    )?;
    string_method(call, result, b"GetTapNoteScore", &score)
}

fn note_methods(
    call: &mut HostCall<'_>,
    params: TableId,
    note: TableId,
    result: TableId,
) -> Result<(), Error> {
    let kind = text_field(call, note, &[b"TapNoteType", b"Type", b"NoteType"])?
        .or(text_field(
            call,
            params,
            &[b"TapNoteType", b"Type", b"NoteType"],
        )?)
        .unwrap_or_else(|| b"TapNoteType_Tap".to_vec());
    let source = text_field(call, note, &[b"TapNoteSource", b"Source"])?
        .unwrap_or_else(|| b"TapNoteSource_Original".to_vec());
    let subtype = text_field(call, note, &[b"TapNoteSubType", b"SubType"])?
        .unwrap_or_else(|| b"TapNoteSubType_Hold".to_vec());
    let player =
        text_field(call, params, &[b"Player"])?.unwrap_or_else(|| b"PlayerNumber_P1".to_vec());
    let hold = float_field(call, note, &[b"HoldDuration"])?.unwrap_or(0.0);
    let attack = float_field(call, note, &[b"AttackDuration"])?.unwrap_or(0.0);
    let mods = text_field(call, note, &[b"AttackModifiers"])?.unwrap_or_default();
    let keysound = first(call, note, &[b"KeysoundIndex"], integer_scalar)?.unwrap_or(0);
    for (name, value) in [
        (b"GetTapNoteType".as_slice(), kind),
        (b"GetTapNoteSource", source),
        (b"GetTapNoteSubType", subtype),
        (b"GetPlayerNumber", player),
        (b"GetAttackModifiers", mods),
    ] {
        string_method(call, note, name, &value)?;
    }
    method(call, note, b"GetTapNoteResult", Value::Table(result))?;
    method(call, note, b"GetHoldNoteResult", Value::Table(result))?;
    for (name, value) in [
        (b"GetHoldDuration".as_slice(), hold as f64),
        (b"GetAttackDuration", attack as f64),
        (b"GetKeysoundIndex", keysound as f64),
    ] {
        method(call, note, name, Value::Number(value))?;
    }
    Ok(())
}
