use super::{App, CurrentScreen};
use crate::config;
use deadsync_gameplay::{
    GameplayQueuedEvent, GameplayRawKeyEvent, RawKeyAction, gameplay_raw_key_input,
    gameplay_raw_modifier_key,
};
use deadsync_input as logical_input;
pub(super) use deadsync_shell::screen_accepts_queued_input;
use deadsync_shell::{
    allowed_gameplay_raw_action, gameplay_dispatch_continues as dispatch_continues,
    raw_keyboard_capture_enabled,
};
use std::error::Error;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn flush_due_input_events(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<bool, Box<dyn Error>> {
        if !screen_accepts_queued_input(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            return Ok(false);
        }
        let mut flushed = false;
        let mut err: Option<Box<dyn Error>> = None;
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let start_screen = self.state.screens.current_screen;
        let mut discard_gameplay_batch = false;
        logical_input::drain_debounced_input_events_with(|ev| {
            flushed = true;
            if gameplay_screen {
                if discard_gameplay_batch || err.is_some() {
                    return;
                }
                if let Err(e) =
                    self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                {
                    err = Some(e);
                    return;
                }
                if !self.gameplay_dispatch_continues(start_screen) {
                    discard_gameplay_batch = true;
                }
            } else if err.is_none()
                && let Err(e) = self.route_input_event(event_loop, ev)
            {
                err = Some(e);
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(flushed)
    }

    #[inline(always)]
    pub(super) fn gameplay_dispatch_continues(&self, start_screen: CurrentScreen) -> bool {
        dispatch_continues(
            start_screen,
            self.state.screens.current_screen,
            &self.state.shell.transition,
        )
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
        if self.state.screens.current_screen != CurrentScreen::Gameplay {
            return;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_mut() else {
            return;
        };
        let allow_commands = self.state.gameplay_offset_save_prompt.is_none();
        gs.set_raw_modifier_state(
            self.state.shell.interaction.controls().shift(),
            self.state.shell.interaction.controls().ctrl(),
        );
        let now_music_time =
            gs.music_time_from_audio_snapshot(crate::screens::gameplay::audio_snapshot());
        let action = gs.handle_queued_raw_key_input(
            gameplay_raw_key_input(ev.code),
            gameplay_raw_modifier_key(ev.code),
            ev.pressed,
            ev.timestamp,
            now_music_time,
            allow_commands,
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
