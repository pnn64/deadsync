use crate::{
    ActiveProfile, JudgmentGraphic, MiniIndicator, NoteSkin, PLAYER_SLOTS, PlayerOptionsData,
    PlayerSide, ScrollOption, player_side_index,
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

/// Participation and confirmation for one physical side in the picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerSeat {
    Unjoined,
    Choosing,
    Ready,
}

impl PickerSeat {
    pub const fn joined(self) -> bool {
        !matches!(self, Self::Unjoined)
    }
}

/// Result of attempting to join or confirm a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerStart {
    Joined,
    Waiting,
    DuplicateProfile,
    Confirmed,
}

/// Result of undoing confirmation, leaving a seat, or cancelling the picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerBack {
    SeatChanged,
    OtherJoined,
    Cancelled,
}

/// Shared participation rules for a standalone picker or a late join.
pub struct PickerSeats([PickerSeat; PLAYER_SLOTS]);

impl PickerSeats {
    pub fn new(joined: [bool; PLAYER_SLOTS]) -> Self {
        Self(joined.map(|joined| {
            if joined {
                PickerSeat::Choosing
            } else {
                PickerSeat::Unjoined
            }
        }))
    }

    pub const fn states(&self) -> [PickerSeat; PLAYER_SLOTS] {
        self.0
    }

    pub fn late_join(side: PlayerSide) -> Self {
        let mut seats = [PickerSeat::Ready; PLAYER_SLOTS];
        seats[player_side_index(side)] = PickerSeat::Choosing;
        Self(seats)
    }

    pub fn start(
        &mut self,
        side: PlayerSide,
        choices: [Option<&ActiveProfile>; PLAYER_SLOTS],
    ) -> PickerStart {
        let player = player_side_index(side);
        let other = 1 - player;
        match self.0[player] {
            PickerSeat::Unjoined => {
                self.0[player] = PickerSeat::Choosing;
                return PickerStart::Joined;
            }
            PickerSeat::Ready => return PickerStart::Waiting,
            PickerSeat::Choosing => {}
        }
        if self.0[other] == PickerSeat::Ready
            && matches!(choices[player], Some(ActiveProfile::Local { .. }))
            && choices[player] == choices[other]
        {
            return PickerStart::DuplicateProfile;
        }
        self.0[player] = PickerSeat::Ready;
        if self.0[other] == PickerSeat::Choosing {
            PickerStart::Waiting
        } else {
            PickerStart::Confirmed
        }
    }

    pub fn back(&mut self, side: PlayerSide) -> PickerBack {
        let player = player_side_index(side);
        match self.0[player] {
            PickerSeat::Ready => self.0[player] = PickerSeat::Choosing,
            PickerSeat::Choosing => self.0[player] = PickerSeat::Unjoined,
            PickerSeat::Unjoined if self.0[1 - player].joined() => return PickerBack::OtherJoined,
            PickerSeat::Unjoined => return PickerBack::Cancelled,
        }
        PickerBack::SeatChanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_join_rejects_an_in_use_profile_and_accepts_a_guest() {
        let local = ActiveProfile::Local {
            id: "same-profile".into(),
        };
        let mut picker = PickerSeats::late_join(PlayerSide::P1);
        assert_eq!(
            picker.start(PlayerSide::P1, [Some(&local), Some(&local)]),
            PickerStart::DuplicateProfile,
        );
        assert_eq!(picker.states(), [PickerSeat::Choosing, PickerSeat::Ready]);
        assert_eq!(
            picker.start(PlayerSide::P1, [Some(&ActiveProfile::Guest), Some(&local)]),
            PickerStart::Confirmed,
        );
        assert_eq!(picker.states(), [PickerSeat::Ready; PLAYER_SLOTS]);
        assert_eq!(picker.back(PlayerSide::P1), PickerBack::SeatChanged);
        assert_eq!(picker.states(), [PickerSeat::Choosing, PickerSeat::Ready]);
        assert_eq!(picker.back(PlayerSide::P1), PickerBack::SeatChanged);
        assert_eq!(picker.back(PlayerSide::P1), PickerBack::OtherJoined);
    }
}
