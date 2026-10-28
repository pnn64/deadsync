//! Machine-wide streaming preferences. Channel names are bounded and IRC-safe.
use crate::ini::SimpleIni;
use crate::options::load_bool_option;
use crate::writer::{push_bool, push_line, push_section};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TwitchChannel {
    bytes: [u8; 25],
    len: u8,
}

impl TwitchChannel {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
        if raw.len() > 25 || !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        let mut channel = Self::default();
        for (out, byte) in channel.bytes.iter_mut().zip(raw.bytes()) {
            *out = byte.to_ascii_lowercase();
        }
        channel.len = raw.len() as u8;
        Some(channel)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwitchOptions {
    pub enabled: bool,
    pub channel: TwitchChannel,
    pub show_badges: bool,
    pub show_emotes: bool,
    pub show_gameplay: bool,
}

impl Default for TwitchOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            channel: TwitchChannel::default(),
            show_badges: true,
            show_emotes: true,
            show_gameplay: true,
        }
    }
}

pub fn load(conf: &SimpleIni, default: TwitchOptions) -> TwitchOptions {
    let section = "StreamingServices";
    TwitchOptions {
        enabled: load_bool_option(conf, section, "EnableTwitchChat", default.enabled),
        channel: conf
            .get(section, "TwitchChannel")
            .and_then(TwitchChannel::parse)
            .unwrap_or(default.channel),
        show_badges: load_bool_option(conf, section, "TwitchShowBadges", default.show_badges),
        show_emotes: load_bool_option(conf, section, "TwitchShowEmotes", default.show_emotes),
        show_gameplay: load_bool_option(
            conf,
            section,
            "TwitchShowDuringGameplay",
            default.show_gameplay,
        ),
    }
}

pub fn push_options(content: &mut String, options: TwitchOptions) {
    push_section(content, "[StreamingServices]");
    push_bool(content, "EnableTwitchChat", options.enabled);
    push_line(content, "TwitchChannel", options.channel.as_str());
    push_bool(content, "TwitchShowBadges", options.show_badges);
    push_bool(content, "TwitchShowEmotes", options.show_emotes);
    push_bool(content, "TwitchShowDuringGameplay", options.show_gameplay);
    content.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_channels_normalize_and_reject_command_injection() {
        assert_eq!(
            TwitchChannel::parse(" #Some_Channel ").unwrap().as_str(),
            "some_channel"
        );
        assert!(TwitchChannel::parse("").unwrap().is_empty());
        for value in [
            "foo\r\nJOIN #other",
            "foo bar",
            "https://twitch.tv/foo",
            "é",
            "abcdefghijklmnopqrstuvwxyz",
        ] {
            assert!(TwitchChannel::parse(value).is_none(), "{value:?}");
        }
    }

    #[test]
    fn twitch_preferences_round_trip_and_old_configs_stay_disabled() {
        let mut ini = SimpleIni::new();
        assert_eq!(
            load(&ini, TwitchOptions::default()),
            TwitchOptions::default()
        );
        let options = TwitchOptions {
            enabled: true,
            channel: TwitchChannel::parse("Some_Channel").unwrap(),
            show_badges: false,
            show_emotes: false,
            show_gameplay: false,
        };
        let cfg = crate::app_config::Config {
            twitch: options,
            ..Default::default()
        };
        let saved = crate::save::build_saved_app_config_file(
            &cfg,
            &deadsync_input::Keymap::default(),
            "default",
            &[],
            &[],
            "",
            "",
            "",
            "",
        );
        ini.load_str(&saved);
        assert_eq!(
            crate::load::load_app_config(&ini, Default::default()).twitch,
            options
        );
    }
}
