//! Shell-owned background worker for the Select Music song search.
//!
//! The service owns the index, so neither indexing nor ranking touches the
//! render thread: the screen hands over its catalog once (`BuildIndex`) and
//! then only sends queries (`Rank`). Results come back through
//! [`Service::poll`] tagged with a generation the screen can check.

use deadsync_theme_simply_love::{
    SimplyLoveSongSearchRequest, SimplyLoveSongSearchResult, SongSearchIndex, SongSearchScope,
    build_pack_matches, build_song_matches, build_song_search_index,
};
use std::sync::mpsc;

/// A long-lived worker plus its result queue.
pub(crate) struct Service {
    tx: mpsc::Sender<SimplyLoveSongSearchRequest>,
    rx: mpsc::Receiver<SimplyLoveSongSearchResult>,
    _worker: std::thread::JoinHandle<()>,
}

impl Default for Service {
    fn default() -> Self {
        let (req_tx, req_rx) = mpsc::channel::<SimplyLoveSongSearchRequest>();
        let (res_tx, res_rx) = mpsc::channel::<SimplyLoveSongSearchResult>();
        let worker = std::thread::Builder::new()
            .name("song-search".to_string())
            .spawn(move || worker_loop(req_rx, res_tx))
            .expect("spawn song-search worker");
        Self {
            tx: req_tx,
            rx: res_rx,
            _worker: worker,
        }
    }
}

impl Service {
    /// Queue work; this only sends over a channel.
    pub(crate) fn submit(&self, request: SimplyLoveSongSearchRequest) {
        // If the worker died the overlay just keeps its old results.
        let _ = self.tx.send(request);
    }

    /// Return the freshest ready result, discarding older ones.
    pub(crate) fn poll(&self) -> Option<SimplyLoveSongSearchResult> {
        let mut latest: Option<SimplyLoveSongSearchResult> = None;
        while let Ok(result) = self.rx.try_recv() {
            match &latest {
                // Strictly older only: on a tie the later arrival is the newer
                // ranking, so keeping the earlier one would serve a stale list.
                Some(prev) if prev.generation > result.generation => {}
                _ => latest = Some(result),
            }
        }
        latest
    }
}

/// A ranking the worker still owes the screen.
struct PendingRank {
    generation: u64,
    query: String,
    scope: SongSearchScope,
    chart_type: &'static str,
}

/// Fold one request into the worker's state.
///
/// Rankings coalesce to the newest, but every `BuildIndex` is applied, or a
/// reload arriving mid-burst would be dropped in favour of a keystroke.
fn absorb(
    request: SimplyLoveSongSearchRequest,
    index: &mut Option<SongSearchIndex>,
    pending: &mut Option<PendingRank>,
) {
    match request {
        SimplyLoveSongSearchRequest::BuildIndex { entries } => {
            *index = Some(build_song_search_index(&entries));
        }
        SimplyLoveSongSearchRequest::Rank {
            generation,
            query,
            scope,
            chart_type,
        } => {
            if pending
                .as_ref()
                .is_none_or(|prev| generation >= prev.generation)
            {
                *pending = Some(PendingRank {
                    generation,
                    query,
                    scope,
                    chart_type,
                });
            }
        }
    }
}

fn worker_loop(
    rx: mpsc::Receiver<SimplyLoveSongSearchRequest>,
    tx: mpsc::Sender<SimplyLoveSongSearchResult>,
) {
    let mut index: Option<SongSearchIndex> = None;

    // Block for work; a closed channel ends the thread.
    while let Ok(request) = rx.recv() {
        let mut pending: Option<PendingRank> = None;
        absorb(request, &mut index, &mut pending);
        // Drain what queued while we were busy before doing real work.
        while let Ok(newer) = rx.try_recv() {
            absorb(newer, &mut index, &mut pending);
        }

        let Some(rank) = pending else {
            continue; // an index rebuild with nothing to rank yet
        };

        // Requests are ordered, so the index precedes any ranking that needs it.
        let matches = index
            .as_ref()
            .map_or_else(Vec::new, |index| match rank.scope {
                SongSearchScope::Song => build_song_matches(index, &rank.query, rank.chart_type),
                SongSearchScope::Pack => build_pack_matches(index, &rank.query),
            });

        if tx
            .send(SimplyLoveSongSearchResult {
                generation: rank.generation,
                matches,
            })
            .is_err()
        {
            break;
        }
    }
}
