//! Game-owned lobby song synchronization, selection and publication policy.
//!
//! The screen retains its lobby view and performs presentation changes between
//! these operations. Synchronization stores only selection keys, so warmed
//! successful and failed requests do not allocate or repeat catalog searches.

use crate::lobbies::{ConnectionState, LobbySongInfo, Snapshot};
use crate::lobby_wait::{LocalLobbyPlayers, SELECT_MUSIC_SCREEN_NAME};
use deadsync_chart::{ChartData, STANDARD_DIFFICULTY_COUNT, SongData, SongPack};
use deadsync_profile::{PlayStyle, PlayerSide, runtime_player_index};
use deadsync_simfile::playlist::{
    normalize_song_path, pack_and_song_name_from_path, song_pack_and_dir_name,
};
use std::sync::Arc;

/// Presence is queued before applying a remote selection. The online runtime
/// deduplicates the resulting machine state across repeated screen updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicPresence {
    pub screen_name: &'static str,
    pub ready: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LobbySelectionSyncStep<'snapshot> {
    pub presence: Option<SelectMusicPresence>,
    pub remote_to_apply: Option<&'snapshot LobbySongInfo>,
}

/// Resolution succeeds when the song exists, even without a playable chart for
/// the current local style. Presentation focus does not determine this outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteSelectionOutcome {
    Resolved,
    Unresolved,
}

#[derive(Clone, Debug)]
struct LobbySongKey {
    song_path: Box<str>,
    chart_hash: Box<str>,
    chart_type: Box<str>,
    rate_bits: u32,
}

impl LobbySongKey {
    fn new(song: &LobbySongInfo) -> Self {
        Self {
            song_path: normalize_song_path(&song.song_path).into_boxed_str(),
            chart_hash: song.chart_hash.as_deref().unwrap_or("").into(),
            chart_type: song.chart_type.as_deref().unwrap_or("").into(),
            rate_bits: song.rate.unwrap_or(1.0).to_bits(),
        }
    }

    fn matches(&self, song: &LobbySongInfo) -> bool {
        song_paths_match(&self.song_path, &song.song_path)
            && self.chart_hash.as_ref() == song.chart_hash.as_deref().unwrap_or("")
            && self.chart_type.as_ref() == song.chart_type.as_deref().unwrap_or("")
            && self.rate_bits == song.rate.unwrap_or(1.0).to_bits()
    }
}

#[derive(Clone, Debug)]
struct FailedLobbySong {
    key: LobbySongKey,
    content_generation: u64,
}

impl FailedLobbySong {
    fn matches(&self, song: &LobbySongInfo, content_generation: u64) -> bool {
        self.content_generation == content_generation && self.key.matches(song)
    }
}

/// Successful and failed selections are independent: revisiting the last
/// success must not discard a different song's cached lookup failure.
#[derive(Clone, Debug, Default)]
pub(crate) struct LobbySelectionSyncState {
    room: Option<String>,
    applied: Option<LobbySongKey>,
    failed: Option<FailedLobbySong>,
}

impl LobbySelectionSyncState {
    pub(crate) fn prepare_sync<'snapshot>(
        &mut self,
        snapshot: &'snapshot Snapshot,
        content_generation: u64,
    ) -> LobbySelectionSyncStep<'snapshot> {
        let Some(joined) = snapshot.joined_lobby.as_ref() else {
            self.room = None;
            self.applied = None;
            self.failed = None;
            return LobbySelectionSyncStep::default();
        };

        if self.room.as_deref() != Some(joined.code.as_str()) {
            self.room = Some(joined.code.clone());
            self.applied = None;
            self.failed = None;
        }

        if !matches!(snapshot.connection, ConnectionState::Connected) {
            self.failed = None;
            return LobbySelectionSyncStep::default();
        }

        let remote_to_apply = if let Some(song) = joined.song_info.as_ref() {
            let already_applied = self.applied.as_ref().is_some_and(|key| key.matches(song));
            let failed_without_content_change = self
                .failed
                .as_ref()
                .is_some_and(|failed| failed.matches(song, content_generation));
            (!already_applied && !failed_without_content_change).then_some(song)
        } else {
            self.failed = None;
            None
        };

        LobbySelectionSyncStep {
            presence: Some(SelectMusicPresence {
                screen_name: SELECT_MUSIC_SCREEN_NAME,
                ready: true,
            }),
            remote_to_apply,
        }
    }

    pub(crate) fn record_attempt(
        &mut self,
        remote: &LobbySongInfo,
        content_generation: u64,
        outcome: RemoteSelectionOutcome,
    ) {
        match outcome {
            RemoteSelectionOutcome::Resolved => {
                self.applied = Some(LobbySongKey::new(remote));
                self.failed = None;
            }
            RemoteSelectionOutcome::Unresolved => {
                self.failed = Some(FailedLobbySong {
                    key: LobbySongKey::new(remote),
                    content_generation,
                });
            }
        }
    }
}

