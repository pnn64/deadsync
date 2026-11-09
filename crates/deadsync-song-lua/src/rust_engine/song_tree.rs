//! Rooted compiler overlay rows in the existing native draw-tree order.
use super::*;
use crate::{SongLuaOverlayActor, SongLuaOverlayKind, SongLuaProxyTarget};
use actors::song_tree::{Answer, Query, Tree};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct SongOverlayActor<Kind> {
    pub actor: SongLuaOverlayActor<Kind>,
    pub message_sounds: Vec<(String, PathBuf)>,
    pub(super) owner: Root,
    pub(super) slot: usize,
}
pub struct SongOverlayActors<Kind> {
    pub actors: Vec<SongOverlayActor<Kind>>,
    pub(super) owner: Option<Root>,
}
type Kind<S, V> = SongLuaOverlayKind<S, V, deadlib_present::actors::TextAttribute>;
struct Row<K> {
    value: Value,
    identity: usize,
    actor: SongLuaOverlayActor<K>,
    sounds: Vec<(String, PathBuf)>,
}
struct Build<'a, S, V, F, G, H> {
    tree: &'a Tree,
    context: &'a SongLuaCompileContext,
    aft_names: HashSet<String>,
    referenced: HashSet<TableId>,
    rows: Vec<Row<Kind<S, V>>>,
    output_pins: &'a mut Vec<Root>,
    slots: F,
    layer: G,
    skipped: H,
}
impl<S, V, F, G, H> Build<'_, S, V, F, G, H>
where
    F: Fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>,
    G: Fn(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
    H: FnMut(String),
{
    fn query(
        &self,
        runtime: &mut RustSongRuntime,
        actor: Value,
        query: Query,
    ) -> Result<Answer, String> {
        let observation = self
            .tree
            .query(&mut runtime.vm, actor, query)
            .map_err(|e| e.to_string())?;
        // Metadata observations have no cursor handle.
        Ok(observation.answer)
    }
    fn visit(
        &mut self,
        runtime: &mut RustSongRuntime,
        actor: Value,
        parent: Option<usize>,
    ) -> Result<(), String> {
        let mut parent = parent;
        let wrappers = self
            .tree
            .query(&mut runtime.vm, actor, Query::Wrappers)
            .map_err(|e| e.to_string())?;
        let result: Result<(), String> = (|| {
            let Answer::Wrappers(entries) = &wrappers.answer else {
                unreachable!()
            };
            if let Some((array, len)) = *entries {
                for index in (1..=len).rev() {
                    let entry = self
                        .tree
                        .query(&mut runtime.vm, array, Query::Index(index, true))
                        .map_err(|e| e.to_string())?;
                    let Answer::Value(wrapper) = &entry.answer else {
                        unreachable!()
                    };
                    let wrapper = *wrapper;
                    let result = self.visit(runtime, wrapper, parent);
                    entry.release(&mut runtime.vm).map_err(|e| e.to_string())?;
                    result?;
                    parent = Some(
                        self.rows
                            .len()
                            .checked_sub(1)
                            .ok_or_else(|| "wrapper did not produce an overlay".to_owned())?,
                    );
                }
            }
            Ok(())
        })();
        wrappers
            .release(&mut runtime.vm)
            .map_err(|e| e.to_string())?;
        result?;
        let next = if let Some(row) = self.row(runtime, actor, parent)? {
            let i = self.rows.len();
            self.output_pins
                .push(runtime.vm.pin(row.value).map_err(|e| e.to_string())?);
            self.rows.push(row);
            Some(i)
        } else {
            parent
        };
        let Answer::Flag(wrapper) = self.query(runtime, actor, Query::Wrapper)? else {
            unreachable!()
        };
        if wrapper {
            return Ok(());
        }
        let mut index = 1;
        loop {
            let entry = self
                .tree
                .query(&mut runtime.vm, actor, Query::Index(index, false))
                .map_err(|e| e.to_string())?;
            let Answer::Value(child) = &entry.answer else {
                unreachable!()
            };
            let child = *child;
            let result = if matches!(child, Value::Table(_)) {
                self.visit(runtime, child, next)
            } else {
                Ok(())
            };
            entry.release(&mut runtime.vm).map_err(|e| e.to_string())?;
            result?;
            if child == Value::Nil {
                break;
            }
            index += 1;
        }
        Ok(())
    }
    fn row(
        &mut self,
        runtime: &mut RustSongRuntime,
        actor: Value,
        parent: Option<usize>,
    ) -> Result<Option<Row<Kind<S, V>>>, String> {
        let Answer::Header(header) = self.query(runtime, actor, Query::Header)? else {
            unreachable!()
        };
        let Some(header) = header else {
            return Ok(None);
        };
        let (class, initial, identity) = *header;
        let commands = runtime.capture_song_actor_message_commands(actor)?;
        for detail in commands.skipped {
            (self.skipped)(detail);
        }
        let Answer::Name(name) = self.query(runtime, actor, Query::Name)? else {
            unreachable!()
        };
        let draw = if class.eq_ignore_ascii_case("ActorFrame") {
            let Answer::Flag(draw) = self.query(runtime, actor, Query::Draw)? else {
                unreachable!()
            };
            draw
        } else {
            false
        };
        let plain = class.eq_ignore_ascii_case("Actor");
        let frame = class.eq_ignore_ascii_case("ActorFrame") && parent.is_none();
        if (plain || frame)
            && !self.context.live
            && name.is_none()
            && initial == crate::SongLuaOverlayState::default()
            && commands.commands.is_empty()
            && commands.sounds.is_empty()
            && !self.tree.deferred(actor)
            && !draw
            && !matches!(actor,Value::Table(id)if self.referenced.contains(&id))
        {
            return Ok(None);
        }
        let Some(kind) = runtime.read_song_overlay_kind(
            actor,
            SongOverlayKindInputs {
                actor_type: &class,
                initial_state: initial,
                context: self.context,
                aft_capture_names: &self.aft_names,
            },
            &self.slots,
            &self.layer,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(Row {
            value: actor,
            identity,
            actor: SongLuaOverlayActor {
                kind,
                name,
                parent_index: parent,
                initial_state: initial,
                message_commands: commands.commands,
            },
            sounds: commands.sounds,
        }))
    }
}
impl RustSongRuntime {
    /// Collect the existing renderer tree, including wrappers, source messages,
    /// AFT-name prepass, plain-actor elision and final generic-proxy remapping.
    /// Release the returned batch after all compiler/session Lua use completes.
    pub fn read_song_overlay_actors<S, V>(
        &mut self,
        root: Value,
        context: &SongLuaCompileContext,
        slots: impl Fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>,
        layer: impl Fn(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
        skipped: impl FnMut(String),
    ) -> Result<SongOverlayActors<Kind<S, V>>, String> {
        if !matches!(root, Value::Table(_)) {
            return Ok(SongOverlayActors {
                actors: Vec::new(),
                owner: None,
            });
        }
        let input = self.vm.pin(root).map_err(|e| e.to_string())?;
        let tree = self.actors.overlay_tree();
        let mut owner = None;
        let mut output_pins = Vec::new();
        let result = (|| {
            let Answer::Names(aft_names) = tree
                .query(&mut self.vm, root, Query::AftNames)
                .map_err(|e| e.to_string())?
                .answer
            else {
                unreachable!()
            };
            let Answer::References(referenced) = tree
                .query(&mut self.vm, root, Query::References)
                .map_err(|e| e.to_string())?
                .answer
            else {
                unreachable!()
            };
            let mut build = Build {
                tree: &tree,
                context,
                aft_names,
                referenced,
                rows: Vec::new(),
                output_pins: &mut output_pins,
                slots,
                layer,
                skipped,
            };
            build.visit(self, root, None)?;
            let identities = build
                .rows
                .iter()
                .enumerate()
                .map(|(i, r)| (r.identity, i))
                .collect::<HashMap<_, _>>();
            for row in &mut build.rows {
                if let SongLuaOverlayKind::ActorProxy {
                    target: SongLuaProxyTarget::Actor { overlay_index },
                } = &mut row.actor.kind
                {
                    *overlay_index = identities.get(overlay_index).copied().unwrap_or(usize::MAX);
                }
            }
            // Only collected output rows acquire the returned batch lifetime.
            let table = self.vm.try_new_table().map_err(|e| e.to_string())?;
            let pin = self
                .vm
                .pin(Value::Table(table))
                .map_err(|e| e.to_string())?;
            owner = Some(pin);
            let mut actors = Vec::new();
            for (index, row) in build.rows.into_iter().enumerate() {
                let slot = index + 1;
                self.vm
                    .raw_table_set(table, Value::Number(slot as f64), row.value)
                    .map_err(|e| e.to_string())?;
                actors.push(SongOverlayActor {
                    actor: row.actor,
                    message_sounds: row.sounds,
                    owner: pin,
                    slot,
                });
            }
            Ok(actors)
        })();
        for pin in output_pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        self.vm.unpin(input).map_err(|e| e.to_string())?;
        match result {
            Ok(actors) => Ok(SongOverlayActors { actors, owner }),
            Err(error) => {
                if let Some(owner) = owner {
                    self.vm.unpin(owner).map_err(|e| e.to_string())?;
                }
                Err(error)
            }
        }
    }
    pub fn song_overlay_actor_value<K>(&self, actor: &SongOverlayActor<K>) -> Result<Value, Error> {
        let Value::Table(owner) = self.vm.rooted(actor.owner)? else {
            unreachable!()
        };
        self.vm
            .raw_table_get(owner, Value::Number(actor.slot as f64))
    }
    pub fn release_song_overlay_actors<K>(
        &mut self,
        actors: &mut SongOverlayActors<K>,
    ) -> Result<bool, Error> {
        let Some(owner) = actors.owner else {
            return Ok(false);
        };
        self.vm.unpin(owner)?;
        actors.owner = None;
        Ok(true)
    }
}
