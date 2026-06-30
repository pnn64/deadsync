use mlua::{Function, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;

use crate::{
    SongLuaColumnOffsetWindow, SongLuaCompileInfo, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaOverlayEase, SongLuaSpanMode, SongLuaTimeUnit, read_easing_name, read_f32, read_player,
    read_span_mode,
};

pub struct SongLuaReadEasesResult {
    pub eases: Vec<SongLuaEaseWindow>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindow>,
    pub info: SongLuaCompileInfo,
    pub stats: SongLuaReadEasesStats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaReadEasesStats {
    pub entry_count: usize,
    pub function_targets: usize,
    pub overlay_capture_attempts: usize,
    pub overlay_capture_outputs: usize,
    pub probe_ms: f64,
    pub overlay_capture_ms: f64,
}

#[derive(Clone)]
pub struct SongLuaFunctionEaseInput {
    pub function: Function,
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

pub struct SongLuaFunctionEaseResult {
    pub decision: SongLuaFunctionEaseDecision,
    pub stats: SongLuaReadEasesStats,
}

pub enum SongLuaFunctionEaseDecision {
    ColumnOffsets(Vec<SongLuaColumnOffsetWindow>),
    OverlayEases(Vec<SongLuaOverlayEase>),
    Target(SongLuaEaseTarget),
    Skip,
}

pub fn read_eases_with_function_capture<F>(
    table: Option<Table>,
    unit: SongLuaTimeUnit,
    easing_names: &HashMap<*const c_void, String>,
    mut capture_function: F,
) -> Result<SongLuaReadEasesResult, String>
where
    F: FnMut(
        SongLuaFunctionEaseInput,
        &mut SongLuaCompileInfo,
    ) -> Result<SongLuaFunctionEaseResult, String>,
{
    let Some(table) = table else {
        return Ok(SongLuaReadEasesResult {
            eases: Vec::new(),
            overlay_eases: Vec::new(),
            column_offsets: Vec::new(),
            info: SongLuaCompileInfo::default(),
            stats: SongLuaReadEasesStats::default(),
        });
    };
    let mut eases = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut column_offsets = Vec::new();
    let mut info = SongLuaCompileInfo::default();
    let mut stats = SongLuaReadEasesStats::default();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        stats.entry_count += 1;
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(limit) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(from) = read_f32(entry.raw_get::<Value>(3).map_err(|err| err.to_string())?) else {
            continue;
        };
        let Some(to) = read_f32(entry.raw_get::<Value>(4).map_err(|err| err.to_string())?) else {
            continue;
        };
        let field6 = entry.raw_get::<Value>(6).map_err(|err| err.to_string())?;
        let (span_mode, easing_value, player_value, sustain_value, opt1_value, opt2_value) =
            if let Some(span_mode) = read_span_mode(field6.clone()) {
                (
                    span_mode,
                    entry.raw_get::<Value>(7).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(8).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(9).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(10).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(11).map_err(|err| err.to_string())?,
                )
            } else {
                (
                    SongLuaSpanMode::Len,
                    field6,
                    entry.raw_get::<Value>(7).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(8).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(9).map_err(|err| err.to_string())?,
                    entry.raw_get::<Value>(10).map_err(|err| err.to_string())?,
                )
            };
        let easing = read_easing_name(easing_value, easing_names);
        let sustain = read_f32(sustain_value);
        let opt1 = read_f32(opt1_value);
        let opt2 = read_f32(opt2_value);
        let target_value = entry.raw_get::<Value>(5).map_err(|err| err.to_string())?;
        let (target, is_function_target) = match target_value {
            Value::String(text) => (
                SongLuaEaseTarget::Mod(text.to_str().map_err(|err| err.to_string())?.to_string()),
                false,
            ),
            Value::Function(function) => {
                stats.function_targets += 1;
                let result = capture_function(
                    SongLuaFunctionEaseInput {
                        function,
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
                    &mut info,
                )?;
                merge_read_eases_stats(&mut stats, result.stats);
                match result.decision {
                    SongLuaFunctionEaseDecision::ColumnOffsets(compiled) => {
                        column_offsets.extend(compiled);
                        continue;
                    }
                    SongLuaFunctionEaseDecision::OverlayEases(compiled) => {
                        stats.overlay_capture_outputs += compiled.len();
                        overlay_eases.extend(compiled);
                        continue;
                    }
                    SongLuaFunctionEaseDecision::Target(target) => (target, true),
                    SongLuaFunctionEaseDecision::Skip => continue,
                }
            }
            _ => continue,
        };
        if is_function_target && matches!(target, SongLuaEaseTarget::Function) {
            continue;
        }

        eases.push(SongLuaEaseWindow {
            unit,
            start,
            limit,
            span_mode,
            from,
            to,
            target,
            easing,
            player: read_player(player_value),
            sustain,
            opt1,
            opt2,
        });
    }
    Ok(SongLuaReadEasesResult {
        eases,
        overlay_eases,
        column_offsets,
        info,
        stats,
    })
}

fn merge_read_eases_stats(out: &mut SongLuaReadEasesStats, stats: SongLuaReadEasesStats) {
    out.overlay_capture_attempts += stats.overlay_capture_attempts;
    out.overlay_capture_outputs += stats.overlay_capture_outputs;
    out.probe_ms += stats.probe_ms;
    out.overlay_capture_ms += stats.overlay_capture_ms;
}
