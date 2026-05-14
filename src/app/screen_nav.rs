use super::{
    App, Command, CurrentScreen, FADE_OUT_DURATION, MENU_TO_SELECT_COLOR_OUT_DURATION, credits,
    evaluation, evaluation_summary, gameover, gameplay, init, initials, input_screen,
    manage_local_profiles, mappings, menu, options, player_options, profile_load, sandbox,
    select_color, select_course, select_mode, select_music, select_profile, select_style,
    test_lights,
};
use crate::assets::visual_styles;
use crate::config;
use crate::config::dirs;
use crate::engine::present::actors::Actor;
use crate::game::profile;
use log::{debug, info};
use winit::event_loop::ActiveEventLoop;

/* -------------------- transition state machine -------------------- */
#[derive(Debug)]
pub(super) enum TransitionState {
    Idle,
    FadingOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    FadingIn {
        elapsed: f32,
        duration: f32,
    },
    ActorsFadeOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    ActorsFadeIn {
        elapsed: f32,
    },
}

#[inline(always)]
const fn machine_startup_screen_enabled(cfg: &config::Config, screen: CurrentScreen) -> bool {
    match screen {
        CurrentScreen::SelectProfile => cfg.machine_show_select_profile,
        CurrentScreen::SelectColor => cfg.machine_show_select_color,
        CurrentScreen::SelectStyle => cfg.machine_show_select_style,
        CurrentScreen::SelectPlayMode => cfg.machine_show_select_play_mode,
        _ => true,
    }
}

#[inline(always)]
const fn machine_preferred_style(style: config::MachinePreferredPlayStyle) -> profile::PlayStyle {
    match style {
        config::MachinePreferredPlayStyle::Single => profile::PlayStyle::Single,
        config::MachinePreferredPlayStyle::Versus => profile::PlayStyle::Versus,
        config::MachinePreferredPlayStyle::Double => profile::PlayStyle::Double,
    }
}

#[inline(always)]
const fn machine_preferred_mode(mode: config::MachinePreferredPlayMode) -> profile::PlayMode {
    match mode {
        config::MachinePreferredPlayMode::Regular => profile::PlayMode::Regular,
        config::MachinePreferredPlayMode::Marathon => profile::PlayMode::Marathon,
    }
}

fn machine_resolve_startup_target(cfg: &config::Config, target: CurrentScreen) -> CurrentScreen {
    let order = [
        CurrentScreen::SelectProfile,
        CurrentScreen::SelectColor,
        CurrentScreen::SelectStyle,
        CurrentScreen::SelectPlayMode,
    ];
    let Some(start_idx) = order.iter().position(|screen| *screen == target) else {
        return target;
    };
    for screen in order.iter().skip(start_idx) {
        if machine_startup_screen_enabled(cfg, *screen) {
            return *screen;
        }
    }
    CurrentScreen::ProfileLoad
}

#[inline(always)]
fn machine_first_post_select_target(cfg: &config::Config) -> CurrentScreen {
    if cfg.machine_show_eval_summary {
        CurrentScreen::EvaluationSummary
    } else if cfg.machine_show_name_entry {
        CurrentScreen::Initials
    } else if cfg.machine_show_gameover {
        CurrentScreen::GameOver
    } else {
        CurrentScreen::Menu
    }
}

