//! The existing compiler's raw ease sequence and loaded easing identities.
use super::*;
use crate::{
    SongLuaCompileInfo, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaFunctionEaseDecision,
    SongLuaFunctionEaseResult, SongLuaReadEasesResult, SongLuaReadEasesStats, SongLuaSpanMode,
    SongLuaTimeUnit,
};

/// Compiler-owned names keyed by rooted function/table identities. Release at
/// the end of preparation; a released registry cannot be read or registered.
pub struct SongEasingNames {
    owner: Option<Root>,
}

/// The function is retained throughout the capture callback, including reentry
/// and GC. A callback which keeps it afterward must acquire its own root.
#[derive(Clone)]
pub struct SongFunctionEaseInput {
    pub function: Value,
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: f32,
    pub to: f32,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

impl SongRuntime {
    pub fn new_song_easing_names(&mut self) -> Result<SongEasingNames, Error> {
        let table = self.vm.try_new_table()?;
        Ok(SongEasingNames {
            owner: Some(self.vm.pin(Value::Table(table))?),
        })
    }

    /// Read ease then xero with ordinary lookup, retaining the original table
    /// through getters. Aliases use the last registered name, as native. An
    /// error preserves earlier registrations and raw table-name annotations.
    pub fn register_loaded_song_easing_names(
        &mut self,
        names: &mut SongEasingNames,
    ) -> Result<(), String> {
        let registry = self.song_easing_registry(names)?;
        let globals = self
            .vm
            .with_native_stack(|stack| Ok(stack.value(-10002).unwrap()))
            .map_err(|e| e.to_string())?;
        let function = self
            .vm
            .new_host_function(register_names, &[globals, Value::Table(registry)])
            .map_err(|e| e.to_string())?;
        self.vm.call(function, &[]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn release_song_easing_names(
        &mut self,
        names: &mut SongEasingNames,
    ) -> Result<bool, Error> {
        let Some(owner) = names.owner else {
            return Ok(false);
        };
        self.vm.unpin(owner)?;
        names.owner = None;
        Ok(true)
    }

    pub(super) fn song_easing_registry(&self, names: &SongEasingNames) -> Result<TableId, String> {
        let owner = names.owner.ok_or("released song easing names")?;
        let Value::Table(table) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        Ok(table)
    }

    /// Iterate the original raw sequence up to its first nil. Entry callbacks
    /// can change later slots, replace globals, nest readers, or collect. Owned
    /// results are published only on success; callback mutations are not rewound
    /// here, since that scope belongs to the existing outer compiler pass.
    pub fn read_song_eases_with_function_capture<F>(
        &mut self,
        table: Option<Value>,
        unit: SongLuaTimeUnit,
        names: &SongEasingNames,
        mut capture: F,
    ) -> Result<SongLuaReadEasesResult, String>
    where
        F: FnMut(
            &mut Self,
            SongFunctionEaseInput,
            &mut SongLuaCompileInfo,
        ) -> Result<SongLuaFunctionEaseResult, String>,
    {
        let mut output = SongLuaReadEasesResult {
            eases: Vec::new(),
            overlay_eases: Vec::new(),
            column_offsets: Vec::new(),
            info: SongLuaCompileInfo::default(),
            stats: SongLuaReadEasesStats::default(),
        };
        let Some(table) = table.filter(|v| *v != Value::Nil) else {
            return Ok(output);
        };
        let registry = self.song_easing_registry(names)?;
        let pin = self.vm.pin(table).map_err(|e| e.to_string())?;
        let result = (|| {
            let Value::Table(table) = table else {
                return Err("ease table expected".into());
            };
            let mut index = 1;
            loop {
                let entry = self.ease_slot(table, index)?;
                if entry == Value::Nil {
                    break;
                }
                index += 1;
                let Value::Table(entry) = entry else {
                    continue;
                };
                output.stats.entry_count += 1;
                let entry_pin = self
                    .vm
                    .pin(Value::Table(entry))
                    .map_err(|e| e.to_string())?;
                let result: Result<(), String> = (|| {
                    let Some(start) = self.ease_float_slot(entry, 1)? else {
                        return Ok(());
                    };
                    let Some(limit) = self.ease_float_slot(entry, 2)? else {
                        return Ok(());
                    };
                    let Some(from) = self.ease_float_slot(entry, 3)? else {
                        return Ok(());
                    };
                    let Some(to) = self.ease_float_slot(entry, 4)? else {
                        return Ok(());
                    };
                    let field6 = self.ease_slot(entry, 6)?;
                    let span = match field6 {
                        Value::String(id) => match self.vm.string_bytes(id).unwrap() {
                            bytes if bytes.eq_ignore_ascii_case(b"len") => {
                                Some(SongLuaSpanMode::Len)
                            }
                            bytes if bytes.eq_ignore_ascii_case(b"end") => {
                                Some(SongLuaSpanMode::End)
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    let offset = usize::from(span.is_some());
                    let span_mode = span.unwrap_or(SongLuaSpanMode::Len);
                    let easing_value = if offset == 0 {
                        field6
                    } else {
                        self.ease_slot(entry, 7)?
                    };
                    let player_value = self.ease_slot(entry, 7 + offset)?;
                    let sustain = self.ease_float_slot(entry, 8 + offset)?;
                    let opt1 = self.ease_float_slot(entry, 9 + offset)?;
                    let opt2 = self.ease_float_slot(entry, 10 + offset)?;
                    let easing = self.song_easing_name(easing_value, registry)?;
                    let target = self.ease_slot(entry, 5)?;
                    let target = match target {
                        Value::String(id) => SongLuaEaseTarget::Mod(
                            std::str::from_utf8(self.vm.string_bytes(id).unwrap())
                                .map_err(|e| e.to_string())?
                                .to_owned(),
                        ),
                        Value::Function(_) | Value::Native(_) => {
                            output.stats.function_targets += 1;
                            let function_pin = self.vm.pin(target).map_err(|e| e.to_string())?;
                            let result = capture(
                                self,
                                SongFunctionEaseInput {
                                    function: target,
                                    unit,
                                    start,
                                    limit,
                                    span_mode,
                                    from,
                                    to,
                                    easing: easing.clone(),
                                    sustain,
                                    opt1,
                                    opt2,
                                },
                                &mut output.info,
                            );
                            self.vm.unpin(function_pin).map_err(|e| e.to_string())?;
                            let result = result?;
                            output.stats.overlay_capture_attempts +=
                                result.stats.overlay_capture_attempts;
                            output.stats.overlay_capture_outputs +=
                                result.stats.overlay_capture_outputs;
                            output.stats.probe_ms += result.stats.probe_ms;
                            output.stats.overlay_capture_ms += result.stats.overlay_capture_ms;
                            match result.decision {
                                SongLuaFunctionEaseDecision::ColumnOffsets(rows) => {
                                    output.column_offsets.extend(rows);
                                    return Ok(());
                                }
                                SongLuaFunctionEaseDecision::OverlayEases(rows) => {
                                    output.stats.overlay_capture_outputs += rows.len();
                                    output.overlay_eases.extend(rows);
                                    return Ok(());
                                }
                                SongLuaFunctionEaseDecision::Target(
                                    SongLuaEaseTarget::Function,
                                )
                                | SongLuaFunctionEaseDecision::Skip => return Ok(()),
                                SongLuaFunctionEaseDecision::Target(target) => target,
                            }
                        }
                        _ => return Ok(()),
                    };
                    output.eases.push(SongLuaEaseWindow {
                        approach_speed: None,
                        unit,
                        start,
                        limit,
                        span_mode,
                        from,
                        to,
                        target,
                        easing,
                        player: match player_value {
                            Value::Number(v) if (1. ..=2.).contains(&v) => Some(v as u8),
                            _ => None,
                        },
                        sustain,
                        opt1,
                        opt2,
                    });
                    Ok(())
                })();
                self.vm.unpin(entry_pin).map_err(|e| e.to_string())?;
                result?;
            }
            Ok(output)
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }

    fn ease_slot(&self, table: TableId, slot: usize) -> Result<Value, String> {
        self.vm
            .raw_table_get(table, Value::Number(slot as f64))
            .map_err(|e| e.to_string())
    }
    fn ease_float_slot(&self, table: TableId, slot: usize) -> Result<Option<f32>, String> {
        Ok(match self.ease_slot(table, slot)? {
            Value::Number(v) => {
                let v = v as f32;
                v.is_finite().then_some(v)
            }
            Value::String(id) => std::str::from_utf8(self.vm.string_bytes(id).unwrap())
                .ok()
                .and_then(|v| v.trim().parse().ok()),
            _ => None,
        })
    }
    pub(super) fn song_easing_name(
        &mut self,
        value: Value,
        registry: TableId,
    ) -> Result<Option<String>, String> {
        let pin = self.vm.pin(value).map_err(|e| e.to_string())?;
        let result = (|| {
            let value = match value {
                Value::String(_) => value,
                Value::Function(_) | Value::Native(_) | Value::Table(_) => {
                    let name = self
                        .vm
                        .raw_table_get(registry, value)
                        .map_err(|e| e.to_string())?;
                    if name != Value::Nil {
                        name
                    } else if let Value::Table(table) = value {
                        let key = self
                            .vm
                            .try_intern(crate::SONG_LUA_EASING_NAME_KEY.as_bytes())
                            .map_err(|e| e.to_string())?;
                        self.vm
                            .raw_table_get(table, key)
                            .map_err(|e| e.to_string())?
                    } else {
                        Value::Nil
                    }
                }
                _ => Value::Nil,
            };
            // mlua's Option<String> raw table tag permits numeric coercion. This
            // conversion does not consult an authored tostring or __tostring.
            self.vm
                .with_native_stack(|stack| {
                    stack.push(value)?;
                    Ok(stack
                        .to_bytes(-1)?
                        .and_then(|b| std::str::from_utf8(b).ok().map(str::to_owned)))
                })
                .map_err(|e| e.to_string())
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
}

fn register_names(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = call.captures()[0];
    let Value::Table(registry) = call.captures()[1] else {
        unreachable!()
    };
    for namespace in ["ease", "xero"] {
        let table = field(call, globals, namespace)?;
        if table == Value::Nil {
            continue;
        }
        if !matches!(table, Value::Table(_)) {
            return Err(call.error(b"easing namespace: table expected"));
        }
        call.with_roots(&[table], |call| {
            for name in crate::SONG_LUA_EASING_NAMES {
                let value = field(call, table, name)?;
                if matches!(
                    value,
                    Value::Function(_) | Value::Native(_) | Value::Table(_)
                ) {
                    call.with_roots(&[value], |call| {
                        let name = call.try_intern(name.as_bytes())?;
                        call.raw_table_set(registry, value, name)?;
                        if let Value::Table(table) = value {
                            let key =
                                call.try_intern(crate::SONG_LUA_EASING_NAME_KEY.as_bytes())?;
                            call.raw_table_set(table, key, name)?;
                        }
                        Ok(())
                    })?;
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
