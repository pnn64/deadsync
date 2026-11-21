//! Game-owned lobby readiness, input gating and stage start coordination.
//!
//! The host only exposes the game state and converts typed lobby actions to
//! application effects. Readiness and stage timing never depend on those effects.

use crate::lobbies::{self, GameplayLobbyWaitStatus, MachinePlayerStats};
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use std::sync::Arc;
use std::time::Instant;

pub const GAMEPLAY_SCREEN_NAME: &str = "ScreenGameplay";

/// A retained online snapshot shared by game logic and its presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct LobbyRuntimeView {
    pub snapshot: Arc<lobbies::Snapshot>,
    pub reconnect_status_text: Option<String>,
    pub disconnect_hold_seconds: f32,
}

impl Default for LobbyRuntimeView {
    fn default() -> Self {
        Self {
            snapshot: Arc::default(),
            reconnect_status_text: None,
            disconnect_hold_seconds: lobbies::LOBBY_DISCONNECT_HOLD_SECONDS,
        }
    }
}

/// Network work emitted in the same order as the gameplay start decisions.
#[derive(Clone, Debug)]
pub enum GameplayLobbyAction {
    Disconnect,
    UpdateMachineStats {
        p1_ready: bool,
        p2_ready: bool,
        p1_stats: Option<MachinePlayerStats>,
        p2_stats: Option<MachinePlayerStats>,
    },
}

/// Statically bound state access and application-effect conversion.
pub trait GameplayLobbyHost: Sized {
    type Profile: GameplayProfileData;
    type OverlayActor;
    type CapturedActor;
    type StateDelta;
    type Effect;

    fn lobby_parts(&mut self) -> (&mut HostGame<Self>, &mut GameplayLobbySession);
    fn lobby_session(&self) -> &GameplayLobbySession;
    fn lobby_effect(action: GameplayLobbyAction) -> Self::Effect;
}

type HostGame<H> = GameplayRuntimeState<
    <H as GameplayLobbyHost>::Profile,
    <H as GameplayLobbyHost>::OverlayActor,
    <H as GameplayLobbyHost>::CapturedActor,
    <H as GameplayLobbyHost>::StateDelta,
>;

/// Per-stage lobby policy with one authoritative retained online snapshot.
///
/// Local sides are captured when the stage is constructed. Live snapshot
/// refreshes never turn an offline stage into a lobby stage, and never pause a
/// stage after its music has started.
#[derive(Debug)]
pub struct GameplayLobbySession {
    runtime_view: LobbyRuntimeView,
    live_runtime: bool,
    joined_sides: [bool; 2],
    player_side: PlayerSide,
    music_started: bool,
    ready: [bool; 2],
    disconnect_holds: [Option<Instant>; 2],
}

impl GameplayLobbySession {
    pub fn new(
        runtime_view: LobbyRuntimeView,
        joined_sides: [bool; 2],
        player_side: PlayerSide,
    ) -> Self {
        Self {
            live_runtime: runtime_view.snapshot.joined_lobby.is_some(),
            runtime_view,
            joined_sides,
            player_side,
            music_started: false,
            ready: [false; 2],
            disconnect_holds: [None; 2],
        }
    }

    #[inline]
    pub const fn runtime_view(&self) -> &LobbyRuntimeView {
        &self.runtime_view
    }

    #[inline]
    pub const fn uses_live_runtime(&self) -> bool {
        self.live_runtime
    }

    #[inline]
    pub const fn music_started(&self) -> bool {
        self.music_started
    }

    pub fn ready_sides(&self) -> [bool; 2] {
        [
            self.local_side_is_active(PlayerSide::P1) && self.ready[0],
            self.local_side_is_active(PlayerSide::P2) && self.ready[1],
        ]
    }

    pub fn local_players_ready(&self) -> bool {
        let ready = self.ready_sides();
        let mut any_active = false;
        let mut all_ready = true;
        for (index, side) in [PlayerSide::P1, PlayerSide::P2].into_iter().enumerate() {
            if self.local_side_is_active(side) {
                any_active = true;
                all_ready &= ready[index];
            }
        }
        any_active && all_ready
    }

