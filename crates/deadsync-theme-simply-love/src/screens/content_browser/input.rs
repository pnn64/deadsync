//! Every key the browser answers to.
//!
//! The shape is the ITGmania browser's, and it is worth stating because it is
//! not the obvious one:
//!
//! * UP and DOWN move within whatever you are in.
//! * LEFT and RIGHT **page** the list. They do not change tabs from inside a
//!   list, and they do not wrap -- paging off the end of nine thousand packs
//!   to land back at the top is never what anyone meant.
//! * BACK climbs one rung at a time -- rows to the strip above them, the strip
//!   to the tabs -- and only leaves from the tabs. Paging deep into a year and
//!   losing all of it to one press is a poor trade.
//! * START on a pack opens its detail page. Downloading only ever happens from
//!   there: selecting a pack in a list must never cost 400 MB.

use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use deadsync_input::{InputEvent, VirtualAction};

use super::layout as lo;
use crate::SimplyLoveEffect as ThemeEffect;
use crate::screens::Screen;
use crate::screens::content_browser::state::{
    FEATURED_COLUMNS, NAV_INITIAL_HOLD_DELAY, NAV_REPEAT_INTERVAL, NavHold, PendingTurn,
    QUERY_MAX_CHARS, ReloadPrompt, State, TABS, Tab, Zone, at_list_end, clamp_cursor,
    clamp_doubles, doubles_column, doubles_cursor, doubles_showing, downloads_active,
    featured_goto, featured_page, featured_row_col, focused_pack, grid_landable, installed_at,
    installed_cell, installed_goto, installed_page, installed_showing, list_page, more_loading,
    rebuild_results, row_count, search_field_showing, suggested_sync, total_pages, visible_rows,
    years_showing,
};
use deadsync_chart::song::SyncPref;

/// What a keypress did, so the sound and the effect are chosen once rather
/// than at every call site.
// Not Copy: one variant carries the name of the pack to delete, and that name
// has to be the one the cursor was on when the question was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    None,
    /// Nothing there. The original plays its own sound for this rather than
    /// staying silent, so the edge of a list feels solid.
    Invalid,
    Moved,
    Opened,
    Closed,
    Download(u64),
    Refresh,
    /// Ask about deleting, which is not the same as deleting.
    Confirm,
    Delete(String),
    /// Ask the site for the next page of the list.
    LoadMore,
    SetSync {
        group_name: String,
        itg: bool,
    },
    Leave,
}

pub fn handle_input(state: &mut State, event: &InputEvent) -> ThemeEffect {
    if !event.pressed {
        if let Some(delta) = nav_delta(event.action)
            && state.nav_hold.is_some_and(|hold| hold.delta == delta)
        {
            state.nav_hold = None;
        }
        return ThemeEffect::None;
    }

    match press(state, event.action) {
        Outcome::None => ThemeEffect::None,
        Outcome::Invalid => crate::effects::sfx("assets/sounds/common_invalid.ogg"),
        Outcome::Moved | Outcome::Closed => crate::effects::sfx("assets/sounds/change.ogg"),
        Outcome::Opened => crate::effects::sfx("assets/sounds/start.ogg"),
        Outcome::Refresh => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            online(crate::SimplyLoveOnlineRequest::RefreshStepManiaOnlineCatalog),
        ),
        Outcome::Confirm => crate::effects::sfx("assets/sounds/start.ogg"),
        Outcome::LoadMore => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            online(crate::SimplyLoveOnlineRequest::LoadMoreStepManiaOnlinePacks),
        ),
        Outcome::SetSync { group_name, itg } => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
                crate::SimplyLoveContentRequest::SetPackSync { group_name, itg },
            )),
        ),
        Outcome::Delete(group_name) => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
                crate::SimplyLoveContentRequest::DeletePack { group_name },
            )),
        ),
        Outcome::Download(pack_id) => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            online(crate::SimplyLoveOnlineRequest::DownloadStepManiaOnlinePack { pack_id }),
        ),
        Outcome::Leave => ThemeEffect::NavigateNoFade(Screen::Menu),
    }
}

fn online(request: crate::SimplyLoveOnlineRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(request))
}

fn press(state: &mut State, action: VirtualAction) -> Outcome {
    // Any other press is the reader going somewhere else; a page turn still
    // waiting on its fetch is theirs to cancel. A Right that asks again sets
    // it again below.
    state.pending_turn = None;
    // The reload question is the only thing on screen while it is up.
    if state.reload_prompt.is_some() {
        return press_reload(state, action);
    }
    match state.zone {
        Zone::Detail => press_detail(state, action),
        Zone::Tabs => press_tabs(state, action),
        Zone::Featured => press_featured(state, action),
        Zone::Years => press_years(state, action),
        Zone::DoublesPick => press_doubles_pick(state, action),
        Zone::DoublesRows => press_doubles_rows(state, action),
        Zone::Installed => press_installed(state, action),
        Zone::List => press_list(state, action),
    }
}

// --- the search box -----------------------------------------------------------

/// Put the reader on the search tab, with the box live.
fn focus_search(state: &mut State) {
    if let Some(index) = TABS.iter().position(|tab| *tab == Tab::Search) {
        state.tab_index = index;
    }
    state.zone = Zone::List;
    state.caret_elapsed = 0.0;
    state.nav_hold = None;
}

/// Change the query and re-derive the list.
///
/// The list answers from the catalogue's own names immediately; the two
/// network passes wait for the typing to stop -- see `SEARCH_DEBOUNCE`.
fn edit_query(state: &mut State, edit: impl FnOnce(&mut String)) -> bool {
    let before = state.query.clone();
    edit(&mut state.query);
    if state.query == before {
        return false;
    }
    state.query_idle = 0.0;
    state.caret_elapsed = 0.0;
    state.search_why.clear();
    rebuild_results(state, None);
    true
}

// --- the tab strip ------------------------------------------------------------

