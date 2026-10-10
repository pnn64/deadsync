//! Chart preview and single-song download, from a pack's page.
//!
//! The original's model, kept: one song is read out of the pack's zip on
//! StepManiaOnline -- its simfile, and then its audio -- for the sample
//! window and every chart's notes inside it. The original had a relay
//! (itgcontent.net) do that reading; here the game does it, with ranged reads
//! of the pack, so nothing else is needed. The window then scrolls those notes
//! past the reader's own noteskin, timed to the music. It is a picture of the
//! chart, not a notefield: no judgements, no mods, no holds -- hold heads are
//! drawn as taps -- which is exactly what the original draws.
//!
//! Everything here is state and arithmetic. The network is `smo_songs`'s, the
//! audio and the noteskin are the shell's, and the drawing is
//! `chart_window`'s.

use std::borrow::Cow;
use std::sync::Arc;

use deadsync_online::pack_page::SongRow;
use deadsync_online::smo_songs::{
    PreviewChart, PreviewPhase, PreviewSnapshot, SongInstall, SongInstallPhase,
};
use deadsync_theme::{AudioCut, AudioRequest};

use super::state::State;

/// The original's speed: C516 at 50% mini. A picture of the chart at one fixed
/// speed, so a dense chart reads as dense and a sparse one as sparse.
pub(super) const PX_PER_SECOND: f32 = 64.0 * 516.0 / 60.0 * 0.5;
/// One arrow, and the spacing between lanes, at 50% mini.
pub(super) const ARROW_PX: f32 = 32.0;
/// Doubles is the widest chart the catalogue holds.
pub(super) const MAX_LANES: usize = 8;
/// How long the window takes to arrive, and to go.
pub(super) const FADE_IN: f32 = 0.18;
pub(super) const FADE_OUT: f32 = 0.40;
/// The preview stops this long after its sample ends, as the original's does.
const TAIL: f32 = 1.6;
/// How much longer than that a sample may run on the wall clock while the
/// music clock has not moved: room for a slow decoder to start, after which a
/// sample that never made a sound is given up on rather than held open.
const CLOCK_GRACE: f32 = 2.0;
/// The sample's fades, the original's.
const AUDIO_FADE_IN: f64 = 0.5;
const AUDIO_FADE_OUT: f64 = 1.5;
/// The tempo the equalizers keep when the simfile gave none, the original's.
const FALLBACK_BPM: f32 = 128.0;

/// One song being previewed.
#[derive(Clone, Debug)]
pub(super) struct Preview {
    pub(super) pack_id: u64,
    pub(super) title: String,
    /// As [`request_artist`] gives it for the song.
    pub(super) artist: String,
    /// The song row it was started from, for the list's playing marker.
    pub(super) row: usize,
    /// The chart showing, as an index into the song's charts.
    pub(super) chart: Option<usize>,
    /// A difficulty chosen in the song menu before this answer arrived.
    pub(super) want: Option<Want>,
    /// The audio has been asked to play.
    pub(super) playing: bool,
    /// Seconds since the audio was asked to play, on the wall clock -- the
    /// original's `startedAt`. The picture runs off the music clock wherever
    /// there is one; this is for where there is not, and for a music clock
    /// that never starts.
    pub(super) played_for: f32,
    /// Notes before this index have reached the receptors.
    pub(super) passed: usize,
    /// Song time each lane was last hit, for its receptor and explosion.
    pub(super) hit_at: [Option<f32>; MAX_LANES],
    /// 0..1 while arriving; counts back down once closing.
    pub(super) fade: f32,
    pub(super) closing: bool,
}

impl Preview {
    fn new(pack_id: u64, title: String, artist: String, row: usize, want: Option<Want>) -> Self {
        Self {
            pack_id,
            title,
            artist,
            row,
            chart: None,
            want,
            playing: false,
            played_for: 0.0,
            passed: 0,
            hit_at: [None; MAX_LANES],
            fade: 0.0,
            closing: false,
        }
    }
}

/// The chart the song menu offered, from the charts an earlier preview of the
/// song turned up: the original's `Snd.want`, an index into them. The rest is
/// what the index pointed at, to check it against the list that arrives and to
/// find the chart again should that list be ordered differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Want {
    pub(super) index: usize,
    pub(super) doubles: bool,
    pub(super) difficulty: String,
    pub(super) meter: u32,
}

impl Want {
    /// The chart at `index` of a list of known charts.
    pub(super) fn at(charts: &[PreviewChart], index: usize) -> Option<Self> {
        charts.get(index).map(|chart| Self {
            index,
            doubles: chart.doubles,
            difficulty: chart.difficulty.clone(),
            meter: chart.meter,
        })
    }
}

