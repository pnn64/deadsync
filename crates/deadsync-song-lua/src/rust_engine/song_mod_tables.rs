//! Existing mod-window and runtime-ease tables, read through Rust host values.
use super::*;
use crate::{
    RuntimeModEaseEntry, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaModWindow,
    SongLuaOverlayEase, SongLuaOverlayStateDelta, SongLuaSpanMode, SongLuaTimeUnit,
};
use std::collections::{HashMap, HashSet};

impl RustSongRuntime {
    pub(super) fn schedule_rows<T>(
        &mut self,
        source: Option<Value>,
        mut read: impl FnMut(&mut Self, TableId) -> Result<Option<T>, String>,
    ) -> Result<Vec<T>, String> {
        let Some(value) = source.filter(|v| *v != Value::Nil) else {
            return Ok(Vec::new());
        };
        let pin = self.vm.pin(value).map_err(|e| e.to_string())?;
        let result = (|| {
            let Value::Table(source) = value else {
                return Err("schedule table expected".into());
            };
            let mut output = Vec::new();
            for i in 1usize.. {
                let value = self
                    .vm
                    .raw_table_get(source, Value::Number(i as f64))
                    .map_err(|e| e.to_string())?;
                if value == Value::Nil {
                    break;
                }
                let Value::Table(entry) = value else { continue };
                let entry_pin = self.vm.pin(value).map_err(|e| e.to_string())?;
                let row = read(self, entry);
                self.vm.unpin(entry_pin).map_err(|e| e.to_string())?;
                if let Some(row) = row? {
                    output.push(row)
                }
            }
            Ok(output)
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
    pub(super) fn schedule_field(&mut self, entry: TableId, name: &str) -> Result<Value, String> {
        let key = self
            .vm
            .try_intern(name.as_bytes())
            .map_err(|e| e.to_string())?;
        self.vm.raw_table_get(entry, key).map_err(|e| e.to_string())
    }
    pub(super) fn schedule_slot(&self, entry: TableId, index: usize) -> Result<Value, String> {
        self.vm
            .raw_table_get(entry, Value::Number(index as f64))
            .map_err(|e| e.to_string())
    }
    pub(super) fn schedule_text(&self, value: Value) -> Option<String> {
        if let Value::String(id) = value {
            std::str::from_utf8(self.vm.string_bytes(id).unwrap())
                .ok()
                .map(str::to_owned)
        } else {
            None
        }
    }
    pub(super) fn schedule_float(&self, value: Value) -> Option<f32> {
        match value {
            Value::Number(v) => {
                let v = v as f32;
                v.is_finite().then_some(v)
            }
            Value::String(_) => self.schedule_text(value)?.trim().parse().ok(),
            _ => None,
        }
    }
    /// Read the native raw first-nil sequence without imposing new span rules.
    pub fn read_song_mod_windows(
        &mut self,
        table: Option<Value>,
        unit: SongLuaTimeUnit,
    ) -> Result<Vec<SongLuaModWindow>, String> {
        self.schedule_rows(table, |runtime, entry| {
            let Some(start) = runtime.schedule_float(runtime.schedule_slot(entry, 1)?) else {
                return Ok(None);
            };
            let Some(limit) = runtime.schedule_float(runtime.schedule_slot(entry, 2)?) else {
                return Ok(None);
            };
            let Some(mods) = runtime.schedule_text(runtime.schedule_slot(entry, 3)?) else {
                return Ok(None);
            };
            let mode = runtime.schedule_text(runtime.schedule_slot(entry, 4)?);
            let span_mode = if mode
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("end"))
            {
                SongLuaSpanMode::End
            } else {
                SongLuaSpanMode::Len
            };
            let player = schedule_player(runtime.schedule_slot(entry, 5)?);
            Ok(Some(SongLuaModWindow {
                unit,
                start,
                limit,
                span_mode,
                mods,
                player,
            }))
        })
    }
    /// Compile the non-recurring runtime-mod format. Callback-owned tables are
    /// consumed by chronological replay instead of duplicating these endpoints.
    pub fn read_song_runtime_mod_eases(
        &mut self,
        table: Option<Value>,
        names: &SongEasingNames,
        static_overlay: Option<usize>,
        context: &SongLuaCompileContext,
    ) -> Result<(Vec<SongLuaEaseWindow>, Vec<SongLuaOverlayEase>), String> {
        if table.is_none_or(|v| v == Value::Nil) {
            return Ok((Vec::new(), Vec::new()));
        }
        let registry = self.song_easing_registry(names)?;
        let entries = self.schedule_rows(table, |runtime, entry| {
            runtime.runtime_mod_entry(entry, registry)
        })?;
        let mut seen = HashSet::new();
        let entries = entries.into_iter().filter(|e| {
            seen.insert((
                e.unit as u8,
                e.start.to_bits(),
                e.limit.to_bits(),
                e.easing.clone(),
                e.to.to_bits(),
                e.target.clone(),
                e.start_val.map(f32::to_bits),
                e.opt1.map(f32::to_bits),
                e.opt2.map(f32::to_bits),
                e.player,
                e.add,
            ))
        });
        let mut current: [HashMap<String, f32>; LUA_PLAYERS] =
            std::array::from_fn(|_| HashMap::new());
        let static_player = context.players.iter().position(|p| p.enabled).unwrap_or(0);
        let mut eases = Vec::new();
        let mut overlays = Vec::new();
        for entry in entries {
            let key = crate::runtime_mod_key(&entry.target);
            let target = match key.as_str() {
                "static" => None,
                "z" => Some(SongLuaEaseTarget::PlayerZ),
                "rotationx" => Some(SongLuaEaseTarget::PlayerRotationX),
                "rotationy" => Some(SongLuaEaseTarget::PlayerRotationY),
                "rotationz" => Some(SongLuaEaseTarget::PlayerRotationZ),
                "zoom" => Some(SongLuaEaseTarget::PlayerZoom),
                "zoomx" => Some(SongLuaEaseTarget::PlayerZoomX),
                "zoomy" => Some(SongLuaEaseTarget::PlayerZoomY),
                "zoomz" => Some(SongLuaEaseTarget::PlayerZoomZ),
                "x" | "y" => continue,
                _ => Some(SongLuaEaseTarget::Mod(entry.target.clone())),
            };
            for player in crate::runtime_mod_entry_players(entry.player) {
                let from = crate::runtime_mod_start_value(&mut current[player], &key, &entry);
                let to = crate::runtime_mod_end_value(from, &entry);
                current[player].insert(key.clone(), to);
                if let Some(target) = &target {
                    eases.push(SongLuaEaseWindow {
                        approach_speed: None,
                        unit: entry.unit,
                        start: entry.start,
                        limit: entry.limit,
                        span_mode: SongLuaSpanMode::Len,
                        from,
                        to,
                        target: target.clone(),
                        easing: Some(entry.easing.clone()),
                        player: Some((player + 1) as u8),
                        sustain: None,
                        opt1: entry.opt1,
                        opt2: entry.opt2,
                    });
                } else if let Some(index) = static_overlay
                    && player == static_player
                {
                    overlays.push(SongLuaOverlayEase {
                        overlay_index: index,
                        unit: SongLuaTimeUnit::Beat,
                        start: entry.start,
                        limit: entry.limit,
                        span_mode: SongLuaSpanMode::Len,
                        from: SongLuaOverlayStateDelta {
                            diffuse: Some([1., 1., 1., from]),
                            ..Default::default()
                        },
                        to: SongLuaOverlayStateDelta {
                            diffuse: Some([1., 1., 1., to]),
                            ..Default::default()
                        },
                        easing: Some(entry.easing.clone()),
                        sustain: None,
                        opt1: entry.opt1,
                        opt2: entry.opt2,
                    });
                }
            }
        }
        crate::extend_runtime_mod_sustains(&mut eases);
        Ok((eases, overlays))
    }
    fn runtime_mod_entry(
        &mut self,
        entry: TableId,
        registry: TableId,
    ) -> Result<Option<RuntimeModEaseEntry>, String> {
        let Some(start) = self.schedule_float(self.schedule_slot(entry, 1)?) else {
            return Ok(None);
        };
        let Some(mut limit) = self.schedule_float(self.schedule_slot(entry, 2)?) else {
            return Ok(None);
        };
        let Some(easing) = self.song_easing_name(self.schedule_slot(entry, 3)?, registry)? else {
            return Ok(None);
        };
        let Some(to) = self.schedule_float(self.schedule_slot(entry, 4)?) else {
            return Ok(None);
        };
        let Some(target) = self.schedule_text(self.schedule_slot(entry, 5)?) else {
            return Ok(None);
        };
        let timing = self.schedule_field(entry, "timing")?;
        if self
            .schedule_text(timing)
            .is_some_and(|s| s.eq_ignore_ascii_case("end"))
        {
            limit -= start
        }
        if !start.is_finite() || !limit.is_finite() || limit < 0. || !to.is_finite() {
            return Ok(None);
        }
        let mut player = schedule_player(self.schedule_field(entry, "plr")?);
        if player.is_none() {
            player = schedule_player(self.schedule_field(entry, "pn")?)
        }
        let start_val = self.schedule_field(entry, "startVal")?;
        let start_val = self.schedule_float(start_val);
        let opt1 = self.schedule_field(entry, "opt1")?;
        let opt1 = self.schedule_float(opt1);
        let opt2 = self.schedule_field(entry, "opt2")?;
        let opt2 = self.schedule_float(opt2);
        let add = self.schedule_field(entry, "add")?.is_truthy();
        Ok(Some(RuntimeModEaseEntry {
            unit: SongLuaTimeUnit::Beat,
            start,
            limit,
            easing,
            to,
            target,
            start_val,
            opt1,
            opt2,
            player,
            add,
        }))
    }
}
pub(super) fn schedule_player(value: Value) -> Option<u8> {
    match value {
        Value::Number(n) if (1. ..=2.).contains(&n) => Some(n as u8),
        _ => None,
    }
}