fn press_tabs(state: &mut State, action: VirtualAction) -> Outcome {
    if let Some(delta) = nav_delta(action) {
        if delta > 0 {
            return enter_body(state);
        }
        return Outcome::None;
    }
    if let Some(delta) = tab_delta(action) {
        return move_tab(state, delta);
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => enter_body(state),
        VirtualAction::p1_back | VirtualAction::p2_back => leave(state),
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

/// Coming down off the tabs, into whichever thing this tab actually shows.
fn enter_body(state: &mut State) -> Outcome {
    state.zone = if years_showing(state) {
        Zone::Years
    } else if doubles_showing(state) {
        // The doubles tab opens on its column picker, on a column that has
        // something to look at.
        if state.doubles_left.is_empty() && !state.doubles_right.is_empty() {
            state.doubles_column = 1;
            clamp_doubles(state);
        }
        Zone::DoublesPick
    } else if installed_showing(state) {
        Zone::Installed
    } else if grid_landable(state) {
        Zone::Featured
    } else {
        Zone::List
    };
    Outcome::Moved
}

// --- the library grid ---------------------------------------------------------

/// Two columns of eleven, paged.
///
/// Every move is to an absolute (page, column, row), and a move that lands on
/// nothing is refused rather than clamped -- which is what makes the edges of
/// the grid feel solid instead of mushy.
fn press_installed(state: &mut State, action: VirtualAction) -> Outcome {
    // Same rule as the delete confirm: while a dialog is up it is the only
    // thing on screen, so it is the only thing the keys answer.
    if state.syncing.is_some() {
        if tab_delta(action).is_some() {
            state.sync_choice = match state.sync_choice {
                SyncPref::Itg => SyncPref::Null,
                SyncPref::Default | SyncPref::Null => SyncPref::Itg,
            };
            return Outcome::Moved;
        }
        return match action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                let itg = state.sync_choice == SyncPref::Itg;
                match state.syncing.take() {
                    Some(group_name) => Outcome::SetSync { group_name, itg },
                    None => Outcome::None,
                }
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                state.syncing = None;
                Outcome::Closed
            }
            _ => Outcome::None,
        };
    }

    // While a delete is being asked about, the grid answers that question and
    // nothing else -- a cursor that can still move is a cursor that can delete
    // a pack the reader was no longer looking at.
    if state.removing.is_some() {
        return match action {
            VirtualAction::p1_start | VirtualAction::p2_start => match state.removing.take() {
                Some(name) => Outcome::Delete(name),
                None => Outcome::None,
            },
            VirtualAction::p1_back | VirtualAction::p2_back => {
                state.removing = None;
                Outcome::Closed
            }
            _ => Outcome::None,
        };
    }

    let page = installed_page(state) as isize;
    let (column, row) = installed_cell(state);
    let (column, row) = (column as isize, row as isize);

    if let Some(delta) = nav_delta(action) {
        if delta < 0 {
            // up the column, carrying back a page at its top
            if installed_goto(state, page, column, row - 1)
                || installed_goto(state, page - 1, column, lo::INST_PER_COL as isize - 1)
            {
                return Outcome::Moved;
            }
            state.zone = Zone::Tabs;
            return Outcome::Moved;
        }
        // down the column, carrying to the next page at its bottom
        return if installed_goto(state, page, column, row + 1)
            || installed_goto(state, page + 1, column, 0)
        {
            Outcome::Moved
        } else {
            Outcome::Invalid
        };
    }

    if let Some(delta) = tab_delta(action) {
        let moved = if delta < 0 {
            installed_goto(state, page, column - 1, row)
                || installed_goto(state, page - 1, lo::INST_COLS as isize - 1, row)
        } else {
            installed_goto(state, page, column + 1, row) || installed_goto(state, page + 1, 0, row)
        };
        return if moved {
            Outcome::Moved
        } else {
            Outcome::Invalid
        };
    }

    match action {
        // START is the sync writer, as it is in the original -- the library is
        // where you go to fix a pack you already have. The pack's catalogue
        // page is reachable from any of the other tabs.
        VirtualAction::p1_start | VirtualAction::p2_start => match installed_at(state).cloned() {
            Some(entry) => {
                state.sync_choice = suggested_sync(state, &entry);
                state.syncing = Some(entry.name);
                state.remove_result = None;
                Outcome::Confirm
            }
            None => Outcome::Invalid,
        },
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = Zone::Tabs;
            Outcome::Closed
        }
        // SELECT removes, START opens. They are this way round because
        // removing is the rarer act and the one that cannot be undone, so it
        // is the one that stays off the button every other list uses to open
        // something with.
        VirtualAction::p1_select | VirtualAction::p2_select => {
            match installed_at(state).map(|entry| entry.name.clone()) {
                Some(name) => {
                    state.removing = Some(name);
                    state.remove_result = None;
                    Outcome::Confirm
                }
                None => Outcome::Invalid,
            }
        }
        _ => Outcome::None,
    }
}

/// Out of the browser -- or first, when this session installed packs, the
/// question of whether to reload songs so they show up.
fn leave(state: &mut State) -> Outcome {
    // Not while a download is still landing: a reload now would race its
    // unzip and miss the pack. It carries on in the background, and the next
    // way out asks again.
    if !state.installed_dirs.is_empty() && !downloads_active(state) {
        state.reload_prompt = Some(ReloadPrompt::default());
        return Outcome::Closed;
    }
    Outcome::Leave
}

/// The reload dialog: LEFT/RIGHT choose, START goes, BACK leaves without.
fn press_reload(state: &mut State, action: VirtualAction) -> Outcome {
    let Some(prompt) = state.reload_prompt.as_mut() else {
        return Outcome::None;
    };
    if prompt.reloading {
        return Outcome::None;
    }
    if let Some(delta) = tab_delta(action) {
        let want = usize::from(delta > 0);
        if want == prompt.choice {
            return Outcome::Invalid;
        }
        prompt.choice = want;
        return Outcome::Moved;
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            if prompt.choice == 0 {
                prompt.reloading = true;
                state.pending_reload_dirs = state.installed_dirs.clone();
                Outcome::Confirm
            } else {
                // "Not yet" is the same as backing out.
                state.reload_prompt = None;
                Outcome::Leave
            }
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.reload_prompt = None;
            Outcome::Leave
        }
        _ => Outcome::None,
    }
}

