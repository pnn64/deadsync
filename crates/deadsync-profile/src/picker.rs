use crate::{
    ActiveProfile, JudgmentGraphic, MiniIndicator, NoteSkin, PlayerOptionsData, ScrollOption,
};
use deadsync_config::theme::GameFlag;

/// Catalog and session preferences used by the profile picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePickerView {
    pub game: GameFlag,
    pub guest: ProfilePickerEntryView,
    pub profiles: Vec<ProfilePickerEntryView>,
    pub default_profiles: [ActiveProfile; 2],
    pub three_key_navigation: bool,
}

impl Default for ProfilePickerView {
    fn default() -> Self {
        let options = PlayerOptionsData::default();
        Self {
            game: GameFlag::default(),
            guest: ProfilePickerEntryView {
                id: String::new(),
                display_name: String::new(),
                speed_mod: options.scroll_speed.to_string(),
                avatar_key: None,
                total_songs_played: 0,
                scroll_option: options.scroll_option,
                mini_indicator: options.mini_indicator,
                noteskin: options.noteskin,
                judgment: options.judgment_graphic,
            },
            profiles: Vec::new(),
            default_profiles: std::array::from_fn(|_| ActiveProfile::Guest),
            three_key_navigation: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePickerEntryView {
    pub id: String,
    pub display_name: String,
    pub speed_mod: String,
    pub avatar_key: Option<String>,
    pub total_songs_played: u32,
    pub scroll_option: ScrollOption,
    pub mini_indicator: MiniIndicator,
    pub noteskin: NoteSkin,
    pub judgment: JudgmentGraphic,
}
