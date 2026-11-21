//! Apply frozen poses and current content to the existing prepared visual slots.
use super::*;
use crate::{SongLuaLiveTarget as Target, SongLuaOverlayKind as Kind, SongLuaProxyTarget as Proxy};
use deadlib_present::actors::TextAttribute;

type Visuals<N, M> = deadsync_gameplay::SongLuaRuntimeVisuals<
    crate::SongLuaOverlayActor<Kind<N, M, TextAttribute>>,
    crate::SongLuaCapturedActor,
    crate::gameplay::SongLuaRuntimeOverlayStateDelta,
>;

fn target_key(target: Target) -> (u8, usize, usize) {
    match target {
        Target::Root(index) => (0, 0, index),
        Target::Background(layer, index) => (1, layer, index),
        Target::Foreground(layer, index) => (2, layer, index),
    }
}

fn target_at(target: Target, index: usize) -> Target {
    match target {
        Target::Root(_) => Target::Root(index),
        Target::Background(layer, _) => Target::Background(layer, index),
        Target::Foreground(layer, _) => Target::Foreground(layer, index),
    }
}

impl SongLiveScene {
    /// Bind shared actor positions to the screen's prepared visual slots.
    pub fn bind_targets(&mut self, targets: Vec<(usize, Target)>) -> Result<(), String> {
        if targets
            .iter()
            .any(|(global, _)| *global >= self.states.len())
        {
            return Err("live Lua binding index is outside its scene".into());
        }
        let mut runtime = self.runtime.access();
        self.visual_targets = targets
            .iter()
            .map(|&(global, target)| {
                (
                    runtime
                        .vm
                        .object_pointer(self.actors[global])
                        .expect("retained actor identity"),
                    target,
                )
            })
            .collect();
        self.targets = targets;
        Ok(())
    }

    fn proxy_target(
        &self,
        runtime: &mut SongRuntime,
        actor: Value,
        owner: Target,
    ) -> Result<Proxy, String> {
        let mut target = runtime
            .actor_proxy_target(actor)
            .map_err(|e| e.to_string())?
            .unwrap_or(Proxy::Actor {
                overlay_index: usize::MAX,
            });
        if let Proxy::Actor { overlay_index } = &mut target {
            *overlay_index = self
                .visual_targets
                .get(overlay_index)
                .and_then(|target| match (*target, owner) {
                    (Target::Root(local), Target::Root(_)) => Some(local),
                    (Target::Background(layer, local), Target::Background(owner, _))
                    | (Target::Foreground(layer, local), Target::Foreground(owner, _))
                        if layer == owner =>
                    {
                        Some(local)
                    }
                    _ => None,
                })
                .unwrap_or(usize::MAX);
        }
        Ok(target)
    }

    /// Remove deleted wrappers from the visual ancestry without moving slots.
    fn detach_wrapper_visuals<N, M>(
        &mut self,
        runtime: &mut SongRuntime,
        visuals: &mut Visuals<N, M>,
    ) -> Result<(), String> {
        if self.published_wrapper_epoch == self.wrapper_epoch {
            return Ok(());
        }
        for &(global, target) in &self.targets {
            let value = self.actors[global];
            if runtime.actors.live_actor_exists(&runtime.vm, value) {
                continue;
            }
            let (overlays, local) = match target {
                Target::Root(local) => (&mut visuals.overlays, local),
                Target::Background(layer, local) => (
                    &mut visuals.background_visual_layers.get_mut(layer)
                        .ok_or("removed wrapper background layer is missing")?.overlays,
                    local,
                ),
                Target::Foreground(layer, local) => (
                    &mut visuals.foreground_visual_layers.get_mut(layer)
                        .ok_or("removed wrapper foreground layer is missing")?.overlays,
                    local,
                ),
            };
            let wrapper = overlays.get_mut(local)
                .ok_or("removed wrapper visual slot is missing")?;
            if !matches!(wrapper.kind, Kind::WrapperState) {
                return Err("removed live actor is not a wrapper state".into());
            }
            let parent = wrapper.parent_index.take();
            for overlay in overlays {
                if overlay.parent_index == Some(local) {
                    overlay.parent_index = parent;
                }
            }
            if let Some(identity) = runtime.vm.object_pointer(value) {
                self.visual_targets.remove(&identity);
            }
        }
        self.published_wrapper_epoch = self.wrapper_epoch;
        Ok(())
    }

