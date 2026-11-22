use super::App;
use crate::gameplay_runtime::{self, GameplayFrameEffect, GameplayFrameEffects};
use crate::navigation::gameplay_exit_screen;
use crate::{ActiveTheme, TransitionState};
use deadsync_gameplay::exit_route::{GameplayExitRouteContext, resolve_gameplay_exit_route};
use deadsync_gameplay::{GameplayAction, GameplayExit};
use deadsync_online::gameplay_lobby::{GAMEPLAY_SCREEN_NAME, GameplayLobbyAction};
use deadsync_online::lobbies;
use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;
use deadsync_theme::ThemeScreenFlow;
use log::warn;
use winit::event_loop::ActiveEventLoop;

const MAX_TRANSITION_LOBBY_EFFECTS: usize = 3;

fn retain_lobby_effects(effects: &mut GameplayFrameEffects) {
    effects.retain(|effect| matches!(effect, GameplayFrameEffect::Lobby(_)));
    debug_assert!(effects.len() <= MAX_TRANSITION_LOBBY_EFFECTS);
}

impl App {
    pub(super) fn step_gameplay(
        &mut self,
        delta_time: f32,
        lobby_only: bool,
        event_loop: &ActiveEventLoop,
    ) {
        let Some(gameplay) = self.state.screens.gameplay_state.as_mut() else {
            return;
        };
        let mut effects = gameplay_runtime::update(
            gameplay,
            delta_time,
            self.state.play_input_policy.smx_input,
            &mut self.audio,
            &self.gameplay_sfx,
            &mut self.music_clock,
            &mut self.state.screens.gameplay_score_cursor,
        );
        if lobby_only {
            retain_lobby_effects(&mut effects);
        }
        for effect in effects {
            match effect {
                GameplayFrameEffect::Lobby(GameplayLobbyAction::Disconnect) => {
                    lobbies::runtime_disconnect();
                }
                GameplayFrameEffect::Lobby(GameplayLobbyAction::UpdateMachineStats {
                    p1_ready,
                    p2_ready,
                    p1_stats,
                    p2_stats,
                }) => {
                    lobbies::runtime_update_machine_state_sides_with_stats_default(
                        GAMEPLAY_SCREEN_NAME,
                        p1_ready,
                        p2_ready,
                        p1_stats,
                        p2_stats,
                    );
                }
                GameplayFrameEffect::Session(SongLuaGameplayEffect::Action(action)) => {
                    self.handle_gameplay_action(action, event_loop);
                }
                GameplayFrameEffect::Session(SongLuaGameplayEffect::ReloadSongLua) => {
                    self.handle_song_lua_reload(event_loop);
                }
                GameplayFrameEffect::Session(SongLuaGameplayEffect::Navigate(screen)) => {
                    self.handle_immediate_navigation(screen, event_loop);
                }
            }
        }
    }

    pub(super) fn handle_gameplay_action(
        &mut self,
        action: GameplayAction,
        event_loop: &ActiveEventLoop,
    ) {
        match action {
            GameplayAction::None => return,
            GameplayAction::Restart => {
                self.try_gameplay_restart(event_loop, "missed target");
                return;
            }
            GameplayAction::Navigate(_) | GameplayAction::NavigateNoFade(_) => {}
        }
        let from_gameplay = self.state.screens.current_screen == ActiveTheme::SCREENS.gameplay;
        let course = self.state.session.course_run.as_ref();
        let all_joined_players_failed =
            matches!(action, GameplayAction::Navigate(GameplayExit::Complete))
                && from_gameplay
                && self.current_gameplay_stage_failed();
        let route = resolve_gameplay_exit_route(
            action,
            GameplayExitRouteContext {
                from_gameplay,
                restart_pending: self.state.session.restart_pending,
                course_active: course.is_some(),
                course_has_next_stage: course.is_some_and(|course| course.run.has_next_stage()),
                all_joined_players_failed,
            },
        )
        .expect("navigation action must have a gameplay exit route");
        if route.consume_restart_pending {
            self.state.session.restart_pending = false;
        }
        let target = gameplay_exit_screen::<ActiveTheme>(route.destination);
        if route.immediate {
            self.handle_immediate_navigation(target, event_loop);
        } else {
            self.handle_navigation_action(target);
        }
    }

    pub(super) fn handle_song_lua_reload(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.screens.current_screen != ActiveTheme::SCREENS.gameplay
            || self.pending_gameplay_init.is_some()
            || !self
                .state
                .screens
                .gameplay_state
                .as_ref()
                .is_some_and(|state| state.song_lua.is_some())
            || !self.prepare_player_options_for_gameplay_restart()
        {
            warn!("Cannot reload song Lua without an active gameplay screen");
            return;
        }
        self.pending_song_lua_reload =
            self.state
                .screens
                .gameplay_state
                .as_mut()
                .and_then(|state| {
                    state
                        .song_lua
                        .take()
                        .map(|scene| (scene, state.take_song_lua_held_inputs()))
                });
        self.state.shell.transition = TransitionState::Idle;
        self.state.session.restart_pending = false;
        self.state.gameplay_offset_save_prompt = None;
        self.on_fade_complete(ActiveTheme::SCREENS.gameplay, event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_gameplay_retains_bounded_lobby_order() {
        let mut effects = GameplayFrameEffects::new();
        effects.extend([
            GameplayFrameEffect::Session(SongLuaGameplayEffect::ReloadSongLua),
            GameplayFrameEffect::Lobby(GameplayLobbyAction::Disconnect),
            GameplayFrameEffect::Lobby(GameplayLobbyAction::UpdateMachineStats {
                p1_ready: false,
                p2_ready: false,
                p1_stats: None,
                p2_stats: None,
            }),
            GameplayFrameEffect::Session(SongLuaGameplayEffect::Action(GameplayAction::Navigate(
                GameplayExit::Complete,
            ))),
            GameplayFrameEffect::Lobby(GameplayLobbyAction::UpdateMachineStats {
                p1_ready: true,
                p2_ready: true,
                p1_stats: None,
                p2_stats: None,
            }),
        ]);

        retain_lobby_effects(&mut effects);

        assert!(matches!(
            effects.as_slice(),
            [
                GameplayFrameEffect::Lobby(GameplayLobbyAction::Disconnect),
                GameplayFrameEffect::Lobby(GameplayLobbyAction::UpdateMachineStats {
                    p1_ready: false,
                    ..
                }),
                GameplayFrameEffect::Lobby(GameplayLobbyAction::UpdateMachineStats {
                    p1_ready: true,
                    ..
                }),
            ]
        ));
        assert!(!effects.spilled());
    }
}