    pub fn waiting(&self) -> bool {
        if self.music_started {
            return false;
        }
        let Some(joined) = self.runtime_view.snapshot.joined_lobby.as_ref() else {
            return false;
        };
        self.runtime_view.reconnect_status_text.is_some()
            || lobbies::gameplay_lobby_wait_status(joined, GAMEPLAY_SCREEN_NAME)
                != GameplayLobbyWaitStatus::Ready
    }

    /// The longest active hold, retaining the native real-time clock behavior.
    pub fn disconnect_hold_elapsed(&self) -> Option<f32> {
        self.disconnect_hold_elapsed_with(|started_at| started_at.elapsed().as_secs_f32())
    }

    /// Deterministic counterpart for callers with a captured monotonic time.
    pub fn disconnect_hold_elapsed_at(&self, now: Instant) -> Option<f32> {
        self.disconnect_hold_elapsed_with(|started_at| {
            now.saturating_duration_since(started_at).as_secs_f32()
        })
    }

    fn disconnect_hold_elapsed_with(&self, elapsed: impl FnMut(Instant) -> f32) -> Option<f32> {
        self.disconnect_holds
            .into_iter()
            .flatten()
            .map(elapsed)
            .max_by(f32::total_cmp)
    }

    fn local_side_is_active(&self, side: PlayerSide) -> bool {
        let [p1_joined, p2_joined] = self.joined_sides;
        if !(p1_joined || p2_joined) {
            return self.player_side == side;
        }
        match side {
            PlayerSide::P1 => p1_joined,
            PlayerSide::P2 => p2_joined,
        }
    }

    fn set_all_local_players_ready(&mut self, ready: bool) {
        self.ready = [
            self.local_side_is_active(PlayerSide::P1) && ready,
            self.local_side_is_active(PlayerSide::P2) && ready,
        ];
    }

    fn handle_waiting_input(&mut self, event: &InputEvent) -> bool {
        if !self.waiting() {
            return false;
        }
        let side = match event.action {
            VirtualAction::p1_start => Some(PlayerSide::P1),
            VirtualAction::p2_start => Some(PlayerSide::P2),
            _ => None,
        };
        if let Some(side) = side
            && self.local_side_is_active(side)
        {
            let index = deadsync_profile::player_side_index(side);
            if event.pressed {
                self.ready[index] = true;
                self.disconnect_holds[index] = Some(event.timestamp);
            } else {
                self.disconnect_holds[index] = None;
            }
        }
        true
    }

    fn player_stats<P: GameplayProfileData, O, C, D>(
        &self,
        game: &GameplayRuntimeState<P, O, C, D>,
        side: PlayerSide,
    ) -> Option<MachinePlayerStats> {
        let player = if game.num_players() >= 2 {
            deadsync_profile::player_side_index(side)
        } else if game.num_players() == 0 || self.player_side != side {
            return None;
        } else {
            0
        };
        let blue_window_ms = game.player_blue_window_ms(player);
        let ex_data = game.display_ex_score_data(player, blue_window_ms);
        Some(MachinePlayerStats {
            judgments: Some(lobbies::LobbyJudgments {
                fantastic_plus: ex_data.counts.w0,
                fantastics: ex_data.counts.w1,
                excellents: ex_data.counts.w2,
                greats: ex_data.counts.w3,
                decents: ex_data.counts.w4,
                way_offs: ex_data.counts.w5,
                misses: ex_data.counts.miss,
                total_steps: ex_data.total_steps,
                mines_hit: ex_data.mines_hit,
                total_mines: ex_data.mines_total,
                holds_held: ex_data.holds_held,
                total_holds: ex_data.holds_total,
                rolls_held: ex_data.rolls_held,
                total_rolls: ex_data.rolls_total,
            }),
            // Each participant retains its own score, judgments and EX score.
            score: Some((game.participant_display_itg_score_percent(player) * 100.0) as f32),
            ex_score: Some(game.display_ex_score_percent(player, blue_window_ms) as f32),
        })
    }

