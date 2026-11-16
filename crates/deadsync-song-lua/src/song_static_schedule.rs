//! Assemble the existing compiler's prefix/global/Xero/action schedule in order.
use super::song_compiler_roots::Roots;
use super::*;
use crate::{
    SongLuaColumnOffsetWindow, SongLuaCompileInfo, SongLuaEaseWindow, SongLuaMessageEvent,
    SongLuaModWindow, SongLuaOverlayEase, SongLuaSoundEvent, SongLuaTimeUnit,
};

/// Seed messages, sounds and diagnostics with the compiler's startup output.
/// On a failed pass the enclosing compiler discards this partial owned output.
#[derive(Default, Debug, Clone)]
pub struct SongStaticSchedule {
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
    pub sound_events: Vec<SongLuaSoundEvent>,
    pub info: SongLuaCompileInfo,
    pub counter: usize,
}
pub struct SongStaticInputs<'a> {
    pub root: Value,
    /// Original post-Init tables: actions, perframes, mods, mods_ease.
    pub initialized_globals: [Option<Value>; 4],
    pub names: &'a SongEasingNames,
    pub context: &'a SongLuaCompileContext,
    pub static_overlay: Option<usize>,
}
/// Retained lexical inputs plus the two tables passed to sampled perframes.
/// Release when the enclosing compiler no longer needs these Lua identities.
pub struct SongStaticTables {
    pub(super) roots: Option<Roots>,
    pub(super) perframes: [Option<usize>; 2],
    pub prefix_is_runtime: bool,
    pub global_perframes_are_runtime: bool,
    pub global_mods_are_runtime: bool,
    pub global_mod_eases_are_runtime: bool,
    pub global_actions_are_runtime: bool,
}
impl SongRuntime {
    pub fn compile_song_static_schedule(
        &mut self,
        inputs: SongStaticInputs<'_>,
        overlays: &mut [SongMessageTarget<'_>],
        tracked: &mut [SongMessageTarget<'_>],
        output: &mut SongStaticSchedule,
        mut stage: impl FnMut(&'static str),
    ) -> Result<SongStaticTables, String> {
        let mut roots = Roots::default();
        let result = (|| {
            roots.keep(&mut self.vm, inputs.root)?;
            for value in inputs.initialized_globals.into_iter().flatten() {
                if !matches!(value, Value::Table(_)) {
                    return Err("initialized schedule table expected".into());
                }
                roots.keep(&mut self.vm, value)?;
            }
            for row in overlays.iter().chain(tracked.iter()) {
                roots.keep(&mut self.vm, row.actor)?;
            }
            let globals = self.compiler_globals()?;
            let globals = roots.keep(&mut self.vm, globals)?;
            let prefix_is_runtime = self
                .song_update_tree_reads_global(inputs.root, "prefix_globals")
                .map_err(|e| format!("failed to inspect prefix update callbacks: {e}"))?;
            let prefix = self
                .static_field(&mut roots, globals, "prefix_globals")
                .map_err(|e| format!("failed to read prefix_globals: {e}"))?;
            let prefix_perframes = if roots.value(&self.vm, prefix)? == Value::Nil {
                None
            } else {
                self.static_field(&mut roots, prefix, "perframes").ok()
            };
            roots.release(&mut self.vm, prefix)?;
            let global_perframes = self
                .static_field(&mut roots, globals, "mod_perframes")
                .map_err(|e| format!("failed to read mod_perframes: {e}"))?;
            let mut perframe_tables = self
                .read_song_update_function_tables(inputs.root, &["mod_perframes", "perframes"])
                .map_err(|e| format!("failed to read update callback tables: {e}"))?;
            let perframe_tables = roots.take_batch(self, &mut perframe_tables)?;
            let reads = self
                .song_update_tree_reads_global(inputs.root, "mod_perframes")
                .map_err(|e| format!("failed to inspect global update callbacks: {e}"))?;
            let global_perframes_are_runtime = self.static_runtime_table(
                &roots,
                global_perframes,
                &perframe_tables,
                reads,
                inputs.initialized_globals[1].is_some(),
            )?;
            stage("read_globals");
            let selected = overlays
                .iter()
                .enumerate()
                .map(|(i, r)| (i, r.actor))
                .collect::<Vec<_>>();
            if !prefix_is_runtime {
                let prefix = self.static_field(&mut roots, globals, "prefix_globals")?;
                if roots.value(&self.vm, prefix)? != Value::Nil {
                    let table = self.static_field(&mut roots, prefix, "mods")?;
                    output.beat_mods.extend(self.read_song_mod_windows(
                        self.static_option(&roots, table)?,
                        SongLuaTimeUnit::Beat,
                    )?);
                    roots.release(&mut self.vm, table)?;
                    stage("prefix_mods");
                    let table = self.static_field(&mut roots, prefix, "ease")?;
                    let rows = self.read_song_eases_for_overlay_actors(
                        self.static_option(&roots, table)?,
                        SongLuaTimeUnit::Beat,
                        inputs.names,
                        &selected,
                    )?;
                    roots.release(&mut self.vm, table)?;
                    append_eases(output, rows);
                    stage("prefix_eases");
                    let table = self.static_field(&mut roots, prefix, "actions")?;
                    self.static_actions(&roots, table, overlays, tracked, output)?;
                    roots.release(&mut self.vm, table)?;
                    stage("prefix_actions");
                }
                roots.release(&mut self.vm, prefix)?;
            }
            let global_mods = self.static_field(&mut roots, globals, "mods")?;
            let global_mods_are_runtime = self.static_ownership(
                &mut roots,
                inputs.root,
                global_mods,
                &["mods"],
                "mods",
                inputs.initialized_globals[2].is_some(),
            )?;
            if !global_mods_are_runtime {
                let table = self.static_option(&roots, global_mods)?;
                output
                    .beat_mods
                    .extend(self.read_song_mod_windows(table, SongLuaTimeUnit::Beat)?);
                let (eases, overlay_eases) = self.read_song_runtime_mod_eases(
                    table,
                    inputs.names,
                    inputs.static_overlay,
                    inputs.context,
                )?;
                output.eases.extend(eases);
                output.overlay_eases.extend(overlay_eases);
            }
            let table = self.static_field(&mut roots, globals, "mod_time")?;
            output.time_mods.extend(self.read_song_mod_windows(
                self.static_option(&roots, table)?,
                SongLuaTimeUnit::Second,
            )?);
            roots.release(&mut self.vm, table)?;
            let mut tables = self
                .read_song_update_function_tables(inputs.root, &["mod_time"])
                .map_err(|e| e.to_string())?;
            for table in roots.take_batch(self, &mut tables)? {
                output.time_mods.extend(self.read_song_mod_windows(
                    self.static_option(&roots, table)?,
                    SongLuaTimeUnit::Second,
                )?);
                roots.release(&mut self.vm, table)?;
            }
            stage("global_mods");
            let global_mod_eases = self.static_field(&mut roots, globals, "mods_ease")?;
            let global_mod_eases_are_runtime = self.static_ownership(
                &mut roots,
                inputs.root,
                global_mod_eases,
                &["mods_ease"],
                "mods_ease",
                inputs.initialized_globals[3].is_some(),
            )?;
            if !global_mod_eases_are_runtime {
                let rows = self.read_song_eases_for_overlay_actors(
                    self.static_option(&roots, global_mod_eases)?,
                    SongLuaTimeUnit::Beat,
                    inputs.names,
                    &selected,
                )?;
                roots.release(&mut self.vm, global_mod_eases)?;
                append_eases(output, rows);
            }
            stage("global_eases");
            let mut nodes = self
                .read_song_update_function_nested_tables(inputs.root, &["nodes"])
                .map_err(|e| e.to_string())?;
            let mut node_sources = roots.take_batch(self, &mut nodes)?;
            let mut nodes = self
                .read_song_global_function_nested_tables("xero", &["definemod", "node"], &["nodes"])
                .map_err(|e| e.to_string())?;
            node_sources.extend(roots.take_batch(self, &mut nodes)?);
            let mut eases = self
                .read_song_update_function_nested_tables(inputs.root, &["eases"])
                .map_err(|e| e.to_string())?;
            let ease_sources = roots.take_batch(self, &mut eases)?;
            let registry = self.song_easing_registry(inputs.names)?;
            let xero = self.compile_song_xero_sources(
                &mut roots,
                ease_sources,
                node_sources,
                registry,
                &selected,
            )?;
            output.eases.extend(xero.eases);
            output.overlay_eases.extend(xero.overlay_eases);
            crate::merge_compile_info(&mut output.info, xero.info);
            stage("xero_eases");
            let global_actions = self.static_field(&mut roots, globals, "mod_actions")?;
            let global_actions_are_runtime = self.static_ownership(
                &mut roots,
                inputs.root,
                global_actions,
                &["mod_actions", "actions"],
                "mod_actions",
                inputs.initialized_globals[0].is_some(),
            )?;
            if !global_actions_are_runtime {
                self.static_actions(&roots, global_actions, overlays, tracked, output)?;
                roots.release(&mut self.vm, global_actions)?;
            }
            stage("global_actions");
            Ok(SongStaticTables {
                roots: None,
                perframes: [
                    if prefix_is_runtime {
                        None
                    } else {
                        prefix_perframes
                    },
                    if global_perframes_are_runtime {
                        None
                    } else {
                        Some(global_perframes)
                    },
                ],
                prefix_is_runtime,
                global_perframes_are_runtime,
                global_mods_are_runtime,
                global_mod_eases_are_runtime,
                global_actions_are_runtime,
            })
        })();
        match result {
            Ok(mut tables) => {
                tables.roots = Some(roots);
                Ok(tables)
            }
            Err(error) => {
                roots.clear(&mut self.vm)?;
                Err(error)
            }
        }
    }
    fn static_field(
        &mut self,
        roots: &mut Roots,
        source: usize,
        name: &str,
    ) -> Result<usize, String> {
        let value = self.compiler_table_field(roots.value(&self.vm, source)?, name)?;
        roots.keep(&mut self.vm, value)
    }
    fn static_option(&self, roots: &Roots, index: usize) -> Result<Option<Value>, String> {
        let value = roots.value(&self.vm, index)?;
        Ok((value != Value::Nil).then_some(value))
    }
    fn static_runtime_table(
        &self,
        roots: &Roots,
        global: usize,
        tables: &[usize],
        reads: bool,
        initialized: bool,
    ) -> Result<bool, String> {
        let value = roots.value(&self.vm, global)?;
        if value == Value::Nil {
            return Ok(false);
        }
        for &i in tables {
            if roots.value(&self.vm, i)? == value {
                return Ok(true);
            }
        }
        Ok(reads && initialized)
    }
    fn static_ownership(
        &mut self,
        roots: &mut Roots,
        root: Value,
        table: usize,
        names: &[&str],
        global: &str,
        initialized: bool,
    ) -> Result<bool, String> {
        let mut tables = self
            .read_song_update_function_tables(root, names)
            .map_err(|e| e.to_string())?;
        let tables = roots.take_batch(self, &mut tables)?;
        let reads = self
            .song_update_tree_reads_global(root, global)
            .map_err(|e| e.to_string())?;
        self.static_runtime_table(roots, table, &tables, reads, initialized)
    }
    fn static_actions(
        &mut self,
        roots: &Roots,
        table: usize,
        overlays: &mut [SongMessageTarget<'_>],
        tracked: &mut [SongMessageTarget<'_>],
        output: &mut SongStaticSchedule,
    ) -> Result<(), String> {
        self.read_song_overlay_actions(
            self.static_option(roots, table)?,
            overlays,
            tracked,
            &mut SongActionOutput {
                counter: &mut output.counter,
                messages: &mut output.messages,
                sound_events: &mut output.sound_events,
                info: &mut output.info,
            },
        )
    }
    pub fn song_static_perframe_tables(
        &self,
        tables: &SongStaticTables,
    ) -> Result<[Option<Value>; 2], String> {
        let roots = tables
            .roots
            .as_ref()
            .ok_or("released static schedule tables")?;
        roots.validate(&self.vm)?;
        let get = |slot: Option<usize>| {
            slot.map(|i| self.static_option(roots, i))
                .transpose()
                .map(Option::flatten)
        };
        Ok([get(tables.perframes[0])?, get(tables.perframes[1])?])
    }
    pub fn release_song_static_tables(
        &mut self,
        tables: &mut SongStaticTables,
    ) -> Result<bool, String> {
        let Some(roots) = tables.roots.as_mut() else {
            return Ok(false);
        };
        roots.validate(&self.vm)?;
        roots.clear(&mut self.vm)?;
        tables.roots = None;
        Ok(true)
    }
}
fn append_eases(output: &mut SongStaticSchedule, rows: crate::SongLuaReadEasesResult) {
    output.eases.extend(rows.eases);
    output.overlay_eases.extend(rows.overlay_eases);
    output.column_offsets.extend(rows.column_offsets);
    crate::merge_compile_info(&mut output.info, rows.info);
}
