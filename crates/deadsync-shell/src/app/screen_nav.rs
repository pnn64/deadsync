use super::stage_start::{StageSelection, stage_launch_target};
use super::{App, Command, CurrentScreen, select_color, select_course};
use crate::ActiveTheme;
use crate::interaction::ProcessExitRequest;
use crate::navigation::{
    FadeCompletionEffect, NavigationTransitionStart, ProcessExitNavigationEffect,
    apply_actor_entry_transition, apply_actor_fade_out_transition, apply_global_entry_transition,
    apply_global_fade_out_transition, course_plan_required, fade_completion_plan,
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
use deadsync_theme::evaluation::ThemeEvaluation;
use deadsync_theme::{TransitionKind, TransitionState};
use deadsync_theme_simply_love::views::OverscanAdjustmentView;
use deadsync_theme::app_requests::QrLoginService;
use deadsync_theme_simply_love::screens;
use log::{debug, info, warn};
use winit::event_loop::ActiveEventLoop;

pub(super) fn overscan_view() -> OverscanAdjustmentView {
    let config = config::runtime::get();
    OverscanAdjustmentView {
        translate_x: config.center_image_translate_x,
        translate_y: config.center_image_translate_y,
        add_width: config.center_image_add_width,
        add_height: config.center_image_add_height,
    }
}

pub(super) fn stage_motion_view(
    state: &super::GameplayState,
    transition: &TransitionState<CurrentScreen>,
    restarting: bool,
) -> deadsync_theme::stage::StageMotionView {
    use deadsync_gameplay::ExitTransitionKind;
    use deadsync_theme::stage::StageMotionView;
    use deadsync_theme::views::GameplayHudMotion;

    let chart_outro = state.stage.song_lua.out_transition().map(|out| out.elapsed_seconds());
    let give_up_outro = if chart_outro.is_none() {
        state.exit_prompt_state().exit_transition
            .filter(|out| out.kind == ExitTransitionKind::Out)
            .map(|out| out.started_at.elapsed().as_secs_f32())
    } else {
        None
    };
    let transition = match *transition {
        TransitionState::FadingOut { elapsed, target: CurrentScreen::Evaluation, .. } => {
            GameplayHudMotion::Leaving { elapsed_seconds: elapsed }
        }
        TransitionState::FadingIn { elapsed, .. } => GameplayHudMotion::Entering { elapsed_seconds: elapsed },
        _ => GameplayHudMotion::Settled,
    };
    StageMotionView {
        chart_outro, give_up_outro, transition, restarting,
        hide_intro: state.presentation.hide_song_intro,
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
        if previous != CurrentScreen::Gameplay
            || target != CurrentScreen::Gameplay
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
        CurrentScreen::Evaluation
    }

    pub(super) fn reject_missing_course_plan(
        &mut self,
        previous: CurrentScreen,
        target: CurrentScreen,
    ) -> bool {
        if !course_plan_required(previous, target) {
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
        let color_index = self.state.screens.menu_state.color_index();
        let state = &mut self.state.screens.arrowcloud_login_state;
        state.active_color_index = color_index;
        let target = state
            .target_profile
            .as_ref()
            .map(|target| (target.id.clone(), target.display_name.clone()));
        let request = crate::qr_login::request(QrLoginService::ArrowCloud, target);
        screens::arrowcloud_login::on_enter(
            state,
            request,
            config.three_key_navigation && config.only_dedicated_menu_buttons,
        );
    }

    fn enter_groovestats_login(&mut self) {
        let config = config::runtime::get();
        let color_index = self.state.screens.menu_state.color_index();
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
        let request = crate::qr_login::request(QrLoginService::GrooveStats, target);
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
        self.state.screens.gameplay_outro = None;
        if prev == CurrentScreen::Evaluation && target != prev {
            self.state.screens.evaluation_departure = None;
            self.state.screens.stage_presentation.leave_evaluation();
        }
        if (prev == CurrentScreen::SelectMusic || prev == CurrentScreen::SelectCourse)
            && target != prev
            && !stage_launch_target(target)
                .is_some_and(|target| self.state.screens.stage_launch.ready_for(target))
        {
            self.clear_stage_launch();
        }
        if !target.is_play() {
            self.state.pending_stage = None;
        }
        let plan = screen_change_plan(prev, target);
        if let Some(rate) = practice_music_rate_on_screen_change(
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
            if matches!(prev, CurrentScreen::SelectMusic | CurrentScreen::SelectCourse)
                && let Some(backend) = self.backend.as_mut()
            {
                self.dynamic_media.expire_outgoing_banners(&mut self.asset_manager, backend, None);
            }
            if matches!(
                target,
                CurrentScreen::SelectMusic | CurrentScreen::SelectCourse
            ) && let Some(selection) = self.state.screens.wheel_mut(target)
            {
                selection.enter_wheel(super::Instant::now());
            }
        }
        self.state.screens.current_screen = target;
        self.sync_gameplay_input_capture();
        write_current_screen_file(
            target.current_screen_file_name(),
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
            let state = self.state.screens.select_color_mut();
            select_color::sync_runtime_view(state, crate::select_flow::runtime_view());
            select_color::on_enter(state);
        }
        if target_screen == CurrentScreen::ArrowCloudLogin {
            self.enter_arrowcloud_login();
        }
        if target_screen == CurrentScreen::GrooveStatsLogin {
            self.enter_groovestats_login();
        }

        self.prepare_screen_state(prev, target_screen);

        let config = config::runtime::get();
        let music = (self.state.screens.transitions.music)(
            prev,
            target_screen,
            TransitionKind::Actors,
            &config,
        );
        if let Some(command) = transition_music_command(music) {
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

        self.apply_selected_actor_entry(target_screen);
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
    }

    pub(super) fn handle_navigation_action(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, true);
    }

    pub(super) fn leave_evaluation(&mut self) {
        let screens = &mut self.state.screens;
        if screens.current_screen != CurrentScreen::Evaluation
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            return;
        }
        let view = ActiveTheme::evaluation_capture_view(&screens.evaluation_state);
        let departure = if view.overview && screens.evaluation_overview.is_some() {
            screens.stage_presentation.evaluation_departure()
        } else {
            None
        };
        if departure.as_ref().is_some_and(|exit| view.elapsed < exit.input_ready_seconds) {
            return;
        }
        let origin = screens.evaluation_origin;
        let cue = departure.as_ref().map_or_else(
            || ActiveTheme::evaluation_departure_cue(origin),
            |exit| exit.cue,
        );
        screens.evaluation_departure = departure;
        self.handle_navigation_action(evaluation_return_to(origin));
        if let Some(cue) = cue {
            self.ui_sfx.play_cue(&mut self.audio, cue);
        }
    }

    pub(super) fn handle_immediate_navigation(
        &mut self,
        target: CurrentScreen,
        event_loop: &ActiveEventLoop,
    ) {
        if target == CurrentScreen::SelectColor && self.state.screens.select_color_state.is_none() {
            self.handle_navigation_action(target);
            return;
        }
        let current = self.state.screens.current_screen;
        if target == CurrentScreen::Gameplay
            && current == CurrentScreen::SelectMusic
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
        let policy = crate::screen_flow::navigation_policy(&cfg)
            .intersection(self.state.screens.session_screen_support);
        let plan = navigation_route_plan(
            policy,
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

        let transition_plan = navigation_transition_effect_plan(
            from,
            target,
            &self.state.shell.transition,
            self.state.screens.transitions.kind,
        );
        if transition_plan.clear_exit_intent {
            self.state.shell.interaction.clear_exit_intent();
        }
        match transition_plan.start {
            NavigationTransitionStart::DirectEntry { target } => {
                if from == CurrentScreen::Init && target == CurrentScreen::Menu {
                    debug!("Instant navigation Init?Menu (out-transition handled by Init screen)");
                    self.state.session.menu_start_side = profile_data::PlayerSide::P1;
                    self.state.screens.menu_state.reset();
                }
                self.commit_screen_change(target);
                self.apply_selected_actor_entry(target);
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
        if from == CurrentScreen::Menu {
            self.state.shell.transition = self.state.screens.menu_state.actor_exit(target);
        } else if let Some(duration) = self.options_transition_seconds(from) {
            self.state.shell.transition = TransitionState::ActorsFadeOut {
                elapsed: 0.0,
                duration,
                target,
            };
        } else {
            let duration = (self.state.screens.transitions.actor_exit_seconds)(from);
            apply_actor_fade_out_transition(&mut self.state.shell, target, duration);
        }
        if let Some(sound) = (self.state.screens.transitions.exit_sound)(from, target) {
            self.ui_sfx.play(&mut self.audio, sound);
        }
        self.sync_gameplay_input_capture();
    }

    fn apply_selected_actor_entry(&mut self, target: CurrentScreen) {
        if target == CurrentScreen::Menu {
            self.state.shell.transition = self.state.screens.menu_state.actor_entry();
        } else if let Some(duration) = self.options_transition_seconds(target) {
            self.state.shell.transition = TransitionState::ActorsFadeIn {
                elapsed: 0.0,
                duration,
            };
        } else {
            let duration = (self.state.screens.transitions.actor_entry_seconds)(target);
            apply_actor_entry_transition(&mut self.state.shell, duration);
        }
        if let Some(sound) = (self.state.screens.transitions.entry_sound)(target) {
            self.ui_sfx.play(&mut self.audio, sound);
        }
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        debug!("Starting global fade out to screen: {target:?}");
        let current = self.state.screens.current_screen;
        if !current.is_play()
            && (self.state.screens.transitions.stop_screen_sfx_on_exit)(current, target)
        {
            self.audio.stop_screen_sfx();
        }
        let (_, out_duration) = self.get_out_transition_for_route(current, target);
        let screens = &self.state.screens;
        let outro = if current == CurrentScreen::Gameplay && target == CurrentScreen::Evaluation {
            screens.gameplay_state.as_ref().and_then(|stage| {
                screens.stage_presentation.gameplay_outro(deadsync_theme::stage::StageOutroView {
                    failed: stage.all_joined_players_failed(),
                    simfile: &stage.song().simfile_path,
                    authored_transition: stage.stage.song_lua.is_some(),
                })
            })
        } else {
            None
        };
        let out_duration = outro.as_ref().map_or(out_duration, |outro| {
            if let Some(cue) = outro.cue() {
                self.ui_sfx.play_cue(&mut self.audio, cue);
            }
            let duration = out_duration.max(outro.duration());
            debug!("Theme gameplay outro for {duration:.2} s");
            duration
        });
        self.state.screens.gameplay_outro = outro;
        if let Some(sound) = (self.state.screens.transitions.exit_sound)(current, target) {
            self.ui_sfx.play(&mut self.audio, sound);
        }
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
        let skip_entry_fade = (self.state.screens.transitions.skip_entry_fade)(prev, target);
        apply_global_entry_transition(&mut self.state.shell, target, in_duration, skip_entry_fade);
        self.sync_gameplay_input_capture();
        deadlib_present::runtime::clear_all();
        self.run_commands(commands, event_loop);
        if let Some(sound) = (self.state.screens.transitions.entry_sound)(target) {
            self.ui_sfx.play(&mut self.audio, sound);
        }
    }

    /// Application state changes for a completed global fade. The event-loop
    /// wrapper only applies window effects, fade timing and returned commands.
    pub(super) fn prepare_global_fade_entry(
        &mut self,
        requested: CurrentScreen,
    ) -> (CurrentScreen, Vec<Command>) {
        let prev = self.state.screens.current_screen;
        let target = self.resolve_course_stage_transition(prev, requested);
        if prev == CurrentScreen::SelectMusic && target != CurrentScreen::Gameplay {
            self.gameplay_preload = None;
        }
        self.commit_screen_change(target);
        if target != CurrentScreen::Gameplay {
            self.state.gameplay_offset_save_prompt = None;
        }
        if target == CurrentScreen::SelectColor {
            let state = self.state.screens.select_color_mut();
            select_color::sync_runtime_view(state, crate::select_flow::runtime_view());
            select_color::on_enter(state);
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

    fn options_transition_seconds(&self, screen: CurrentScreen) -> Option<f32> {
        (screen == CurrentScreen::Options && self.state.screens.system_options_presentation.is_some())
            .then(|| self.state.screens.options_state.page_transition_seconds())
    }

    pub(super) fn get_out_transition_for_route(
        &self,
        screen: CurrentScreen,
        target: CurrentScreen,
    ) -> (Vec<Actor>, f32) {
        if screen == CurrentScreen::Menu {
            return self.state.screens.menu_state.exit_transition(target);
        }
        if let Some(duration) = self.options_transition_seconds(screen) {
            return (Vec::new(), duration);
        }
        if target == CurrentScreen::PlayerOptions
            && matches!(
                screen,
                CurrentScreen::SelectMusic | CurrentScreen::SelectCourse
            )
            && let Some(duration) = self
                .state
                .screens
                .wheel(screen)
                .and_then(|selection| selection.player_options_exit_seconds())
        {
            return (Vec::new(), duration);
        }
        if screen == CurrentScreen::PlayerOptions
            && let Some(options) = self.state.screens.player_options_state.as_ref()
            && let Some(transition) = options.exit_transition(target)
        {
            return transition;
        }
        if screen == CurrentScreen::Evaluation
            && let Some(exit) = &self.state.screens.evaluation_departure
        {
            return (Vec::new(), exit.duration);
        }
        let (actors, duration) = (self.state.screens.transitions.exit)(screen, target);
        let duration = stage_exit_duration(screen).unwrap_or(duration);
        (actors, duration)
    }

    pub(super) fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        if screen == CurrentScreen::Menu {
            return self.state.screens.menu_state.entry_transition();
        }
        if let Some(duration) = self.options_transition_seconds(screen) {
            return (Vec::new(), duration);
        }
        if screen == CurrentScreen::PlayerOptions
            && let Some(options) = self.state.screens.player_options_state.as_ref()
            && let Some(transition) = options.entry_transition()
        {
            return transition;
        }
        let play = if screen == CurrentScreen::Gameplay {
            Some((
                self.state.screens.gameplay_state.as_ref(),
                self.state.session.gameplay_restart_count > 0,
            ))
        } else if screen == CurrentScreen::Practice {
            Some((
                self.state.screens.practice_state.as_ref().map(|state| &state.gameplay),
                false,
            ))
        } else {
            None
        };
        let restarting = play.is_some_and(|(_, restart)| restart);
        let duration = stage_entry_duration(screen, restarting);
        if screen == CurrentScreen::Gameplay
            && let Some(actors) = self.state.screens.stage_presentation.gameplay_entry(
                restarting,
                play.is_some_and(|(gameplay, _)| gameplay.is_some()),
            )
        {
            let duration = self
                .state
                .screens
                .stage_presentation
                .gameplay_entry_seconds(restarting)
                .or(duration)
                .expect("gameplay entry uses the stage clock");
            return (actors, duration);
        }
        let (actors, visual_duration) = if let Some((gameplay, restart)) = play {
            (self.gameplay_presentation.entry)(
                gameplay, &self.asset_manager, restart, &config::runtime::get(),
            )
        } else {
            (self.state.screens.transitions.entry)(screen)
        };
        (actors, duration.unwrap_or(visual_duration))
    }
}
