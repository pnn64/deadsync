use super::{
    App, Command, CurrentScreen, credits, evaluation, evaluation_summary, gameover, gameplay, init,
    initials, input_screen, manage_local_profiles, mappings, menu, options, overscan_adjustment,
    player_options, profile_load, sandbox, select_color, select_course, select_mode, select_music,
    select_profile, select_style, test_lights,
};
use crate::interaction::ProcessExitRequest;
use crate::navigation::{
    FadeCompletionEffect, NavigationTransitionStart, ProcessExitNavigationEffect,
    actor_transition_music_commands, apply_actor_entry_transition, apply_actor_fade_out_transition,
    apply_global_entry_transition, apply_global_fade_out_transition, fade_completion_plan,
    navigation_transition_effect_plan, process_exit_navigation_plan, screen_change_plan,
    write_current_screen_file,
};
use crate::screen_flow::navigation_route_plan;
use deadlib_present::actors::Actor;
use deadsync_config::prelude as config;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_theme_simply_love::{SimplyLoveQrLoginService, screens, visual_styles};
use log::{debug, info};
use winit::event_loop::ActiveEventLoop;

impl App {
    fn enter_arrowcloud_login(&mut self) {
        let color_index = self.state.screens.menu_state.active_color_index;
        let state = &mut self.state.screens.arrowcloud_login_state;
        state.active_color_index = color_index;
        let target = state
            .target_profile
            .as_ref()
            .map(|target| (target.id.clone(), target.display_name.clone()));
        let request = crate::qr_login::request(SimplyLoveQrLoginService::ArrowCloud, target);
        screens::arrowcloud_login::on_enter(state, request);
    }

    fn enter_groovestats_login(&mut self) {
        let color_index = self.state.screens.menu_state.active_color_index;
        let state = &mut self.state.screens.groovestats_login_state;
        state.active_color_index = color_index;
        let target = state
            .target_profile
            .as_ref()
            .map(|target| (target.id.clone(), target.display_name.clone()));
        let show_arrowcloud_next = target.is_none()
            && crate::qr_login::should_auto_show_arrowcloud(config::get().arrowcloud_qr_login_when);
        let request = crate::qr_login::request(SimplyLoveQrLoginService::GrooveStats, target);
        screens::groovestats_login::on_enter(state, request, show_arrowcloud_next);
    }

    #[inline(always)]
    pub(super) fn commit_screen_change(&mut self, target: CurrentScreen) {
        let prev = self.state.screens.current_screen;
        let plan = screen_change_plan(prev, target);
        if prev != target
            && matches!(
                prev,
                CurrentScreen::ArrowCloudLogin | CurrentScreen::GrooveStatsLogin
            )
        {
            self.qr_login.cancel_any();
        }
        if plan.leave_lobby {
            deadsync_online::lobbies::runtime_leave_lobby_default();
        }
        // Leaving gameplay by ANY path (bail-out to the song wheel, fail, give up,
        // course abort, results, etc.) must stop SMX sensor test mode, or it keeps
        // streaming on later screens. This is the one chokepoint every transition
        // routes through, so handle it here rather than per exit path. on_exit is
        // idempotent; skip gameplay->gameplay (restart / next course stage) since
        // on_enter re-establishes the mode anyway.
        if plan.exit_gameplay
            && let Some(gs) = self.state.screens.gameplay_state.as_mut()
        {
            crate::gameplay_runtime::exit(gs);
        }
        self.state.screens.current_screen = target;
        self.sync_gameplay_input_capture();
        write_current_screen_file(target);
        if plan.clear_text_layout_cache {
            self.ui_text_layout_cache.clear();
        }
    }

