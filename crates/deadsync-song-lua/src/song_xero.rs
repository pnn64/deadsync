//! Existing Xero node/ease parsing and overlay endpoint publication.
use super::song_compiler_roots::Roots;
use super::song_mod_tables::schedule_player;
use super::*;
use crate::{
    RuntimeModEaseEntry, SongLuaCompileInfo, SongLuaEaseWindow, SongLuaOverlayEase,
    SongLuaSpanMode, SongLuaTimeUnit,
};
use std::collections::{HashMap, HashSet};

#[derive(Default, Debug)]
pub(super) struct SongXeroEases {
    pub eases: Vec<SongLuaEaseWindow>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub info: SongLuaCompileInfo,
}
struct Entry {
    data: RuntimeModEaseEntry,
    function: Option<usize>,
}
impl SongRuntime {
    pub(super) fn compile_song_xero_sources(
        &mut self,
        roots: &mut Roots,
        ease_sources: Vec<usize>,
        node_sources: Vec<usize>,
        registry: TableId,
        overlays: &[(usize, Value)],
    ) -> Result<SongXeroEases, String> {
        let nodes = self.xero_node_functions(roots, node_sources)?;
        let mut entries = Vec::new();
        for index in ease_sources {
            let source = roots.value(&self.vm, index)?;
            let rows = self.schedule_rows(Some(source), |runtime, entry| {
                runtime
                    .xero_entries(entry, registry, &nodes, roots)
                    .map(Some)
            })?;
            entries.extend(rows.into_iter().flatten());
            roots.release(&mut self.vm, index)?;
        }
        for index in nodes.into_values() {
            roots.release(&mut self.vm, index)?;
        }
        let mut current: [HashMap<String, f32>; LUA_PLAYERS] =
            std::array::from_fn(|_| HashMap::new());
        let mut captured = HashSet::new();
        let mut output = SongXeroEases::default();
        for row in entries {
            let entry = &row.data;
            let key = crate::runtime_mod_key(&entry.target);
            for player in crate::runtime_mod_entry_players(entry.player) {
                let from = crate::runtime_mod_start_value(&mut current[player], &key, entry);
                let to = crate::runtime_mod_end_value(from, entry);
                current[player].insert(key.clone(), to);
                if let Some(index) = row.function {
                    let function = roots.value(&self.vm, index)?;
                    let identity = self.vm.object_pointer(function).unwrap();
                    let capture_key = (
                        identity,
                        entry.unit as u8,
                        entry.start.to_bits(),
                        entry.limit.to_bits(),
                        entry.easing.clone(),
                        key.clone(),
                        from.to_bits(),
                        to.to_bits(),
                        entry.opt1.map(f32::to_bits),
                        entry.opt2.map(f32::to_bits),
                    );
                    if captured.insert(capture_key) {
                        output.overlay_eases.extend(self.compile_song_xero_node(
                            overlays,
                            entry,
                            function,
                            [from, to],
                            &mut output.info,
                        )?);
                    }
                } else if let Some(target) =
                    crate::runtime_player_option_ease_target(&key, &entry.target)
                {
                    output.eases.push(SongLuaEaseWindow {
                        approach_speed: None,
                        unit: entry.unit,
                        start: entry.start,
                        limit: entry.limit,
                        span_mode: SongLuaSpanMode::Len,
                        from,
                        to,
                        target,
                        easing: Some(entry.easing.clone()),
                        player: Some((player + 1) as u8),
                        sustain: None,
                        opt1: entry.opt1,
                        opt2: entry.opt2,
                    });
                }
            }
            if let Some(index) = row.function {
                roots.release(&mut self.vm, index)?;
            }
        }
        crate::extend_runtime_mod_sustains(&mut output.eases);
        Ok(output)
    }
    fn xero_node_functions(
        &mut self,
        roots: &mut Roots,
        sources: Vec<usize>,
    ) -> Result<HashMap<String, usize>, String> {
        let mut nodes = HashMap::new();
        for index in sources {
            let source = roots.value(&self.vm, index)?;
            self.schedule_rows(Some(source), |runtime, node| {
                let Value::Table(inputs) = runtime.schedule_slot(node, 1)? else {
                    return Ok(None::<()>);
                };
                let function = runtime.schedule_slot(node, 3)?;
                if !matches!(function, Value::Function(_) | Value::Native(_)) {
                    return Ok(None);
                }
                let function_pin = roots.keep(&mut runtime.vm, function)?;
                // The native input iterator retains the original input table.
                let pin = roots.keep(&mut runtime.vm, Value::Table(inputs))?;
                for i in 1usize.. {
                    let value = runtime.schedule_slot(inputs, i)?;
                    if value == Value::Nil {
                        break;
                    }
                    let Some(name) = runtime.schedule_text(value) else {
                        continue;
                    };
                    let key = crate::runtime_mod_key(&name);
                    if let std::collections::hash_map::Entry::Vacant(entry) = nodes.entry(key) {
                        entry.insert(roots.keep(&mut runtime.vm, function)?);
                    }
                }
                roots.release(&mut runtime.vm, pin)?;
                roots.release(&mut runtime.vm, function_pin)?;
                Ok(None)
            })?;
            roots.release(&mut self.vm, index)?;
        }
        Ok(nodes)
    }
    fn xero_entries(
        &mut self,
        entry: TableId,
        registry: TableId,
        nodes: &HashMap<String, usize>,
        roots: &mut Roots,
    ) -> Result<Vec<Entry>, String> {
        let Some(start) = self.schedule_float(self.schedule_slot(entry, 1)?) else {
            return Ok(Vec::new());
        };
        let Some(limit) = self.schedule_float(self.schedule_slot(entry, 2)?) else {
            return Ok(Vec::new());
        };
        let Some(easing) = self.song_easing_name(self.schedule_slot(entry, 3)?, registry)? else {
            return Ok(Vec::new());
        };
        if !start.is_finite() || !limit.is_finite() || limit < 0. {
            return Ok(Vec::new());
        }
        let unit = if self.schedule_field(entry, "time")?.is_truthy() {
            SongLuaTimeUnit::Second
        } else {
            SongLuaTimeUnit::Beat
        };
        let player = schedule_player(self.schedule_field(entry, "plr")?);
        let add = self.schedule_field(entry, "relative")?.is_truthy();
        let mut output = Vec::new();
        for i in (4usize..).step_by(2) {
            let to = self.schedule_slot(entry, i)?;
            let target = self.schedule_slot(entry, i + 1)?;
            if to == Value::Nil && target == Value::Nil {
                break;
            }
            let Some(to) = self.schedule_float(to) else {
                continue;
            };
            let Some(target) = self.schedule_text(target) else {
                continue;
            };
            let key = crate::runtime_mod_key(&target);
            let function = if crate::runtime_player_option_ease_target(&key, &target).is_some() {
                None
            } else if let Some(index) = nodes.get(&key) {
                let function = roots.value(&self.vm, *index)?;
                Some(roots.keep(&mut self.vm, function)?)
            } else {
                continue;
            };
            output.push(Entry {
                data: RuntimeModEaseEntry {
                    unit,
                    start,
                    limit,
                    easing: easing.clone(),
                    to,
                    target,
                    start_val: None,
                    opt1: None,
                    opt2: None,
                    player,
                    add,
                },
                function,
            });
        }
        Ok(output)
    }
    fn compile_song_xero_node(
        &mut self,
        overlays: &[(usize, Value)],
        entry: &RuntimeModEaseEntry,
        function: Value,
        endpoints: [f32; 2],
        info: &mut SongLuaCompileInfo,
    ) -> Result<Vec<SongLuaOverlayEase>, String> {
        let mut probe = self.probe_song_function_ease_target(function)?;
        let result = (|| {
            let actors = self.song_function_ease_probe_actors(&probe)?;
            if !overlays.iter().any(|r| actors.contains(&r.1))
                || !matches!(
                    probe.target,
                    None | Some(crate::SongLuaEaseTarget::Function)
                )
            {
                return Ok(Vec::new());
            }
            let captured = self.capture_song_overlay_function_eases(
                overlays,
                function,
                crate::SongLuaOverlayEaseBuildParams {
                    unit: entry.unit,
                    start: entry.start,
                    limit: entry.limit,
                    span_mode: SongLuaSpanMode::Len,
                    easing: Some(entry.easing.clone()),
                    sustain: None,
                    opt1: entry.opt1,
                    opt2: entry.opt2,
                },
                endpoints,
                &actors,
            );
            match captured {
                Ok(rows) if !rows.is_empty() => Ok(rows),
                result => {
                    let detail = crate::record_unsupported_xero_overlay_function_ease(
                        info,
                        entry,
                        endpoints[0],
                        endpoints[1],
                        &probe.methods,
                    );
                    log::debug!("Unsupported xero overlay function ease capture: {detail}");
                    if let Err(error) = result {
                        log::debug!(
                            "Unsupported xero overlay function ease capture for '{}': {error}",
                            entry.target
                        );
                    }
                    Ok(Vec::new())
                }
            }
        })();
        self.release_song_function_ease_probe(&mut probe)
            .map_err(|e| e.to_string())?;
        result
    }
}
