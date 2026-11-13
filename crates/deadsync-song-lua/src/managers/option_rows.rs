//! Existing option-row factories and their preference/SL publication callbacks.
use super::catalog::{arg, text};
use super::fields::{array, object, set};
use super::*;
use crate::option_rows as specs;

#[derive(Clone)]
enum Op {
    Custom,
    Conf(&'static str),
    Theme,
    Init,
    Operator(String),
    OperatorIndex,
    Enabled,
    Load(usize),
    Save(Save, String),
}
#[derive(Clone, Copy)]
enum Save {
    Effect,
    Pref,
    Theme,
    Custom,
}
const OPERATORS: &[&str] = &[
    "Theme",
    "EditorNoteskin",
    "DefaultFailType",
    "LongAndMarathonTime",
    "MusicWheelSpeed",
    "VideoRenderer",
    "GlobalOffsetSeconds",
    "VisualDelaySeconds",
    "MemoryCards",
    "CustomSongsMaxSeconds",
    "CustomSongsMaxMegabytes",
    "CustomSongsLoadTimeout",
];

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let captures = [
        vm.get_global(b"_G")?,
        vm.get_global(b"tonumber")?,
        vm.get_global(b"setmetatable")?,
    ];
    let setup = vm.new_host_function(initialize, &captures)?;
    vm.call(setup, &[]).map(|_| ())
}
fn binding(call: &mut HostCall<'_>, op: Op, row: Value) -> Result<Value, Error> {
    let captures = [
        call.captures()[0],
        call.captures()[1],
        call.captures()[2],
        row,
    ];
    call.new_host_function_with_data(callback, &captures, op)
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let custom = binding(call, Op::Custom, Value::Nil)?;
    set(globals, "CustomOptionRow", custom, call)?;
    for name in [
        "ConfAspectRatio",
        "ConfDisplayResolution",
        "ConfDisplayMode",
        "ConfRefreshRate",
        "ConfFullscreenType",
    ] {
        let f = binding(call, Op::Conf(name), Value::Nil)?;
        set(globals, name, f, call)?;
    }
    let theme = object(call, |call, owner| {
        for (name, op) in [("GetRow", Op::Theme), ("InitAll", Op::Init)] {
            let f = binding(call, op, Value::Nil)?;
            set(owner, name, f, call)?;
        }
        Ok(())
    })?;
    set(globals, "ThemePrefsRows", theme, call)?;
    let operators = object(call, |call, owner| {
        for &name in OPERATORS {
            let f = binding(call, Op::Operator(name.into()), Value::Nil)?;
            set(owner, name, f, call)?;
        }
        let mt = object(call, |call, mt| {
            let f = binding(call, Op::OperatorIndex, Value::Nil)?;
            set(mt, "__index", f, call)
        })?;
        call.call(call.captures()[2], &[Value::Table(owner), Value::Table(mt)])?;
        Ok(())
    })?;
    set(globals, "OperatorMenuOptionRows", operators, call)
}
fn get(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    super::super::field(call, owner, key)
}
fn get_table(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<TableId, Error> {
    let value = get(call, owner, key)?;
    table(call, value)
}
fn function(call: &HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"function expected"))
    }
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn list(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    if let Value::Table(list) = arg(call, 0) {
        return Ok(list);
    }
    if let Some(Value::Table(list)) = call.arguments().first() {
        return Ok(*list);
    }
    call.try_new_table()
}
fn selected(call: &mut HostCall<'_>, row: Value, list: TableId) -> Result<Value, Error> {
    let values = match get(call, row, "Values")? {
        Value::Table(values) => values,
        _ => get_table(call, row, "Choices")?,
    };
    let count = call.raw_table_len(values)?.max(call.raw_table_len(list)?);
    for i in 1..=count {
        if truthy(call.raw_table_get(list, Value::Number(i as f64))?) {
            return call.raw_table_get(values, Value::Number(i as f64));
        }
    }
    call.raw_table_get(values, Value::Number(1.))
}
fn create(
    call: &mut HostCall<'_>,
    name: &str,
    spec: specs::SongLuaOptionRowSpec,
    save: Save,
    save_name: &str,
) -> Result<TableId, Error> {
    object(call, |call, row| {
        set(row, "Name", name, call)?;
        set(
            row,
            "Choices",
            super::theme_preferences::options(call, spec.choices)?,
            call,
        )?;
        if let Some(values) = spec.values {
            set(
                row,
                "Values",
                super::theme_preferences::options(call, values)?,
                call,
            )?;
        }
        set(row, "LayoutType", spec.layout_type, call)?;
        set(row, "SelectType", spec.select_type, call)?;
        set(
            row,
            "OneChoiceForAllPlayers",
            spec.one_choice_for_all_players,
            call,
        )?;
        set(row, "ExportOnChange", spec.export_on_change, call)?;
        set(row, "HideOnDisable", spec.hide_on_disable, call)?;
        set(
            row,
            "ReloadRowMessages",
            array(call, spec.reload_row_messages.iter().copied())?,
            call,
        )?;
        set(
            row,
            "BroadcastOnExport",
            array(call, spec.broadcast_on_export.iter().copied())?,
            call,
        )?;
        for (name, op, owner) in [
            ("EnabledForPlayers", Op::Enabled, Value::Nil),
            ("LoadSelections", Op::Load(spec.choices.len()), Value::Nil),
            (
                "SaveSelections",
                Op::Save(save, save_name.into()),
                if matches!(save, Save::Effect) {
                    Value::Nil
                } else {
                    Value::Table(row)
                },
            ),
        ] {
            let f = binding(call, op, owner)?;
            set(row, name, f, call)?;
        }
        Ok(())
    })
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = call.function_data::<Op>().unwrap().clone();
    let value = match op {
        Op::Custom => {
            let name = call
                .arguments()
                .first()
                .copied()
                .and_then(|v| text(call, v));
            let Some(name) = name else {
                return call.push(Value::Boolean(false));
            };
            let Some(spec) = specs::custom_option_row_spec(&name) else {
                return call.push(Value::Boolean(false));
            };
            Value::Table(create(call, &name, spec, Save::Custom, &name)?)
        }
        Op::Conf(name) => {
            let spec = specs::conf_option_row_spec(name);
            Value::Table(create(
                call,
                &spec.row_name,
                spec.spec,
                Save::Pref,
                &spec.row_name,
            )?)
        }
        Op::Theme => {
            let name = text(call, arg(call, 0)).unwrap_or_default();
            Value::Table(create(
                call,
                &name,
                specs::theme_pref_row_spec(&name),
                Save::Theme,
                &name,
            )?)
        }
        Op::Init => {
            let supplied = if call.arguments().len() == 1 {
                call.arguments()[0]
            } else {
                arg(call, 0)
            };
            // The control eagerly creates defaults even when supplied definitions win.
            let defaults = super::theme_preferences::definitions(call)?;
            let defs = if supplied == Value::Nil {
                Value::Table(defaults)
            } else {
                supplied
            };
            return call.with_roots(&[defs], |call| {
                let manager = get_table(call, call.captures()[0], "ThemePrefs")?;
                let f = get(call, Value::Table(manager), "InitAll")?;
                let f = function(call, f)?;
                call.call(f, &[defs]).map(|_| ())
            });
        }
        Op::Operator(name) => {
            let kind = text(call, arg(call, 0));
            let spec = specs::operator_menu_option_row_spec(&name, kind.as_deref());
            let (save, key) = if let Some(key) = spec.pref_name.as_deref() {
                (Save::Pref, key)
            } else {
                (Save::Effect, "")
            };
            Value::Table(create(call, &spec.row_name, spec.spec, save, key)?)
        }
        Op::OperatorIndex => {
            let name = text(call, arg(call, 0)).unwrap_or_default();
            binding(call, Op::Operator(name), Value::Nil)?
        }
        Op::Enabled => Value::Table(array(
            call,
            [crate::player_number_name(0), crate::player_number_name(1)],
        )?),
        Op::Load(count) => {
            let list = list(call)?;
            let mut found = false;
            if count > 0 {
                for i in 1..=count.max(call.raw_table_len(list)?) {
                    if truthy(call.raw_table_get(list, Value::Number(i as f64))?) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    call.raw_table_set(list, Value::Number(1.), Value::Boolean(true))?;
                }
            }
            Value::Table(list)
        }
        Op::Save(kind, name) => {
            super::side_effect(call, call.captures()[0], call.captures()[1])?;
            if matches!(kind, Save::Effect) {
                return Ok(());
            }
            let list = list(call)?;
            return call.with_roots(&[Value::Table(list)], |call| save(call, kind, &name, list));
        }
    };
    call.push(value)
}
fn save(call: &mut HostCall<'_>, kind: Save, name: &str, list: TableId) -> Result<(), Error> {
    let row = call.captures()[3];
    if matches!(kind, Save::Pref | Save::Theme) {
        let value = selected(call, row, list)?;
        return call.with_roots(&[value], |call| {
            let (manager, method) = if matches!(kind, Save::Pref) {
                ("PREFSMAN", "SetPreference")
            } else {
                ("ThemePrefs", "Set")
            };
            let manager = get_table(call, call.captures()[0], manager)?;
            call.with_roots(&[Value::Table(manager)], |call| {
                let f = get(call, Value::Table(manager), method)?;
                let f = function(call, f)?;
                let key = call.try_intern(name.as_bytes())?;
                call.call(f, &[Value::Table(manager), key, value])
                    .map(|_| ())
            })
        });
    }
    let args = call.arguments();
    let index = if matches!(args.first(), Some(Value::Table(_)))
        && matches!(args.get(1), Some(Value::Table(_)))
    {
        2
    } else {
        1
    };
    let player = super::game_state::player(call, args.get(index).copied().unwrap_or(Value::Nil))
        .unwrap_or(0);
    let sl = get_table(call, call.captures()[0], "SL")?;
    call.with_roots(&[Value::Table(sl)], |call| {
        let global = name.eq_ignore_ascii_case("MusicRate");
        let owner = get_table(
            call,
            Value::Table(sl),
            if global {
                "Global"
            } else {
                crate::player_short_name(player)
            },
        )?;
        call.with_roots(&[Value::Table(owner)], |call| {
            let mods = get_table(call, Value::Table(owner), "ActiveModifiers")?;
            call.with_roots(&[Value::Table(mods)], |call| {
                if global {
                    let value = selected(call, row, list)?;
                    return super::super::set(call, Value::Table(mods), "MusicRate", value);
                }
                let select = get(call, row, "SelectType")?;
                let id = call
                    .coerce_string(select)?
                    .ok_or_else(|| call.error(b"SelectType string expected"))?;
                let select = text(call, Value::String(id))
                    .ok_or_else(|| call.error(b"invalid UTF-8 SelectType"))?;
                if select == "SelectMultiple" {
                    let choices = get_table(call, row, "Choices")?;
                    call.with_roots(&[Value::Table(choices)], |call| {
                        let count = call.raw_table_len(choices)?.max(call.raw_table_len(list)?);
                        for i in 1..=count {
                            let selected =
                                truthy(call.raw_table_get(list, Value::Number(i as f64))?);
                            let value = call.raw_table_get(choices, Value::Number(i as f64))?;
                            let choice = text(call, value).unwrap_or_default();
                            let key = crate::custom_multi_modifier_key(name, &choice);
                            if !key.is_empty() {
                                super::super::set(
                                    call,
                                    Value::Table(mods),
                                    &key,
                                    Value::Boolean(selected),
                                )?;
                            }
                        }
                        Ok(())
                    })
                } else {
                    let value = selected(call, row, list)?;
                    super::super::set(call, Value::Table(mods), name, value)
                }
            })
        })
    })
}
