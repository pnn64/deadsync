//! Changing an installed pack's sync, from the Installed tab.
//!
//! The dialog offers DeadSync's own ways of syncing a pack, in this order:
//!
//! * **Measure** -- every song's chart is measured against its music
//!   (Null-or-Die), and the reader reviews the offsets before anything is
//!   saved. The shared pack sync overlay does the measuring and the review.
//! * **Shift** -- every simfile's offsets moved 9 ms at once: ITG to NULL, or
//!   back. For a pack known to be synced the old ITG way, where measuring
//!   would only find the same 9 ms one song at a time.
//! * **Pack.ini** -- record ITG or NULL in the pack's `Pack.ini` and leave the
//!   simfiles alone. Offered only while the engine reads it (Machine Options >
//!   Pack.ini Offsets); otherwise it would do nothing at all.
//!
//! Rewriting offsets and recording a sync belong together. Once a pack's
//! offsets are null, a `Pack.ini` still saying ITG moves it 9 ms more whenever
//! the engine reads it. So both rewrites finish by recording what the pack now
//! is -- which is also what turns a second shift the same way into its undo.
//!
//! The rewriting itself is the shell's: it owns the song cache, the files and
//! the analysis workers. This module is the dialog's state and what the shell
//! calls to start, feed and finish the work.

use std::sync::Arc;

use deadsync_chart::SongData;
use deadsync_chart::song::SyncPref;

use super::state::{InstalledPack, State};
use crate::screens::pack_sync as shared;

/// How far a shift moves every offset, in milliseconds: the engine's own
/// correction for an ITG-synced pack.
pub(super) const SHIFT_MS: u32 = 9;

/// Updates a shift waits before it is handed over. The shift holds the frame
/// it runs in, so the panel saying it is under way is drawn first.
const FRAMES_BEFORE_SHIFT: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyncAction {
    Measure,
    Shift,
    PackIni,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyncStep {
    /// Choosing what to do.
    Choose,
    /// Asked once more, because every simfile in the pack is about to change.
    ConfirmShift,
    /// The shift is under way, and the dialog only says so.
    Working { frames: u8, sent: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyncDialog {
    pub(super) group: String,
    pub(super) songs: usize,
    /// What the pack's `Pack.ini` says now.
    pub(super) declared: SyncPref,
    /// Whether the pack may be changed from here, as the shell finds it.
    pub(super) check: PackCheck,
    pub(super) action: SyncAction,
    /// The value the Pack.ini row would record.
    pub(super) record: SyncPref,
    pub(super) step: SyncStep,
}

/// The shell's answer to whether a pack may be changed from here. Asked once,
/// when the dialog opens, with the same check every change makes again before
/// it writes anything -- so the dialog offers exactly what will work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PackCheck {
    Checking { asked: bool },
    Usable,
    Refused(String),
}

impl SyncDialog {
    pub(super) const fn usable(&self) -> bool {
        matches!(self.check, PackCheck::Usable)
    }

    /// Whether a shift moves this pack from ITG to NULL, rather than back. A
    /// pack already recorded as NULL is only offered the way back, which is
    /// how the same shift is never made twice.
    pub(super) fn shift_to_null(&self) -> bool {
        self.declared != SyncPref::Null
    }
}

/// The shift the shell is to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackShift {
    pub group_name: String,
    /// ITG to NULL, moving every offset 9 ms later; else NULL to ITG.
    pub to_null: bool,
}

/// The dialog's rows, top to bottom.
pub(super) const fn actions(state: &State) -> &'static [SyncAction] {
    if state.pack_ini_offsets_on {
        &[SyncAction::Measure, SyncAction::Shift, SyncAction::PackIni]
    } else {
        &[SyncAction::Measure, SyncAction::Shift]
    }
}

/// Open the dialog on a library pack. It starts on the shift for a pack that
/// says it is ITG -- the one thing known to be wrong with it -- and on
/// measuring otherwise.
pub(super) fn open(state: &mut State, entry: &InstalledPack) {
    let action = if entry.sync == SyncPref::Itg {
        SyncAction::Shift
    } else {
        SyncAction::Measure
    };
    state.sync_dialog = Some(SyncDialog {
        group: entry.name.clone(),
        songs: entry.songs,
        declared: entry.sync,
        check: PackCheck::Checking { asked: false },
        action,
        record: super::state::suggested_sync(state, entry),
        step: SyncStep::Choose,
    });
    state.remove_result = None;
    state.nav_hold = None;
}

/// One frame passing, for the shift's wait.
pub(super) fn tick(state: &mut State) {
    if let Some(SyncDialog {
        step: SyncStep::Working { frames, .. },
        ..
    }) = state.sync_dialog.as_mut()
    {
        *frames = frames.saturating_add(1);
    }
}

/// The pack the dialog wants checked, asked for once.
pub fn wanted_pack_check(state: &mut State) -> Option<String> {
    let dialog = state.sync_dialog.as_mut()?;
    let PackCheck::Checking { asked } = &mut dialog.check else {
        return None;
    };
    if *asked {
        return None;
    }
    *asked = true;
    Some(dialog.group.clone())
}