    pub(super) fn finish_actor_fade_out(
        &mut self,
        target_screen: CurrentScreen,
        event_loop: &ActiveEventLoop,
    ) {
        let prev = self.state.screens.current_screen;
        self.commit_screen_change(target_screen);
        if target_screen == CurrentScreen::SelectColor {
            select_color::on_enter(&mut self.state.screens.select_color_state);
        }
        if target_screen == CurrentScreen::ArrowCloudLogin {
            self.enter_arrowcloud_login();
        }
        if target_screen == CurrentScreen::GrooveStatsLogin {
            self.enter_groovestats_login();
        }

        let commands = actor_transition_music_commands(
            prev,
            target_screen,
            config::get().menu_music,
            visual_styles::srpg10_active(),
            visual_styles::menu_music_resolved_path(),
            visual_styles::srpg10_gameover_music_path(),
        );
        let _ = self.run_commands(commands, event_loop);

        if target_screen == CurrentScreen::Menu {
            let current_color_index = self.state.screens.menu_state.active_color_index;
            self.state.screens.menu_state = menu::init();
            self.state.screens.menu_state.active_color_index = current_color_index;
        } else if target_screen == CurrentScreen::Options {
            self.reset_options_state_for_entry(prev);
        } else if target_screen == CurrentScreen::ConfigurePads {
            // The full screen is reached only from Options (Song Select uses an
            // in-place overlay instead): return there and show all pads.
            let pad_state = &mut self.state.screens.pad_config_state;
            screens::pad_config::set_return_screen(pad_state, CurrentScreen::Options);
            screens::pad_config::set_filter(pad_state, screens::pad_config::PadFilter::All);
            screens::pad_config::reset_modes(pad_state);
        } else if target_screen == CurrentScreen::ManageLocalProfiles {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.manage_local_profiles_state = manage_local_profiles::init();
            self.state
                .screens
                .manage_local_profiles_state
                .active_color_index = color_index;
        } else if target_screen == CurrentScreen::SelectProfile {
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            self.state.screens.select_profile_state = select_profile::init();
            self.state.screens.select_profile_state.active_color_index = current_color_index;
            select_profile::set_fast_switch(
                &mut self.state.screens.select_profile_state,
                prev == CurrentScreen::SelectMusic,
            );
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
            }
        } else if target_screen == CurrentScreen::SelectStyle {
            let current_color_index = self.state.screens.select_style_state.active_color_index;
            self.state.screens.select_style_state = select_style::init();
            self.state.screens.select_style_state.active_color_index = current_color_index;
            let session = profile::get_session_snapshot();
            let p1_joined = session.side_joined(profile_data::PlayerSide::P1);
            let p2_joined = session.side_joined(profile_data::PlayerSide::P2);
            select_style::set_selected_index(
                &mut self.state.screens.select_style_state,
                if p1_joined && p2_joined { 1 } else { 0 },
            );
        } else if target_screen == CurrentScreen::Mappings {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.mappings_state = mappings::init();
            self.state.screens.mappings_state.active_color_index = color_index;
        } else if target_screen == CurrentScreen::TestLights {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.test_lights_state = test_lights::init();
            self.state.screens.test_lights_state.active_color_index = color_index;
            test_lights::on_enter(&mut self.state.screens.test_lights_state);
            self.lights.set_test_auto_cycle();
        } else if target_screen == CurrentScreen::OverscanAdjustment {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.overscan_adjustment_state = overscan_adjustment::init();
            self.state
                .screens
                .overscan_adjustment_state
                .active_color_index = color_index;
            overscan_adjustment::on_enter(&mut self.state.screens.overscan_adjustment_state);
        } else if target_screen == CurrentScreen::SmxAssignPads {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.smx_assign_state = screens::smx_assign::init();
            self.state.screens.smx_assign_state.active_color_index = color_index;
            screens::smx_assign::on_enter(
                &mut self.state.screens.smx_assign_state,
                &crate::smx_config::smx_assignment_view(),
            );
        }

        if prev == CurrentScreen::SelectColor {
            let idx = self.state.screens.select_color_state.active_color_index;
            self.sync_screen_color_index(idx);
        } else if prev == CurrentScreen::Options {
            let idx = self.state.screens.options_state.active_color_index;
            self.sync_screen_color_index(idx);
        }