    fn machine_state_action<P: GameplayProfileData, O, C, D>(
        &self,
        game: &GameplayRuntimeState<P, O, C, D>,
    ) -> Option<GameplayLobbyAction> {
        if !lobbies::can_update_machine_state(&self.runtime_view.snapshot) {
            return None;
        }
        let [p1_ready, p2_ready] = self.ready_sides();
        Some(GameplayLobbyAction::UpdateMachineStats {
            p1_ready,
            p2_ready,
            p1_stats: self.player_stats(game, PlayerSide::P1),
            p2_stats: self.player_stats(game, PlayerSide::P2),
        })
    }
}

/// Reset readiness on entry and start immediately only for an offline stage.
pub fn on_enter<H: GameplayLobbyHost>(host: &mut H) {
    let (game, lobby) = host.lobby_parts();
    lobby.music_started = false;
    lobby.set_all_local_players_ready(false);
    lobby.disconnect_holds = [None; 2];
    if lobbies::gameplay_lobby_wait_required(lobby.runtime_view.snapshot.joined_lobby.as_ref()) {
        return;
    }
    lobby.set_all_local_players_ready(true);
    game.start_stage_music();
    lobby.music_started = true;
}

/// Prepare a gameplay frame, using real elapsed time only for retained holds.
///
/// Starting music queues an audio command. The shell must drain it before
/// sampling the stream clock for the deterministic gameplay update.
pub fn prepare_update<H: GameplayLobbyHost>(host: &mut H, effects: &mut Vec<H::Effect>) -> bool {
    prepare_update_with_elapsed(host, effects, |started_at| {
        started_at.elapsed().as_secs_f32()
    })
}

/// The same frame boundary with an explicit monotonic time for deterministic use.
pub fn prepare_update_at<H: GameplayLobbyHost>(
    host: &mut H,
    effects: &mut Vec<H::Effect>,
    now: Instant,
) -> bool {
    prepare_update_with_elapsed(host, effects, |started_at| {
        now.saturating_duration_since(started_at).as_secs_f32()
    })
}

fn prepare_update_with_elapsed<H: GameplayLobbyHost>(
    host: &mut H,
    effects: &mut Vec<H::Effect>,
    elapsed: impl FnMut(Instant) -> f32,
) -> bool {
    let (game, lobby) = host.lobby_parts();
    if !lobby.music_started {
        if lobby
            .disconnect_hold_elapsed_with(elapsed)
            .is_some_and(|elapsed| elapsed >= lobby.runtime_view.disconnect_hold_seconds)
        {
            lobby.disconnect_holds = [None; 2];
            effects.push(H::lobby_effect(GameplayLobbyAction::Disconnect));
            lobbies::apply_local_lobby_disconnect(Arc::make_mut(&mut lobby.runtime_view.snapshot));
            lobby.runtime_view.reconnect_status_text = None;
        }

        if let Some(action) = lobby.machine_state_action(game) {
            effects.push(H::lobby_effect(action));
        }
        if lobby.waiting() {
            return false;
        }

        lobby.disconnect_holds = [None; 2];
        lobby.set_all_local_players_ready(true);
        game.start_stage_music();
        lobby.music_started = true;
    }

    if let Some(action) = lobby.machine_state_action(game) {
        effects.push(H::lobby_effect(action));
    }
    true
}

/// Consume all input while waiting, applying Start edges only to active sides.
pub fn handle_input<H: GameplayLobbyHost>(host: &mut H, event: &InputEvent) -> bool {
    host.lobby_parts().1.handle_waiting_input(event)
}

/// Replace the single retained online view without changing the stage's sides.
pub fn sync_runtime_view<H: GameplayLobbyHost>(host: &mut H, view: LobbyRuntimeView) {
    host.lobby_parts().1.runtime_view = view;
}

/// Offline stages skip snapshot/reconnect polling for their entire lifetime.
pub fn uses_live_runtime<H: GameplayLobbyHost>(host: &H) -> bool {
    host.lobby_session().uses_live_runtime()
}

#[cfg(test)]
#[path = "gameplay_lobby_tests.rs"]
mod tests;