/// Live local selection facts, sampled again after presentation focuses a song.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalSongSelection {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub music_rate: f32,
    pub selected_steps: [usize; 2],
    pub preferred_steps: [usize; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteChartSelection {
    pub player_index: usize,
    pub steps_index: usize,
    pub preferred_difficulty_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RemoteSongOptions {
    pub chart: Option<RemoteChartSelection>,
    pub music_rate: Option<f32>,
}

impl RemoteSongOptions {
    /// Compare canonical paths only if the rate did not already change.
    #[must_use]
    pub fn should_clear_preview(self, previous_path: Option<&str>, target: &SongData) -> bool {
        self.music_rate.is_some() || previous_path != song_path(target).as_deref()
    }
}

/// Lobby paths identify physical pack and song directories, independent of the
/// selected scan roots or presentation grouping.
#[must_use]
pub fn song_path(song: &SongData) -> Option<String> {
    let (pack, song) = song_pack_and_dir_name(song)?;
    Some(format!("{pack}/{song}"))
}

#[must_use]
pub fn chart_label(chart: &ChartData) -> String {
    if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.trim().is_empty() {
        format!("{} ({})", chart.difficulty, chart.description)
    } else {
        chart.difficulty.clone()
    }
}

#[must_use]
pub fn local_song_info(song: &SongData, local: LocalSongSelection) -> Option<LobbySongInfo> {
    let song_path = song_path(song)?;
    let player_index = runtime_player_index(local.play_style, local.player_side);
    let chart = song.chart_for_steps_index(
        local.play_style.chart_type(),
        local.selected_steps[player_index],
    )?;
    Some(LobbySongInfo {
        song_path,
        title: Some(song.display_full_title(false)),
        artist: Some(song.artist.clone()),
        song_length_seconds: Some(song.music_length_seconds),
        chart_hash: Some(chart.short_hash.clone()),
        chart_type: Some(chart.chart_type.clone()),
        chart_label: Some(chart_label(chart)),
        rate: Some(local.music_rate),
    })
}

/// Visible exact matches precede the ordered catalog. Within the catalog, an
/// exact or pack/song match precedes the first remembered song-name fallback.
#[must_use]
pub fn resolve_remote_song<'song>(
    remote_path: &str,
    group_songs: impl IntoIterator<Item = &'song Arc<SongData>>,
    song_packs: &[SongPack],
) -> Option<Arc<SongData>> {
    let needle = normalize_song_path(remote_path);
    let needle_leaf = needle.rsplit('/').next().unwrap_or(needle.as_str());
    let needle_pack_and_song = pack_and_song_name_from_path(remote_path);
    for song in group_songs {
        if song_path(song.as_ref()).is_some_and(|path| path.eq_ignore_ascii_case(needle.as_str())) {
            return Some(Arc::clone(song));
        }
    }

    let mut leaf_match = None;
    for pack in song_packs {
        for song in &pack.songs {
            let Some(path) = song_path(song.as_ref()) else {
                continue;
            };
            if path.eq_ignore_ascii_case(needle.as_str()) {
                return Some(Arc::clone(song));
            }
            if let Some((needle_pack, needle_song)) = needle_pack_and_song.as_ref()
                && let Some((pack_dir, song_dir)) = song_pack_and_dir_name(song.as_ref())
                && pack_dir.eq_ignore_ascii_case(needle_pack.as_str())
                && song_dir.eq_ignore_ascii_case(needle_song.as_str())
            {
                return Some(Arc::clone(song));
            }
            if leaf_match.is_none()
                && path
                    .rsplit('/')
                    .next()
                    .is_some_and(|leaf| leaf.eq_ignore_ascii_case(needle_leaf))
            {
                leaf_match = Some(Arc::clone(song));
            }
        }
    }
    leaf_match
}

/// Remote metadata chooses a chart within the current local style. A missing
/// chart does not prevent applying a valid rate or completing song resolution.
#[must_use]
pub fn remote_song_options(
    song: &SongData,
    remote: &LobbySongInfo,
    local: LocalSongSelection,
    previous_rate: f32,
) -> RemoteSongOptions {
    let player_index = runtime_player_index(local.play_style, local.player_side);
    let chart_type = local.play_style.chart_type();
    let steps_index = remote
        .chart_hash
        .as_deref()
        .and_then(|hash| song.steps_index_for_chart_hash(chart_type, hash))
        .or_else(|| song.best_steps_index(chart_type, local.preferred_steps[player_index]));
    let chart = steps_index.map(|steps_index| RemoteChartSelection {
        player_index,
        steps_index,
        preferred_difficulty_index: (steps_index < STANDARD_DIFFICULTY_COUNT)
            .then_some(steps_index),
    });
    let music_rate = remote
        .rate
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .map(|rate| rate.clamp(0.5, 3.0))
        .filter(|rate| (*rate - previous_rate).abs() >= 0.0005);
    RemoteSongOptions { chart, music_rate }
}

/// Build an owned publication only after establishing that remote participants
/// can receive it. Confirmation tolerates optional remote metadata, unlike the
/// exact/default bits used by synchronization keys.
#[must_use]
pub fn confirmed_song_to_publish(
    snapshot: &Snapshot,
    local: LocalLobbyPlayers,
    build_local: impl FnOnce() -> Option<LobbySongInfo>,
) -> Option<LobbySongInfo> {
    let joined = snapshot.joined_lobby.as_ref()?;
    if joined.players.len() <= local.count() {
        return None;
    }
    if !matches!(snapshot.connection, ConnectionState::Connected) {
        return None;
    }
    let song = build_local()?;
    if joined
        .song_info
        .as_ref()
        .is_some_and(|remote| song_matches_remote_selection(&song, remote))
    {
        return None;
    }
    Some(song)
}

fn song_paths_match(left: &str, right: &str) -> bool {
    song_path_parts(left).eq(song_path_parts(right))
}

fn song_path_parts(path: &str) -> impl Iterator<Item = &str> {
    path.trim()
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
}

fn song_matches_remote_selection(local: &LobbySongInfo, remote: &LobbySongInfo) -> bool {
    if !song_paths_match(&local.song_path, &remote.song_path) {
        return false;
    }
    if let Some(remote_chart_hash) = remote
        .chart_hash
        .as_deref()
        .filter(|chart_hash| !chart_hash.is_empty())
        && local.chart_hash.as_deref() != Some(remote_chart_hash)
    {
        return false;
    }
    if let Some(remote_chart_type) = remote
        .chart_type
        .as_deref()
        .map(str::trim)
        .filter(|chart_type| !chart_type.is_empty())
        && !local
            .chart_type
            .as_deref()
            .is_some_and(|chart_type| chart_type.eq_ignore_ascii_case(remote_chart_type))
    {
        return false;
    }
    if let Some(remote_rate) = remote.rate.filter(|rate| rate.is_finite() && *rate > 0.0)
        && !local
            .rate
            .is_some_and(|local_rate| (local_rate - remote_rate).abs() < 0.0005)
    {
        return false;
    }
    true
}

#[cfg(test)]
#[path = "lobby_selection_tests.rs"]
mod tests;