/// The "Listen or Download?" popup a song's START opens.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SongMenu {
    /// 0 preview, 1 get this song, 2 download the pack.
    pub(super) choice: usize,
    /// The chart the preview choice will open on, once this song's charts are
    /// known from an earlier preview.
    pub(super) chart: Option<usize>,
}

/// What the page asks of `smo_songs`. Queued here and taken by the shell each
/// frame, which owns the cache and Songs directories. `artist` tells apart two
/// songs of one title in the same pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongRequest {
    Preview {
        pack_id: u64,
        title: String,
        artist: String,
    },
    StopPreview,
    GetSong {
        pack_id: u64,
        title: String,
        artist: String,
        itg_sync: bool,
    },
}

/// The artist a request for `song` carries: its own when the pack's page
/// lists its title more than once, so the pack's simfiles can tell those
/// apart, and none otherwise -- a page often spells an artist unlike the
/// simfile does, and a title listed once needs no help.
pub(super) fn request_artist<'a>(songs: &[SongRow], song: &'a SongRow) -> &'a str {
    let title = song.title.trim();
    let repeated = songs
        .iter()
        .filter(|other| other.title.trim().eq_ignore_ascii_case(title))
        .nth(1)
        .is_some();
    if repeated { &song.artist } else { "" }
}

/// Whether this snapshot is the answer for the preview showing.
fn answers(snapshot: &PreviewSnapshot, preview: &Preview) -> bool {
    snapshot.pack_id == preview.pack_id
        && snapshot.title == preview.title
        && snapshot.artist == preview.artist
}

/// A preview request still queued for the shell. The preview snapshot was read
/// before it went out, so whatever that snapshot says belongs to an earlier
/// attempt -- the failure of the try before a retry of the same song, or the
/// sample of the one a stop and a start in the same frame replaced.
fn awaiting_send(state: &State) -> bool {
    state
        .pending_songs
        .iter()
        .any(|request| matches!(request, SongRequest::Preview { .. }))
}

/// The preview's answer, when it has one.
pub(super) fn snapshot_for<'a>(state: &'a State, preview: &Preview) -> Option<&'a PreviewSnapshot> {
    let snapshot = state.song_preview.as_ref();
    (!awaiting_send(state) && answers(snapshot, preview)).then_some(snapshot)
}

/// Whether a preview is loading or playing -- a press then stops it.
pub(super) fn busy(state: &State) -> bool {
    state
        .preview
        .as_ref()
        .is_some_and(|preview| !preview.closing)
}

/// Whether a chart is on screen, so UP/DOWN change difficulty rather than
/// move the song list. Only once the sample plays -- the original's
/// `Snd.status == "playing"`: while it loads, the window is a spinner over
/// empty receptors, and UP/DOWN still move the song highlight.
pub(super) fn chart_showing(state: &State) -> bool {
    state.preview.as_ref().is_some_and(|preview| {
        !preview.closing
            && preview.playing
            && preview.chart.is_some()
            && snapshot_for(state, preview).is_some_and(|snapshot| !snapshot.charts.is_empty())
    })
}

/// Start previewing a song. The shell is asked for it; the window opens now,
/// as the original's does, and fills in as the answer arrives.
pub(super) fn start(
    state: &mut State,
    pack_id: u64,
    title: String,
    artist: String,
    row: usize,
    want: Option<Want>,
) {
    if state
        .preview
        .as_ref()
        .is_some_and(|preview| preview.playing)
    {
        state.pending_audio.push(AudioRequest::StopMusic);
    }
    state.preview_message = None;
    // A new preview is the next thing the header talks about.
    state.watched_song = None;
    state.preview = Some(Preview::new(
        pack_id,
        title.clone(),
        artist.clone(),
        row,
        want,
    ));
    state.pending_songs.push(SongRequest::Preview {
        pack_id,
        title,
        artist,
    });
}

/// Stop the preview, letting the window fade out.
pub(super) fn stop(state: &mut State) {
    if let Some(preview) = state.preview.as_mut() {
        if preview.playing {
            state.pending_audio.push(AudioRequest::StopMusic);
        }
        preview.playing = false;
        preview.closing = true;
    }
    state.pending_songs.push(SongRequest::StopPreview);
}

/// Step the chart showing. No wrap: the ends say so, as the original's do.
pub(super) fn step_chart(state: &mut State, delta: isize) -> bool {
    let Some(count) = state
        .preview
        .as_ref()
        .and_then(|preview| snapshot_for(state, preview).map(|snapshot| snapshot.charts.len()))
    else {
        return false;
    };
    let time = elapsed(state);
    let Some(preview) = state.preview.as_mut() else {
        return false;
    };
    let Some(current) = preview.chart else {
        return false;
    };
    let next = current as isize + delta;
    if next < 0 || next >= count as isize {
        return false;
    }
    preview.chart = Some(next as usize);
    preview.hit_at = [None; MAX_LANES];
    preview.passed = 0;
    // Notes already behind the receptors are not replayed as hits.
    if let Some(time) = time {
        resync_passed(state, time);
    }
    true
}

