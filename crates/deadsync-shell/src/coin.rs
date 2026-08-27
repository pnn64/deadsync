use deadsync_config::prelude::{CoinMode, CoinOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct Bookkeeping {
    pub coins_inserted: u64,
    pub credits_spent: u64,
    pub plays_started: u64,
    pub stages_played: u64,
}

pub struct State {
    coin_balance: u32,
    stages_left: u8,
    premium_started: Option<Instant>,
    bookkeeping: Bookkeeping,
    bookkeeping_path: PathBuf,
}

impl State {
    pub fn load(bookkeeping_path: PathBuf) -> Self {
        let bookkeeping = load_bookkeeping(&bookkeeping_path);
        Self {
            coin_balance: 0,
            stages_left: 0,
            premium_started: None,
            bookkeeping,
            bookkeeping_path,
        }
    }

    #[cfg(test)]
    pub const fn coin_balance(&self) -> u32 {
        self.coin_balance
    }

    pub const fn credits(&self, options: CoinOptions) -> u32 {
        self.coin_balance / options.coins_per_credit as u32
    }

    #[cfg(test)]
    pub const fn stages_left(&self) -> u8 {
        self.stages_left
    }

    pub const fn bookkeeping(&self) -> Bookkeeping {
        self.bookkeeping
    }

    pub fn insert_coin(&mut self) {
        self.coin_balance = self.coin_balance.saturating_add(1);
        self.bookkeeping.coins_inserted = self.bookkeeping.coins_inserted.saturating_add(1);
        self.save_bookkeeping();
    }

    pub fn begin_play(&mut self, options: CoinOptions) -> bool {
        if !self.spend_credits(options, 1) {
            return false;
        }
        self.stages_left = options.songs_per_play;
        self.premium_started = None;
        self.bookkeeping.plays_started = self.bookkeeping.plays_started.saturating_add(1);
        self.save_bookkeeping();
        true
    }

    pub fn continue_play(&mut self, options: CoinOptions, joined_players: u8) -> bool {
        if !self.spend_credits(options, joined_players.max(1)) {
            return false;
        }
        self.stages_left = options.songs_per_play;
        self.bookkeeping.plays_started = self.bookkeeping.plays_started.saturating_add(1);
        self.save_bookkeeping();
        true
    }

    pub fn join_player(&mut self, options: CoinOptions) -> bool {
        let paid = self.spend_credits(options, 1);
        if paid {
            self.save_bookkeeping();
        }
        paid
    }

    pub const fn start_premium_free(&mut self, now: Instant) {
        self.premium_started = Some(now);
    }

    pub const fn premium_free_active(&self) -> bool {
        self.premium_started.is_some()
    }

    pub fn premium_seconds_left(&self, options: CoinOptions, now: Instant) -> Option<u32> {
        let started = self.premium_started?;
        Some(
            Duration::from_secs(u64::from(options.premium_free_seconds()))
                .saturating_sub(now.saturating_duration_since(started))
                .as_secs() as u32,
        )
    }

    pub fn song_allowed(
        &self,
        options: CoinOptions,
        song_seconds: f32,
        music_rate: f32,
        now: Instant,
    ) -> bool {
        if let Some(seconds_left) = self.premium_seconds_left(options, now) {
            return song_seconds.max(0.0)
                <= seconds_left.saturating_add(u32::from(options.premium_free_grace_seconds))
                    as f32;
        }
        options.event_mode() || options.stage_cost(song_seconds, music_rate) <= self.stages_left
    }

    pub fn premium_song_allowed(
        &self,
        options: CoinOptions,
        song_seconds: f32,
        now: Instant,
    ) -> bool {
        let seconds_left = self
            .premium_seconds_left(options, now)
            .unwrap_or_else(|| options.premium_free_seconds());
        song_seconds.max(0.0)
            <= seconds_left.saturating_add(u32::from(options.premium_free_grace_seconds)) as f32
    }

    pub fn record_stage(
        &mut self,
        options: CoinOptions,
        song_seconds: f32,
        music_rate: f32,
        gave_up: bool,
    ) {
        self.bookkeeping.stages_played = self.bookkeeping.stages_played.saturating_add(1);
        if !options.event_mode()
            && !self.premium_free_active()
            && !(gave_up && options.continue_on_give_up)
        {
            self.stages_left = self
                .stages_left
                .saturating_sub(options.stage_cost(song_seconds, music_rate));
        }
        self.save_bookkeeping();
    }

    pub fn record_course_stage(&mut self) {
        self.bookkeeping.stages_played = self.bookkeeping.stages_played.saturating_add(1);
        self.save_bookkeeping();
    }

    pub fn set_over(&self, options: CoinOptions, now: Instant) -> bool {
        if options.event_mode() {
            return false;
        }
        self.premium_seconds_left(options, now)
            .map_or(self.stages_left == 0, |seconds| seconds == 0)
    }

    fn spend_credits(&mut self, options: CoinOptions, credits: u8) -> bool {
        if !matches!(options.mode, CoinMode::Pay) {
            return true;
        }
        let coins = u32::from(options.coins_per_credit) * u32::from(credits);
        if self.coin_balance < coins {
            return false;
        }
        self.coin_balance -= coins;
        self.bookkeeping.credits_spent = self
            .bookkeeping
            .credits_spent
            .saturating_add(u64::from(credits));
        true
    }

    fn save_bookkeeping(&self) {
        if let Err(error) = save_bookkeeping(&self.bookkeeping_path, self.bookkeeping) {
            log::warn!("Failed to save coin bookkeeping: {error}");
        }
    }
}

fn load_bookkeeping(path: &Path) -> Bookkeeping {
    let Ok(bytes) = std::fs::read(path) else {
        return Bookkeeping::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        log::warn!(
            "Failed to load coin bookkeeping '{}': {error}",
            path.display()
        );
        Bookkeeping::default()
    })
}

