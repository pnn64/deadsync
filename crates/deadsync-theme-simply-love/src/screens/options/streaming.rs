use super::*;
use deadsync_config::streaming::TwitchChannel;

const YES_NO: &[Choice] = &[
    localized_choice("Common", "No"),
    localized_choice("Common", "Yes"),
];
pub(super) const STREAMING_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::TwitchEnabled,
        label: lookup_key("OptionsStreaming", "EnableTwitchChat"),
        choices: YES_NO,
        inline: true,
    },
    SubRow {
        id: SubRowId::TwitchChannel,
        label: lookup_key("OptionsStreaming", "Channel"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::TwitchBadges,
        label: lookup_key("OptionsStreaming", "ShowBadges"),
        choices: YES_NO,
        inline: true,
    },
    SubRow {
        id: SubRowId::TwitchEmotes,
        label: lookup_key("OptionsStreaming", "ShowEmotes"),
        choices: YES_NO,
        inline: true,
    },
    SubRow {
        id: SubRowId::TwitchGameplay,
        label: lookup_key("OptionsStreaming", "ShowDuringGameplay"),
        choices: YES_NO,
        inline: true,
    },
];
pub(super) const STREAMING_ITEMS: &[Item] = &[
    Item {
        id: ItemId::TwitchEnabled,
        name: lookup_key("OptionsStreaming", "EnableTwitchChat"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsStreaming",
            "EnableHelp",
        ))],
    },
    Item {
        id: ItemId::TwitchChannel,
        name: lookup_key("OptionsStreaming", "Channel"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsStreaming",
            "ChannelHelp",
        ))],
    },
    Item {
        id: ItemId::TwitchBadges,
        name: lookup_key("OptionsStreaming", "ShowBadges"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsStreaming",
            "BadgesHelp",
        ))],
    },
    Item {
        id: ItemId::TwitchEmotes,
        name: lookup_key("OptionsStreaming", "ShowEmotes"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsStreaming",
            "EmotesHelp",
        ))],
    },
    Item {
        id: ItemId::TwitchGameplay,
        name: lookup_key("OptionsStreaming", "ShowDuringGameplay"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsStreaming",
            "GameplayHelp",
        ))],
    },
];

pub(super) fn twitch_effect(state: &State) -> ThemeEffect {
    online_config_effect(crate::SimplyLoveOnlineConfigRequest::TwitchChat(
        state.twitch,
    ))
}

pub(super) fn begin_twitch_channel_entry(state: &mut State) {
    clear_navigation_holds(state);
    state.twitch_channel_entry = Some(state.twitch.channel.as_str().to_owned());
    state.twitch_channel_invalid = false;
}

fn finish_channel_entry(state: &mut State, save: bool) -> ThemeEffect {
    if save {
        let Some(channel) = state
            .twitch_channel_entry
            .as_deref()
            .and_then(TwitchChannel::parse)
        else {
            state.twitch_channel_invalid = true;
            return ThemeEffect::None;
        };
        state.twitch.channel = channel;
    }
    state.twitch_channel_entry = None;
    state.twitch_channel_invalid = false;
    clear_navigation_holds(state);
    clear_render_cache(state);
    if save {
        twitch_effect(state)
    } else {
        ThemeEffect::None
    }
}

pub(super) fn handle_twitch_channel_input(
    state: &mut State,
    ev: &InputEvent,
) -> Option<ThemeEffect> {
    state.twitch_channel_entry.as_ref()?;
    Some(if ev.pressed {
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => finish_channel_entry(state, true),
            VirtualAction::p1_back | VirtualAction::p2_back => finish_channel_entry(state, false),
            _ => ThemeEffect::None,
        }
    } else {
        ThemeEffect::None
    })
}

