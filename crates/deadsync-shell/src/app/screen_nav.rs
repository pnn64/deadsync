use super::stage_start::{StageSelection, stage_launch_target};
use super::{App, Command, CurrentScreen, select_color, select_course};
use crate::ActiveTheme;
use crate::interaction::ProcessExitRequest;
use crate::navigation::{
    FadeCompletionEffect, NavigationTransitionStart, ProcessExitNavigationEffect,
    apply_actor_entry_transition, apply_actor_fade_out_transition, apply_global_entry_transition,
    apply_global_fade_out_transition, course_plan_required, fade_completion_plan, is_play_screen,
    navigation_transition_effect_plan, practice_music_rate_on_screen_change, process_exit_navigation_plan,
    screen_change_plan, stage_entry_duration, stage_exit_duration, write_current_screen_file,
};
use crate::screen_flow::{
    evaluation_return_to, navigation_route_plan, preferred_play_style_for_joined,
};
use crate::transition_effects::transition_music_command;
use deadlib_present::actors::Actor;
use deadsync_config as config;
use deadsync_gameplay::course_progress::CourseRenewal;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_theme::{
    Theme, ThemeScreenFlow, ThemeTransitionAudio, ThemeTransitions, TransitionKind,
};
use deadsync_theme::main_menu::ThemeMainMenu;
use deadsync_theme_simply_love::screens::transition_audio::TransitionAudioContext;
use deadsync_theme_simply_love::screens::transitions::PlayEntryTransition;
use deadsync_theme_simply_love::views::OverscanAdjustmentView;
use deadsync_theme_simply_love::{SimplyLoveQrLoginService, screens};
use log::{debug, info, warn};
use winit::event_loop::ActiveEventLoop;

pub(super) fn transition_audio_context(
    config: &config::app_config::Config,
) -> TransitionAudioContext {
    TransitionAudioContext {
        menu_music_enabled: config.menu_music,
        style: config.visual_style,
        variant: config.srpg_variant,
    }
}

pub(super) fn overscan_view() -> OverscanAdjustmentView {
    let config = config::runtime::get();
    OverscanAdjustmentView {
        translate_x: config.center_image_translate_x,
        translate_y: config.center_image_translate_y,
        add_width: config.center_image_add_width,
        add_height: config.center_image_add_height,
    }
}

impl App {
    /// Resolve a failed Endless renewal before committing a screen change.
    /// Replaying by modulo here would reuse an old cycle without its totals.
    fn resolve_course_stage_transition(
        &mut self,
        previous: CurrentScreen,
        target: CurrentScreen,
    ) -> CurrentScreen {
        if previous != ActiveTheme::SCREENS.gameplay
            || target != ActiveTheme::SCREENS.gameplay
            || self.pending_song_lua_reload.is_some()
        {
            return target;
        }
        let Some(course) = self.state.session.course_run.as_mut() else {
            return target;
        };
        let renewed = match course.run.renewal() {
            CourseRenewal::AppendCycle => {
                super::renew_endless_cycle(course, profile::get_session_play_style().chart_type())
            }
            // Skipping unprepared history cannot repair an invalid cursor.
            CourseRenewal::InvalidCursor => false,
            CourseRenewal::NotRequired => return target,
        };
        if renewed {
            return target;
        }
        course.run.progress.interrupt();
        warn!("Unable to renew Endless course; finishing with the recorded course result.");
        self.state.shell.interaction.show_message(
            "Unable to prepare next course stage".to_string(),
            std::time::Instant::now(),
        );
        self.state.screens.player_options_state = None;
        self.state.pending_stage = None;
        self.pending_gameplay_init = None;
        // The normal evaluation lifecycle records the finished song, saves the
        // accumulated course score and retains its per-song evaluation pages.
        ActiveTheme::SCREENS.results
    }