        if target_screen == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        apply_actor_entry_transition(&mut self.state.shell, target_screen);
        deadlib_present::runtime::clear_all();
    }

    pub(super) fn handle_navigation_action(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, true);
    }

    pub(super) fn handle_navigation_action_after_prompt(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, false);
    }

    fn handle_navigation_action_inner(&mut self, target: CurrentScreen, allow_offset_prompt: bool) {
        let from = self.state.screens.current_screen;
        let cfg = config::get();
        self.lights.clear_button_pressed();
        let plan = navigation_route_plan(
            &cfg,
            from,
            target,
            !self.state.session.played_stages.is_empty(),
        );
        let target = plan.target;
        if let Some(pending) = plan.pending_post_select_summary_exit {
            self.state.session.pending_post_select_summary_exit = pending;
        }
        if plan.apply_preferred_style {
            let play_style =
                profile_data::play_style_from_machine_preference(cfg.machine_preferred_style);
            profile::set_session_play_style(play_style);
            self.state.session.preferred_difficulty_index =
                profile::get().last_played(play_style).difficulty_index;
        }
        if plan.apply_preferred_play_mode {
            profile::set_session_play_mode(profile_data::play_mode_from_machine_preference(
                cfg.machine_preferred_play_mode,
            ));
        }
        if plan.initialize_session_side {
            let p2_started = self.state.screens.menu_state.started_by_p2;
            profile::set_session_player_side(if p2_started {
                profile_data::PlayerSide::P2
            } else {
                profile_data::PlayerSide::P1
            });
            profile::set_session_joined(!p2_started, p2_started);
        }

        if allow_offset_prompt && self.maybe_begin_gameplay_offset_prompt(from, target, false) {
            return;
        }

        if from == CurrentScreen::Init && target == CurrentScreen::Menu {
            debug!("Instant navigation Init→Menu (out-transition handled by Init screen)");
            self.commit_screen_change(target);
            apply_actor_entry_transition(&mut self.state.shell, target);
            deadlib_present::runtime::clear_all();
            return;
        }

        let transition_plan =
            navigation_transition_effect_plan(from, target, &self.state.shell.transition);
        if transition_plan.clear_exit_intent {
            self.state.shell.interaction.clear_exit_intent();
        }
        match transition_plan.start {
            NavigationTransitionStart::DirectEntry { target } => {
                self.commit_screen_change(target);
                apply_actor_entry_transition(&mut self.state.shell, target);
                deadlib_present::runtime::clear_all();
            }
            NavigationTransitionStart::Busy => {}
            NavigationTransitionStart::ActorFade { from, target } => {
                self.start_actor_fade(from, target);
            }
            NavigationTransitionStart::GlobalFade {
                target,
                stop_screen_sfx,
            } => {
                self.start_global_fade(target, stop_screen_sfx);
            }
        }
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        debug!("Starting actor-only fade out to screen: {target:?}");
        apply_actor_fade_out_transition(
            &mut self.state.shell,
            from,
            target,
            select_color::exit_anim_duration(),
            select_profile::exit_anim_duration(),
        );
        self.sync_gameplay_input_capture();
    }

    fn start_global_fade(&mut self, target: CurrentScreen, stop_screen_sfx: bool) {
        debug!("Starting global fade out to screen: {target:?}");
        if stop_screen_sfx {
            deadsync_audio_stream::stop_screen_sfx();
        }
        let (_, out_duration) =
            self.get_out_transition_for_screen(self.state.screens.current_screen);
        apply_global_fade_out_transition(&mut self.state.shell, target, out_duration);
        self.sync_gameplay_input_capture();
    }

    pub(super) fn handle_process_exit(&mut self, request: ProcessExitRequest) -> Vec<Command> {
        let current = self.state.screens.current_screen;
        let shell = &mut self.state.shell;
        let plan = process_exit_navigation_plan(
            &mut shell.interaction,
            request,
            current,
            &shell.transition,
        );
        info!("{}", plan.log.message());
        match plan.effect {
            ProcessExitNavigationEffect::BeginFade { target } => {
                let (_, out_duration) = self.get_out_transition_for_screen(current);
                apply_global_fade_out_transition(&mut self.state.shell, target, out_duration);
                Vec::new()
            }
            ProcessExitNavigationEffect::Execute(command) => vec![command],
        }
    }

    pub(super) fn on_fade_complete(&mut self, target: CurrentScreen, event_loop: &ActiveEventLoop) {
        let completion_plan = fade_completion_plan(self.state.shell.interaction.exit_intent());
        if let Some(message) = completion_plan.log {
            info!("{message}");
        }
        match completion_plan.effect {
            FadeCompletionEffect::Shutdown => {
                if let Err(e) = deadlib_platform::power::shutdown_host() {
                    log::warn!("host shutdown failed; exiting application only: {e}");
                }
                event_loop.exit();
                return;
            }
            FadeCompletionEffect::Exit => {
                event_loop.exit();
                return;
            }
            FadeCompletionEffect::Continue => {}
        }

        let prev = self.state.screens.current_screen;
        self.commit_screen_change(target);
        if target != CurrentScreen::Gameplay {
            self.state.gameplay_offset_save_prompt = None;
        }
        if target == CurrentScreen::SelectColor {
            select_color::on_enter(&mut self.state.screens.select_color_state);
        }
        if target == CurrentScreen::ArrowCloudLogin {
            self.enter_arrowcloud_login();
        }
        if target == CurrentScreen::GrooveStatsLogin {
            self.enter_groovestats_login();
        }

        let mut commands: Vec<Command> = Vec::new();
        commands.extend(self.handle_audio_and_profile_on_fade(prev, target));
        self.handle_screen_state_on_fade(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let (_, in_duration) = self.get_in_transition_for_screen(target);
        apply_global_entry_transition(&mut self.state.shell, prev, target, in_duration);
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
        let _ = self.run_commands(commands, event_loop);
    }

    pub(super) fn get_out_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => {
                menu::out_transition(self.state.screens.menu_state.active_color_index)
            }
            CurrentScreen::Gameplay => gameplay::out_transition(),
            CurrentScreen::Practice => gameplay::out_transition(),
            CurrentScreen::Options => options::out_transition(),
            CurrentScreen::Credits => credits::out_transition(),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::out_transition(),
            CurrentScreen::Mappings => mappings::out_transition(),
            CurrentScreen::TestLights => test_lights::out_transition(),
            CurrentScreen::OverscanAdjustment => overscan_adjustment::out_transition(),
            CurrentScreen::PlayerOptions => player_options::out_transition(),
            CurrentScreen::SelectProfile => select_profile::out_transition(),
            CurrentScreen::SelectColor => select_color::out_transition(),
            CurrentScreen::ArrowCloudLogin => screens::arrowcloud_login::out_transition(),
            CurrentScreen::GrooveStatsLogin => screens::groovestats_login::out_transition(),
            CurrentScreen::SelectStyle => select_style::out_transition(),
            CurrentScreen::SelectPlayMode => select_mode::out_transition(),
            CurrentScreen::ProfileLoad => profile_load::out_transition(),
            CurrentScreen::SelectMusic => select_music::out_transition(),
            CurrentScreen::SelectCourse => select_course::out_transition(),
            CurrentScreen::Sandbox => sandbox::out_transition(),
            CurrentScreen::Init => init::out_transition(),
            CurrentScreen::Evaluation => evaluation::out_transition(),
            CurrentScreen::EvaluationSummary => evaluation_summary::out_transition(),
            CurrentScreen::Initials => initials::out_transition(),
            CurrentScreen::GameOver => gameover::out_transition(),
            CurrentScreen::Input => input_screen::out_transition(),
            CurrentScreen::ConfigurePads => screens::pad_config::out_transition(),
            CurrentScreen::SmxAssignPads => screens::smx_assign::out_transition(),
        }
    }

    pub(super) fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::in_transition(),
            CurrentScreen::Gameplay => gameplay::in_transition(
                self.state.screens.gameplay_state.as_ref(),
                &self.asset_manager,
                self.state.session.gameplay_restart_count > 0,
            ),
            CurrentScreen::Practice => gameplay::in_transition(
                self.state
                    .screens
                    .practice_state
                    .as_ref()
                    .map(|state| &state.gameplay),
                &self.asset_manager,
                false,
            ),
            CurrentScreen::Options => options::in_transition(),
            CurrentScreen::Credits => credits::in_transition(),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::in_transition(),
            CurrentScreen::Mappings => mappings::in_transition(),
            CurrentScreen::TestLights => test_lights::in_transition(),
            CurrentScreen::OverscanAdjustment => overscan_adjustment::in_transition(),
            CurrentScreen::PlayerOptions => player_options::in_transition(),
            CurrentScreen::SelectProfile => select_profile::in_transition(),
            CurrentScreen::SelectColor => select_color::in_transition(),
            CurrentScreen::ArrowCloudLogin => screens::arrowcloud_login::in_transition(),
            CurrentScreen::GrooveStatsLogin => screens::groovestats_login::in_transition(),
            CurrentScreen::SelectStyle => select_style::in_transition(),
            CurrentScreen::SelectPlayMode => select_mode::in_transition(),
            CurrentScreen::ProfileLoad => profile_load::in_transition(),
            CurrentScreen::SelectMusic => select_music::in_transition(),
            CurrentScreen::SelectCourse => select_course::in_transition(),
            CurrentScreen::Sandbox => sandbox::in_transition(),
            CurrentScreen::Evaluation => evaluation::in_transition(),
            CurrentScreen::EvaluationSummary => evaluation_summary::in_transition(),
            CurrentScreen::Initials => initials::in_transition(),
            CurrentScreen::GameOver => gameover::in_transition(),
            CurrentScreen::Input => input_screen::in_transition(),
            CurrentScreen::ConfigurePads => screens::pad_config::in_transition(),
            CurrentScreen::SmxAssignPads => screens::smx_assign::in_transition(),
            CurrentScreen::Init => (vec![], 0.0),
        }
    }
}