/// The original's choice of opening chart: singles Challenge or Expert, then
/// the doubles one, then the hardest singles chart, then the last.
pub(super) fn default_chart(charts: &[PreviewChart]) -> Option<usize> {
    let top = |doubles: bool| {
        charts.iter().position(|chart| {
            chart.doubles == doubles
                && (chart.difficulty.eq_ignore_ascii_case("challenge")
                    || chart.difficulty.eq_ignore_ascii_case("expert"))
        })
    };
    top(false)
        .or_else(|| top(true))
        .or_else(|| charts.iter().rposition(|chart| !chart.doubles))
        .or_else(|| charts.len().checked_sub(1))
}

/// The chart a choice made before the charts arrived names. The known list and
/// the arriving one come from the same META, so the index is tried first --
/// the only thing that tells two Edits of one meter apart -- then the same
/// difficulty and meter, then the same difficulty.
fn wanted_chart(charts: &[PreviewChart], want: &Want) -> Option<usize> {
    let same = |chart: &PreviewChart| {
        chart.doubles == want.doubles && chart.difficulty.eq_ignore_ascii_case(&want.difficulty)
    };
    if charts
        .get(want.index)
        .is_some_and(|chart| same(chart) && chart.meter == want.meter)
    {
        return Some(want.index);
    }
    charts
        .iter()
        .position(|chart| same(chart) && chart.meter == want.meter)
        .or_else(|| charts.iter().position(same))
}

/// The chart a preview opens on: the song menu's choice, else the original's.
fn opening_chart(charts: &[PreviewChart], want: Option<&Want>) -> Option<usize> {
    want.and_then(|want| wanted_chart(charts, want))
        .or_else(|| default_chart(charts))
}

/// Seconds into the sample. From the music clock, so the notes cannot drift
/// from the audio; from the wall clock where there is no audio output, as the
/// original's always are. `None` before the audio is asked to play: the notes
/// hold still until it is.
pub(super) fn elapsed(state: &State) -> Option<f32> {
    let preview = state.preview.as_ref()?;
    if !preview.playing {
        return None;
    }
    let snapshot = snapshot_for(state, preview)?;
    let Some(music) = state.music_time else {
        return Some(preview.played_for);
    };
    let origin = if snapshot.clip { 0.0 } else { snapshot.start };
    Some(music - origin)
}

/// Beats into the sample at the song's tempo, for the equalizers and the
/// skin's beat-driven animation; 128 when the simfile gave none, as the
/// original's `Snd.BarHeight` does.
pub(super) fn beat(state: &State) -> Option<f32> {
    let time = elapsed(state)?;
    let preview = state.preview.as_ref()?;
    let bpm = snapshot_for(state, preview).map_or(0.0, |snapshot| snapshot.bpm);
    let bpm = if bpm > 0.0 { bpm } else { FALLBACK_BPM };
    Some(time * bpm / 60.0)
}

/// Move the hit cursor to the first note still ahead.
fn resync_passed(state: &mut State, time: f32) {
    let Some(notes) = state.preview.as_ref().and_then(|preview| {
        let snapshot = snapshot_for(state, preview)?;
        preview
            .chart
            .and_then(|index| snapshot.charts.get(index))
            .map(|chart| Arc::clone(&chart.notes))
    }) else {
        return;
    };
    if let Some(preview) = state.preview.as_mut() {
        preview.passed = notes.partition_point(|note| note.time <= time);
    }
}

/// Fold the latest preview answer into the preview showing: pick a chart once
/// charts land, play the audio once it is on disk, close on a failure.
pub(super) fn sync(state: &mut State) {
    let Some(preview) = state.preview.as_ref() else {
        return;
    };
    if preview.closing {
        return;
    }
    // The shell sends what is queued right after this; the next frame's
    // snapshot is the first that can be this attempt's.
    if awaiting_send(state) {
        return;
    }
    let snapshot = Arc::clone(&state.song_preview);
    if !answers(&snapshot, preview) {
        return;
    }
    match snapshot.phase {
        PreviewPhase::Error => {
            state.preview_message = Some(
                snapshot
                    .message
                    .clone()
                    .unwrap_or_else(|| "no sample for this song".to_owned()),
            );
            if let Some(preview) = state.preview.as_mut() {
                preview.closing = true;
            }
            // And the preview back to idle, as the original's Snd.Stop bumps its
            // token: a failure left published would be the answer to the next
            // try of the same song before that try had been asked.
            state.pending_songs.push(SongRequest::StopPreview);
        }
        PreviewPhase::Loading | PreviewPhase::Ready => {
            let Some(preview) = state.preview.as_mut() else {
                return;
            };
            if preview.chart.is_none() && !snapshot.charts.is_empty() {
                preview.chart = opening_chart(&snapshot.charts, preview.want.as_ref());
            }
            if snapshot.phase == PreviewPhase::Ready
                && !preview.playing
                && let Some(path) = snapshot.audio_path.clone()
            {
                preview.playing = true;
                preview.played_for = 0.0;
                let start = if snapshot.clip { 0.0 } else { snapshot.start };
                state.pending_audio.push(AudioRequest::PlayMusic {
                    path,
                    cut: AudioCut {
                        start_sec: f64::from(start),
                        length_sec: f64::from(snapshot.length),
                        fade_in_sec: AUDIO_FADE_IN,
                        fade_out_sec: AUDIO_FADE_OUT,
                    },
                    looping: false,
                    rate: 1.0,
                });
            }
        }
        PreviewPhase::Idle => {}
    }
}

