use deadsync_theme_simply_love::views::SimplyLoveLobbyRuntimeView;
use std::time::Instant;

fn runtime_view(
    refresh: deadsync_online::lobbies::RuntimeViewRefresh,
) -> SimplyLoveLobbyRuntimeView {
    SimplyLoveLobbyRuntimeView {
        snapshot: refresh.snapshot,
        reconnect_status_text: refresh.reconnect_status_text,
        disconnect_hold_seconds: deadsync_online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS,
    }
}

/// Read the current lobby view without retaining its source cursor.
pub(super) fn refresh() -> SimplyLoveLobbyRuntimeView {
    runtime_view(deadsync_online::lobbies::runtime_refresh_view_state_default())
}

/// One app/game-thread-owned cursor for a screen-retained lobby view.
///
/// Lifetime/capacity: one fixed cursor per consuming screen role for the app
/// session. Warmup: screen entry or the first active frame. A hit reads one
/// atomic generation and one optional deadline without locking or allocating.
/// A miss locks the bounded lobby/reconnect states, clones one snapshot `Arc`,
/// may format one short reconnect label, and may enqueue one reconnect command.
/// There is no growth, eviction, scan, or gameplay-frame destruction. Existing
/// frame-update timing accounts for misses; the generation and deadline make
/// their worst-case cadence explicit.
pub(super) struct RuntimeCursor {
    generation: u64,
    refresh_at: Option<Instant>,
    rebuild: bool,
}

impl Default for RuntimeCursor {
    fn default() -> Self {
        Self {
            generation: 0,
            refresh_at: None,
            rebuild: true,
        }
    }
}

impl RuntimeCursor {
    #[inline(always)]
    pub(super) const fn force_refresh(&mut self) {
        self.rebuild = true;
    }

    pub(super) fn refresh_now(&mut self) -> SimplyLoveLobbyRuntimeView {
        let refresh = deadsync_online::lobbies::runtime_refresh_view_state_default();
        self.generation = refresh.generation;
        self.refresh_at = refresh.next_refresh_at;
        self.rebuild = false;
        runtime_view(refresh)
    }

    #[inline(always)]
    fn is_dirty(&self, generation: u64, now: Instant) -> bool {
        self.rebuild
            || self.generation != generation
            || self.refresh_at.is_some_and(|refresh_at| now >= refresh_at)
    }

    pub(super) fn refresh_if_dirty(&mut self, now: Instant) -> Option<SimplyLoveLobbyRuntimeView> {
        let generation = deadsync_online::lobbies::runtime_view_generation();
        self.is_dirty(generation, now).then(|| self.refresh_now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_refresh_marks_a_warmed_cursor_dirty() {
        let generation = 42;
        let now = Instant::now();
        let mut cursor = RuntimeCursor {
            generation,
            refresh_at: None,
            rebuild: false,
        };
        assert!(!cursor.is_dirty(generation, now));

        cursor.force_refresh();

        assert!(cursor.is_dirty(generation, now));
    }

    #[test]
    fn cursor_uses_generation_and_explicit_deadline() {
        let now = Instant::now();
        let cursor = RuntimeCursor {
            generation: 7,
            refresh_at: Some(now),
            rebuild: false,
        };

        assert!(cursor.is_dirty(8, now));
        assert!(cursor.is_dirty(7, now));
        assert!(!cursor.is_dirty(7, now - std::time::Duration::from_millis(1)));
    }
}
