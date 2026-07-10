use super::App;
use crate::config;
use deadsync_gameplay::{GameplayQueuedEvent, GameplayRawKeyEvent, RawKeyAction};
use deadsync_input as logical_input;
use deadsync_shell::{
    GameplayRawKeyRouteContext, QueuedInputBatchState, QueuedInputEventRoute,
    allowed_gameplay_raw_action, gameplay_raw_key_route_plan, queued_input_flush_plan,
    raw_keyboard_capture_enabled,
};
use std::error::Error;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn flush_due_input_events(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<bool, Box<dyn Error>> {
        let Some(plan) = queued_input_flush_plan(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) else {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            return Ok(false);
        };
        let mut err: Option<Box<dyn Error>> = None;
        let mut batch = QueuedInputBatchState::new();
        logical_input::drain_debounced_input_events_with(|ev| {
            match plan.route_drained_event(&mut batch, err.is_some()) {
                QueuedInputEventRoute::Skip => {}
                QueuedInputEventRoute::Gameplay => {
                    if let Err(e) =
                        self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                    {
                        err = Some(e);
                        return;
                    }
                    plan.note_dispatched_event(
                        &mut batch,
                        self.state.screens.current_screen,
                        &self.state.shell.transition,
                    );
                }
                QueuedInputEventRoute::Screen => {
                    if let Err(e) = self.route_input_event(event_loop, ev) {
                        err = Some(e);
                    }
                }
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(batch.flushed)
    }

    #[inline(always)]
    pub(super) fn route_gameplay_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayQueuedEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.gameplay_input_trace.note_queued_input();
        match ev {
            GameplayQueuedEvent::Input(ev) => self.route_input_event(event_loop, ev),
            GameplayQueuedEvent::RawKey(ev) => {
                self.route_gameplay_raw_key_event(event_loop, ev);
                Ok(())
            }
        }
    }

    fn route_gameplay_raw_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: GameplayRawKeyEvent,
    ) {
        let plan = gameplay_raw_key_route_plan(
            ev,
            GameplayRawKeyRouteContext {
                screen: self.state.screens.current_screen,
                gameplay_state_active: self.state.screens.gameplay_state.is_some(),
                offset_prompt_active: self.state.gameplay_offset_save_prompt.is_some(),
                shift_held: self.state.shell.interaction.controls().shift(),
                ctrl_held: self.state.shell.interaction.controls().ctrl(),
            },
        );
        let Some(gs) = self.state.screens.gameplay_state.as_mut() else {
            return;
        };
        let Some(plan) = plan else {
            return;
        };
        gs.set_raw_modifier_state(plan.shift_held, plan.ctrl_held);
        let now_music_time =
            gs.music_time_from_audio_snapshot(crate::screens::gameplay::audio_snapshot());
        let action = gs.handle_queued_raw_key_input(
            plan.input,
            plan.modifier_key,
            plan.pressed,
            plan.timestamp,
            now_music_time,
            plan.allow_commands,
        );
        crate::screens::gameplay::drain_audio_commands(gs);
        let keyboard_features = config::get().keyboard_features;
        match allowed_gameplay_raw_action(
            action,
            keyboard_features,
            self.state.session.course_run.is_some(),
        ) {
            Some(RawKeyAction::Restart) => {
                self.try_gameplay_restart(event_loop, "Ctrl+R");
            }
            Some(RawKeyAction::Reload) => {
                self.try_gameplay_reload(event_loop, "Ctrl+Shift+R");
            }
            _ => {}
        }
    }

    #[inline(always)]
    pub(super) fn sync_gameplay_input_capture(&self) {
        let capture_enabled = raw_keyboard_capture_enabled(
            self.accepts_live_input(),
            self.state.screens.current_screen,
            &self.state.shell.transition,
            cfg!(windows),
        );
        deadsync_input_native::set_raw_keyboard_capture_enabled(capture_enabled);
    }

    #[inline(always)]
    pub(super) fn clear_gameplay_input_events(&self) {
        deadsync_input_native::set_raw_keyboard_capture_enabled(false);
    }
}
