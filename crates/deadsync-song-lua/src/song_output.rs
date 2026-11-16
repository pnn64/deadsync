//! Compiler-owned tracked actor identities and their initial captured state.
use super::{ActorScene, SongRuntime};
use crate::{SongLuaCapturedActor, SongLuaTrackedActorTarget};
use deadlib_lua::{Error, Root, Value};

/// A tracked compiler row. Its Lua identity is retained by the owning batch.
/// Resolve it through song_tracked_actor_value; copied rows reject a released
/// batch or a different VM instead of exposing an unrooted table ID.
#[derive(Clone)]
pub struct SongTrackedActor {
    pub actor: SongLuaCapturedActor,
    pub target: SongLuaTrackedActorTarget,
    pub(super) owner: Root,
    pub(super) slot: usize,
}

/// Pinned identities plus owned compiler output. Release the pin with
/// release_song_tracked_actors when the compiler/session no longer needs Lua.
/// The owned actor state remains readable after release and after VM teardown.
pub struct SongTrackedActors {
    pub actors: Vec<SongTrackedActor>,
    pub(super) owner: Option<Root>,
}
impl SongRuntime {
    /// Read the same player/HUD/foreground/screen-layer targets as production,
    /// including existing lazy child construction and manual draw snapshots.
    pub fn read_song_tracked_actors(
        &mut self,
        screen: ActorScene,
    ) -> Result<Option<SongTrackedActors>, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(None);
        }
        let (values, rows) = self.actors.read_song_tracked_actors(&mut self.vm, screen)?;
        let owner = self.vm.pin(values)?;
        let actors = rows
            .into_iter()
            .enumerate()
            .map(|(slot, row)| SongTrackedActor {
                actor: row.actor,
                target: row.target,
                owner,
                slot: slot + 1,
            })
            .collect();
        Ok(Some(SongTrackedActors {
            actors,
            owner: Some(owner),
        }))
    }

    pub fn song_tracked_actor_value(&self, actor: &SongTrackedActor) -> Result<Value, Error> {
        let Value::Table(owner) = self.vm.rooted(actor.owner)? else {
            unreachable!()
        };
        self.vm
            .raw_table_get(owner, Value::Number(actor.slot as f64))
    }

    /// A foreign VM leaves the batch pinned. Repeated release returns false.
    pub fn release_song_tracked_actors(
        &mut self,
        actors: &mut SongTrackedActors,
    ) -> Result<bool, Error> {
        let Some(owner) = actors.owner else {
            return Ok(false);
        };
        self.vm.unpin(owner)?;
        actors.owner = None;
        Ok(true)
    }
}

/// Inputs already read by the compiler before probing message commands. Keep
/// their original class/state even when a command or getter changes the table.
/// The AFT names come from the compiler's pre-extraction tree pass.
pub struct SongOverlayKindInputs<'a> {
    pub actor_type: &'a str,
    pub initial_state: crate::SongLuaOverlayState,
    pub context: &'a crate::SongLuaCompileContext,
    pub aft_capture_names: &'a std::collections::HashSet<String>,
}
impl SongRuntime {
    /// Read the existing renderer kind, skipping only native unsupported/missing
    /// resources. Model/noteskin resources use the application's existing loader.
    /// This does not probe message commands or decide plain-actor tree elision.
    /// Generic proxy targets carry runtime-local identities for the compiler's
    /// final tree remap; retain their actor roots until that remap completes.
    pub fn read_song_overlay_kind<S, V>(
        &mut self,
        actor: Value,
        inputs: SongOverlayKindInputs<'_>,
        read_slots: impl Fn(
            &std::path::Path,
            &std::path::Path,
            &std::path::Path,
        ) -> Result<std::sync::Arc<[S]>, String>,
        read_layer: impl Fn(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
    ) -> Result<
        Option<crate::SongLuaOverlayKind<S, V, deadlib_present::actors::TextAttribute>>,
        String,
    > {
        // Resource loaders can execute another Lua consumer. Retain this actor
        // until all owned data has been copied, including every error path.
        let root = self.vm.pin(actor).map_err(|error| error.to_string())?;
        let result = (|| match self
            .actors
            .read_song_overlay_kind(&mut self.vm, actor, &inputs)
            .map_err(|error| error.to_string())?
        {
            super::actors::song_kinds::Output::Ready(kind) => {
                Ok(kind.map(|kind| super::actors::song_kinds::typed(*kind)))
            }
            super::actors::song_kinds::Output::Model(Some(model)) => {
                let paths = &model.model_paths;
                let slots = read_slots(&paths[0], &paths[1], &paths[2]).map_err(|error| {
                    format!(
                        "failed to load noteskin actor '{} {}': {error}",
                        model.skin, model.element
                    )
                })?;
                Ok(Some(crate::SongLuaOverlayKind::NoteskinActor {
                    slots,
                    texture_samples: std::sync::Arc::from([]),
                }))
            }
            super::actors::song_kinds::Output::Model(None) => self
                .actor_model_layers(actor, read_slots, read_layer)
                .map(|layers| layers.map(|layers| crate::SongLuaOverlayKind::Model { layers })),
        })();
        self.vm.unpin(root).map_err(|error| error.to_string())?;
        result
    }
}

/// Owned blocks in caller-supplied actor order, retaining duplicate indices.
pub type SongActorFunctionBlocks = Vec<(usize, Vec<crate::SongLuaOverlayCommandBlock>)>;
impl SongRuntime {
    /// Existing compiler defaults for the six supported message commands.
    /// Release the returned root through vm_mut().unpin after its last use.
    pub fn default_song_message_command_params(
        &mut self,
        command_name: &str,
    ) -> Result<Option<Root>, Error> {
        self.actors
            .default_message_command_params(&mut self.vm, command_name)
    }