pub(super) fn handle_twitch_channel_key(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    if state.twitch_channel_entry.is_none() {
        return false;
    }
    let mut changed = false;
    if let Some(key) = key.filter(|key| key.pressed) {
        match key.code {
            KeyCode::Enter | KeyCode::NumpadEnter if !key.repeat => {
                effects.push(finish_channel_entry(state, true));
                return true;
            }
            KeyCode::Escape if !key.repeat => {
                effects.push(finish_channel_entry(state, false));
                return true;
            }
            KeyCode::Backspace => {
                state.twitch_channel_entry.as_mut().unwrap().pop();
                changed = true;
            }
            KeyCode::Delete => {
                state.twitch_channel_entry.as_mut().unwrap().clear();
                changed = true;
            }
            _ => {}
        }
    }
    if let Some(text) = text {
        let entry = state.twitch_channel_entry.as_mut().unwrap();
        for ch in text
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '#'))
        {
            if entry.len() < 26 {
                entry.push(ch.to_ascii_lowercase());
                changed = true;
            }
        }
    }
    if changed {
        state.twitch_channel_invalid = false;
    }
    true
}

pub(super) fn push_twitch_channel_entry(actors: &mut Vec<Actor>, state: &State) {
    let Some(entry) = &state.twitch_channel_entry else {
        return;
    };
    let w = screen_width();
    let h = screen_height();
    let panel_w = (w - 40.0).min(520.0);
    actors.push(act!(quad: align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h): diffuse(0.0, 0.0, 0.0, 0.85): z(1500)));
    actors.push(act!(quad: align(0.5, 0.5): xy(w * 0.5, h * 0.5): zoomto(panel_w, 180.0): diffuse(0.08, 0.08, 0.12, 1.0): z(1501)));
    actors.push(act!(text: align(0.5, 0.5): xy(w * 0.5, h * 0.5 - 60.0): font("miso"): settext(tr("OptionsStreaming", "ChannelTitle")): zoom(1.0): z(1502)));
    actors.push(act!(text: align(0.5, 0.5): xy(w * 0.5, h * 0.5 - 10.0): font("miso"): settext(format!("{entry}_")): zoom(1.0): z(1502)));
    let hint = if state.twitch_channel_invalid {
        "InvalidChannel"
    } else {
        "ChannelEntryHelp"
    };
    actors.push(act!(text: align(0.5, 0.5): xy(w * 0.5, h * 0.5 + 45.0): font("miso"): settext(tr("OptionsStreaming", hint)): zoom(0.75): z(1502)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_channel_editor_saves_cancels_and_validates_without_navigation() {
        let mut state = super::super::tests::init();
        assert!(
            visible_items(&state)
                .iter()
                .any(|item| item.id == ItemId::StreamingServices)
        );
        begin_twitch_channel_entry(&mut state);
        let mut effects = Vec::new();
        assert!(handle_twitch_channel_key(
            &mut state,
            None,
            Some("#My_Channel"),
            &mut effects
        ));
        let result = finish_channel_entry(&mut state, true);
        assert!(!matches!(result, ThemeEffect::None));
        assert_eq!(state.twitch.channel.as_str(), "my_channel");
        assert!(state.twitch_channel_entry.is_none());
        begin_twitch_channel_entry(&mut state);
        state.twitch_channel_entry = Some("invalid#channel".into());
        assert!(matches!(
            finish_channel_entry(&mut state, true),
            ThemeEffect::None
        ));
        assert!(state.twitch_channel_invalid);
        handle_twitch_channel_key(&mut state, None, None, &mut effects);
        assert!(state.twitch_channel_invalid, "releases retain the validation hint");
        finish_channel_entry(&mut state, false);
        assert_eq!(state.twitch.channel.as_str(), "my_channel");
    }

    #[test]
    fn twitch_menu_toggles_emit_persistable_settings() {
        let mut state = super::super::tests::init();
        state.view = OptionsView::Submenu(SubmenuKind::Streaming);
        state.sub_selected = 0;
        let effect = apply_submenu_choice_delta(&mut state, &AssetManager::new(), 1, NavWrap::Wrap);
        assert!(effect.is_some());
        assert!(state.twitch.enabled);
        state.sub_selected = 4;
        apply_submenu_choice_delta(&mut state, &AssetManager::new(), 1, NavWrap::Wrap);
        assert!(!state.twitch.show_gameplay);
        assert!(state.twitch.show_badges && state.twitch.show_emotes);
    }
}
