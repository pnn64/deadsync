pub mod downloads;
pub mod leaderboard;
mod menu;
pub mod replay;
pub mod song_search;

pub use downloads::*;
pub use leaderboard::*;
pub use menu::{
    CategoryItemLists as MenuLists, Entry, FOCUS_TWEEN_SECONDS, InputOutcome, RenderParams,
    VisibleState as MenuState, build_overlay, handle_input, move_selection, open,
};
pub use replay::*;
pub use song_search::*;

use crate::engine::present::actors::Actor;
use crate::engine::present::actors::TextContent;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    BackToMain,
    SortByGroup,
    SortByTitle,
    SortByArtist,
    SortByBpm,
    SortByLength,
    SortByMeter,
    SortByPopularity,
    SortByRecent,
    SortByGenre,
    SortByTopGrades,
    SortByPopularityP1,
    SortByPopularityP2,
    SortByRecentP1,
    SortByRecentP2,
    SortByTopGradesP1,
    SortByTopGradesP2,
    SortByPlaylist(String),
    ToggleFavorite,
    SortByFavorites,
    SwitchToSingle,
    SwitchToDouble,
    TestInput,
    SongSearch,
    SwitchProfile,
    ReloadSongsCourses,
    ShowLobbies,
    ViewDownloads,
    NullOrDie,
    NullOrDiePack,
    SyncSong,
    SyncPack,
    PlayReplay,
    PracticeMode,
    ShowLeaderboard,
    ShowSetSummary,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub top_label: TextContent,
    pub bottom_label: TextContent,
    pub action: Action,
}

const ITEM_SORT_BY_GROUP: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Group"),
    action: Action::SortByGroup,
};
const ITEM_SORT_BY_TITLE: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Title"),
    action: Action::SortByTitle,
};
const ITEM_SORT_BY_ARTIST: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Artist"),
    action: Action::SortByArtist,
};
const ITEM_SORT_BY_BPM: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("BPM"),
    action: Action::SortByBpm,
};
const ITEM_SORT_BY_LENGTH: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Length"),
    action: Action::SortByLength,
};
const ITEM_SORT_BY_METER: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Level"),
    action: Action::SortByMeter,
};
const ITEM_SORT_BY_POPULARITY: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Most Popular"),
    action: Action::SortByPopularity,
};
const ITEM_SORT_BY_RECENT: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Recently Played"),
    action: Action::SortByRecent,
};
pub const ITEM_SORT_BY_GENRE: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Genre"),
    action: Action::SortByGenre,
};
pub const ITEM_SORT_BY_TOP_GRADES: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("Machine Top Scores"),
    action: Action::SortByTopGrades,
};
pub const ITEM_SORT_BY_POPULARITY_P1: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P1 Most Played"),
    action: Action::SortByPopularityP1,
};
pub const ITEM_SORT_BY_POPULARITY_P2: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P2 Most Played"),
    action: Action::SortByPopularityP2,
};
pub const ITEM_SORT_BY_RECENT_P1: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P1 Recent Songs"),
    action: Action::SortByRecentP1,
};
pub const ITEM_SORT_BY_RECENT_P2: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P2 Recent Songs"),
    action: Action::SortByRecentP2,
};
pub const ITEM_SORT_BY_TOP_GRADES_P1: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P1 Clear Rank"),
    action: Action::SortByTopGradesP1,
};
pub const ITEM_SORT_BY_TOP_GRADES_P2: Item = Item {
    top_label: TextContent::Static("Sort By"),
    bottom_label: TextContent::Static("P2 Clear Rank"),
    action: Action::SortByTopGradesP2,
};
pub const ITEM_SWITCH_TO_SINGLE: Item = Item {
    top_label: TextContent::Static("Change Style To"),
    bottom_label: TextContent::Static("Single"),
    action: Action::SwitchToSingle,
};
pub const ITEM_SWITCH_TO_DOUBLE: Item = Item {
    top_label: TextContent::Static("Change Style To"),
    bottom_label: TextContent::Static("Double"),
    action: Action::SwitchToDouble,
};
pub const ITEM_TEST_INPUT: Item = Item {
    top_label: TextContent::Static("Feeling salty?"),
    bottom_label: TextContent::Static("Test Input"),
    action: Action::TestInput,
};
pub const ITEM_SONG_SEARCH: Item = Item {
    top_label: TextContent::Static("Wherefore Art Thou?"),
    bottom_label: TextContent::Static("Song Search"),
    action: Action::SongSearch,
};
pub const ITEM_SWITCH_PROFILE: Item = Item {
    top_label: TextContent::Static("Next Please"),
    bottom_label: TextContent::Static("Switch Profile"),
    action: Action::SwitchProfile,
};
pub const ITEM_RELOAD_SONGS_COURSES: Item = Item {
    top_label: TextContent::Static("Take a Breather~"),
    bottom_label: TextContent::Static("Load New Songs"),
    action: Action::ReloadSongsCourses,
};
pub const ITEM_SHOW_LOBBIES: Item = Item {
    top_label: TextContent::Static("Friends Online?"),
    bottom_label: TextContent::Static("Online Lobbies"),
    action: Action::ShowLobbies,
};
pub const ITEM_VIEW_DOWNLOADS: Item = Item {
    top_label: TextContent::Static("Need More RAM"),
    bottom_label: TextContent::Static("View Downloads"),
    action: Action::ViewDownloads,
};
pub const ITEM_NULL_OR_DIE: Item = Item {
    top_label: TextContent::Static("Sync song with"),
    bottom_label: TextContent::Static("NULL-OR-DIE"),
    action: Action::NullOrDie,
};
pub const ITEM_NULL_OR_DIE_PACK: Item = Item {
    top_label: TextContent::Static("Sync pack with"),
    bottom_label: TextContent::Static("NULL-OR-DIE"),
    action: Action::NullOrDiePack,
};
pub const ITEM_SYNC_SONG: Item = Item {
    top_label: TextContent::Static("Incorrect offset?"),
    bottom_label: TextContent::Static("SYNC SONG"),
    action: Action::SyncSong,
};
pub const ITEM_SYNC_PACK: Item = Item {
    top_label: TextContent::Static("Incorrect offset?"),
    bottom_label: TextContent::Static("SYNC PACK"),
    action: Action::SyncPack,
};
pub const ITEM_PLAY_REPLAY: Item = Item {
    top_label: TextContent::Static("Machine Data"),
    bottom_label: TextContent::Static("Play Replay"),
    action: Action::PlayReplay,
};
pub const ITEM_PRACTICE_MODE: Item = Item {
    top_label: TextContent::Static("Having a hard time?"),
    bottom_label: TextContent::Static("Practice Mode"),
    action: Action::PracticeMode,
};
pub const ITEM_SHOW_LEADERBOARD: Item = Item {
    top_label: TextContent::Static("GrooveStats"),
    bottom_label: TextContent::Static("Leaderboard"),
    action: Action::ShowLeaderboard,
};
pub const ITEM_TOGGLE_FAVORITE: Item = Item {
    top_label: TextContent::Static("I'm Lovin' It"),
    bottom_label: TextContent::Static("Add Favorite"),
    action: Action::ToggleFavorite,
};
pub const ITEM_SORT_BY_FAVORITES: Item = Item {
    top_label: TextContent::Static("Check Out My Mix Tape"),
    bottom_label: TextContent::Static("Favorites"),
    action: Action::SortByFavorites,
};
pub const ITEM_GO_BACK: Item = Item {
    top_label: TextContent::Static(""),
    bottom_label: TextContent::Static("Go Back"),
    action: Action::BackToMain,
};
pub const ITEM_SET_SUMMARY: Item = Item {
    top_label: TextContent::Static("Relive Your Memories"),
    bottom_label: TextContent::Static("Set Summary"),
    action: Action::ShowSetSummary,
};