    /// Retain the selected handler, its source actor and one default-parameter
    /// object for repeated compiler probes. Calls use the command recursion guard
    /// and drain work queued by that handler. Release the returned VM root.
    pub fn create_song_message_capture_runner(
        &mut self,
        actor: Value,
        command_name: &str,
        command: Value,
    ) -> Result<Root, Error> {
        self.actors
            .message_capture_runner(&mut self.vm, actor, command_name, command)
    }

    /// Capture an existing compiler easing/action function against indexed owned
    /// actors, rewind their semantic state, and leave their capture buffers empty.
    /// Script globals/upvalues and RNG are owned by the caller's outer probe scope.
    pub fn capture_song_actor_function(
        &mut self,
        actors: &[(usize, Value)],
        function: Value,
        arg: Option<f32>,
        song_beat: Option<f32>,
    ) -> Result<SongActorFunctionBlocks, String> {
        if !self.actors.actor_function_capture_ready(&mut self.vm) {
            return Err("actor function capture requires preparation before frame replay".into());
        }
        let mut pins = Vec::new();
        let result = (|| {
            pins.push(self.vm.pin(function).map_err(|e| e.to_string())?);
            let mut selected = Vec::with_capacity(actors.len());
            for &(index, value) in actors {
                pins.push(self.vm.pin(value).map_err(|e| e.to_string())?);
                let Value::Table(actor) = value else {
                    return Err("capture actor table expected".into());
                };
                selected.push((index, actor));
            }
            let previous = self.values().map_err(|e| e.to_string())?;
            if let Some(beat) = song_beat {
                self.set_beat(beat).map_err(|e| e.to_string())?;
            }
            let (result, restored, returned) =
                self.actors
                    .capture_actor_function(&mut self.vm, selected, function, arg);
            pins.extend(returned);
            if restored {
                self.set_values(previous.0, previous.1)
                    .map_err(|e| e.to_string())?;
            }
            result.map_err(|e| e.to_string())
        })();
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}

impl SongRuntime {
    /// Command-local portion of the outer actor-aware probe scope. Actor state,
    /// capture buffers, global binding snapshots and output belong to its caller.
    #[cfg(feature = "test-support")]
    pub(super) fn preserving_command_locals<T>(
        &mut self,
        probe: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let random = self.preserve_random();
        let previous = self.actors.begin_command_locals(&mut self.vm);
        let result = probe(self);
        let restored = self.actors.restore_command_locals(&mut self.vm, previous);
        drop(random);
        restored?;
        result
    }
}

impl SongRuntime {
    /// Capture only actors first touched by capture-producing methods. Direct
    /// script/global mutations remain owned by the caller's outer probe scope.
    pub fn capture_song_touched_actor_function(
        &mut self,
        actors: &[(usize, Value)],
        function: Value,
        arg: Option<f32>,
        song_beat: Option<f32>,
    ) -> Result<SongActorFunctionBlocks, String> {
        if !self.actors.actor_function_capture_ready(&mut self.vm) {
            return Err("actor function capture requires preparation before frame replay".into());
        }
        let mut pins = Vec::new();
        let result = (|| {
            pins.push(self.vm.pin(function).map_err(|e| e.to_string())?);
            let mut selected = Vec::new();
            for &(index, value) in actors {
                pins.push(self.vm.pin(value).map_err(|e| e.to_string())?);
                let Value::Table(actor) = value else {
                    return Err("capture actor table expected".into());
                };
                selected.push((index, actor));
            }
            let previous = self.values().map_err(|e| e.to_string())?;
            if let Some(beat) = song_beat {
                self.set_beat(beat).map_err(|e| e.to_string())?;
            }
            let (result, restored, returned) = self.actors.capture_touched_actor_function(
                &mut self.vm,
                selected,
                function,
                arg,
                &self.random,
            );
            pins.extend(returned);
            if restored {
                self.set_values(previous.0, previous.1)
                    .map_err(|e| e.to_string())?;
            }
            result.map_err(|e| e.to_string())
        })();
        for root in pins {
            self.vm.unpin(root).map_err(|e| e.to_string())?;
        }
        result
    }
}