/// The shell's answer to [`wanted_pack_check`], for the dialog still open on
/// that pack.
pub fn set_pack_check(state: &mut State, group_name: &str, result: Result<(), String>) {
    if let Some(dialog) = state
        .sync_dialog
        .as_mut()
        .filter(|dialog| dialog.group == group_name)
    {
        dialog.check = match result {
            Ok(()) => PackCheck::Usable,
            Err(reason) => PackCheck::Refused(reason),
        };
    }
}

/// The simfiles the review found already null -- nothing to change, or a fix
/// an earlier review saved. With the ones it saves, these are the songs the
/// review leaves null; the shell moves the rest of an ITG pack itself.
#[must_use]
pub fn measured_settled_simfiles(state: &State) -> Vec<std::path::PathBuf> {
    shared::settled_simfiles(&state.pack_sync_overlay)
}

/// Whether the shared pack sync overlay is up. While it is, it is the screen.
pub(super) const fn overlay_visible(state: &State) -> bool {
    shared::poll(&state.pack_sync_overlay)
}

pub(super) const fn nav_policy(state: &State) -> shared::NavigationPolicy {
    shared::NavigationPolicy {
        only_dedicated_menu_buttons: state.pack_sync_menu_only,
        three_key_navigation: state.pack_sync_three_key,
    }
}

/// The shift the dialog is waiting on, handed over once. The dialog stays up,
/// saying the shift is under way, until [`finish_pack_sync`].
pub fn take_pack_shift(state: &mut State) -> Option<PackShift> {
    let dialog = state.sync_dialog.as_mut()?;
    let SyncStep::Working { frames, sent } = &mut dialog.step else {
        return None;
    };
    if *sent || *frames < FRAMES_BEFORE_SHIFT {
        return None;
    }
    *sent = true;
    Some(PackShift {
        group_name: dialog.group.clone(),
        to_null: dialog.shift_to_null(),
    })
}

/// Start measuring a pack: one chart per song, of the play style and
/// difficulty the shell names, handed to the shared overlay. Returns the
/// request that starts the analysis, or `None` -- with a note on the grid --
/// when the pack has nothing of that style to measure.
pub fn begin_pack_measure(
    state: &mut State,
    group_name: &str,
    songs: &[Arc<SongData>],
    chart_type: &str,
    preferred_difficulty_index: usize,
) -> Option<crate::SimplyLoveSyncRequest> {
    state.sync_dialog = None;
    if overlay_visible(state) {
        return None;
    }
    state.nav_hold = None;
    let targets: Vec<shared::TargetSpec> = songs
        .iter()
        .filter_map(|song| {
            let steps = song.best_steps_index(chart_type, preferred_difficulty_index)?;
            let chart_ix = crate::screens::select_music::selected_chart_ix_for_sync(
                song.as_ref(),
                chart_type,
                steps,
            )?;
            let chart = song.charts.get(chart_ix)?;
            Some(shared::TargetSpec {
                song: Arc::clone(song),
                simfile_path: song.simfile_path.clone(),
                song_title: song.display_full_title(false),
                chart_label: shared::chart_label(chart),
                chart_ix,
            })
        })
        .collect();
    let request = shared::begin(
        &mut state.pack_sync_overlay,
        crate::SimplyLoveSyncOwner::ContentBrowserPack,
        group_name.to_owned(),
        targets,
        state.pack_sync_confidence,
    );
    if request.is_some() {
        state.pack_sync_group = Some(group_name.to_owned());
    } else {
        state.remove_result = Some(format!(
            "nothing in {group_name} to measure: no {chart_type} charts"
        ));
    }
    request
}

/// The pack the overlay is measuring, for the shell to record once it saves.
#[must_use]
pub fn pack_sync_group(state: &State) -> Option<&str> {
    state.pack_sync_group.as_deref()
}

/// The analysis' progress and the save's result, routed to the overlay.
pub fn apply_sync_analysis_events(state: &mut State, events: &mut Vec<crate::SimplyLoveSyncEvent>) {
    for event in events.drain(..) {
        shared::apply_event(&mut state.pack_sync_overlay, event);
    }
}

/// The shell's answer to any of the dialog's work: what it did, and the sync
/// the pack's `Pack.ini` now records, which the grid's chip shows at once
/// rather than after the next library scan.
pub fn finish_pack_sync(
    state: &mut State,
    group_name: &str,
    recorded: Option<SyncPref>,
    result: Result<String, String>,
) {
    state.sync_dialog = None;
    if let Some(sync) = recorded {
        let wanted = group_name.to_lowercase();
        for entry in state
            .installed
            .iter_mut()
            .filter(|entry| entry.lower == wanted)
        {
            entry.sync = sync;
        }
    }
    state.remove_result = Some(match result {
        Ok(note) => note,
        Err(error) => format!("could not change sync: {error}"),
    });
}