    /// Insert newly owned wrapper states around their existing visual owners.
    /// As in finite compilation's append_update_wrapper_rows, each new wrapper
    /// surrounds the previous outermost wrapper belonging to that same owner.
    /// Appending keeps every existing visual, proxy and capture index stable.
    fn append_wrapper_visuals<N, M>(
        &mut self,
        runtime: &mut SongRuntime,
        visuals: &mut Visuals<N, M>,
    ) -> Result<(), String> {
        while let Some(&(owner, global)) = self.pending_wrappers.front() {
            if !runtime.actors.live_actor_exists(&runtime.vm, self.actors[global]) {
                self.pending_wrappers.pop_front();
                continue;
            }
            let target = self.targets.iter().find_map(|&(index, target)|
                (index == owner).then_some(target))
                .ok_or("live wrapper owner has no prepared visual slot")?;
            let initial_state = *self.states.get(global)
                .ok_or("live wrapper has no completed state snapshot")?;
            let owner_value = self.actors[owner];
            let (overlays, events, ease_ranges) = match target {
                Target::Root(_) => (&mut visuals.overlays, &mut visuals.overlay_events,
                    &mut visuals.overlay_ease_ranges),
                Target::Background(layer, _) => {
                    let layer = visuals.background_visual_layers.get_mut(layer)
                        .ok_or("live wrapper background layer is missing")?;
                    (&mut layer.overlays, &mut layer.overlay_events, &mut layer.overlay_ease_ranges)
                }
                Target::Foreground(layer, _) => {
                    let layer = visuals.foreground_visual_layers.get_mut(layer)
                        .ok_or("live wrapper foreground layer is missing")?;
                    (&mut layer.overlays, &mut layer.overlay_events, &mut layer.overlay_ease_ranges)
                }
            };
            let mut inner = target_key(target).2;
            if inner >= overlays.len() {
                return Err("live wrapper owner visual index is outside its layer".into());
            }
            while let Some(parent) = overlays[inner].parent_index {
                let wrapper = overlays.get(parent)
                    .ok_or("live wrapper has an invalid visual parent")?;
                if !matches!(wrapper.kind, Kind::WrapperState) {
                    break;
                }
                let key = target_key(target_at(target, parent));
                let parent_global = self.targets.iter().find_map(|&(index, target)|
                    (target_key(target) == key).then_some(index))
                    .ok_or("live wrapper parent has no owned visual binding")?;
                let Value::Table(parent_actor) = self.actors[parent_global] else {
                    return Err("live wrapper parent is not an owned actor".into());
                };
                if runtime.actors.wrapper_parent(&mut runtime.vm, parent_actor)
                    .map(Value::Table) != Some(owner_value)
                {
                    break;
                }
                inner = parent;
            }
            let local = overlays.len();
            let parent_index = overlays[inner].parent_index;
            overlays.push(crate::SongLuaOverlayActor {
                kind: Kind::WrapperState,
                name: None,
                parent_index,
                initial_state,
                message_commands: Vec::new(),
            });
            overlays[inner].parent_index = Some(local);
            events.resize_with(overlays.len(), Vec::new);
            ease_ranges.resize(overlays.len(), 0..0);
            let target = target_at(target, local);
            let identity = runtime.vm.object_pointer(self.actors[global])
                .expect("retained live wrapper identity");
            self.targets.push((global, target));
            self.visual_targets.insert(identity, target);
            self.pending_wrappers.pop_front();
        }
        Ok(())
    }

    /// Publish in binding order. Earlier slots and fields survive a later error.
    pub fn write_visuals<N, M>(&mut self, visuals: &mut Visuals<N, M>) -> Result<(), String> {
        let owner = self.runtime.clone();
        let mut runtime = owner.access();
        self.detach_wrapper_visuals(&mut runtime, visuals)?;
        self.append_wrapper_visuals(&mut runtime, visuals)?;
        for &(global, target) in &self.targets {
            let actor = match target {
                Target::Root(local) => visuals.overlays.get_mut(local),
                Target::Background(layer, local) => visuals
                    .background_visual_layers
                    .get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
                Target::Foreground(layer, local) => visuals
                    .foreground_visual_layers
                    .get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
            }
            .ok_or_else(|| "live Lua actor has no prepared visual slot".to_owned())?;
            actor.initial_state = self.states[global];
            let owned = self.actors[global];
            if !runtime.actors.live_actor_exists(&runtime.vm, owned) {
                continue;
            }
            let SongRuntime { actors, vm, .. } = &mut *runtime;
            actors.publish_visual_content(vm, owned, &mut actor.kind)?;
            if let Kind::ActorProxy { target: proxy } = &mut actor.kind {
                *proxy = self.proxy_target(&mut runtime, owned, target)?;
            }
        }
        for (target, state) in self.tracked_states() {
            use crate::SongLuaTrackedActorTarget as T;
            match target {
                T::Player(p) => visuals.player_actors[p].initial_state = state,
                T::PlayerJudgment(p) => visuals.player_actors[p].judgment.initial_state = state,
                T::PlayerCombo(p) => visuals.player_actors[p].combo.initial_state = state,
                T::SongForeground => visuals.song_foreground.initial_state = state,
                T::ScreenLayer(layer) => visuals.screen_layers[layer].initial_state = state,
            }
        }
        for &(target, value) in &self.tracked {
            use crate::SongLuaTrackedActorTarget as T;
            let child = match target {
                T::PlayerJudgment(p) => &mut visuals.player_actors[p].judgment,
                T::PlayerCombo(p) => &mut visuals.player_actors[p].combo,
                _ => continue,
            };
            let SongRuntime { actors, vm, .. } = &mut *runtime;
            let super::super::actors::chronological::Answer::Hud(states) = actors
                .chronological_read(
                    vm,
                    value,
                    super::super::actors::chronological::Query::Hud([
                        self.last_music,
                        self.last_beat,
                    ]),
                )
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            child.render_frames.clear();
            if !states.is_empty() {
                child.render_frames.push(crate::SongLuaNoteFieldFrame {
                    second: self.last_music,
                    wrappers: states.into(),
                });
            }
        }
        visuals.hidden_players =
            std::array::from_fn(|p| !visuals.player_actors[p].initial_state.visible);
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub fn visual_binding_epoch(&self) -> u64 {
        self.wrapper_epoch
    }

    #[cfg(feature = "test-support")]
    pub fn visual_bindings(&self) -> Result<Vec<(String, Target)>, String> {
        let mut runtime = self.runtime.access();
        let mut bindings = Vec::with_capacity(self.targets.len());
        for &(global, target) in &self.targets {
            let owned = self.actors[global];
            let SongRuntime { actors, vm, .. } = &mut *runtime;
            if actors.live_actor_exists(vm, owned) {
                let path = actors.visual_path(vm, owned).map_err(|e| e.to_string())?;
                bindings.push((path, target));
            }
        }
        Ok(bindings)
    }

    #[cfg(feature = "test-support")]
    pub fn visual_draw_bindings(&self) -> &[(usize, Target)] {
        &self.targets
    }
}
