use log::{debug, info};
use mlua::{Function, Lua, Table, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Instant;

use crate::{
    SongLuaColumnOffsetWindow, SongLuaCompileInfo, SongLuaEaseTarget, SongLuaEaseWindow,
    SongLuaOverlayCompileActor, SongLuaOverlayEase, SongLuaSpanMode, SongLuaTimeUnit,
    capture_overlay_compile_actor_function_eases, compile_note_column_pos_function_ease,
    probe_function_ease_target, read_easing_name, read_f32, read_player, read_span_mode,
    record_unsupported_function_ease_capture,
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

pub fn read_eases_for_overlay_actors<Kind>(
    lua: &Lua,
    table: Option<Table>,
    unit: SongLuaTimeUnit,
    easing_names: &HashMap<*const c_void, String>,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        Vec<SongLuaColumnOffsetWindow>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let Some(table) = table else {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SongLuaCompileInfo::default(),
        ));
    };
    let trace_started = Instant::now();
    let result =
        read_eases_with_function_capture(Some(table), unit, easing_names, |input, info| {
            capture_function_ease(lua, overlays, input, info)
        })?;
    let elapsed_ms = trace_started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms >= 1000.0 {
        let stats = result.stats;
        info!(
            "Song lua read_eases timing: unit={unit:?} entries={} function_targets={} overlay_capture_attempts={} overlay_capture_outputs={} player_eases={} overlay_eases={} unsupported_function_eases={} probe_ms={probe_ms:.3} overlay_capture_ms={overlay_capture_ms:.3} elapsed_ms={elapsed_ms:.3}",
            stats.entry_count,
            stats.function_targets,
            stats.overlay_capture_attempts,
            stats.overlay_capture_outputs,
            result.eases.len(),
            result.overlay_eases.len(),
            result.info.unsupported_function_eases,
            probe_ms = stats.probe_ms,
            overlay_capture_ms = stats.overlay_capture_ms,
        );
    }
    Ok((
        result.eases,
        result.overlay_eases,
        result.column_offsets,
        result.info,
    ))
}

fn capture_function_ease<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    input: SongLuaFunctionEaseInput,
    info: &mut SongLuaCompileInfo,
) -> Result<SongLuaFunctionEaseResult, String> {
    let mut stats = SongLuaReadEasesStats::default();
    match compile_note_column_pos_function_ease(
        lua,
        &input.function,
        input.unit,
        input.start,
        input.limit,
        input.span_mode,
        input.from,
        input.to,
        input.easing.clone(),
        input.sustain,
        input.opt1,
        input.opt2,
    ) {
        Ok(compiled) if !compiled.is_empty() => {
            return Ok(SongLuaFunctionEaseResult {
                decision: SongLuaFunctionEaseDecision::ColumnOffsets(compiled),
                stats,
            });
        }
        Ok(_) => {}
        Err(err) => {
            debug!("Skipping song lua note-column position function ease capture: {err}");
        }
    }
    let probe_started = Instant::now();
    let (probed_target, probe_methods, probe_actor_ptrs) =
        probe_function_ease_target(lua, &input.function).map_err(|err| err.to_string())?;
    stats.probe_ms += probe_started.elapsed().as_secs_f64() * 1000.0;
    let target = probed_target.unwrap_or(SongLuaEaseTarget::Function);
    if matches!(target, SongLuaEaseTarget::Function) {
        stats.overlay_capture_attempts += 1;
        let capture_started = Instant::now();
        let captured = capture_overlay_compile_actor_function_eases(
            lua,
            overlays,
            &input.function,
            input.unit,
            input.start,
            input.limit,
            input.span_mode,
            input.from,
            input.to,
            input.easing.clone(),
            input.sustain,
            input.opt1,
            input.opt2,
            &probe_actor_ptrs,
        );
        let capture_ms = capture_started.elapsed().as_secs_f64() * 1000.0;
        stats.overlay_capture_ms += capture_ms;
        if capture_ms >= 1000.0 {
            info!(
                "Slow song lua function ease capture: unit={:?} start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?} probe_actors={} overlays={} capture_ms={capture_ms:.3}",
                input.unit,
                input.start,
                input.limit,
                input.span_mode,
                input.from,
                input.to,
                input.easing,
                probe_actor_ptrs.len(),
                overlays.len(),
            );
        }
        return match captured {
            Ok(compiled) if !compiled.is_empty() => Ok(SongLuaFunctionEaseResult {
                decision: SongLuaFunctionEaseDecision::OverlayEases(compiled),
                stats,
            }),
            _ => {
                let detail = record_unsupported_function_ease_capture(
                    info,
                    input.unit,
                    input.start,
                    input.limit,
                    input.span_mode,
                    input.from,
                    input.to,
                    &input.easing,
                    &probe_methods,
                );
                debug!("Unsupported song lua function ease capture: {detail}");
                Ok(SongLuaFunctionEaseResult {
                    decision: SongLuaFunctionEaseDecision::Skip,
                    stats,
                })
            }
        };
    }
    Ok(SongLuaFunctionEaseResult {
        decision: SongLuaFunctionEaseDecision::Target(target),
        stats,
    })
}

fn merge_read_eases_stats(out: &mut SongLuaReadEasesStats, stats: SongLuaReadEasesStats) {
    out.overlay_capture_attempts += stats.overlay_capture_attempts;
    out.overlay_capture_outputs += stats.overlay_capture_outputs;
    out.probe_ms += stats.probe_ms;
    out.overlay_capture_ms += stats.overlay_capture_ms;
}