fn move_tab(state: &mut State, delta: isize) -> Outcome {
    let len = TABS.len() as isize;
    let next = (state.tab_index as isize + delta).rem_euclid(len) as usize;
    if next == state.tab_index {
        return Outcome::None;
    }
    state.tab_index = next;
    // A query belongs to the tab it was typed on. Leaving SEARCH with one
    // still running made every other tab show the same results, so the tabs
    // looked like they had stopped working.
    if TABS[next] != Tab::Search && !state.query.is_empty() {
        state.query.clear();
        state.query_idle = f32::MAX;
        state.search_why.clear();
    }
    // A tab change is a different question, so it starts at the top rather
    // than trying to keep a selection that may not be in the answer.
    rebuild_results(state, None);
    Outcome::Moved
}

// --- the featured grid --------------------------------------------------------

fn press_featured(state: &mut State, action: VirtualAction) -> Outcome {
    let page = featured_page(state) as isize;
    let (row, column) = featured_row_col(state);
    let (row, column) = (row as isize, column as isize);

    // UP and DOWN are the only way between the grid's two rows, and running
    // off the top or the bottom of it leaves the grid entirely.
    if let Some(delta) = nav_delta(action) {
        if featured_goto(state, page, row + delta, column) {
            return Outcome::Moved;
        }
        state.zone = if delta < 0 { Zone::Tabs } else { Zone::List };
        return Outcome::Moved;
    }

    // LEFT and RIGHT walk the row and carry a whole page at its edge -- they
    // never wrap into the row below, which is the thing that made the grid
    // feel like a list folded in half.
    if let Some(delta) = tab_delta(action) {
        let moved = if delta < 0 {
            if column > 0 {
                featured_goto(state, page, row, column - 1)
            } else {
                featured_goto(state, page - 1, row, FEATURED_COLUMNS as isize - 1)
            }
        } else if column < FEATURED_COLUMNS as isize - 1 {
            featured_goto(state, page, row, column + 1)
        } else {
            featured_goto(state, page + 1, row, 0)
        };
        return if moved {
            Outcome::Moved
        } else {
            Outcome::Invalid
        };
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            if state.featured.get(state.featured_index).is_some() {
                state.featured_from = true;
                state.zone = Zone::Detail;
                Outcome::Opened
            } else {
                Outcome::Invalid
            }
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = Zone::Tabs;
            Outcome::Closed
        }
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

// --- the year strip -----------------------------------------------------------

fn press_years(state: &mut State, action: VirtualAction) -> Outcome {
    if let Some(delta) = nav_delta(action) {
        if delta > 0 {
            state.zone = Zone::List;
            return Outcome::Moved;
        }
        state.zone = Zone::Tabs;
        return Outcome::Moved;
    }
    if let Some(delta) = tab_delta(action) {
        // The year is picked immediately on the move -- there is no separate
        // confirm -- and the strip does not wrap.
        let next = state.year_slot as isize + delta;
        if next < 0 || next >= lo::year_slots() as isize {
            return Outcome::Invalid;
        }
        state.year_slot = next as usize;
        rebuild_results(state, None);
        return Outcome::Moved;
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            state.zone = Zone::List;
            Outcome::Moved
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = Zone::Tabs;
            Outcome::Closed
        }
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

// --- the doubles view ---------------------------------------------------------

fn press_doubles_pick(state: &mut State, action: VirtualAction) -> Outcome {
    if let Some(delta) = nav_delta(action) {
        if delta > 0 {
            return dive_into_column(state);
        }
        state.zone = Zone::Tabs;
        return Outcome::Moved;
    }
    if let Some(delta) = tab_delta(action) {
        let want = if delta > 0 { 1 } else { 0 };
        if want == state.doubles_column {
            return Outcome::Invalid;
        }
        state.doubles_column = want;
        clamp_doubles(state);
        return Outcome::Moved;
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => dive_into_column(state),
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = Zone::Tabs;
            Outcome::Closed
        }
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

/// An empty column can be looked at, not entered.
fn dive_into_column(state: &mut State) -> Outcome {
    if doubles_column(state, state.doubles_column).is_empty() {
        return Outcome::Invalid;
    }
    state.zone = Zone::DoublesRows;
    Outcome::Moved
}

fn press_doubles_rows(state: &mut State, action: VirtualAction) -> Outcome {
    let here = doubles_cursor(state);
    if let Some(delta) = nav_delta(action) {
        if delta < 0 && here == 0 {
            // The top of a column climbs back to the column picker.
            state.zone = Zone::DoublesPick;
            return Outcome::Moved;
        }
        let next = here as isize + delta;
        return if go_to_doubles(state, state.doubles_column, next) {
            Outcome::Moved
        } else {
            Outcome::Invalid
        };
    }
    if let Some(delta) = tab_delta(action) {
        // Left and Right page the column you are in. Only at the end of one do
        // they hand over to the other, so the columns are sticky and a press
        // meant for paging cannot throw the cursor across the screen.
        let mine = doubles_column(state, state.doubles_column).len();
        let forwards = delta > 0;
        let at_end = if forwards {
            here + 1 >= mine
        } else {
            here == 0
        };
        if !at_end {
            let step = lo::DBL_ROWS as isize * delta;
            let want = (here as isize + step).clamp(0, mine as isize - 1);
            if go_to_doubles(state, state.doubles_column, want) {
                return Outcome::Moved;
            }
        }
        let other = usize::from(forwards);
        let other_len = doubles_column(state, other).len();
        if other != state.doubles_column && other_len > 0 {
            let landing = if forwards { 0 } else { other_len - 1 };
            if go_to_doubles(state, other, landing as isize) {
                return Outcome::Moved;
            }
        }
        return Outcome::Invalid;
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            if focused_pack(state).is_some() {
                state.featured_from = false;
                state.zone = Zone::Detail;
                Outcome::Opened
            } else {
                Outcome::Invalid
            }
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = Zone::DoublesPick;
            Outcome::Closed
        }
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

/// Put the cursor on an absolute row of a column, scrolling that column when
/// the row is off screen. False when there is no such row, which is what makes
/// the ends of a column feel solid.
fn go_to_doubles(state: &mut State, column: usize, index: isize) -> bool {
    let column = column.min(1);
    let len = doubles_column(state, column).len() as isize;
    if index < 0 || index >= len {
        return false;
    }
    let index = index as usize;
    let window = &mut state.doubles_window[column];
    if index < *window {
        *window = index;
    } else if index >= *window + lo::DBL_ROWS {
        *window = index + 1 - lo::DBL_ROWS;
    }
    state.doubles_row = index - *window;
    state.doubles_column = column;
    true
}

// --- the pack list ------------------------------------------------------------

fn press_list(state: &mut State, action: VirtualAction) -> Outcome {
    if let Some(delta) = nav_delta(action) {
        // Up off the top row reaches whatever strip is above the list, rather
        // than wrapping to the bottom of nine thousand packs.
        if delta < 0 && state.cursor == 0 {
            state.zone = if years_showing(state) {
                Zone::Years
            } else if grid_landable(state) {
                Zone::Featured
            } else {
                Zone::Tabs
            };
            state.nav_hold = None;
            return Outcome::Moved;
        }
        // Down off the last pack of a list with more to find asks for the next
        // helping. The cursor stays on that pack, so it is still there when the
        // new rows land under it, and the next Down steps onto them.
        if delta > 0 && at_list_end(state) {
            state.nav_hold = None;
            return if more_loading(state) {
                Outcome::None
            } else {
                Outcome::LoadMore
            };
        }
        state.nav_hold = Some(NavHold {
            delta,
            held_for: std::time::Duration::ZERO,
            since_scroll: std::time::Duration::ZERO,
        });
        return move_cursor(state, delta);
    }
    if let Some(delta) = tab_delta(action) {
        return page(state, delta);
    }
    match action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            if focused_pack(state).is_some() {
                state.featured_from = false;
                state.zone = Zone::Detail;
                Outcome::Opened
            } else {
                Outcome::Invalid
            }
        }
        // Back climbs a rung rather than leaving outright, so a reader paged
        // deep into a list does not lose it to one press.
        VirtualAction::p1_back | VirtualAction::p2_back => {
            state.zone = if years_showing(state) {
                Zone::Years
            } else {
                Zone::Tabs
            };
            Outcome::Closed
        }
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

fn move_cursor(state: &mut State, delta: isize) -> Outcome {
    let len = row_count(state) as isize;
    if len == 0 {
        return Outcome::None;
    }
    // A held Down stops at the last pack of a list with more to find, rather
    // than wrapping past the point where more would be asked for.
    if delta > 0 && at_list_end(state) {
        return Outcome::None;
    }
    let next = (state.cursor as isize + delta).rem_euclid(len) as usize;
    if next == state.cursor {
        return Outcome::None;
    }
    state.cursor = next;
    clamp_cursor(state);
    Outcome::Moved
}

/// A page at a time, clamped rather than wrapping.
fn page(state: &mut State, direction: isize) -> Outcome {
    let here = list_page(state) as isize;
    let want = here + direction;
    // Past the last page of a list with more to find is the next helping, as
    // the original's page-right is -- even on a list that has found nothing
    // yet, which is exactly when the reader needs to be able to ask.
    if direction > 0 && want >= total_pages(state) as isize && state.can_load_more {
        // and turns to the new page once it has a pack on it
        state.pending_turn = Some(PendingTurn {
            tab_index: state.tab_index,
            page: want as usize,
            saw_loading: false,
        });
        return if more_loading(state) {
            Outcome::None
        } else {
            Outcome::LoadMore
        };
    }
    if row_count(state) == 0 {
        return Outcome::Invalid;
    }
    if want < 0 || want >= total_pages(state) as isize {
        return Outcome::Invalid;
    }
    // A page turn lands on the top of the new page, as the original does.
    state.cursor = (want as usize * visible_rows()).min(row_count(state) - 1);
    Outcome::Moved
}

// --- the detail page ----------------------------------------------------------

fn press_detail(state: &mut State, action: VirtualAction) -> Outcome {
    if let Some(delta) = nav_delta(action) {
        return move_song(state, delta);
    }
    if let Some(delta) = tab_delta(action) {
        return move_song(state, delta * lo::SONG_ROWS as isize);
    }
    match action {
        VirtualAction::p1_back | VirtualAction::p2_back => {
            // Back to wherever it was opened from.
            state.zone = if state.featured_from {
                Zone::Featured
            } else if doubles_showing(state) {
                Zone::DoublesRows
            } else if installed_showing(state) {
                Zone::Installed
            } else {
                Zone::List
            };
            state.featured_from = false;
            Outcome::Closed
        }
        // The one place a download starts, and only ever on an explicit press.
        VirtualAction::p1_start | VirtualAction::p2_start => match focused_pack(state) {
            Some(pack) => Outcome::Download(pack.id),
            None => Outcome::Invalid,
        },
        VirtualAction::p1_select | VirtualAction::p2_select => Outcome::Refresh,
        _ => Outcome::None,
    }
}

/// Move the song pick, dragging the window with it.
fn move_song(state: &mut State, delta: isize) -> Outcome {
    let Some(page) = state.page.page.as_ref() else {
        return Outcome::None;
    };
    let total = page.songs.len();
    if total == 0 {
        return Outcome::None;
    }
    let next = (state.song_pick as isize + delta).clamp(0, total as isize - 1) as usize;
    if next == state.song_pick {
        return Outcome::Invalid;
    }
    state.song_pick = next;
    if state.song_pick < state.song_window {
        state.song_window = state.song_pick;
    } else if state.song_pick >= state.song_window + lo::SONG_ROWS {
        state.song_window = state.song_pick + 1 - lo::SONG_ROWS;
    }
    Outcome::Moved
}

// --- shared -------------------------------------------------------------------

const fn nav_delta(action: VirtualAction) -> Option<isize> {
    match action {
        VirtualAction::p1_up | VirtualAction::p1_menu_up => Some(-1),
        VirtualAction::p2_up | VirtualAction::p2_menu_up => Some(-1),
        VirtualAction::p1_down | VirtualAction::p1_menu_down => Some(1),
        VirtualAction::p2_down | VirtualAction::p2_menu_down => Some(1),
        _ => None,
    }
}

const fn tab_delta(action: VirtualAction) -> Option<isize> {
    match action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => Some(-1),
        VirtualAction::p2_left | VirtualAction::p2_menu_left => Some(-1),
        VirtualAction::p1_right | VirtualAction::p1_menu_right => Some(1),
        VirtualAction::p2_right | VirtualAction::p2_menu_right => Some(1),
        _ => None,
    }
}

/// Advance a held direction. Called from `update`, because a hold produces
/// movement while nothing is being pressed.
pub(super) fn repeat_nav_hold(state: &mut State, dt: f32) {
    if row_count(state) <= 1 || state.zone != Zone::List {
        state.nav_hold = None;
        return;
    }
    let Some(hold) = state.nav_hold.as_mut() else {
        return;
    };
    let elapsed = std::time::Duration::from_secs_f32(dt);
    hold.held_for = hold.held_for.saturating_add(elapsed);
    hold.since_scroll = hold.since_scroll.saturating_add(elapsed);
    if hold.held_for < NAV_INITIAL_HOLD_DELAY || hold.since_scroll < NAV_REPEAT_INTERVAL {
        return;
    }
    let delta = hold.delta;
    if move_cursor(state, delta) == Outcome::Moved
        && let Some(hold) = state.nav_hold.as_mut()
    {
        hold.since_scroll = std::time::Duration::ZERO;
    }
}

/// Keyboard input.
///
/// Typing searches, wherever the reader is. On the SEARCH tab the characters
/// go straight into the box that is already on screen; anywhere else the first
/// character carries them to that tab and lands there too, so a keystroke is
/// never swallowed. Text entry owns the keyboard while the browser is open, so
/// a WASD-style pad mapping cannot scroll the list out from under someone
/// mid-word.
pub fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    if let Some(key) = key {
        if !key.pressed {
            return false;
        }
        // A pack's page is read with the pad's actions, not typed into: Escape
        // there is BACK, out to the results it was opened from -- not a key
        // that clears the search those results came from. The reload question
        // is answered the same way.
        if state.zone == Zone::Detail || state.reload_prompt.is_some() {
            return false;
        }
        if search_field_showing(state) {
            match key.code {
                KeyCode::Backspace => {
                    if edit_query(state, |query| {
                        query.pop();
                    }) {
                        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    }
                    return true;
                }
                // The box is already live, so there is nothing to confirm --
                // Enter just puts the reader on the answer. Once they are on
                // it, Enter is START: it opens the pack under the cursor.
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if state.zone == Zone::List {
                        return false;
                    }
                    state.zone = Zone::List;
                    state.cursor = 0;
                    return true;
                }
                KeyCode::Escape => {
                    if edit_query(state, String::clear) {
                        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    }
                    return true;
                }
                _ => {}
            }
        }
        return match key.code {
            KeyCode::PageUp | KeyCode::PageDown => {
                state.nav_hold = None;
                let direction = if key.code == KeyCode::PageUp { -1 } else { 1 };
                if page(state, direction) == Outcome::Moved {
                    effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                }
                true
            }
            // Escape clears a committed search before it leaves, so a query
            // costs one key to undo rather than a trip out through the menu.
            KeyCode::Escape => {
                if state.query.is_empty() {
                    false
                } else {
                    state.query.clear();
                    rebuild_results(state, None);
                    effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
                    true
                }
            }
            _ => false,
        };
    }

    let Some(text) = text else {
        return false;
    };
    let printable: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if printable.is_empty() {
        return false;
    }
    // Reading a pack is not searching for one.
    if state.zone == Zone::Detail
        || state.removing.is_some()
        || state.syncing.is_some()
        || state.reload_prompt.is_some()
    {
        return false;
    }
    if !search_field_showing(state) {
        focus_search(state);
    }
    if edit_query(state, |query| {
        for ch in printable.chars() {
            if query.chars().count() >= QUERY_MAX_CHARS {
                break;
            }
            query.push(ch);
        }
    }) {
        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::content_browser::state;
    use crate::screens::content_browser::state::FEATURED_VISIBLE;

    /// Enter on a search result is START, so it opens the pack; before the
    /// reader is on the results it puts them there.
    #[test]
    fn enter_opens_a_search_result_rather_than_being_swallowed() {
        let mut state = browsing(3);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Search).unwrap();
        state.query = "rosewood".to_owned();
        state.zone = Zone::Tabs;
        let mut effects = Vec::new();
        let enter = raw_key(KeyCode::Enter);
        assert!(handle_raw_key_event(
            &mut state,
            Some(&enter),
            None,
            &mut effects
        ));
        assert_eq!(state.zone, Zone::List, "on the results");

        state.cursor = 2;
        assert!(
            !handle_raw_key_event(&mut state, Some(&enter), None, &mut effects),
            "left for START"
        );
        assert_eq!(state.cursor, 2, "and not thrown back to the top");
    }

    /// A pack opened from a search is read, not typed into: Escape and
    /// Backspace leave the query alone, so BACK returns to the same results.
    #[test]
    fn a_pack_opened_from_search_keeps_the_search() {
        let mut state = browsing(3);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Search).unwrap();
        state.query = "rosewood".to_owned();
        state.zone = Zone::Detail;
        let mut effects = Vec::new();
        for code in [KeyCode::Escape, KeyCode::Backspace, KeyCode::PageDown] {
            assert!(
                !handle_raw_key_event(&mut state, Some(&raw_key(code)), None, &mut effects),
                "{code:?} is the page's, not the search box's"
            );
        }
        assert_eq!(state.query, "rosewood");
        assert_eq!(press(&mut state, BACK), Outcome::Closed);
        assert_eq!(state.zone, Zone::List);
        assert_eq!(state.query, "rosewood", "back on the same search");
        assert_eq!(state.results.len(), 3);
    }

    /// Leaving after installs asks whether to reload songs; "Not yet" and
    /// BACK leave without, "Reload songs" hands the folders over and waits.
    #[test]
    fn leaving_after_installs_asks_to_reload_songs() {
        let mut state = state::init();
        state.zone = Zone::Tabs;
        assert_eq!(press(&mut state, BACK), Outcome::Leave, "nothing installed");

        state.installed_dirs = vec![std::path::PathBuf::from("songs/New Pack")];
        assert_eq!(press(&mut state, BACK), Outcome::Closed, "asked first");
        assert!(state.reload_prompt.is_some());
        assert_eq!(
            press(&mut state, LEFT),
            Outcome::Invalid,
            "already on reload"
        );
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(press(&mut state, START), Outcome::Leave, "not yet");
        assert!(state.reload_prompt.is_none());
        assert_eq!(state.installed_dirs.len(), 1, "still owed for next time");

        assert_eq!(press(&mut state, BACK), Outcome::Closed);
        assert_eq!(press(&mut state, START), Outcome::Confirm, "reload songs");
        assert_eq!(state.pending_reload_dirs.len(), 1);
        assert_eq!(press(&mut state, BACK), Outcome::None, "waits for it");
        assert!(state.reload_prompt.is_some());
    }

    /// RIGHT on the last page asks for more and remembers it asked to turn
    /// the page; any other press is the reader moving on.
    #[test]
    fn right_past_the_last_page_waits_to_turn_it() {
        let mut state = browsing(7);
        state.zone = Zone::List;
        state.can_load_more = true;
        assert_eq!(press(&mut state, RIGHT), Outcome::LoadMore);
        let turn = state.pending_turn.expect("a turn is waiting");
        assert_eq!(turn.page, 1);
        assert_eq!(state.cursor, 0, "not until the page exists");

        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert!(state.pending_turn.is_none(), "cancelled by moving");
    }

    fn browsing(results: usize) -> State {
        let mut base = state::init();
        base.results = (0..results).collect();
        base
    }

    const LEFT: VirtualAction = VirtualAction::p1_menu_left;
    const RIGHT: VirtualAction = VirtualAction::p1_menu_right;
    const UP: VirtualAction = VirtualAction::p1_menu_up;
    const DOWN: VirtualAction = VirtualAction::p1_menu_down;
    const BACK: VirtualAction = VirtualAction::p1_back;
    const START: VirtualAction = VirtualAction::p1_start;
    const SELECT: VirtualAction = VirtualAction::p1_select;

    fn raw_key(code: KeyCode) -> RawKeyboardEvent {
        RawKeyboardEvent {
            code,
            pressed: true,
            repeat: false,
            timestamp: std::time::Instant::now(),
            host_nanos: 0,
        }
    }

    /// LEFT and RIGHT page the list. They do not change tab and they do not
    /// wrap: this is the single navigation rule the browser is most often
    /// gotten wrong, because every other list in the theme moves by one.
    #[test]
    fn left_and_right_page_the_list_without_wrapping() {
        let mut state = browsing(20);
        state.zone = Zone::List;
        let tab_before = state.tab_index;

        assert_eq!(
            press(&mut state, LEFT),
            Outcome::Invalid,
            "already page one"
        );
        assert_eq!(state.cursor, 0);

        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(state.cursor, 7, "the top of page two");
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(state.cursor, 14);
        assert_eq!(press(&mut state, RIGHT), Outcome::Invalid, "last page");
        assert_eq!(press(&mut state, LEFT), Outcome::Moved);
        assert_eq!(state.cursor, 7);

        assert_eq!(state.tab_index, tab_before, "paging never changes the tab");
    }

    /// Down off the last pack of a list with more to find asks for the next
    /// helping and stays put, rather than wrapping to row one -- and while a
    /// helping is being gathered it does nothing at all.
    #[test]
    fn down_from_the_end_of_a_list_with_more_asks_for_it() {
        let mut state = browsing(7);
        state.zone = Zone::List;
        state.can_load_more = true;
        state.cursor = 6;

        assert_eq!(press(&mut state, DOWN), Outcome::LoadMore);
        assert_eq!(state.cursor, 6, "the cursor stays on the last pack");
        assert!(state.nav_hold.is_none(), "a held Down does not repeat it");
        assert_eq!(
            move_cursor(&mut state, 1),
            Outcome::None,
            "a repeat never wraps past it either"
        );

        // RIGHT on the last page is the same request, as the original's is.
        state.cursor = 2;
        assert_eq!(press(&mut state, RIGHT), Outcome::LoadMore);

        // While the details service is already fetching, neither does anything.
        state.details = std::sync::Arc::new(deadsync_online::smo_details::DetailsSnapshot {
            phase: deadsync_online::smo_details::DetailsPhase::Loading,
            ..deadsync_online::smo_details::DetailsSnapshot::default()
        });
        state.cursor = 6;
        assert_eq!(press(&mut state, DOWN), Outcome::None);
        assert_eq!(press(&mut state, RIGHT), Outcome::None);

        // An empty list that can find more is asked the same way: a helping
        // that turned up nothing is not the end of the list.
        let mut empty = browsing(0);
        empty.zone = Zone::List;
        empty.can_load_more = true;
        assert_eq!(press(&mut empty, DOWN), Outcome::LoadMore);
        assert_eq!(press(&mut empty, RIGHT), Outcome::LoadMore);
        assert_eq!(press(&mut empty, LEFT), Outcome::Invalid);

        // With nothing more to find, Down still wraps and Right is the end.
        let mut done = browsing(7);
        done.zone = Zone::List;
        done.cursor = 6;
        assert_eq!(press(&mut done, DOWN), Outcome::Moved);
        assert_eq!(done.cursor, 0);
        assert_eq!(press(&mut done, RIGHT), Outcome::Invalid);
    }

    /// Up off the top of the list reaches the strip above it, and BACK climbs
    /// the same rung rather than leaving the browser.
    #[test]
    fn back_climbs_one_rung_at_a_time() {
        let mut state = browsing(20);
        state.zone = Zone::List;
        assert_eq!(press(&mut state, BACK), Outcome::Closed);
        assert_eq!(state.zone, Zone::Tabs);
        assert_eq!(press(&mut state, BACK), Outcome::Leave, "and only then out");
    }

    #[test]
    fn up_from_the_top_row_reaches_the_tabs() {
        let mut state = browsing(20);
        state.zone = Zone::List;
        state.cursor = 3;
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.cursor, 2, "still in the list");
        state.cursor = 0;
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::Tabs);
    }

    /// The year strip picks on the move, does not wrap, and its ends report
    /// themselves rather than silently doing nothing.
    #[test]
    fn the_year_strip_moves_without_wrapping() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Years).unwrap();
        state.zone = Zone::Years;

        assert_eq!(press(&mut state, LEFT), Outcome::Invalid, "at the newest");
        assert_eq!(state.year_slot, 0);
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(state.year_slot, 1);

        state.year_slot = lo::year_slots() - 1;
        assert_eq!(press(&mut state, RIGHT), Outcome::Invalid, "OLDER is last");

        // and the strip is a rung between the tabs and the rows
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::Tabs);
        state.zone = Zone::Years;
        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(state.zone, Zone::List);
        // back out of the rows lands on the strip, not on the tabs
        assert_eq!(press(&mut state, BACK), Outcome::Closed);
        assert_eq!(state.zone, Zone::Years);
    }

    /// Deleting is the one act here that cannot be undone, so it is asked
    /// about twice -- and while the question is up, nothing else moves.
    #[test]
    fn deleting_a_pack_is_asked_about_before_it_happens() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Installed).unwrap();
        state.installed = vec![
            state::InstalledPack {
                name: "Keepers".to_owned(),
                lower: "keepers".to_owned(),
                songs: 12,
                sync: SyncPref::Default,
                banner: None,
            },
            state::InstalledPack {
                name: "Goners".to_owned(),
                lower: "goners".to_owned(),
                songs: 3,
                sync: SyncPref::Default,
                banner: None,
            },
        ];
        state.zone = Zone::Installed;
        state.installed_cursor = 1;

        // SELECT asks; it does not delete
        assert_eq!(press(&mut state, SELECT), Outcome::Confirm);
        assert_eq!(state.removing.as_deref(), Some("Goners"));

        // and while it is asking, the cursor cannot wander onto another pack
        assert_eq!(press(&mut state, UP), Outcome::None);
        assert_eq!(press(&mut state, LEFT), Outcome::None);
        assert_eq!(state.installed_cursor, 1);

        // BACK answers no
        assert_eq!(press(&mut state, BACK), Outcome::Closed);
        assert!(state.removing.is_none());
        assert_eq!(state.zone, Zone::Installed, "and stays in the grid");

        // START answers yes, and names the pack the cursor was actually on
        assert_eq!(press(&mut state, SELECT), Outcome::Confirm);
        assert_eq!(
            press(&mut state, START),
            Outcome::Delete("Goners".to_owned())
        );
        assert!(state.removing.is_none(), "the question is answered once");
    }

    /// The grid says what happened, including when it only partly happened --
    /// a song whose audio is still open does not go, and quietly showing one
    /// fewer row would be a lie.
    #[test]
    fn a_delete_reports_what_it_actually_did() {
        let mut state = browsing(0);
        state.removing = Some("Goners".to_owned());

        state::finish_pack_deletion(&mut state, Ok(3));
        assert_eq!(
            state.remove_result.as_deref(),
            Some("deleted 3 song folders")
        );
        assert!(state.removing.is_none());

        state::finish_pack_deletion(&mut state, Ok(0));
        assert_eq!(state.remove_result.as_deref(), Some("nothing was deleted"));

        state::finish_pack_deletion(&mut state, Err("in use".to_owned()));
        assert_eq!(
            state.remove_result.as_deref(),
            Some("could not delete: in use")
        );
    }

    /// The library grid moves by absolute cells and carries between pages at
    /// the ends of a column, rather than clamping.
    #[test]
    fn the_library_grid_carries_between_columns_and_pages() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Installed).unwrap();
        state.installed = (0..lo::INST_ROWS + 3)
            .map(|n| state::InstalledPack {
                name: format!("Pack {n}"),
                lower: format!("pack {n}"),
                songs: n,
                sync: SyncPref::Default,
                banner: None,
            })
            .collect();
        state.zone = Zone::Installed;

        // down the left column to its foot, then on to the next page
        state.installed_cursor = lo::INST_PER_COL - 1;
        assert_eq!(installed_cell(&state), (0, lo::INST_PER_COL - 1));
        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(installed_page(&state), 1, "carried to page two");
        assert_eq!(installed_cell(&state), (0, 0));

        // and back up again
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(installed_page(&state), 0);
        assert_eq!(installed_cell(&state), (0, lo::INST_PER_COL - 1));

        // across to the right column, and off its edge is nothing
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(installed_cell(&state), (1, lo::INST_PER_COL - 1));
        assert_eq!(press(&mut state, RIGHT), Outcome::Invalid);

        // the top of a column climbs to the tabs
        state.installed_cursor = 0;
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::Tabs);
    }

    /// The doubles view is a column picker with two lists under it, and the
    /// rungs run the same way in both directions.
    #[test]
    fn the_doubles_columns_are_picked_before_they_are_entered() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Doubles).unwrap();
        state.doubles_left = vec![0, 1, 2];
        state.doubles_right = vec![3, 4];
        state.zone = Zone::DoublesPick;

        assert_eq!(
            press(&mut state, LEFT),
            Outcome::Invalid,
            "already leftmost"
        );
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(state.doubles_column, 1);
        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(state.zone, Zone::DoublesRows);
        assert_eq!(doubles_cursor(&state), 0);

        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(doubles_cursor(&state), 1);
        assert_eq!(press(&mut state, DOWN), Outcome::Invalid, "end of column");

        // the top of a column climbs back to the picker, not to the tabs
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::DoublesPick);
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::Tabs);
    }

    /// An empty column can be looked at, not entered.
    #[test]
    fn an_empty_doubles_column_refuses_the_dive() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Doubles).unwrap();
        state.doubles_right = vec![1, 2];
        state.zone = Zone::DoublesPick;
        assert_eq!(press(&mut state, START), Outcome::Invalid);
        assert_eq!(state.zone, Zone::DoublesPick);
    }

    /// The columns are sticky: LEFT and RIGHT page the column you are in, and
    /// only at its very end do they hand over to the other one -- so a press
    /// meant for paging cannot throw the cursor across the screen.
    #[test]
    fn a_column_pages_before_it_hands_over() {
        let mut state = browsing(0);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Doubles).unwrap();
        state.doubles_left = vec![0, 1];
        state.doubles_right = vec![2, 3];
        state.doubles_column = 1;
        state.doubles_row = 1;
        state.zone = Zone::DoublesRows;

        // the last row of the right column: nothing further right
        assert_eq!(press(&mut state, RIGHT), Outcome::Invalid, "nothing right");
        // and not at its start, so LEFT pages within it rather than crossing
        assert_eq!(press(&mut state, LEFT), Outcome::Moved);
        assert_eq!(state.doubles_column, 1, "still in the right column");
        assert_eq!(doubles_cursor(&state), 0, "paged to its top");

        // now at its start, LEFT hands over to the left column's last row
        assert_eq!(press(&mut state, LEFT), Outcome::Moved);
        assert_eq!(state.doubles_column, 0, "handed to the left column");
        assert_eq!(doubles_cursor(&state), 1, "landing on its last row");
    }

    /// The box is the tab. There is no prompt to open and nothing to press
    /// first: a reader who arrives on SEARCH and types is already searching.
    #[test]
    fn the_search_box_is_live_the_moment_the_tab_is_open() {
        let mut state = browsing(20);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Search).unwrap();
        state.zone = Zone::Tabs;
        assert!(search_field_showing(&state));

        let mut effects = Vec::new();
        assert!(handle_raw_key_event(
            &mut state,
            None,
            Some("ky"),
            &mut effects
        ));
        assert_eq!(state.query, "ky", "straight into the box");

        // backspace edits it, escape empties it
        let key = raw_key(KeyCode::Backspace);
        assert!(handle_raw_key_event(
            &mut state,
            Some(&key),
            None,
            &mut effects
        ));
        assert_eq!(state.query, "k");

        let key = raw_key(KeyCode::Escape);
        assert!(handle_raw_key_event(
            &mut state,
            Some(&key),
            None,
            &mut effects
        ));
        assert!(state.query.is_empty());
    }

    /// The two network passes wait for the typing to stop. Every keystroke is
    /// a different query, and running them per key would be eight searches for
    /// an eight-letter charter's name -- while the catalogue's own names are
    /// matched locally in the meantime, so the list still answers at once.
    #[test]
    fn the_network_passes_wait_for_the_typing_to_stop() {
        let mut state = browsing(20);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Search).unwrap();
        let mut effects = Vec::new();

        handle_raw_key_event(&mut state, None, Some("r"), &mut effects);
        assert!(
            crate::screens::content_browser::state::wanted_search(&state).is_none(),
            "still typing"
        );

        handle_raw_key_event(&mut state, None, Some("o"), &mut effects);
        crate::screens::content_browser::state::update(&mut state, 0.1);
        assert!(
            crate::screens::content_browser::state::wanted_search(&state).is_none(),
            "not still enough"
        );

        crate::screens::content_browser::state::update(
            &mut state,
            crate::screens::content_browser::state::SEARCH_DEBOUNCE,
        );
        assert_eq!(
            crate::screens::content_browser::state::wanted_search(&state),
            Some("ro")
        );
    }

    /// Typing anywhere carries the reader to the search tab with that
    /// character already in the box, so a keystroke is never swallowed.
    #[test]
    fn typing_anywhere_carries_to_the_search_tab() {
        let mut state = browsing(20);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Pad).unwrap();
        state.zone = Zone::List;

        let mut effects = Vec::new();
        assert!(handle_raw_key_event(
            &mut state,
            None,
            Some("k"),
            &mut effects
        ));
        assert!(search_field_showing(&state), "carried to SEARCH");
        assert_eq!(state.query, "k", "and kept the character");

        assert!(handle_raw_key_event(
            &mut state,
            None,
            Some("y"),
            &mut effects
        ));
        assert_eq!(state.query, "ky");
    }

    /// A query belongs to the tab it was typed on. Carrying it to every other
    /// tab made them all show the same results, so the tabs looked broken.
    #[test]
    fn leaving_the_search_tab_clears_the_query() {
        let mut state = browsing(20);
        state.tab_index = TABS.iter().position(|t| *t == Tab::Search).unwrap();
        state.zone = Zone::Tabs;

        let mut effects = Vec::new();
        handle_raw_key_event(&mut state, None, Some("rosewood"), &mut effects);
        assert_eq!(state.query, "rosewood");

        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert!(!search_field_showing(&state), "a different tab");
        assert!(state.query.is_empty(), "and a different question");
    }

    /// The grid is a page, not a folded list. LEFT and RIGHT walk one row and
    /// carry a whole page at its edge; they never drop into the row below.
    #[test]
    fn the_featured_grid_pages_along_a_row_rather_than_wrapping() {
        let mut state = browsing(0);
        state.featured = (0..FEATURED_VISIBLE * 2).collect();
        state.zone = Zone::Featured;

        // the last card of the TOP row carries to the top row of page two,
        // not to the first card of the row underneath it
        state.featured_index = FEATURED_COLUMNS - 1;
        assert_eq!(press(&mut state, RIGHT), Outcome::Moved);
        assert_eq!(featured_page(&state), 1);
        assert_eq!(featured_row_col(&state), (0, 0), "same row, next page");
        assert_eq!(state.featured_window, FEATURED_VISIBLE);

        // and back again
        assert_eq!(press(&mut state, LEFT), Outcome::Moved);
        assert_eq!(featured_page(&state), 0);
        assert_eq!(featured_row_col(&state), (0, FEATURED_COLUMNS - 1));

        // UP and DOWN are the only way between the two rows
        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(featured_row_col(&state), (1, FEATURED_COLUMNS - 1));
        assert_eq!(state.zone, Zone::Featured);
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(featured_row_col(&state), (0, FEATURED_COLUMNS - 1));
    }

    /// The edges of the grid are solid: off the top is the tab strip, off the
    /// bottom is the list, and off the last card is nothing at all.
    #[test]
    fn the_featured_grid_has_solid_edges() {
        let mut state = browsing(0);
        // one and a half rows, so the bottom row is short
        state.featured = (0..FEATURED_COLUMNS + 2).collect();
        state.zone = Zone::Featured;

        state.featured_index = 0;
        assert_eq!(press(&mut state, LEFT), Outcome::Invalid, "no page before");
        assert_eq!(press(&mut state, UP), Outcome::Moved);
        assert_eq!(state.zone, Zone::Tabs, "off the top is the strip");

        state.zone = Zone::Featured;
        // a column the short bottom row does not reach falls through to the list
        state.featured_index = FEATURED_COLUMNS - 1;
        assert_eq!(press(&mut state, DOWN), Outcome::Moved);
        assert_eq!(state.zone, Zone::List);
    }
}
