//! Resolved compiler entries retain the author's distinct Lua source spelling.
use super::{ActorScene, SongRuntime};
use crate::SongLuaCompileContext;
use deadlib_lua::{Error, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SongLayerPlan {
    pub(super) entries: Vec<Entry>,
    pub(super) live: bool,
    pub(super) screen_loading: bool,
    primary_index: usize,
}
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub path: PathBuf,
    pub source: PathBuf,
    pub background: bool,
    pub name: Option<String>,
    pub beat: Option<f32>,
    pub color: Option<[f32; 4]>,
}
impl SongLayerPlan {
    /// Validate all entries before constructing a runtime or executing Lua.
    pub fn new(
        paths: &[&Path],
        primary_index: usize,
        context: &SongLuaCompileContext,
    ) -> Result<Self, String> {
        // The production compiler returns immediately for an empty preparation.
        if paths.is_empty() && !context.live {
            return Ok(Self {
                entries: Vec::new(),
                live: false,
                screen_loading: context.screen_loading,
                primary_index,
            });
        }
        if context.background_layer_count > paths.len() {
            return Err("song lua background count exceeds its session entries".into());
        }
        if !context.foreground_names.is_empty()
            && context.foreground_names.len() != paths.len() - context.background_layer_count
        {
            return Err("song lua foreground names do not match its session entries".into());
        }
        if !context.layer_start_beats.is_empty()
            && (context.layer_start_beats.len() != paths.len()
                || context
                    .layer_start_beats
                    .iter()
                    .any(|beat| !beat.is_finite()))
        {
            return Err("song lua layer beats do not match its session entries".into());
        }
        if primary_index >= paths.len() && !(paths.is_empty() && primary_index == 0) {
            return Err(format!(
                "song lua primary index {primary_index} is outside {} entries",
                paths.len()
            ));
        }
        let entries = paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let path = crate::entry_file_path(path)
                    .ok_or_else(|| format!("song lua entry '{}' does not exist", path.display()))?;
                let name = index
                    .checked_sub(context.background_layer_count)
                    .and_then(|i| context.foreground_names.get(i))
                    .cloned();
                let source = name
                    .as_ref()
                    .map(|name| {
                        let suffix = if context.song_dir.join(name).is_dir() {
                            "/default.lua"
                        } else {
                            ""
                        };
                        PathBuf::from(format!(
                            "{}/{name}{suffix}",
                            crate::file_path_string(&context.song_dir).trim_end_matches('/')
                        ))
                    })
                    .unwrap_or_else(|| path.clone());
                Ok(Entry {
                    path,
                    source,
                    name,
                    background: index < context.background_layer_count,
                    beat: context
                        .live
                        .then(|| context.layer_start_beats.get(index).copied())
                        .flatten(),
                    color: context
                        .background_upper_left_colors
                        .get(index)
                        .copied()
                        .flatten(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            entries,
            live: context.live,
            screen_loading: context.screen_loading,
            primary_index,
        })
    }
    pub fn primary_index(&self) -> usize {
        self.primary_index
    }
    pub fn paths(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.entries.iter().map(|entry| entry.path.as_path())
    }
}
impl SongRuntime {
    /// Install the production song compiler's public compatibility globals once.
    /// Call before loading the gameplay theme/authored files. Reloads preserve
    /// author replacements; internal loaders already captured their VM primitives.
    pub fn begin_song_compilation(&mut self) -> Result<bool, Error> {
        if self.compiler_globals_installed {
            return Ok(false);
        }
        super::compiler_debug::install(&mut self.vm)?;
        if self.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            self.actor_loader.install_live(&mut self.vm)?;
        }
        self.compiler_globals_installed = true;
        self.actors.begin_song_capture();
        Ok(true)
    }

    /// Execute and initialize the compiler's authored layers in their shared VM.
    /// The supplied screen owns all loaded actors, including partial failed loads.
    /// The returned loader table is traced by that screen; pin it separately if
    /// it must outlive the screen. Pre-Init states are available through `song_initial_states`; Begin/On and
    /// Update(0) are later phases.
    /// A foreign or retired screen returns None without running authored code.
    pub fn initialize_song_layers(
        &mut self,
        screen: ActorScene,
        plan: &SongLayerPlan,
    ) -> Result<Option<Value>, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(None);
        }
        self.begin_song_compilation()?;
        self.begin_actor_preparation()?;
        self.actors
            .initialize_song_layers(&mut self.vm, screen, plan)
    }
}

/// Values captured before authored Init, keyed by the actual actor identity.
/// The owning active screen traces all actors, including children detached by
/// Init. The table handles are borrowed: pin them if retained after retirement
/// or release. The numeric/render states are independent owned copies.
#[derive(Clone, Debug, Default)]
pub struct SongInitialStates {
    pub actors: std::collections::HashMap<deadlib_lua::TableId, crate::SongLuaOverlayState>,
    pub screen_layers: std::collections::HashMap<deadlib_lua::TableId, crate::SongLuaOverlayState>,
}
impl SongRuntime {
    /// Latest authored initialization's pre-Init captures for this active screen.
    pub fn song_initial_states(&self, screen: ActorScene) -> Option<SongInitialStates> {
        self.actors.song_initial_states(screen)
    }
}