fn save_bookkeeping(path: &Path, bookkeeping: Bookkeeping) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(&bookkeeping)
        .expect("serializing fixed coin bookkeeping fields cannot fail");
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pay_options() -> CoinOptions {
        CoinOptions {
            mode: CoinMode::Pay,
            ..CoinOptions::default()
        }
    }

    fn state() -> State {
        State {
            coin_balance: 0,
            stages_left: 0,
            premium_started: None,
            bookkeeping: Bookkeeping::default(),
            bookkeeping_path: PathBuf::new(),
        }
    }

    #[test]
    fn pay_play_requires_and_deducts_one_credit() {
        let mut state = state();
        let options = pay_options();
        assert!(!state.begin_play(options));
        state.insert_coin();
        assert!(state.begin_play(options));
        assert_eq!(state.coin_balance(), 0);
        assert_eq!(state.stages_left(), 3);
    }

    #[test]
    fn long_songs_consume_multiple_stages() {
        let mut state = state();
        let options = CoinOptions {
            mode: CoinMode::Free,
            event_mode: false,
            ..CoinOptions::default()
        };
        assert!(state.begin_play(options));
        state.record_stage(options, 150.0, 1.0, false);
        assert_eq!(state.stages_left(), 1);
        assert!(state.song_allowed(options, 149.0, 1.0, Instant::now()));
        assert!(!state.song_allowed(options, 150.0, 1.0, Instant::now()));
    }

    #[test]
    fn premium_free_uses_time_plus_song_grace() {
        let mut state = state();
        let options = CoinOptions {
            mode: CoinMode::Free,
            event_mode: false,
            premium_free_minutes: 10,
            premium_free_grace_seconds: 300,
            ..CoinOptions::default()
        };
        let now = Instant::now();
        state.start_premium_free(now - Duration::from_secs(590));
        assert!(state.song_allowed(options, 310.0, 1.0, now));
        assert!(!state.song_allowed(options, 311.0, 1.0, now));
    }

    #[test]
    fn give_up_can_preserve_the_stage_set() {
        let mut state = state();
        let options = CoinOptions {
            mode: CoinMode::Free,
            event_mode: false,
            continue_on_give_up: true,
            ..CoinOptions::default()
        };
        state.begin_play(options);
        state.record_stage(options, 120.0, 1.0, true);
        assert_eq!(state.stages_left(), 3);
    }

    #[test]
    fn course_stages_count_without_spending_regular_stages() {
        let mut state = state();
        let options = CoinOptions {
            mode: CoinMode::Free,
            event_mode: false,
            ..CoinOptions::default()
        };
        state.begin_play(options);
        state.record_course_stage();
        assert_eq!(state.bookkeeping().stages_played, 1);
        assert_eq!(state.stages_left(), 3);
    }
}