/// Per-frame: fades, hits, and the end of the sample.
pub(super) fn update(state: &mut State, delta_time: f32) {
    let time = elapsed(state);
    let snapshot = state
        .preview
        .as_ref()
        .and_then(|preview| snapshot_for(state, preview));
    let length = snapshot.map(|snapshot| snapshot.length);
    // A sample with no chart to draw plays with no window, as the original's
    // does: the window fades and the preview runs on underneath it.
    let windowless = snapshot.is_some_and(|snapshot| {
        snapshot.phase == PreviewPhase::Ready && snapshot.charts.is_empty()
    });
    let notes = state.preview.as_ref().and_then(|preview| {
        let snapshot = snapshot_for(state, preview)?;
        preview
            .chart
            .and_then(|index| snapshot.charts.get(index))
            .map(|chart| Arc::clone(&chart.notes))
    });
    let Some(preview) = state.preview.as_mut() else {
        return;
    };

    if preview.closing {
        preview.fade -= delta_time / FADE_OUT;
        if preview.fade <= 0.0 {
            state.preview = None;
        }
        return;
    }
    preview.fade = if windowless {
        (preview.fade - delta_time / FADE_OUT).max(0.0)
    } else {
        (preview.fade + delta_time / FADE_IN).min(1.0)
    };

    // The wall clock's end, for a sample whose music clock never moved -- no
    // audio output, or a file the decoder would not take. The music clock's
    // end below comes first whenever the audio really plays.
    if preview.playing {
        preview.played_for += delta_time;
        if length.is_some_and(|length| preview.played_for > length + TAIL + CLOCK_GRACE) {
            stop(state);
            return;
        }
    }

    let Some(time) = time else {
        return;
    };
    // Every note that reached its receptor since the last frame lights it --
    // but not a mine. A mine is not stepped on: it scrolls on past, as an
    // avoided mine does in play, and the receptor stays still.
    if let Some(notes) = notes {
        while let Some(note) = notes.get(preview.passed) {
            if note.time > time {
                break;
            }
            let struck = note.cols & !note.mines;
            for lane in 0..MAX_LANES {
                if struck & (1 << lane) != 0 {
                    preview.hit_at[lane] = Some(note.time);
                }
            }
            preview.passed += 1;
        }
    }
    // The sample is over: stop, as the original does, a moment after it ends.
    if length.is_some_and(|length| time > length + TAIL) {
        stop(state);
    }
}

/// The lanes the window should lay out before any chart has arrived: the
/// charts already known for this song -- the one the song menu chose, else the
/// one a preview would open on -- else a doubles pack's eight, else four.
pub(super) fn predicted_lanes(state: &State, preview: &Preview) -> usize {
    if let Some(known) = deadsync_online::smo_songs::runtime_known_charts(
        preview.pack_id,
        &preview.title,
        &preview.artist,
    ) && let Some(chart) =
        opening_chart(&known, preview.want.as_ref()).and_then(|index| known.get(index))
    {
        return usize::from(chart.lanes).clamp(1, MAX_LANES);
    }
    let doubles_pack = super::state::focused_pack(state)
        .is_some_and(|pack| super::state::is_dedicated_doubles(state, pack));
    if doubles_pack { 8 } else { 4 }
}

/// What the song list's header says about the preview while it is busy, the
/// original's `Snd.Label`: playing, or how much of the sample has arrived.
/// Only a percentage is built; the rest is the same words every frame.
pub(super) fn sample_label(state: &State) -> Option<Cow<'static, str>> {
    let preview = state.preview.as_ref().filter(|preview| !preview.closing)?;
    if preview.playing {
        return Some(Cow::Borrowed("playing a sample"));
    }
    Some(
        match snapshot_for(state, preview).and_then(|snapshot| snapshot.progress) {
            Some(fraction) => Cow::Owned(format!(
                "loading sample  {:.0}%",
                fraction.clamp(0.0, 1.0) * 100.0
            )),
            None => Cow::Borrowed("loading sample..."),
        },
    )
}

