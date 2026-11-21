//! Game-owned lobby waiting, input gating and held-Start disconnect policy.
//!
//! Evaluation and music selection share retained input state while preserving
//! their different disconnect effects. Presentations consume typed wait reasons
//! and ordered actions; localized text never decides whether input is accepted.

use crate::gameplay_lobby::LobbyRuntimeView;
use crate::lobbies::{self, JoinedLobby, MachinePlayerStats};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use deadsync_rules::timing::WindowCounts;
use std::sync::Arc;
use std::time::Instant;

pub const EVALUATION_SCREEN_NAME: &str = "ScreenEvaluationStage";
pub const SELECT_MUSIC_SCREEN_NAME: &str = "ScreenSelectMusic";
const GAMEPLAY_SCREEN_NAME: &str = "ScreenGameplay";

/// Current local session facts. These screens may change joined sides while open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalLobbyPlayers {
    pub joined: [bool; 2],
    pub player_side: PlayerSide,
}

impl LocalLobbyPlayers {
    /// A session without joined profiles still represents its selected local side.
    #[must_use]
    pub fn count(self) -> usize {
        let count = self.joined.into_iter().filter(|joined| *joined).count();
        if count == 0 { 1 } else { count }
    }

    fn side_is_active(self, side: PlayerSide) -> bool {
        let [p1_joined, p2_joined] = self.joined;
        if !(p1_joined || p2_joined) {
            return self.player_side == side;
        }
        match side {
            PlayerSide::P1 => p1_joined,
            PlayerSide::P2 => p2_joined,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyWaitReason {
    Reconnecting,
    WaitingForGameplay,
    WaitingForEvaluation,
    WaitingForSync,
}

/// Evaluation remains locked until every remote gameplay screen has finished.
#[must_use]
pub fn evaluation_wait_reason(
    joined: &JoinedLobby,
    local_player_count: usize,
    reconnecting: bool,
) -> Option<LobbyWaitReason> {
    if joined.players.len() <= local_player_count {
        return None;
    }
    if reconnecting {
        return Some(LobbyWaitReason::Reconnecting);
    }
    joined
        .players
        .iter()
        .any(|player| {
            player
                .screen_name
                .eq_ignore_ascii_case(GAMEPLAY_SCREEN_NAME)
        })
        .then_some(LobbyWaitReason::WaitingForGameplay)
}

/// A selected song releases music selection once nobody remains in evaluation.
#[must_use]
pub fn select_music_wait_reason(
    joined: &JoinedLobby,
    local_player_count: usize,
    reconnecting: bool,
) -> Option<LobbyWaitReason> {
    if joined.players.len() <= local_player_count {
        return None;
    }
    if reconnecting {
        return Some(LobbyWaitReason::Reconnecting);
    }
    let any_in_gameplay = joined.players.iter().any(|player| {
        player
            .screen_name
            .eq_ignore_ascii_case(GAMEPLAY_SCREEN_NAME)
    });
    let any_in_evaluation = joined.players.iter().any(|player| {
        player
            .screen_name
            .eq_ignore_ascii_case(EVALUATION_SCREEN_NAME)
    });
    let all_in_select_music = joined.players.iter().all(|player| {
        player
            .screen_name
            .eq_ignore_ascii_case(SELECT_MUSIC_SCREEN_NAME)
    });
    if any_in_evaluation {
        return Some(LobbyWaitReason::WaitingForEvaluation);
    }
    // Pre-song radar counters may already contain mines or scores while paused.
    // Only the selected song and screen barriers govern this transition.
    if joined.song_info.is_some() {
        return None;
    }
    if any_in_gameplay {
        return Some(LobbyWaitReason::WaitingForGameplay);
    }
    if all_in_select_music {
        return None;
    }
    Some(LobbyWaitReason::WaitingForSync)
}

/// Final game facts projected from a result without copying its presentation.
#[derive(Clone, Copy, Debug, Default)]
pub struct EvaluationPlayerStatsFacts {
    pub window_counts: WindowCounts,
    pub total_steps: u32,
    pub mines_total: u32,
    pub mines_avoided: u32,
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub score_percent: f64,
    pub ex_score_percent: f64,
}

impl EvaluationPlayerStatsFacts {
    #[must_use]
    pub fn machine_stats(self) -> MachinePlayerStats {
        MachinePlayerStats {
            judgments: Some(lobbies::LobbyJudgments {
                fantastic_plus: self.window_counts.w0,
                fantastics: self.window_counts.w1,
                excellents: self.window_counts.w2,
                greats: self.window_counts.w3,
                decents: self.window_counts.w4,
                way_offs: self.window_counts.w5,
                misses: self.window_counts.miss,
                total_steps: self.total_steps,
                mines_hit: self.mines_total.saturating_sub(self.mines_avoided),
                total_mines: self.mines_total,
                holds_held: self.holds_held,
                total_holds: self.holds_total,
                rolls_held: self.rolls_held,
                total_rolls: self.rolls_total,
            }),
            score: Some((self.score_percent * 100.0) as f32),
            ex_score: Some(self.ex_score_percent as f32),
        }
    }
}

/// Delivered immediately in publication order, without an intermediate queue.
#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationLobbyAction {
    UpdateMachineStats {
        p1_ready: bool,
        p2_ready: bool,
        p1_stats: Option<MachinePlayerStats>,
        p2_stats: Option<MachinePlayerStats>,
    },
    Disconnect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectMusicLobbyUpdate {
    pub waiting: bool,
    pub disconnect_requested: bool,
}

#[derive(Clone, Debug, Default)]
struct LobbyWaitState {
    runtime_view: LobbyRuntimeView,
    disconnect_holds: [Option<Instant>; 2],
}

impl LobbyWaitState {
    fn new(runtime_view: LobbyRuntimeView) -> Self {
        Self {
            runtime_view,
            disconnect_holds: [None; 2],
        }
    }

    fn wait_reason(
        &self,
        local: LocalLobbyPlayers,
        rule: impl FnOnce(&JoinedLobby, usize, bool) -> Option<LobbyWaitReason>,
    ) -> Option<LobbyWaitReason> {
        let joined = self.runtime_view.snapshot.joined_lobby.as_ref()?;
        rule(
            joined,
            local.count(),
            self.runtime_view.reconnect_status_text.is_some(),
        )
    }

    fn handle_input(
        &mut self,
        local: LocalLobbyPlayers,
        event: &InputEvent,
        waiting: bool,
    ) -> bool {
        if !waiting {
            return false;
        }
        let side = match event.action {
            VirtualAction::p1_start => Some(PlayerSide::P1),
            VirtualAction::p2_start => Some(PlayerSide::P2),
            _ => None,
        };
        if let Some(side) = side
            && local.side_is_active(side)
        {
            let index = deadsync_profile::player_side_index(side);
            // Every press replaces the timestamp, including repeated Start
            // events; releases only clear holds for currently active sides.
            self.disconnect_holds[index] = event.pressed.then_some(event.timestamp);
        }
        true
    }

    fn disconnect_hold_elapsed_with(&self, elapsed: impl FnMut(Instant) -> f32) -> Option<f32> {
        self.disconnect_holds
            .into_iter()
            .flatten()
            .map(elapsed)
            .max_by(f32::total_cmp)
    }

    fn update_disconnect(&mut self, waiting: bool, elapsed: impl FnMut(Instant) -> f32) -> bool {
        if !waiting {
            self.disconnect_holds = [None; 2];
            return false;
        }
        if self
            .disconnect_hold_elapsed_with(elapsed)
            .is_some_and(|held| held >= self.runtime_view.disconnect_hold_seconds)
        {
            self.disconnect_holds = [None; 2];
            return true;
        }
        false
    }
}

/// Evaluation's sole retained lobby view and disconnect input state.
#[derive(Clone, Debug, Default)]
pub struct EvaluationLobbySession {
    state: LobbyWaitState,
}

impl EvaluationLobbySession {
    #[must_use]
    pub fn new(runtime_view: LobbyRuntimeView) -> Self {
        Self {
            state: LobbyWaitState::new(runtime_view),
        }
    }

    #[inline]
    #[must_use]
    pub const fn runtime_view(&self) -> &LobbyRuntimeView {
        &self.state.runtime_view
    }

    /// Snapshot refresh does not discard inputs held across a live update.
    pub fn sync_runtime_view(&mut self, runtime_view: LobbyRuntimeView) {
        self.state.runtime_view = runtime_view;
    }

    #[must_use]
    pub fn wait_reason(&self, local: LocalLobbyPlayers) -> Option<LobbyWaitReason> {
        self.state.wait_reason(local, evaluation_wait_reason)
    }

    /// All locked logical input is consumed; only active Start edges alter holds.
    pub fn handle_input(&mut self, local: LocalLobbyPlayers, event: &InputEvent) -> bool {
        let waiting = self.wait_reason(local).is_some();
        self.state.handle_input(local, event, waiting)
    }

    /// Sample the original real-time clock lazily, once for each existing hold.
    #[must_use]
    pub fn disconnect_hold_elapsed(&self) -> Option<f32> {
        self.state
            .disconnect_hold_elapsed_with(|started_at| started_at.elapsed().as_secs_f32())
    }

    #[must_use]
    pub fn disconnect_hold_elapsed_at(&self, now: Instant) -> Option<f32> {
        self.state.disconnect_hold_elapsed_with(|started_at| {
            now.saturating_duration_since(started_at).as_secs_f32()
        })
    }

    pub fn update(
        &mut self,
        local: LocalLobbyPlayers,
        player_facts: impl FnMut(PlayerSide) -> Option<EvaluationPlayerStatsFacts>,
        emit: impl FnMut(EvaluationLobbyAction),
    ) {
        self.update_with(local, player_facts, emit, |started_at| {
            started_at.elapsed().as_secs_f32()
        });
    }

    pub fn update_at(
        &mut self,
        local: LocalLobbyPlayers,
        now: Instant,
        player_facts: impl FnMut(PlayerSide) -> Option<EvaluationPlayerStatsFacts>,
        emit: impl FnMut(EvaluationLobbyAction),
    ) {
        self.update_with(local, player_facts, emit, |started_at| {
            now.saturating_duration_since(started_at).as_secs_f32()
        });
    }

    fn update_with(
        &mut self,
        local: LocalLobbyPlayers,
        mut player_facts: impl FnMut(PlayerSide) -> Option<EvaluationPlayerStatsFacts>,
        mut emit: impl FnMut(EvaluationLobbyAction),
        elapsed: impl FnMut(Instant) -> f32,
    ) {
        // Preserve stats generation and publication before checking disconnect.
        // Disconnected and unjoined frames do not request any result facts.
        if lobbies::can_update_machine_state(&self.state.runtime_view.snapshot) {
            let p1_stats =
                player_facts(PlayerSide::P1).map(EvaluationPlayerStatsFacts::machine_stats);
            let p2_stats =
                player_facts(PlayerSide::P2).map(EvaluationPlayerStatsFacts::machine_stats);
            emit(EvaluationLobbyAction::UpdateMachineStats {
                p1_ready: true,
                p2_ready: true,
                p1_stats,
                p2_stats,
            });
        }
        let waiting = self.wait_reason(local).is_some();
        if self.state.update_disconnect(waiting, elapsed) {
            emit(EvaluationLobbyAction::Disconnect);
            lobbies::apply_local_lobby_disconnect(Arc::make_mut(
                &mut self.state.runtime_view.snapshot,
            ));
            self.state.runtime_view.reconnect_status_text = None;
        }
    }
}

/// Music selection retains the server snapshot after requesting disconnection.
#[derive(Clone, Debug, Default)]
pub struct SelectMusicLobbySession {
    state: LobbyWaitState,
}

impl SelectMusicLobbySession {
    #[must_use]
    pub fn new(runtime_view: LobbyRuntimeView) -> Self {
        Self {
            state: LobbyWaitState::new(runtime_view),
        }
    }

    #[inline]
    #[must_use]
    pub const fn runtime_view(&self) -> &LobbyRuntimeView {
        &self.state.runtime_view
    }

    /// Snapshot refresh does not discard inputs held across a live update.
    pub fn sync_runtime_view(&mut self, runtime_view: LobbyRuntimeView) {
        self.state.runtime_view = runtime_view;
    }

    #[must_use]
    pub fn wait_reason(&self, local: LocalLobbyPlayers) -> Option<LobbyWaitReason> {
        self.state.wait_reason(local, select_music_wait_reason)
    }

    pub fn handle_input(&mut self, local: LocalLobbyPlayers, event: &InputEvent) -> bool {
        let waiting = self.wait_reason(local).is_some();
        self.state.handle_input(local, event, waiting)
    }

    #[must_use]
    pub fn disconnect_hold_elapsed(&self) -> Option<f32> {
        self.state
            .disconnect_hold_elapsed_with(|started_at| started_at.elapsed().as_secs_f32())
    }

    #[must_use]
    pub fn disconnect_hold_elapsed_at(&self, now: Instant) -> Option<f32> {
        self.state.disconnect_hold_elapsed_with(|started_at| {
            now.saturating_duration_since(started_at).as_secs_f32()
        })
    }

    /// Call after the presentation's locked-navigation cleanup. The returned
    /// disconnect is queued before subsequent music-selection synchronization.
    #[must_use]
    pub fn update(&mut self, local: LocalLobbyPlayers) -> SelectMusicLobbyUpdate {
        self.update_with(local, |started_at| started_at.elapsed().as_secs_f32())
    }

    #[must_use]
    pub fn update_at(&mut self, local: LocalLobbyPlayers, now: Instant) -> SelectMusicLobbyUpdate {
        self.update_with(local, |started_at| {
            now.saturating_duration_since(started_at).as_secs_f32()
        })
    }

    fn update_with(
        &mut self,
        local: LocalLobbyPlayers,
        elapsed: impl FnMut(Instant) -> f32,
    ) -> SelectMusicLobbyUpdate {
        let waiting = self.wait_reason(local).is_some();
        SelectMusicLobbyUpdate {
            waiting,
            disconnect_requested: self.state.update_disconnect(waiting, elapsed),
        }
    }
}

#[cfg(test)]
#[path = "lobby_wait_tests.rs"]
mod tests;
