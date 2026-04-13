pub mod categories;
pub mod classic;
pub mod downloads;
pub mod leaderboard;
pub mod replay;
pub mod song_search;

pub use classic::{build_overlay, RenderParams, FOCUS_TWEEN_SECONDS};
pub use downloads::*;
pub use leaderboard::*;
pub use replay::*;
pub use song_search::*;

use crate::engine::present::actors::Actor;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OpenSorts,
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
    SyncSong,
    SyncPack,
    PlayReplay,
    ShowLeaderboard,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub top_label: &'static str,
    pub bottom_label: &'static str,
    pub action: Action,
}

pub const ITEM_CATEGORY_SORTS: Item = Item {
    top_label: "",
    bottom_label: "SORTS...",
    action: Action::OpenSorts,
};
const ITEM_SORT_BY_GROUP: Item = Item {
    top_label: "Sort By",
    bottom_label: "Group",
    action: Action::SortByGroup,
};
const ITEM_SORT_BY_TITLE: Item = Item {
    top_label: "Sort By",
    bottom_label: "Title",
    action: Action::SortByTitle,
};
const ITEM_SORT_BY_ARTIST: Item = Item {
    top_label: "Sort By",
    bottom_label: "Artist",
    action: Action::SortByArtist,
};
const ITEM_SORT_BY_BPM: Item = Item {
    top_label: "Sort By",
    bottom_label: "BPM",
    action: Action::SortByBpm,
};
const ITEM_SORT_BY_LENGTH: Item = Item {
    top_label: "Sort By",
    bottom_label: "Length",
    action: Action::SortByLength,
};
const ITEM_SORT_BY_METER: Item = Item {
    top_label: "Sort By",
    bottom_label: "Level",
    action: Action::SortByMeter,
};
const ITEM_SORT_BY_POPULARITY: Item = Item {
    top_label: "Sort By",
    bottom_label: "Most Popular",
    action: Action::SortByPopularity,
};
const ITEM_SORT_BY_RECENT: Item = Item {
    top_label: "Sort By",
    bottom_label: "Recently Played",
    action: Action::SortByRecent,
};
pub const ITEM_SORT_BY_GENRE: Item = Item {
    top_label: "Sort By",
    bottom_label: "Genre",
    action: Action::SortByGenre,
};
pub const ITEM_SORT_BY_TOP_GRADES: Item = Item {
    top_label: "Sort By",
    bottom_label: "Machine Top Scores",
    action: Action::SortByTopGrades,
};
pub const ITEM_SORT_BY_POPULARITY_P1: Item = Item {
    top_label: "Sort By",
    bottom_label: "P1 Most Played",
    action: Action::SortByPopularityP1,
};
pub const ITEM_SORT_BY_POPULARITY_P2: Item = Item {
    top_label: "Sort By",
    bottom_label: "P2 Most Played",
    action: Action::SortByPopularityP2,
};
pub const ITEM_SORT_BY_RECENT_P1: Item = Item {
    top_label: "Sort By",
    bottom_label: "P1 Recent Songs",
    action: Action::SortByRecentP1,
};
pub const ITEM_SORT_BY_RECENT_P2: Item = Item {
    top_label: "Sort By",
    bottom_label: "P2 Recent Songs",
    action: Action::SortByRecentP2,
};
pub const ITEM_SORT_BY_TOP_GRADES_P1: Item = Item {
    top_label: "Sort By",
    bottom_label: "P1 Clear Rank",
    action: Action::SortByTopGradesP1,
};
pub const ITEM_SORT_BY_TOP_GRADES_P2: Item = Item {
    top_label: "Sort By",
    bottom_label: "P2 Clear Rank",
    action: Action::SortByTopGradesP2,
};
pub const ITEM_SWITCH_TO_SINGLE: Item = Item {
    top_label: "Change Style To",
    bottom_label: "Single",
    action: Action::SwitchToSingle,
};
pub const ITEM_SWITCH_TO_DOUBLE: Item = Item {
    top_label: "Change Style To",
    bottom_label: "Double",
    action: Action::SwitchToDouble,
};
pub const ITEM_TEST_INPUT: Item = Item {
    top_label: "Feeling salty?",
    bottom_label: "Test Input",
    action: Action::TestInput,
};
pub const ITEM_SONG_SEARCH: Item = Item {
    top_label: "Wherefore Art Thou?",
    bottom_label: "Song Search",
    action: Action::SongSearch,
};
pub const ITEM_SWITCH_PROFILE: Item = Item {
    top_label: "Next Please",
    bottom_label: "Switch Profile",
    action: Action::SwitchProfile,
};
pub const ITEM_RELOAD_SONGS_COURSES: Item = Item {
    top_label: "Take a Breather~",
    bottom_label: "Load New Songs",
    action: Action::ReloadSongsCourses,
};
pub const ITEM_SHOW_LOBBIES: Item = Item {
    top_label: "Friends Online?",
    bottom_label: "Online Lobbies",
    action: Action::ShowLobbies,
};
pub const ITEM_VIEW_DOWNLOADS: Item = Item {
    top_label: "Need More RAM",
    bottom_label: "View Downloads",
    action: Action::ViewDownloads,
};
pub const ITEM_SYNC_SONG: Item = Item {
    top_label: "Sync",
    bottom_label: "null-or-die",
    action: Action::SyncSong,
};
pub const ITEM_SYNC_PACK: Item = Item {
    top_label: "Sync",
    bottom_label: "Sync Pack",
    action: Action::SyncPack,
};
pub const ITEM_PLAY_REPLAY: Item = Item {
    top_label: "Machine Data",
    bottom_label: "Play Replay",
    action: Action::PlayReplay,
};
pub const ITEM_SHOW_LEADERBOARD: Item = Item {
    top_label: "GrooveStats",
    bottom_label: "Leaderboard",
    action: Action::ShowLeaderboard,
};
pub const ITEM_TOGGLE_FAVORITE: Item = Item {
    top_label: "I'm Lovin' It",
    bottom_label: "Add Favorite",
    action: Action::ToggleFavorite,
};
pub const ITEM_SORT_BY_FAVORITES: Item = Item {
    top_label: "Check Out My Mix Tape",
    bottom_label: "Favorites",
    action: Action::SortByFavorites,
};
const ITEM_GO_BACK: Item = Item {
    top_label: "Options",
    bottom_label: "Go Back",
    action: Action::BackToMain,
};

pub const ITEMS_SORTS: [Item; 11] = [
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
    ITEM_GO_BACK,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Sorts,
}

#[derive(Clone, Debug)]
pub enum State {
    Hidden,
    Classic { page: Page, selected_index: usize },
    Categories(categories::VisibleState),
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
pub(crate) fn set_text_clip_rect(actor: &mut Actor, rect: [f32; 4]) {
    if let Actor::Text { clip, .. } = actor {
        *clip = Some(rect);
    }
}