/// This song's install record, if it has one this session. `artist` as
/// [`request_artist`] gives it.
pub(super) fn song_install<'a>(
    state: &'a State,
    pack_id: u64,
    title: &str,
    artist: &str,
) -> Option<&'a SongInstall> {
    state
        .song_installs
        .installs
        .iter()
        .rev()
        .find(|install| install.is(pack_id, title, artist))
}

/// Whether a finished rescan has already brought this song into the library.
fn reloaded(state: &State, install: &SongInstall) -> bool {
    state
        .reloaded_songs
        .iter()
        .any(|(pack_id, title, artist)| install.is(*pack_id, title, artist))
}

/// The status line for a song's install, in the original's words, naming the
/// song: the line stands for it whichever row the cursor is on. `None` for a
/// song a rescan has since loaded -- the original's "reload songs" is a
/// one-off toast, not a standing caption.
pub(super) fn song_install_line(state: &State, install: &SongInstall) -> Option<String> {
    let title = &install.title;
    Some(match install.phase {
        SongInstallPhase::Queued => format!("getting {title}..."),
        SongInstallPhase::Downloading => match install.total_bytes.filter(|total| *total > 0) {
            Some(total) => format!(
                "getting {title}  {:.0}%",
                install.downloaded_bytes as f64 * 100.0 / total as f64
            ),
            None => format!(
                "getting {title}  {:.1} MB so far",
                install.downloaded_bytes as f64 / (1024.0 * 1024.0)
            ),
        },
        SongInstallPhase::Extracting => format!("unpacking {title}..."),
        SongInstallPhase::Installed if reloaded(state, install) => return None,
        SongInstallPhase::Installed => {
            format!("added {title} - reload songs when you leave to play it")
        }
        SongInstallPhase::Error => format!(
            "could not get {title}: {}",
            install.message.as_deref().unwrap_or("unknown error")
        ),
    })
}

/// Why a request for this song would be refused, before it is made:
/// `smo_songs`'s own rule -- one request a session, unless the last one
/// failed -- checked here too so the refusal sounds as the original's does
/// rather than as a request that went out.
pub(super) fn song_refusal(
    state: &State,
    pack_id: u64,
    title: &str,
    artist: &str,
) -> Option<&'static str> {
    match song_install(state, pack_id, title, artist)?.phase {
        SongInstallPhase::Queued | SongInstallPhase::Downloading | SongInstallPhase::Extracting => {
            Some("that song is already on its way")
        }
        SongInstallPhase::Installed => Some("that song was already added this session"),
        SongInstallPhase::Error => None,
    }
}

/// Songs installed one at a time since the library was last rescanned, for
/// the reload question. Songs a finished rescan already loaded are not owed.
pub(super) fn songs_added(state: &State) -> usize {
    state
        .song_installs
        .installs
        .iter()
        .filter(|install| install.phase == SongInstallPhase::Installed)
        .filter(|install| !reloaded(state, install))
        .count()
}

