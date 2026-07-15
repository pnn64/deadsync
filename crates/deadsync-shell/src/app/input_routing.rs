use super::App;
use crate::input::{
    GameplayQueuedEvent, GameplayRawKeyEvent, GameplayRawKeyRouteContext, PreScreenInputContext,
    PreScreenInputRoute, QueuedInputBatchState, QueuedInputEventRoute, allowed_gameplay_raw_action,
    gameplay_raw_key_route_plan, pre_screen_input_route, queued_input_flush_plan,
    raw_keyboard_capture_enabled,
};
use deadsync_config::prelude as config;
use deadsync_gameplay::RawKeyAction;
use deadsync_input::{self as logical_input, InputEvent};
use deadsync_theme_simply_love::SimplyLoveEffect as ThemeEffect;
use deadsync_theme_simply_love::screens;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as CurrentScreen;
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
        let now_music_time = gs.music_time_from_audio_snapshot(crate::gameplay_runtime::snapshot());
        let action = gs.handle_queued_raw_key_input(
            plan.input,
            plan.modifier_key,
            plan.pressed,
            plan.timestamp,
            now_music_time,
            plan.allow_commands,
        );
        crate::gameplay_runtime::drain(gs);
        let keyboard_features = config::input_routing_config().keyboard_features;
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

    pub(super) fn route_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.sync_light_input(&ev);
        if self.route_operator_menu_button(&ev) {
            return Ok(());
        }
        if self.route_gameplay_offset_prompt_input(event_loop, &ev) {
            return Ok(());
        }
        if let Some(action) = self.try_handle_late_join(&ev) {
            self.handle_action(action, event_loop)?;
            return Ok(());
        }
        let current_screen = self.state.screens.current_screen;
        let evaluation_test_input_active = current_screen == CurrentScreen::Evaluation
            && screens::evaluation::test_input_pane_active(&self.state.screens.evaluation_state);
        match pre_screen_input_route(
            ev.pressed,
            ev.action,
            PreScreenInputContext {
                screen: current_screen,
                only_dedicated_menu_buttons: config::input_routing_config()
                    .only_dedicated_menu_buttons,
                evaluation_test_input_active,
                gameplay_offset_prompt_active: self.state.gameplay_offset_save_prompt.is_some(),
                course_active: self.state.session.course_run.is_some(),
            },
        ) {
            PreScreenInputRoute::Dispatch => {}
            PreScreenInputRoute::Consume => return Ok(()),
            PreScreenInputRoute::RequestScreenshot(side) => {
                self.state.shell.screenshot.request(side);
                return Ok(());
            }
            PreScreenInputRoute::Restart => {
                self.try_gameplay_restart(event_loop, "Restart button");
                return Ok(());
            }
        }
        let action = match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                self.sync_main_menu_runtime_view();
                screens::menu::handle_input(&mut self.state.screens.menu_state, &ev)
            }
            CurrentScreen::SelectProfile => screens::select_profile::handle_input(
                &mut self.state.screens.select_profile_state,
                &ev,
            ),
            CurrentScreen::SelectColor => {
                screens::select_color::handle_input(&mut self.state.screens.select_color_state, &ev)
            }
            CurrentScreen::ArrowCloudLogin => screens::arrowcloud_login::handle_input(
                &mut self.state.screens.arrowcloud_login_state,
                &ev,
            ),
            CurrentScreen::GrooveStatsLogin => screens::groovestats_login::handle_input(
                &mut self.state.screens.groovestats_login_state,
                &ev,
            ),
            CurrentScreen::SelectStyle => {
                screens::select_style::handle_input(&mut self.state.screens.select_style_state, &ev)
            }
            CurrentScreen::SelectPlayMode => screens::select_mode::handle_input(
                &mut self.state.screens.select_play_mode_state,
                &ev,
            ),
            CurrentScreen::ProfileLoad => {
                screens::profile_load::handle_input(&mut self.state.screens.profile_load_state, &ev)
            }
            CurrentScreen::Options => {
                let updater = super::updater::view();
                screens::options::handle_input(
                    &mut self.state.screens.options_state,
                    &self.asset_manager,
                    &updater,
                    &ev,
                )
            }
            CurrentScreen::Credits => {
                screens::credits::handle_input(&mut self.state.screens.credits_state, &ev)
            }
            CurrentScreen::ManageLocalProfiles => screens::manage_local_profiles::handle_input(
                &mut self.state.screens.manage_local_profiles_state,
                &ev,
            ),
            CurrentScreen::Mappings => {
                screens::mappings::handle_input(&mut self.state.screens.mappings_state, &ev)
            }
            CurrentScreen::Input => {
                screens::input::handle_input(&mut self.state.screens.input_state, &ev)
            }
            CurrentScreen::ConfigurePads => screens::pad_config::handle_input(
                &mut self.state.screens.pad_config_state,
                &ev,
                self.state.shell.interaction.controls().shift(),
            ),
            CurrentScreen::TestLights => {
                screens::test_lights::handle_input(&mut self.state.screens.test_lights_state, &ev)
            }
            CurrentScreen::OverscanAdjustment => screens::overscan_adjustment::handle_input(
                &mut self.state.screens.overscan_adjustment_state,
                &ev,
            ),
            CurrentScreen::SmxAssignPads => {
                screens::smx_assign::handle_input(&mut self.state.screens.smx_assign_state, &ev)
            }
            CurrentScreen::SelectMusic => screens::select_music::handle_input(
                &mut self.state.screens.select_music_state,
                &ev,
                self.state.shell.interaction.controls().shift(),
            ),
            CurrentScreen::SelectCourse => screens::select_course::handle_input(
                &mut self.state.screens.select_course_state,
                &ev,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.state.screens.player_options_state {
                    screens::player_options::handle_input(pos, &self.asset_manager, &ev)
                } else {
                    ThemeEffect::None
                }
            }
            CurrentScreen::Evaluation => {
                screens::evaluation::handle_input(&mut self.state.screens.evaluation_state, &ev)
            }
            CurrentScreen::EvaluationSummary => {
                let num_stages = self
                    .post_select_display_stage_count(config::get().show_course_individual_scores);
                screens::evaluation_summary::handle_input(
                    &mut self.state.screens.evaluation_summary_state,
                    num_stages,
                    &ev,
                )
            }
            CurrentScreen::Initials => {
                screens::initials::handle_input(&mut self.state.screens.initials_state, &ev)
            }
            CurrentScreen::GameOver => {
                screens::gameover::handle_input(&mut self.state.screens.gameover_state, &ev)
            }
            CurrentScreen::Sandbox => {
                screens::sandbox::handle_input(&mut self.state.screens.sandbox_state, &ev)
            }
            CurrentScreen::Init => {
                screens::init::handle_input(&mut self.state.screens.init_state, &ev)
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    crate::gameplay_runtime::handle_input(gs, &ev)
                } else {
                    ThemeEffect::None
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = &mut self.state.screens.practice_state {
                    crate::gameplay_runtime::handle_practice_input(ps, &ev)
                } else {
                    ThemeEffect::None
                }
            }
        };
        if matches!(action, ThemeEffect::None) {
            return Ok(());
        }
        self.handle_action(action, event_loop)
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
