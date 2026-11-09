//! Apply frozen poses and current content to the existing prepared visual slots.
use super::*;
use crate::{SongLuaLiveTarget as Target, SongLuaOverlayKind as Kind, SongLuaProxyTarget as Proxy};
use deadlib_present::actors::TextAttribute;

type Visuals<N, M> = deadsync_gameplay::SongLuaRuntimeVisuals<
    crate::SongLuaOverlayActor<Kind<N, M, TextAttribute>>,
    crate::SongLuaCapturedActor,
    crate::gameplay::SongLuaRuntimeOverlayStateDelta,
>;

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

    /// Publish in binding order. Earlier slots and fields survive a later error.
    pub fn write_visuals<N, M>(&self, visuals: &mut Visuals<N, M>) -> Result<(), String> {
        #[cfg(feature = "performance")]
        let _profile = crate::live_profile::scope(crate::live_profile::Phase::VisualPublication);
        let mut runtime = self.runtime.access();
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
        visuals.hidden_players =
            std::array::from_fn(|p| !visuals.player_actors[p].initial_state.visible);
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub fn visual_bindings(&self) -> Result<Vec<(String, Target)>, String> {
        let mut runtime = self.runtime.access();
        self.targets
            .iter()
            .map(|&(global, target)| {
                let SongRuntime { actors, vm, .. } = &mut *runtime;
                actors
                    .visual_path(vm, self.actors[global])
                    .map(|path| (path, target))
                    .map_err(|e| e.to_string())
            })
            .collect()
    }

    #[cfg(feature = "test-support")]
    pub fn visual_draw_bindings(&self) -> &[(usize, Target)] {
        &self.targets
    }
}