    pub(super) fn reject_missing_course_plan(
        &mut self,
        previous: CurrentScreen,
        target: CurrentScreen,
    ) -> bool {
        if !course_plan_required::<ActiveTheme>(previous, target) {
            return false;
        }
        let stage_launch = &self.state.screens.stage_launch;
        let confirmed = stage_launch_target(target).and_then(|target| stage_launch.ready_selection(target));
        let has_plan = match confirmed {
            Some(StageSelection::Course(selection)) => selection.can_start(),
            _ => select_course::has_selected_course(&self.state.screens.select_course_state),
        };
        if has_plan {
            return false;
        }
        warn!("Unable to enter {target:?}: selected course has no playable stages.");
        self.clear_stage_launch();
        self.state.session.course_run = None;
        self.state.screens.player_options_state = None;
        self.state.pending_stage = None;
        self.state.shell.transition = crate::TransitionState::Idle;
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
        true
    }

    fn enter_arrowcloud_login(&mut self) {
        let config = config::runtime::get();
        let color_index = ActiveTheme::menu_color_index(&self.state.screens.menu_state);
        let state = &mut self.state.screens.arrowcloud_login_state;
        state.active_color_index = color_index;
        let target = state
            .target_profile
            .as_ref()
            .map(|target| (target.id.clone(), target.display_name.clone()));
        let request = crate::qr_login::request(SimplyLoveQrLoginService::ArrowCloud, target);
        screens::arrowcloud_login::on_enter(
            state,
            request,
            config.three_key_navigation && config.only_dedicated_menu_buttons,
        );
    }

    fn enter_groovestats_login(&mut self) {
        let config = config::runtime::get();
        let color_index = ActiveTheme::menu_color_index(&self.state.screens.menu_state);
        let state = &mut self.state.screens.groovestats_login_state;
        state.active_color_index = color_index;
        let target = state
            .target_profile
            .as_ref()
            .map(|target| (target.id.clone(), target.display_name.clone()));
        let show_arrowcloud_next = target.is_none()
            && crate::qr_login::should_auto_show_arrowcloud(
                config.enable_arrowcloud,
                config.arrowcloud_qr_login_when,
            );
        let request = crate::qr_login::request(SimplyLoveQrLoginService::GrooveStats, target);
        screens::groovestats_login::on_enter(
            state,
            request,
            show_arrowcloud_next,
            config.three_key_navigation && config.only_dedicated_menu_buttons,
        );
    }

    #[inline(always)]
    pub(super) fn commit_screen_change(&mut self, target: CurrentScreen) {
        let prev = self.state.screens.current_screen;
        if (prev == ActiveTheme::SCREENS.song_selection || prev == ActiveTheme::SCREENS.course_selection)
            && target != prev
            && !stage_launch_target(target)
                .is_some_and(|target| self.state.screens.stage_launch.ready_for(target))
        {
            self.clear_stage_launch();
        }
        if !is_play_screen::<ActiveTheme>(target) {
            self.state.pending_stage = None;
        }
        let plan = screen_change_plan::<ActiveTheme>(prev, target);
        if let Some(rate) = practice_music_rate_on_screen_change::<ActiveTheme>(
            &mut self.state.session.pre_practice_music_rate,
            target,
            profile::get_session_snapshot().music_rate,
        ) {
            profile::set_session_music_rate(rate);
            self.audio.set_music_rate(rate);
        }
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
        if plan.exit_gameplay {
            self.audio.stop_mine_sfx();
            if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                crate::gameplay_runtime::exit(gs, &mut self.state.screens.gameplay_telemetry);
            }
        }
        if prev != target {
            self.mark_screen_entry_runtime_dirty(target);
        }
        self.state.screens.current_screen = target;
        self.sync_gameplay_input_capture();
        write_current_screen_file(
            ActiveTheme::current_screen_file_name(target),
            &self.dirs.current_screen_path(),
        );
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
        if self.reject_missing_course_plan(prev, target_screen) {
            return;
        }
        self.commit_screen_change(target_screen);
        if target_screen == CurrentScreen::SelectColor {
            select_color::sync_runtime_view(
                &mut self.state.screens.select_color_state,
                crate::select_flow::runtime_view(),
            );
            select_color::on_enter(&mut self.state.screens.select_color_state);
        }
        if target_screen == CurrentScreen::ArrowCloudLogin {
            self.enter_arrowcloud_login();
        }
        if target_screen == CurrentScreen::GrooveStatsLogin {
            self.enter_groovestats_login();
        }

