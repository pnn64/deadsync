//! Selection-aware screen construction in the retained process VM.
use super::*;
use actors::gameplay_theme::Phase;

impl RustSongRuntime {
    /// Project retained chart identities and immutable metadata into the next
    /// scene's context. Public chart getters do not redefine those identities.
    pub fn selected_gameplay_context(
        &mut self,
        context: &SongLuaCompileContext,
    ) -> Result<SongLuaCompileContext, Error> {
        let Value::Table(catalog) = self.vm.rooted(self.catalog)? else {
            unreachable!()
        };
        managers::game_state::scene::context(&mut self.vm, catalog, context)
    }

    /// Initial screen construction. Theme loading is a separate phase so the
    /// caller owns the scene even when a later theme callback fails.
    pub fn construct_gameplay_screen(
        &mut self,
        context: &SongLuaCompileContext,
    ) -> Result<(ActorScene, SongLuaCompileContext), Error> {
        self.gameplay_screen(context, false)
    }

    /// Reconstruct screen-owned state without reinstalling process globals or
    /// reseeding the PRNG. The session must run outgoing End callbacks first,
    /// and retains responsibility for releasing its outgoing ActorScenes.
    /// Old actors stop receiving messages before the new constructor runs.
    pub fn reconstruct_gameplay_screen(
        &mut self,
        context: &SongLuaCompileContext,
    ) -> Result<(ActorScene, SongLuaCompileContext), Error> {
        self.restart_options()?;
        self.actors.clear_message_subscriptions(&mut self.vm)?;
        self.gameplay_screen(context, true)
    }

    fn gameplay_screen(
        &mut self,
        context: &SongLuaCompileContext,
        replacing: bool,
    ) -> Result<(ActorScene, SongLuaCompileContext), Error> {
        let Value::Table(catalog) = self.vm.rooted(self.catalog)? else {
            unreachable!()
        };
        managers::game_state::scene::initialize(&mut self.vm, catalog, context)?;
        let selected = self.selected_gameplay_context(context)?;
        if !replacing {
            managers::rebuild_sl(&mut self.vm, &selected)?;
        }
        let (scene, players) = self.actors.gameplay_screen(
            &mut self.vm,
            &selected,
            self.statistics.clone(),
            Value::Table(catalog),
            self.screen.state.clone(),
        )?;
        let screen = self.actor_scene_root(scene).unwrap();
        let result = (|| {
            self.bind_screen_input(screen)?;
            self.publish_screen(screen, players)?;
            if replacing {
                self.reset_stage_statistics(&selected)?;
                self.reset()?;
            } else {
                // Initial native late globals create statistics after the base
                // screen, using the context projected from the seeded slots.
                managers::statistics::install(
                    &mut self.vm,
                    Value::Table(catalog),
                    &selected,
                    self.statistics.clone(),
                    self.clock.clone(),
                )?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.release_actor_scene(scene)?;
            return Err(error);
        }
        Ok((scene, selected))
    }

    /// Load the existing bundled phases after screen publication. Failure keeps
    /// partial actors in the supplied scene so its owner can inspect and retire
    /// them. This does not run authored song startup or replay future commands.
    pub fn load_gameplay_theme(
        &mut self,
        scene: ActorScene,
        context: &SongLuaCompileContext,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(scene).is_none() {
            return Ok(false);
        }
        self.begin_actor_preparation()?;
        #[cfg(feature = "test-support")]
        let capture_probe = context.capture_overlay_probe;
        #[cfg(not(feature = "test-support"))]
        let capture_probe = {
            let _ = context;
            false
        };
        for phase in [
            Phase::Combo,
            Phase::Underlay,
            Phase::Overlay { capture_probe },
            Phase::Decorations,
            Phase::Transitions,
        ] {
            if !self
                .actors
                .load_gameplay_theme_phase(&mut self.vm, scene, phase)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