/// Whether a song is still on its way in, which holds the reload question
/// back the same way a pack does.
pub(super) fn song_installs_active(state: &State) -> bool {
    state.song_installs.installs.iter().any(|install| {
        matches!(
            install.phase,
            SongInstallPhase::Queued | SongInstallPhase::Downloading | SongInstallPhase::Extracting
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_online::smo_songs::{PreviewNote, SongInstallsSnapshot};

    fn chart(doubles: bool, difficulty: &str, meter: u32) -> PreviewChart {
        PreviewChart {
            doubles,
            difficulty: difficulty.to_owned(),
            meter,
            lanes: if doubles { 8 } else { 4 },
            notes: Arc::from(Vec::<PreviewNote>::new()),
        }
    }

    /// Singles Challenge, then the doubles one, then the hardest singles chart.
    #[test]
    fn the_preview_opens_on_the_originals_choice_of_chart() {
        let charts = [
            chart(false, "Easy", 3),
            chart(false, "Challenge", 12),
            chart(true, "Challenge", 13),
        ];
        assert_eq!(default_chart(&charts), Some(1));
        let no_singles_top = [chart(false, "Easy", 3), chart(true, "Expert", 11)];
        assert_eq!(default_chart(&no_singles_top), Some(1));
        let neither = [chart(false, "Easy", 3), chart(false, "Hard", 9)];
        assert_eq!(
            default_chart(&neither),
            Some(1),
            "the hardest singles chart"
        );
        assert_eq!(default_chart(&[]), None);
    }

    #[test]
    fn a_chart_chosen_before_it_was_known_is_found_by_name() {
        let charts = [chart(false, "Easy", 3), chart(true, "Challenge", 13)];
        let want = |index, doubles, difficulty: &str, meter| Want {
            index,
            doubles,
            difficulty: difficulty.to_owned(),
            meter,
        };
        assert_eq!(
            wanted_chart(&charts, &want(1, true, "challenge", 13)),
            Some(1)
        );
        assert_eq!(
            wanted_chart(&charts, &want(0, false, "challenge", 13)),
            None
        );
        // a list in another order still finds it by difficulty and meter
        assert_eq!(
            wanted_chart(&charts, &want(0, true, "Challenge", 13)),
            Some(1)
        );

        // Two Edits are told apart by the index the menu showed, not by name.
        let edits = [
            chart(false, "Edit", 9),
            chart(false, "Edit", 9),
            chart(false, "Edit", 14),
        ];
        let second = Want::at(&edits, 1).expect("a known chart");
        assert_eq!(wanted_chart(&edits, &second), Some(1));
        assert_eq!(wanted_chart(&edits, &want(5, false, "edit", 14)), Some(2));
        assert_eq!(wanted_chart(&edits, &want(5, false, "edit", 1)), Some(0));

        // The menu's choice decides the opening chart, and so the lanes laid
        // out before the charts arrive.
        let mixed = [chart(false, "Challenge", 12), chart(true, "Challenge", 13)];
        assert_eq!(opening_chart(&mixed, None), Some(0));
        let doubles = Want::at(&mixed, 1);
        assert_eq!(opening_chart(&mixed, doubles.as_ref()), Some(1));
    }

    fn previewing(charts: Vec<PreviewChart>) -> State {
        let mut state = super::super::state::init();
        start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        state.pending_songs.clear();
        state.song_preview = Arc::new(PreviewSnapshot {
            phase: PreviewPhase::Loading,
            pack_id: 7,
            title: "Song A".to_owned(),
            start: 30.0,
            length: 15.0,
            bpm: 120.0,
            charts: Arc::from(charts),
            ..PreviewSnapshot::default()
        });
        state
    }

    /// The relay's answer moves to Ready with the sample on disk.
    fn ready(state: &mut State) {
        let mut ready = (*state.song_preview).clone();
        ready.phase = PreviewPhase::Ready;
        ready.audio_path = Some(std::path::PathBuf::from("cache/preview.ogg"));
        state.song_preview = Arc::new(ready);
    }

    /// Charts land first and the window picks one; the audio follows and is
    /// played from the sample window, with the original's fades.
    #[test]
    fn the_preview_picks_a_chart_then_plays_the_sample() {
        let mut state = previewing(vec![chart(false, "Easy", 3), chart(false, "Challenge", 12)]);
        sync(&mut state);
        assert_eq!(state.preview.as_ref().and_then(|p| p.chart), Some(1));
        assert!(state.pending_audio.is_empty(), "no audio yet");
        assert!(
            !chart_showing(&state),
            "loading: UP/DOWN still move the songs, as the original's do"
        );

        ready(&mut state);
        sync(&mut state);
        let Some(AudioRequest::PlayMusic { cut, looping, .. }) = state.pending_audio.first() else {
            panic!("the sample plays");
        };
        assert!((cut.start_sec - 30.0).abs() < 1e-6);
        assert!((cut.length_sec - 15.0).abs() < 1e-6);
        assert!((cut.fade_in_sec - 0.5).abs() < 1e-6);
        assert!((cut.fade_out_sec - 1.5).abs() < 1e-6);
        assert!(!looping);
        sync(&mut state);
        assert_eq!(state.pending_audio.len(), 1, "asked once");

        // UP/DOWN step the chart, without wrapping.
        assert!(chart_showing(&state));
        assert!(!step_chart(&mut state, 1), "already the last");
        assert!(step_chart(&mut state, -1));
        assert_eq!(state.preview.as_ref().and_then(|p| p.chart), Some(0));
    }

    /// Notes light their receptors as the music reaches them, and the
    /// preview stops by itself a moment after the sample ends.
    #[test]
    fn notes_land_on_the_beat_and_the_sample_ends_itself() {
        let mut taps = chart(false, "Challenge", 12);
        taps.notes = Arc::from(vec![
            PreviewNote {
                time: 0.5,
                cols: 0b0001,
                lifts: 0,
                mines: 0,
                quant: 0,
            },
            PreviewNote {
                time: 0.6,
                cols: 0b0010,
                lifts: 0,
                mines: 0b0010,
                quant: 1,
            },
            PreviewNote {
                time: 9.0,
                cols: 0b1000,
                lifts: 0,
                mines: 0,
                quant: 0,
            },
        ]);
        let mut state = previewing(vec![taps]);
        ready(&mut state);
        sync(&mut state);

        // the clock reports seconds into the file; the sample starts at 30
        state.music_time = Some(31.0);
        update(&mut state, 0.016);
        let preview = state.preview.as_ref().expect("still up");
        assert_eq!(preview.passed, 2, "the two notes before 1 s have landed");
        assert_eq!(preview.hit_at[0], Some(0.5));
        assert_eq!(
            preview.hit_at[1], None,
            "a mine passes its receptor without pressing it"
        );

        state.pending_songs.clear();
        state.music_time = Some(30.0 + 15.0 + 2.0);
        update(&mut state, 0.016);
        assert!(!busy(&state), "over");
        assert_eq!(state.pending_songs, vec![SongRequest::StopPreview]);
        assert!(state.pending_audio.contains(&AudioRequest::StopMusic));
    }

    /// With no audio output the picture runs on the wall clock, as the
    /// original's always does; with a music clock that never moves, the
    /// sample is given up on a little after it would have ended.
    #[test]
    fn a_sample_without_a_running_music_clock_still_plays_and_ends() {
        let mut taps = chart(false, "Challenge", 12);
        taps.notes = Arc::from(vec![PreviewNote {
            time: 0.5,
            cols: 0b0100,
            lifts: 0,
            mines: 0,
            quant: 0,
        }]);
        let mut state = previewing(vec![taps.clone()]);
        ready(&mut state);
        sync(&mut state);
        state.music_time = None;
        for _ in 0..40 {
            update(&mut state, 0.02);
        }
        let preview = state.preview.as_ref().expect("still up");
        assert_eq!(preview.hit_at[2], Some(0.5), "the wall clock lands notes");
        assert!(elapsed(&state).is_some_and(|time| (time - 0.8).abs() < 1e-3));

        let mut stuck = previewing(vec![taps]);
        ready(&mut stuck);
        sync(&mut stuck);
        // the clock reads the sample's first instant, and stays there
        stuck.music_time = Some(30.0);
        let mut seconds = 0.0;
        while busy(&stuck) && seconds < 60.0 {
            update(&mut stuck, 0.1);
            seconds += 0.1;
        }
        assert!(!busy(&stuck), "the preview does not hang open");
        assert!(
            seconds > 15.0 + TAIL && seconds < 15.0 + TAIL + CLOCK_GRACE + 0.5,
            "ended after {seconds} s"
        );
        assert!(stuck.pending_songs.contains(&SongRequest::StopPreview));
    }

    /// A failure closes the window, says why in the song list's header, and
    /// puts the preview back to idle.
    #[test]
    fn a_failed_preview_says_why() {
        let mut state = previewing(Vec::new());
        let mut failed = (*state.song_preview).clone();
        failed.phase = PreviewPhase::Error;
        failed.message = Some("no song matching \"Song A\" in pack 7".to_owned());
        state.song_preview = Arc::new(failed);
        sync(&mut state);
        assert!(!busy(&state));
        assert_eq!(
            state.preview_message.as_deref(),
            Some("no song matching \"Song A\" in pack 7")
        );
        assert_eq!(state.pending_songs, vec![SongRequest::StopPreview]);
    }

    /// Trying the same song again is a new attempt: the last one's failure,
    /// still the snapshot this frame, is not its answer.
    #[test]
    fn a_retry_is_not_answered_by_the_last_failure() {
        let mut state = previewing(Vec::new());
        let mut failed = (*state.song_preview).clone();
        failed.phase = PreviewPhase::Error;
        failed.message = Some("the preview endpoint is not reachable".to_owned());
        state.song_preview = Arc::new(failed);
        start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        sync(&mut state);
        update(&mut state, 0.016);
        let preview = state.preview.as_ref().expect("the window is up");
        assert!(!preview.closing, "not closed by the last attempt");
        assert!(state.preview_message.is_none());
        assert!(snapshot_for(&state, preview).is_none());

        // The shell sends it; the next snapshot is this attempt's.
        state.pending_songs.clear();
        let mut loading = (*state.song_preview).clone();
        loading.phase = PreviewPhase::Loading;
        loading.message = None;
        state.song_preview = Arc::new(loading);
        sync(&mut state);
        assert!(busy(&state));
        assert!(
            state
                .preview
                .as_ref()
                .is_some_and(|p| snapshot_for(&state, p).is_some())
        );
    }

    /// A stop and a start of the same song in one frame: the sample the stop
    /// cancelled is not played for the start.
    #[test]
    fn a_restart_in_one_frame_does_not_play_the_cancelled_sample() {
        let mut state = previewing(vec![chart(false, "Challenge", 12)]);
        ready(&mut state);
        stop(&mut state);
        start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        sync(&mut state);
        update(&mut state, 0.016);
        assert!(state.pending_audio.is_empty(), "nothing played");
        assert!(state.preview.as_ref().is_some_and(|p| !p.playing));
    }

    /// A sample with nothing to draw plays with no window: the window fades,
    /// the preview runs on and still stops itself.
    #[test]
    fn a_sample_with_no_charts_plays_without_a_window() {
        let mut state = previewing(Vec::new());
        for _ in 0..20 {
            update(&mut state, 0.02);
        }
        let opened = state.preview.as_ref().map_or(0.0, |p| p.fade);
        assert!(opened > 0.99, "loading keeps the window and its spinner");

        ready(&mut state);
        sync(&mut state);
        state.music_time = Some(31.0);
        for _ in 0..30 {
            update(&mut state, 0.02);
        }
        let preview = state.preview.as_ref().expect("still playing");
        assert!(preview.playing && !preview.closing);
        assert!(preview.fade <= 0.0, "faded out");
        state.music_time = Some(30.0 + 15.0 + 2.0);
        update(&mut state, 0.02);
        assert!(!busy(&state));
    }

    /// The equalizers and the skin keep the song's tempo, and 128 where the
    /// simfile gave none.
    #[test]
    fn the_beat_follows_the_songs_tempo() {
        let mut state = previewing(vec![chart(false, "Challenge", 12)]);
        assert_eq!(beat(&state), None, "nothing before the sample plays");
        ready(&mut state);
        sync(&mut state);
        state.music_time = Some(32.0);
        assert!(beat(&state).is_some_and(|beat| (beat - 4.0).abs() < 1e-4));
        let mut silent = (*state.song_preview).clone();
        silent.bpm = 0.0;
        state.song_preview = Arc::new(silent);
        assert!(beat(&state).is_some_and(|beat| (beat - 2.0 * 128.0 / 60.0).abs() < 1e-4));
    }

    /// The header's word on the sample: how much has arrived, then playing.
    #[test]
    fn the_header_says_what_the_sample_is_doing() {
        let mut state = previewing(Vec::new());
        assert_eq!(sample_label(&state).as_deref(), Some("loading sample..."));
        let mut arriving = (*state.song_preview).clone();
        arriving.progress = Some(0.4);
        state.song_preview = Arc::new(arriving);
        assert_eq!(sample_label(&state).as_deref(), Some("loading sample  40%"));
        ready(&mut state);
        sync(&mut state);
        assert_eq!(sample_label(&state).as_deref(), Some("playing a sample"));
        stop(&mut state);
        assert_eq!(sample_label(&state), None);
    }

    fn install(title: &str, phase: SongInstallPhase) -> SongInstall {
        SongInstall {
            pack_id: 7,
            title: title.to_owned(),
            artist: String::new(),
            group: deadsync_online::smo_songs::SINGLES_GROUP.to_owned(),
            phase,
            downloaded_bytes: 3 * 1024 * 1024,
            total_bytes: None,
            message: Some("HTTP 404".to_owned()),
        }
    }

    /// A song's install is named in its line, so the line still says which
    /// song it is about with the cursor on another; a request that would be
    /// refused is known before it is made.
    #[test]
    fn single_song_lines_name_their_song() {
        let mut state = super::super::state::init();
        state.song_installs = Arc::new(SongInstallsSnapshot {
            installs: Arc::from(vec![
                install("Song A", SongInstallPhase::Downloading),
                install("Song B", SongInstallPhase::Error),
                install("Song C", SongInstallPhase::Installed),
            ]),
            revision: 1,
        });
        let line = |state: &State, title| {
            song_install(state, 7, title, "").and_then(|install| song_install_line(state, install))
        };
        assert_eq!(
            line(&state, "Song A").as_deref(),
            Some("getting Song A  3.0 MB so far")
        );
        assert_eq!(
            line(&state, "Song B").as_deref(),
            Some("could not get Song B: HTTP 404")
        );
        assert_eq!(
            line(&state, "Song C").as_deref(),
            Some("added Song C - reload songs when you leave to play it")
        );
        assert_eq!(
            song_refusal(&state, 7, "Song A", ""),
            Some("that song is already on its way")
        );
        assert_eq!(
            song_refusal(&state, 7, "Song B", ""),
            None,
            "a failure may retry"
        );
        assert_eq!(
            song_refusal(&state, 7, "Song C", ""),
            Some("that song was already added this session")
        );
        assert_eq!(song_refusal(&state, 7, "Song D", ""), None);
        assert_eq!(
            song_refusal(&state, 7, "Song C", "Another"),
            None,
            "the same title by another artist is another song"
        );
        assert_eq!(songs_added(&state), 1);

        // Once a rescan has loaded it, it is neither owed a reload nor
        // captioned as wanting one.
        state.reloaded_songs = vec![(7, "Song C".to_owned(), String::new())];
        assert_eq!(songs_added(&state), 0);
        assert_eq!(line(&state, "Song C"), None);
    }

    /// Fixed speed, as the original: C516 at half size is 275.2 px a second.
    #[test]
    fn the_speed_is_the_originals() {
        assert!((PX_PER_SECOND - 275.2).abs() < 1e-3);
    }
}

#[cfg(test)]
#[path = "preview_pipelines_original.rs"]
mod pipelines_original;
#[cfg(test)]
#[path = "preview_pipelines_perf.rs"]
mod pipelines_perf;
