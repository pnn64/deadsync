use super::{
    App, Command, CurrentScreen, credits, evaluation, evaluation_summary, gameover, gameplay, init,
    initials, input_screen, manage_local_profiles, mappings, menu, options, overscan_adjustment,
    player_options, profile_load, sandbox, select_color, select_course, select_mode, select_music,
    select_profile, select_style, test_lights,
};
use crate::assets::visual_styles;
use crate::config;
use deadlib_present::actors::Actor;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_shell::{
    TransitionMusicAction, TransitionState, actor_entry_transition, actor_fade_out_transition,
    global_entry_transition, global_fade_out_transition, is_actor_only_transition,
    machine_flow_screen, menu_exit_uses_fade, screen_from_machine_flow, transition_music_action,
    write_current_screen_file,
};
use log::{debug, info};
use winit::event_loop::ActiveEventLoop;

impl App {
    #[inline(always)]
    pub(super) fn commit_screen_change(&mut self, target: CurrentScreen) {
        let prev = self.state.screens.current_screen;
        if prev != target && target == CurrentScreen::Menu {
            deadsync_online::lobbies::runtime_leave_lobby_default();
        }
        // Leaving gameplay by ANY path (bail-out to the song wheel, fail, give up,
        // course abort, results, etc.) must stop SMX sensor test mode, or it keeps
        // streaming on later screens. This is the one chokepoint every transition
        // routes through, so handle it here rather than per exit path. on_exit is
        // idempotent; skip gameplay->gameplay (restart / next course stage) since
        // on_enter re-establishes the mode anyway.
        if prev == CurrentScreen::Gameplay
            && target != CurrentScreen::Gameplay
            && let Some(gs) = self.state.screens.gameplay_state.as_mut()
        {
            crate::screens::gameplay::on_exit(gs);
        }
        self.state.screens.current_screen = target;
        self.sync_gameplay_input_capture();
        write_current_screen_file(target);
        if prev != target {
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
            self.state.screens.arrowcloud_login_state.active_color_index =
                self.state.screens.menu_state.active_color_index;
            crate::screens::arrowcloud_login::on_enter(
                &mut self.state.screens.arrowcloud_login_state,
            );
        }
        if target_screen == CurrentScreen::GrooveStatsLogin {
            self.state
                .screens
                .groovestats_login_state
                .active_color_index = self.state.screens.menu_state.active_color_index;
            crate::screens::groovestats_login::on_enter(
                &mut self.state.screens.groovestats_login_state,
            );
        }

        match transition_music_action(
            prev,
            target_screen,
            config::get().menu_music,
            visual_styles::srpg10_active(),
        ) {
            TransitionMusicAction::Keep => {}
            TransitionMusicAction::PlayMenu => {
                deadsync_audio_stream::play_music(
                    visual_styles::menu_music_resolved_path(),
                    deadsync_audio_stream::Cut::default(),
                    true,
                    1.0,
                );
            }
            TransitionMusicAction::PlayGameOver => {
                deadsync_audio_stream::play_music(
                    visual_styles::srpg10_gameover_music_path(),
                    deadsync_audio_stream::Cut::default(),
                    false,
                    1.0,
                );
            }
            TransitionMusicAction::Stop => deadsync_audio_stream::stop_music(),
        }

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
            crate::screens::pad_config::set_return_screen(pad_state, CurrentScreen::Options);
            crate::screens::pad_config::set_filter(
                pad_state,
                crate::screens::pad_config::PadFilter::All,
            );
            crate::screens::pad_config::reset_modes(pad_state);
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
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
            }
        } else if target_screen == CurrentScreen::SelectStyle {
            let current_color_index = self.state.screens.select_style_state.active_color_index;
            self.state.screens.select_style_state = select_style::init();
            self.state.screens.select_style_state.active_color_index = current_color_index;
            let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
            let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
            self.state.screens.select_style_state.selected_index =
                if p1_joined && p2_joined { 1 } else { 0 };
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
            self.state.screens.smx_assign_state = crate::screens::smx_assign::init();
            self.state.screens.smx_assign_state.active_color_index = color_index;
            crate::screens::smx_assign::on_enter(&mut self.state.screens.smx_assign_state);
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

        self.state.shell.transition = actor_entry_transition(target_screen);
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
        let mut target = target;
        let cfg = config::get();
        self.lights.clear_button_pressed();

        if (from == CurrentScreen::SelectMusic || from == CurrentScreen::SelectCourse)
            && target == CurrentScreen::Menu
            && !self.state.session.played_stages.is_empty()
        {
            target = screen_from_machine_flow(config::machine_first_post_select_target(&cfg));
            self.state.session.pending_post_select_summary_exit =
                target == CurrentScreen::EvaluationSummary;
        } else if target == CurrentScreen::EvaluationSummary {
            self.state.session.pending_post_select_summary_exit = false;
        }

        let startup_flow = matches!(
            from,
            CurrentScreen::Menu
                | CurrentScreen::SelectProfile
                | CurrentScreen::SelectColor
                | CurrentScreen::SelectStyle
                | CurrentScreen::SelectPlayMode
        ) && matches!(
            target,
            CurrentScreen::SelectProfile
                | CurrentScreen::SelectColor
                | CurrentScreen::SelectStyle
                | CurrentScreen::SelectPlayMode
                | CurrentScreen::ProfileLoad
        );
        if startup_flow {
            if let Some(route) = machine_flow_screen(target) {
                target =
                    screen_from_machine_flow(config::machine_resolve_startup_target(&cfg, route));
            }
        }
        if let Some(route) = machine_flow_screen(target) {
            target =
                screen_from_machine_flow(config::machine_resolve_post_select_target(&cfg, route));
        }
        if startup_flow {
            if !cfg.machine_show_select_style
                && matches!(
                    target,
                    CurrentScreen::SelectPlayMode | CurrentScreen::ProfileLoad
                )
            {
                let play_style =
                    profile_data::play_style_from_machine_preference(cfg.machine_preferred_style);
                profile::set_session_play_style(play_style);
                self.state.session.preferred_difficulty_index =
                    profile::get().last_played(play_style).difficulty_index;
            }
            if !cfg.machine_show_select_play_mode && target == CurrentScreen::ProfileLoad {
                profile::set_session_play_mode(profile_data::play_mode_from_machine_preference(
                    cfg.machine_preferred_play_mode,
                ));
            }
        }

        if startup_flow
            && from == CurrentScreen::Menu
            && target != CurrentScreen::SelectProfile
            && !cfg.machine_show_select_profile
            && matches!(
                target,
                CurrentScreen::SelectColor
                    | CurrentScreen::SelectStyle
                    | CurrentScreen::SelectPlayMode
                    | CurrentScreen::ProfileLoad
            )
        {
            let p2_started = self.state.screens.menu_state.started_by_p2;
            profile::set_session_player_side(if p2_started {
                profile_data::PlayerSide::P2
            } else {
                profile_data::PlayerSide::P1
            });
            profile::set_session_joined(!p2_started, p2_started);
            profile::set_fast_profile_switch_from_select_music(false);
        }

        if allow_offset_prompt && self.maybe_begin_gameplay_offset_prompt(from, target, false) {
            return;
        }

        if from == CurrentScreen::Init && target == CurrentScreen::Menu {
            debug!("Instant navigation Init→Menu (out-transition handled by Init screen)");
            self.commit_screen_change(target);
            self.state.shell.transition = actor_entry_transition(target);
            deadlib_present::runtime::clear_all();
            return;
        }

        if !matches!(self.state.shell.transition, TransitionState::Idle) {
            return;
        }

        self.state.shell.interaction.clear_exit_intent();
        if is_actor_only_transition(from, target) {
            self.start_actor_fade(from, target);
        } else {
            self.start_global_fade(target);
        }
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        debug!("Starting actor-only fade out to screen: {target:?}");
        self.state.shell.transition = actor_fade_out_transition(
            from,
            target,
            select_color::exit_anim_duration(),
            select_profile::exit_anim_duration(),
        );
        self.sync_gameplay_input_capture();
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        debug!("Starting global fade out to screen: {target:?}");
        if self.state.screens.current_screen == CurrentScreen::Evaluation
            && target != CurrentScreen::Evaluation
        {
            deadsync_audio_stream::stop_screen_sfx();
        }
        let (_, out_duration) =
            self.get_out_transition_for_screen(self.state.screens.current_screen);
        self.state.shell.transition = global_fade_out_transition(target, out_duration);
        self.sync_gameplay_input_capture();
    }

    pub(super) fn handle_exit_action(&mut self) -> Vec<Command> {
        if menu_exit_uses_fade(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) {
            info!("Exit requested from Menu; playing menu out-transition before shutdown.");
            let (_, out_duration) =
                self.get_out_transition_for_screen(self.state.screens.current_screen);
            self.state.shell.transition =
                global_fade_out_transition(self.state.screens.current_screen, out_duration);
            self.state.shell.interaction.request_exit();
            Vec::new()
        } else {
            info!("Exit action received. Shutting down.");
            vec![Command::ExitNow]
        }
    }

    pub(super) fn handle_shutdown_action(&mut self) -> Vec<Command> {
        if menu_exit_uses_fade(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) {
            info!("Host-shutdown requested from Menu; playing out-transition first.");
            let (_, out_duration) =
                self.get_out_transition_for_screen(self.state.screens.current_screen);
            self.state.shell.transition =
                global_fade_out_transition(self.state.screens.current_screen, out_duration);
            self.state.shell.interaction.request_shutdown();
            Vec::new()
        } else {
            info!("Host-shutdown action received. Powering off.");
            vec![Command::Shutdown]
        }
    }

    pub(super) fn on_fade_complete(&mut self, target: CurrentScreen, event_loop: &ActiveEventLoop) {
        if self.state.shell.interaction.exit_intent() == super::ExitIntent::Shutdown {
            info!("Fade-out complete; powering off host and exiting.");
            if let Err(e) = deadlib_platform::power::shutdown_host() {
                log::warn!("host shutdown failed; exiting application only: {e}");
            }
            event_loop.exit();
            return;
        }
        if self.state.shell.interaction.exit_intent() == super::ExitIntent::Exit {
            info!("Fade-out complete; exiting application.");
            event_loop.exit();
            return;
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
            self.state.screens.arrowcloud_login_state.active_color_index =
                self.state.screens.menu_state.active_color_index;
            crate::screens::arrowcloud_login::on_enter(
                &mut self.state.screens.arrowcloud_login_state,
            );
        }
        if target == CurrentScreen::GrooveStatsLogin {
            self.state
                .screens
                .groovestats_login_state
                .active_color_index = self.state.screens.menu_state.active_color_index;
            crate::screens::groovestats_login::on_enter(
                &mut self.state.screens.groovestats_login_state,
            );
        }

        let mut commands: Vec<Command> = Vec::new();
        commands.extend(self.handle_audio_and_profile_on_fade(prev, target));
        self.handle_screen_state_on_fade(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let (_, in_duration) = self.get_in_transition_for_screen(target);
        self.state.shell.transition = global_entry_transition(prev, target, in_duration);
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
            CurrentScreen::ArrowCloudLogin => crate::screens::arrowcloud_login::out_transition(),
            CurrentScreen::GrooveStatsLogin => crate::screens::groovestats_login::out_transition(),
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
            CurrentScreen::ConfigurePads => crate::screens::pad_config::out_transition(),
            CurrentScreen::SmxAssignPads => crate::screens::smx_assign::out_transition(),
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
            CurrentScreen::ArrowCloudLogin => crate::screens::arrowcloud_login::in_transition(),
            CurrentScreen::GrooveStatsLogin => crate::screens::groovestats_login::in_transition(),
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
            CurrentScreen::ConfigurePads => crate::screens::pad_config::in_transition(),
            CurrentScreen::SmxAssignPads => crate::screens::smx_assign::in_transition(),
            CurrentScreen::Init => (vec![], 0.0),
        }
    }
}