pub fn playlist_item(
    top_label: impl Into<String>,
    bottom_label: impl Into<String>,
    id: impl Into<String>,
) -> Item {
    Item {
        top_label: TextContent::Shared(Arc::<str>::from(top_label.into())),
        bottom_label: TextContent::Shared(Arc::<str>::from(bottom_label.into())),
        action: Action::SortByPlaylist(id.into()),
    }
}

pub const SORT_ITEMS: [Item; 10] = [
    ITEM_SORT_BY_GROUP,
    ITEM_SORT_BY_TITLE,
    ITEM_SORT_BY_ARTIST,
    ITEM_SORT_BY_GENRE,
    ITEM_SORT_BY_BPM,
    ITEM_SORT_BY_LENGTH,
    ITEM_SORT_BY_METER,
    ITEM_SORT_BY_POPULARITY,
    ITEM_SORT_BY_RECENT,
    ITEM_SORT_BY_TOP_GRADES,
];

#[derive(Clone, Debug)]
pub enum State {
    Hidden,
    Visible(MenuState),
}

impl State {
    pub fn is_hidden(&self) -> bool {
        matches!(self, State::Hidden)
    }

    pub fn is_visible(&self) -> bool {
        !self.is_hidden()
    }
}

#[inline(always)]
pub fn scroll_dir(len: usize, prev: usize, selected: usize) -> isize {
    if len <= 1 {
        return 0;
    }
    let prev = prev % len;
    let selected = selected % len;
    if selected == (prev + 1) % len {
        1
    } else if prev == (selected + 1) % len {
        -1
    } else {
        0
    }
}

#[inline(always)]
pub fn scroll_anim_dir(len: usize, prev: usize, selected: usize, input_dir: isize) -> isize {
    let dir = scroll_dir(len, prev, selected);
    if len == 2 && dir != 0 {
        match input_dir.cmp(&0) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => dir,
        }
    } else {
        dir
    }
}

#[inline(always)]
pub(crate) fn set_text_clip_rect(actor: &mut Actor, rect: [f32; 4]) {
    if let Actor::Text { clip, .. } = actor {
        *clip = Some(rect);
    }
}

#[cfg(test)]
mod tests {
    use super::{scroll_anim_dir, scroll_dir};

    #[test]
    fn scroll_anim_dir_uses_input_direction_for_two_item_wheels() {
        assert_eq!(scroll_anim_dir(2, 0, 1, -1), -1);
        assert_eq!(scroll_anim_dir(2, 0, 1, 1), 1);
        assert_eq!(scroll_anim_dir(2, 1, 0, -1), -1);
        assert_eq!(scroll_anim_dir(2, 1, 0, 1), 1);
    }

    #[test]
    fn scroll_anim_dir_matches_index_direction_for_longer_wheels() {
        assert_eq!(scroll_anim_dir(4, 0, 1, -1), scroll_dir(4, 0, 1));
        assert_eq!(scroll_anim_dir(4, 1, 0, 1), scroll_dir(4, 1, 0));
    }
}
