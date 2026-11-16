//! Endpoint capture for the existing compiler's function easing entries.
use super::*;

impl SongRuntime {
    /// The existing static compiler pipeline: column endpoints first, then
    /// setter target classification, then indexed/touched overlay endpoints.
    pub fn read_song_eases_for_overlay_actors(
        &mut self,
        table: Option<Value>,
        unit: crate::SongLuaTimeUnit,
        names: &SongEasingNames,
        overlays: &[(usize, Value)],
    ) -> Result<crate::SongLuaReadEasesResult, String> {
        let started = std::time::Instant::now();
        let mut pins = Vec::new();
        let result = (|| {
            if table.is_some_and(|v| v != Value::Nil) {
                for (_, actor) in overlays {
                    pins.push(self.vm.pin(*actor).map_err(|e| e.to_string())?);
                }
            }
            self.read_song_eases_with_function_capture(
                table,
                unit,
                names,
                |runtime, input, info| runtime.capture_function_ease(overlays, input, info),
            )
        })();
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        let output = result?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.;
        if elapsed_ms >= 1000. {
            let stats = output.stats;
            log::info!(
                "Song lua read_eases timing: unit={unit:?} entries={} function_targets={} overlay_capture_attempts={} overlay_capture_outputs={} player_eases={} overlay_eases={} unsupported_function_eases={} probe_ms={:.3} overlay_capture_ms={:.3} elapsed_ms={elapsed_ms:.3}",
                stats.entry_count,
                stats.function_targets,
                stats.overlay_capture_attempts,
                stats.overlay_capture_outputs,
                output.eases.len(),
                output.overlay_eases.len(),
                output.info.unsupported_function_eases,
                stats.probe_ms,
                stats.overlay_capture_ms
            );
        }
        Ok(output)
    }
    fn capture_function_ease(
        &mut self,
        overlays: &[(usize, Value)],
        input: SongFunctionEaseInput,
        info: &mut crate::SongLuaCompileInfo,
    ) -> Result<crate::SongLuaFunctionEaseResult, String> {
        use crate::{
            SongLuaEaseTarget as Target, SongLuaFunctionEaseDecision as Decision,
            SongLuaFunctionEaseResult as Output,
        };
        let mut stats = crate::SongLuaReadEasesStats::default();
        match self.capture_song_note_column_function_eases(
            input.function,
            crate::SongLuaColumnOffsetBuildParams {
                unit: input.unit,
                start: input.start,
                limit: input.limit,
                span_mode: input.span_mode,
                easing: input.easing.clone(),
                sustain: input.sustain,
                opt1: input.opt1,
                opt2: input.opt2,
            },
            [input.from, input.to],
        ) {
            Ok(rows) if !rows.is_empty() => {
                return Ok(Output {
                    decision: Decision::ColumnOffsets(rows),
                    stats,
                });
            }
            Ok(_) => {}
            Err(error) => {
                log::debug!("Skipping song lua note-column position function ease capture: {error}")
            }
        }
        let started = std::time::Instant::now();
        let mut probe = self.probe_song_function_ease_target(input.function)?;
        stats.probe_ms += started.elapsed().as_secs_f64() * 1000.;
        let result = (|| {
            let target = probe.target.clone().unwrap_or(Target::Function);
            if !matches!(target, Target::Function) {
                return Ok(Output {
                    decision: Decision::Target(target),
                    stats,
                });
            }
            stats.overlay_capture_attempts += 1;
            let actors = self.song_function_ease_probe_actors(&probe)?;
            let started = std::time::Instant::now();
            let captured = self.capture_song_overlay_function_eases(
                overlays,
                input.function,
                crate::SongLuaOverlayEaseBuildParams {
                    unit: input.unit,
                    start: input.start,
                    limit: input.limit,
                    span_mode: input.span_mode,
                    easing: input.easing.clone(),
                    sustain: input.sustain,
                    opt1: input.opt1,
                    opt2: input.opt2,
                },
                [input.from, input.to],
                &actors,
            );
            let capture_ms = started.elapsed().as_secs_f64() * 1000.;
            stats.overlay_capture_ms += capture_ms;
            if capture_ms >= 1000. {
                log::info!(
                    "Slow song lua function ease capture: unit={:?} start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?} probe_actors={} overlays={} capture_ms={capture_ms:.3}",
                    input.unit,
                    input.start,
                    input.limit,
                    input.span_mode,
                    input.from,
                    input.to,
                    input.easing,
                    actors.len(),
                    overlays.len()
                );
            }
            let decision = match captured {
                Ok(rows) if !rows.is_empty() => Decision::OverlayEases(rows),
                _ => {
                    let detail = crate::record_unsupported_function_ease_capture(
                        info,
                        input.unit,
                        input.start,
                        input.limit,
                        input.span_mode,
                        input.from,
                        input.to,
                        &input.easing,
                        &probe.methods,
                    );
                    log::debug!("Unsupported song lua function ease capture: {detail}");
                    Decision::Skip
                }
            };
            Ok(Output { decision, stats })
        })();
        self.release_song_function_ease_probe(&mut probe)
            .map_err(|e| e.to_string())?;
        result
    }
    /// Probe the existing from/to endpoints. Known actor identities use indexed
    /// capture; an empty or unknown selection uses lazy touched capture. Shared
    /// script state belongs to the caller's outer compiler snapshot, as native.
    pub fn capture_song_overlay_function_eases(
        &mut self,
        overlays: &[(usize, Value)],
        function: Value,
        params: crate::SongLuaOverlayEaseBuildParams,
        endpoints: [f32; 2],
        probe_actors: &[Value],
    ) -> Result<Vec<crate::SongLuaOverlayEase>, String> {
        let mut pins = Vec::new();
        let result = (|| {
            for value in std::iter::once(function)
                .chain(overlays.iter().map(|r| r.1))
                .chain(probe_actors.iter().copied())
            {
                pins.push(self.vm.pin(value).map_err(|e| e.to_string())?);
            }
            let selected = overlays
                .iter()
                .filter(|(_, actor)| {
                    matches!(actor, Value::Table(_)) && probe_actors.contains(actor)
                })
                .copied()
                .collect::<Vec<_>>();
            let end = crate::song_lua_span_end(params.start, params.limit, params.span_mode)
                .max(params.start);
            let capture = |runtime: &mut Self, value, beat| {
                if selected.is_empty() {
                    runtime.capture_song_touched_actor_function(
                        overlays,
                        function,
                        Some(value),
                        Some(beat),
                    )
                } else {
                    runtime.capture_song_actor_function(
                        &selected,
                        function,
                        Some(value),
                        Some(beat),
                    )
                }
            };
            let from = capture(self, endpoints[0], params.start)?;
            let to = capture(self, endpoints[1], end)?;
            Ok(crate::overlay_eases_from_captures(
                overlays.len(),
                &from,
                &to,
                params,
            ))
        })();
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}