#[inline(always)]
fn machine_resolve_post_select_target(
    cfg: &config::Config,
    target: CurrentScreen,
) -> CurrentScreen {
    match target {
        CurrentScreen::EvaluationSummary => {
            if cfg.machine_show_eval_summary {
                CurrentScreen::EvaluationSummary
            } else if cfg.machine_show_name_entry {
                CurrentScreen::Initials
            } else if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        CurrentScreen::Initials => {
            if cfg.machine_show_name_entry {
                CurrentScreen::Initials
            } else if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        CurrentScreen::GameOver => {
            if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        other => other,
    }
}

impl App {
    #[inline(always)]
    pub(super) const fn is_actor_fade_screen(screen: CurrentScreen) -> bool {
        matches!(
            screen,
            CurrentScreen::Menu
                | CurrentScreen::Options
                | CurrentScreen::ManageLocalProfiles
                | CurrentScreen::Mappings
                | CurrentScreen::Input
                | CurrentScreen::TestLights
        )
    }

    #[inline(always)]
    pub(super) fn commit_screen_change(&mut self, target: CurrentScreen) {
        let prev = self.state.screens.current_screen;
        self.state.screens.current_screen = target;
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

        let menu_music_enabled = config::get().menu_music;
        let target_menu_music = menu_music_enabled
            && matches!(
                target_screen,
                CurrentScreen::SelectColor | CurrentScreen::SelectStyle
            );
        let prev_menu_music = menu_music_enabled
            && matches!(
                prev,
                CurrentScreen::SelectColor | CurrentScreen::SelectStyle
            );
        let keep_preview = (prev == CurrentScreen::SelectMusic
            && target_screen == CurrentScreen::PlayerOptions)
            || (prev == CurrentScreen::PlayerOptions
                && target_screen == CurrentScreen::SelectMusic);

        if target_menu_music {
            if !prev_menu_music {
                crate::engine::audio::play_music(
                    visual_styles::menu_music_resolved_path(),
                    crate::engine::audio::Cut::default(),
                    true,
                    1.0,
                );
            }
        } else if prev_menu_music {
            crate::engine::audio::stop_music();
        } else if !keep_preview {
            crate::engine::audio::stop_music();
        }

        if target_screen == CurrentScreen::Menu {
            let current_color_index = self.state.screens.menu_state.active_color_index;
            self.state.screens.menu_state = menu::init();
            self.state.screens.menu_state.active_color_index = current_color_index;
        } else if target_screen == CurrentScreen::Options {
            self.reset_options_state_for_entry(prev);
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
            let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
            let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
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

        self.state.shell.transition = if Self::is_actor_fade_screen(target_screen) {
            TransitionState::ActorsFadeIn { elapsed: 0.0 }
        } else {
            TransitionState::Idle
        };
        crate::engine::present::runtime::clear_all();
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
            target = machine_first_post_select_target(&cfg);
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
            target = machine_resolve_startup_target(&cfg, target);
        }
        target = machine_resolve_post_select_target(&cfg, target);
        if startup_flow {
            if !cfg.machine_show_select_style
                && matches!(
                    target,
                    CurrentScreen::SelectPlayMode | CurrentScreen::ProfileLoad
                )
            {
                let play_style = machine_preferred_style(cfg.machine_preferred_style);
                profile::set_session_play_style(play_style);
                self.state.session.preferred_difficulty_index =
                    profile::get().last_played(play_style).difficulty_index;
            }
            if !cfg.machine_show_select_play_mode && target == CurrentScreen::ProfileLoad {
                profile::set_session_play_mode(machine_preferred_mode(
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
                profile::PlayerSide::P2
            } else {
                profile::PlayerSide::P1
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
            self.state.shell.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
            crate::engine::present::runtime::clear_all();
            return;
        }

        if !matches!(self.state.shell.transition, TransitionState::Idle) {
            return;
        }

        self.state.shell.pending_exit = false;
        if self.is_actor_only_fade(from, target) {
            self.start_actor_fade(from, target);
        } else {
            self.start_global_fade(target);
        }
    }

    fn is_actor_only_fade(&self, from: CurrentScreen, to: CurrentScreen) -> bool {
        (from == CurrentScreen::Menu
            && (to == CurrentScreen::Options
                || to == CurrentScreen::SelectProfile
                || to == CurrentScreen::SelectColor))
            || ((from == CurrentScreen::Options
                || from == CurrentScreen::SelectProfile
                || from == CurrentScreen::SelectColor)
                && to == CurrentScreen::Menu)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectProfile)
            || (from == CurrentScreen::SelectColor && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::Options && to == CurrentScreen::Mappings)
            || (from == CurrentScreen::Mappings && to == CurrentScreen::Options)
            || (from == CurrentScreen::Options && to == CurrentScreen::TestLights)
            || (from == CurrentScreen::TestLights && to == CurrentScreen::Options)
            || (from == CurrentScreen::Options && to == CurrentScreen::ManageLocalProfiles)
            || (from == CurrentScreen::ManageLocalProfiles && to == CurrentScreen::Options)
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        debug!("Starting actor-only fade out to screen: {target:?}");
        let duration = if from == CurrentScreen::Menu
            && (target == CurrentScreen::SelectProfile
                || target == CurrentScreen::SelectColor
                || target == CurrentScreen::Options)
        {
            MENU_TO_SELECT_COLOR_OUT_DURATION
        } else if from == CurrentScreen::SelectColor {
            select_color::exit_anim_duration()
        } else if from == CurrentScreen::SelectProfile {
            select_profile::exit_anim_duration()
        } else {
            FADE_OUT_DURATION
        };
        self.state.shell.transition = TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration,
            target,
        };
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        debug!("Starting global fade out to screen: {target:?}");
        let (_, out_duration) =
            self.get_out_transition_for_screen(self.state.screens.current_screen);
        self.state.shell.transition = TransitionState::FadingOut {
            elapsed: 0.0,
            duration: out_duration,
            target,
        };
    }

    pub(super) fn handle_exit_action(&mut self) -> Vec<Command> {
        if self.state.screens.current_screen == CurrentScreen::Menu
            && matches!(self.state.shell.transition, TransitionState::Idle)
        {
            info!("Exit requested from Menu; playing menu out-transition before shutdown.");
            let (_, out_duration) =
                self.get_out_transition_for_screen(self.state.screens.current_screen);
            self.state.shell.transition = TransitionState::FadingOut {
                elapsed: 0.0,
                duration: out_duration,
                target: self.state.screens.current_screen,
            };
            self.state.shell.pending_exit = true;
            Vec::new()
        } else {
            info!("Exit action received. Shutting down.");
            vec![Command::ExitNow]
        }
    }

    pub(super) fn on_fade_complete(&mut self, target: CurrentScreen, event_loop: &ActiveEventLoop) {
        if self.state.shell.pending_exit {
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

        let mut commands: Vec<Command> = Vec::new();
        commands.extend(self.handle_audio_and_profile_on_fade(prev, target));
        self.handle_screen_state_on_fade(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let instant_options_credits_swap = matches!(
            (prev, target),
            (CurrentScreen::Options, CurrentScreen::Credits)
                | (CurrentScreen::Credits, CurrentScreen::Options)
        );
        if instant_options_credits_swap {
            self.state.shell.transition = TransitionState::Idle;
        } else {
            let (_, in_duration) = self.get_in_transition_for_screen(target);
            self.state.shell.transition = TransitionState::FadingIn {
                elapsed: 0.0,
                duration: in_duration,
            };
        }
        crate::engine::present::runtime::clear_all();
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
            CurrentScreen::PlayerOptions => player_options::out_transition(),
            CurrentScreen::SelectProfile => select_profile::out_transition(),
            CurrentScreen::SelectColor => select_color::out_transition(),
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
        }
    }

    pub(super) fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::in_transition(),
            CurrentScreen::Gameplay => gameplay::in_transition(
                self.state.screens.gameplay_state.as_ref(),
                &self.asset_manager,
            ),
            CurrentScreen::Practice => gameplay::in_transition(
                self.state
                    .screens
                    .practice_state
                    .as_ref()
                    .map(|state| &state.gameplay),
                &self.asset_manager,
            ),
            CurrentScreen::Options => options::in_transition(),
            CurrentScreen::Credits => credits::in_transition(),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::in_transition(),
            CurrentScreen::Mappings => mappings::in_transition(),
            CurrentScreen::TestLights => test_lights::in_transition(),
            CurrentScreen::PlayerOptions => player_options::in_transition(),
            CurrentScreen::SelectProfile => select_profile::in_transition(),
            CurrentScreen::SelectColor => select_color::in_transition(),
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
            CurrentScreen::Init => (vec![], 0.0),
        }
    }
}

pub(super) fn write_current_screen_file(screen: CurrentScreen) {
    if !config::get().write_current_screen {
        return;
    }
    let path = dirs::app_dirs().current_screen_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("Failed to create current_screen.txt parent dir: {e}");
            return;
        }
    }
    let name = screen.current_screen_file_name();
    if let Err(e) = std::fs::write(&path, name) {
        log::warn!("Failed to write current_screen.txt: {e}");
    }
}