        self.prepare_screen_state(prev, target_screen);

        let config = config::runtime::get();
        let audio = ActiveTheme::transition_audio(
            prev,
            target_screen,
            TransitionKind::Actors,
            transition_audio_context(&config),
        );
        if let Some(command) = transition_music_command(audio.music) {
            self.execute_command(command, event_loop);
        }

        if target_screen == CurrentScreen::ConfigurePads {
            // The full screen is reached only from Options (Song Select uses an
            // in-place overlay instead): return there and show all pads.
            let pad_state = &mut self.state.screens.pad_config_state;
            screens::pad_config::set_return_screen(pad_state, CurrentScreen::Options);
            screens::pad_config::set_filter(pad_state, screens::pad_config::PadFilter::All);
            screens::pad_config::reset_modes(pad_state);
        }

        if target_screen == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        apply_actor_entry_transition(&mut self.state.shell, target_screen);
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
    }

    pub(super) fn handle_navigation_action(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, true);
    }

    pub(super) fn leave_evaluation(&mut self) {
        if self.state.screens.current_screen == ActiveTheme::SCREENS.results {
            let target = evaluation_return_to::<ActiveTheme>(self.state.screens.evaluation_origin);
            self.handle_navigation_action(target);
        }
    }

    pub(super) fn handle_immediate_navigation(
        &mut self,
        target: CurrentScreen,
        event_loop: &ActiveEventLoop,
    ) {
        let current = self.state.screens.current_screen;
        if target == ActiveTheme::SCREENS.gameplay
            && current == ActiveTheme::SCREENS.song_selection
            && self.defer_gameplay_entry(event_loop)
        {
            return;
        }
        if self.maybe_begin_gameplay_offset_prompt(current, target, true) {
            return;
        }
        // Skip the outgoing transition and let the destination play its intro.
        if matches!(self.state.shell.transition, crate::TransitionState::Idle) {
            self.on_fade_complete(target, event_loop);
        }
    }

    pub(super) fn handle_navigation_action_after_prompt(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, false);
    }

    fn handle_navigation_action_inner(&mut self, target: CurrentScreen, allow_offset_prompt: bool) {
        let from = self.state.screens.current_screen;
        let cfg = config::runtime::get();
        self.lights.clear_button_pressed();
        let plan = navigation_route_plan::<ActiveTheme>(
            &cfg,
            from,
            target,
            !self.state.session.played_stages.is_empty(),
        );
        let target = plan.target;
        if let Some(pending) = plan.pending_post_select_summary_exit {
            self.state.session.pending_post_select_summary_exit = pending;
        }
        if plan.initialize_session_side {
            let side = self.state.session.menu_start_side;
            let p2_started = side == profile_data::PlayerSide::P2;
            profile::set_session_player_side(side);
            profile::set_session_joined(!p2_started, p2_started);
        }
        if plan.apply_preferred_style {
            let session = profile::get_session_snapshot();
            // Confirmed players take precedence over the machine's solo default.
            let play_style = preferred_play_style_for_joined(
                &cfg,
                [
                    session.side_joined(profile_data::PlayerSide::P1),
                    session.side_joined(profile_data::PlayerSide::P2),
                ],
            );
            profile::set_session_play_style(play_style);
            self.state.session.preferred_difficulty_index =
                profile::get().last_played(play_style).difficulty_index;
        }
        if plan.apply_preferred_play_mode {
            profile::set_session_play_mode(profile_data::play_mode_from_machine_preference(
                cfg.machine_preferred_play_mode,
            ));
        }
        if allow_offset_prompt && self.maybe_begin_gameplay_offset_prompt(from, target, false) {
            return;
        }

        let transition_plan = navigation_transition_effect_plan::<ActiveTheme>(
            from,
            target,
            &self.state.shell.transition,
        );
        if transition_plan.clear_exit_intent {
            self.state.shell.interaction.clear_exit_intent();
        }
        match transition_plan.start {
            NavigationTransitionStart::DirectEntry { target } => {
                if from == CurrentScreen::Init && target == CurrentScreen::Menu {
                    debug!("Instant navigation Init?Menu (out-transition handled by Init screen)");
                    self.state.session.menu_start_side = profile_data::PlayerSide::P1;
                    ActiveTheme::reset_menu(&mut self.state.screens.menu_state);
                }
                self.commit_screen_change(target);
                apply_actor_entry_transition(&mut self.state.shell, target);
                self.sync_gameplay_input_capture();
                deadlib_present::runtime::clear_all();
            }
            NavigationTransitionStart::Busy => {}
            NavigationTransitionStart::ActorFade { from, target } => {
                self.start_actor_fade(from, target);
            }
            NavigationTransitionStart::GlobalFade { target } => {
                self.start_global_fade(target);
            }
        }
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        debug!("Starting actor-only fade out to screen: {target:?}");
        apply_actor_fade_out_transition(&mut self.state.shell, from, target);
        self.sync_gameplay_input_capture();
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        debug!("Starting global fade out to screen: {target:?}");
        let current = self.state.screens.current_screen;
        if !is_play_screen::<ActiveTheme>(current)
            && ActiveTheme::stop_screen_sfx_on_exit(current, target)
        {
            self.audio.stop_screen_sfx();
        }
        let (_, out_duration) = self.get_out_transition_for_route(current, target);
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
                let (_, out_duration) = self.get_out_transition_for_route(current, target);
                apply_global_fade_out_transition(&mut self.state.shell, target, out_duration);
                self.sync_gameplay_input_capture();
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
        if self.reject_missing_course_plan(prev, target) {
            return;
        }
        let (target, commands) = self.prepare_global_fade_entry(target);

        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let (_, in_duration) = self.get_in_transition_for_screen(target);
        apply_global_entry_transition(&mut self.state.shell, prev, target, in_duration);
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
        self.run_commands(commands, event_loop);
    }

    /// Application state changes for a completed global fade. The event-loop
    /// wrapper only applies window effects, fade timing and returned commands.
    pub(super) fn prepare_global_fade_entry(
        &mut self,
        requested: CurrentScreen,
    ) -> (CurrentScreen, Vec<Command>) {
        let prev = self.state.screens.current_screen;
        let target = self.resolve_course_stage_transition(prev, requested);
        if prev == ActiveTheme::SCREENS.song_selection && target != ActiveTheme::SCREENS.gameplay {
            self.gameplay_preload = None;
        }
        self.commit_screen_change(target);
        if target != ActiveTheme::SCREENS.gameplay {
            self.state.gameplay_offset_save_prompt = None;
        }
        if target == CurrentScreen::SelectColor {
            select_color::sync_runtime_view(
                &mut self.state.screens.select_color_state,
                crate::select_flow::runtime_view(),
            );
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
        self.prepare_screen_state(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        // Loading the requested screen can recover to course evaluation (or
        // Player Options). Apply the fade and input policy to that screen.
        (self.state.screens.current_screen, commands)
    }

    pub(super) fn get_out_transition_for_route(
        &self,
        screen: CurrentScreen,
        target: CurrentScreen,
    ) -> (Vec<Actor>, f32) {
        let (actors, duration) = ActiveTheme::out_transition(screen, target);
        let duration = stage_exit_duration::<ActiveTheme>(screen).unwrap_or(duration);
        (actors, duration)
    }

    pub(super) fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        let play = if screen == ActiveTheme::SCREENS.gameplay {
            Some((
                self.state.screens.gameplay_state.as_ref(),
                self.state.session.gameplay_restart_count > 0,
            ))
        } else if screen == ActiveTheme::SCREENS.practice {
            Some((
                self.state.screens.practice_state.as_ref().map(|state| &state.gameplay),
                false,
            ))
        } else {
            None
        };
        let context = play.map(|(gameplay, restart)| PlayEntryTransition {
            gameplay,
            assets: &self.asset_manager,
            restart,
            policy: super::simply_love_visual_policy(&config::runtime::get()),
        });
        let (actors, duration) = ActiveTheme::in_transition(screen, context);
        let restarting = play.is_some_and(|(_, restart)| restart);
        let duration = stage_entry_duration::<ActiveTheme>(screen, restarting).unwrap_or(duration);
        (actors, duration)
    }
}
