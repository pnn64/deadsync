use crate::bools::parse_loose_bool_str;
use crate::ini::SimpleIni;
use crate::writer::{push_bool, push_line};
use std::str::FromStr;

pub const MIN_COINS_PER_CREDIT: u8 = 1;
pub const MAX_COINS_PER_CREDIT: u8 = 20;
pub const MIN_SONGS_PER_PLAY: u8 = 1;
pub const MAX_SONGS_PER_PLAY: u8 = 7;
pub const MAX_PREMIUM_FREE_MINUTES: u8 = 60;
pub const MAX_SONG_CUTOFF_SECONDS: u16 = 3_600;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoinMode {
    Pay,
    Free,
    #[default]
    Home,
}

impl CoinMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pay => "Pay",
            Self::Free => "Free",
            Self::Home => "Home",
        }
    }
}

impl FromStr for CoinMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pay" => Ok(Self::Pay),
            "free" => Ok(Self::Free),
            "home" => Ok(Self::Home),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoinOptions {
    pub mode: CoinMode,
    pub coins_per_credit: u8,
    pub songs_per_play: u8,
    /// User-selected Event Mode value. Home and Pay override it at runtime.
    pub event_mode: bool,
    pub premium_free_minutes: u8,
    pub premium_free_grace_seconds: u16,
    pub continue_on_give_up: bool,
    pub long_song_seconds: u16,
    pub marathon_song_seconds: u16,
}

impl Default for CoinOptions {
    fn default() -> Self {
        Self {
            mode: CoinMode::Home,
            coins_per_credit: 1,
            songs_per_play: 3,
            event_mode: true,
            premium_free_minutes: 0,
            premium_free_grace_seconds: 300,
            continue_on_give_up: false,
            long_song_seconds: 150,
            marathon_song_seconds: 300,
        }
    }
}

impl CoinOptions {
    /// `ITGmania` parity: Home is always Event Mode and Pay never is.
    pub const fn event_mode(self) -> bool {
        match self.mode {
            CoinMode::Home => true,
            CoinMode::Pay => false,
            CoinMode::Free => self.event_mode,
        }
    }

    pub const fn premium_free_available(self) -> bool {
        self.premium_free_minutes > 0 && !self.event_mode() && !matches!(self.mode, CoinMode::Home)
    }

    pub fn stage_cost(self, song_seconds: f32, music_rate: f32) -> u8 {
        let duration = song_seconds.max(0.0) / music_rate.max(0.05);
        if duration >= f32::from(self.marathon_song_seconds) {
            3
        } else if duration >= f32::from(self.long_song_seconds) {
            2
        } else {
            1
        }
    }

    pub const fn premium_free_seconds(self) -> u32 {
        self.premium_free_minutes as u32 * 60
    }
}

pub fn load_coin_options(conf: &SimpleIni, default: CoinOptions) -> CoinOptions {
    let mode = conf
        .get("Options", "CoinMode")
        .and_then(|value| CoinMode::from_str(value).ok())
        .unwrap_or(default.mode);
    let coins_per_credit = load_u8(conf, "CoinsPerCredit", default.coins_per_credit)
        .clamp(MIN_COINS_PER_CREDIT, MAX_COINS_PER_CREDIT);
    let songs_per_play = load_u8(conf, "SongsPerPlay", default.songs_per_play)
        .clamp(MIN_SONGS_PER_PLAY, MAX_SONGS_PER_PLAY);
    let premium_free_minutes = load_u8(conf, "PremiumFreeMinutes", default.premium_free_minutes)
        .min(MAX_PREMIUM_FREE_MINUTES);
    let premium_free_grace_seconds = load_u16(
        conf,
        "PremiumFreeSongGraceSeconds",
        default.premium_free_grace_seconds,
    )
    .min(MAX_SONG_CUTOFF_SECONDS);
    let long_song_seconds = load_u16(conf, "LongVerSongSeconds", default.long_song_seconds)
        .clamp(1, MAX_SONG_CUTOFF_SECONDS - 1);
    let marathon_song_seconds = load_u16(
        conf,
        "MarathonVerSongSeconds",
        default.marathon_song_seconds,
    )
    .clamp(long_song_seconds + 1, MAX_SONG_CUTOFF_SECONDS);

    CoinOptions {
        mode,
        coins_per_credit,
        songs_per_play,
        event_mode: conf
            .get("Options", "EventMode")
            .and_then(parse_loose_bool_str)
            .unwrap_or(default.event_mode),
        premium_free_minutes,
        premium_free_grace_seconds,
        continue_on_give_up: conf
            .get("Options", "ContinueOnGiveUp")
            .and_then(parse_loose_bool_str)
            .unwrap_or(default.continue_on_give_up),
        long_song_seconds,
        marathon_song_seconds,
    }
}

pub fn push_coin_option_lines(content: &mut String, options: CoinOptions) {
    push_line(content, "CoinMode", options.mode.as_str());
    push_line(content, "CoinsPerCredit", options.coins_per_credit);
    push_line(content, "SongsPerPlay", options.songs_per_play);
    push_bool(content, "EventMode", options.event_mode);
    push_line(content, "PremiumFreeMinutes", options.premium_free_minutes);
    push_line(
        content,
        "PremiumFreeSongGraceSeconds",
        options.premium_free_grace_seconds,
    );
    push_bool(content, "ContinueOnGiveUp", options.continue_on_give_up);
    push_line(content, "LongVerSongSeconds", options.long_song_seconds);
    push_line(
        content,
        "MarathonVerSongSeconds",
        options.marathon_song_seconds,
    );
}

fn load_u8(conf: &SimpleIni, key: &str, default: u8) -> u8 {
    conf.get("Options", key)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn load_u16(conf: &SimpleIni, key: &str, default: u16) -> u16 {
    conf.get("Options", key)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_mode_follows_coin_mode_rules() {
        let mut options = CoinOptions::default();
        assert!(options.event_mode());
        options.mode = CoinMode::Pay;
        assert!(!options.event_mode());
        options.mode = CoinMode::Free;
        options.event_mode = false;
        assert!(!options.event_mode());
        options.event_mode = true;
        assert!(options.event_mode());
    }

    #[test]
    fn stage_cost_is_rate_adjusted() {
        let options = CoinOptions::default();
        assert_eq!(options.stage_cost(149.9, 1.0), 1);
        assert_eq!(options.stage_cost(150.0, 1.0), 2);
        assert_eq!(options.stage_cost(300.0, 1.0), 3);
        assert_eq!(options.stage_cost(200.0, 2.0), 1);
    }

    #[test]
    fn load_clamps_values_and_preserves_cutoff_order() {
        let mut conf = SimpleIni::new();
        conf.load_str("[Options]\nCoinMode=Pay\nCoinsPerCredit=0\nSongsPerPlay=99\nLongVerSongSeconds=500\nMarathonVerSongSeconds=100\n");
        let options = load_coin_options(&conf, CoinOptions::default());
        assert_eq!(options.mode, CoinMode::Pay);
        assert_eq!(options.coins_per_credit, 1);
        assert_eq!(options.songs_per_play, 7);
        assert!(options.long_song_seconds < options.marathon_song_seconds);
    }

    #[test]
    fn writes_itgmania_preference_names() {
        let mut content = String::new();
        push_coin_option_lines(&mut content, CoinOptions::default());
        assert!(content.contains("CoinMode=Home\n"));
        assert!(content.contains("CoinsPerCredit=1\n"));
        assert!(content.contains("SongsPerPlay=3\n"));
        assert!(content.contains("LongVerSongSeconds=150\n"));
    }
}
