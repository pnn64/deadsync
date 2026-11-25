//! Disk cache of completed song-Lua bakes.
//!
//! Baked mode runs a song's scripts before gameplay and keeps only native
//! scene data. That data is a function of the baker, the song's files and the
//! session the scripts were shown. This cache stores it so the same session
//! does not pay for the same bake twice.
//!
//! An entry is reused only when all of the following still hold:
//!
//! - the build that wrote it is the build reading it;
//! - no file under the song folder was added, removed or changed (scripts
//!   by content, media by size and modification time);
//! - every recorded input has the value it had during the bake.
//!
//! Recorded inputs are named values: each field of the compile context, the
//! song's declared Lua layers, inputs the host supplies, calendar values the
//! scripts read, and script files loaded from outside the song folder. Every
//! context field is recorded today, including ones the scripts never asked
//! about. Recording only the fields a bake read would raise the hit rate and
//! needs no format change, but each field's readers must first report through
//! [`crate::bake_inputs`].
//!
//! Bakes that cannot be reproduced are never stored: one that drew from the
//! wall-time random stream, read the time of day, or used Lua's `io`, `os` or
//! native file loading. A bake that kept its Lua scene for runtime execution
//! is not a completed bake and is not stored either.
//!
//! Known gaps, all outside what the scripts can be observed reading: media
//! and noteskin files outside the song folder, and files whose presence alone
//! (not content) steers a path lookup outside the song folder. The host can
//! close any of these by naming a fingerprint in its supplied inputs.
//!
//! Cache contents are regenerable. A file that cannot be read, parsed,
//! verified or decoded is deleted and the song is baked again; the cache
//! never turns a loadable song into an error.
use crate::bake_inputs::{self, BakeReads};
use crate::playback::{PreparedGameplaySongLua, prepare_song_lua_with_runtime_fallback};
use crate::{SongLuaCompileContext, SongLuaPlayerContext};
use bincode::{Decode, Encode};
use deadlib_present::actors::TextAttribute;
use deadlib_render_core::TexturedMeshVertex;
use deadsync_chart::SongData;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use twox_hash::XxHash64;

type CompiledSongLua<S> = crate::CompiledSongLua<
    crate::SongLuaOverlayActor<crate::SongLuaOverlayKind<S, TexturedMeshVertex, TextAttribute>>,
>;

const MAGIC: &[u8; 14] = b"DeadBakeCache\0";
/// Bump when the entry layout below changes. Changes to baked types need no
/// bump: they change the build identity.
const FORMAT: u32 = 1;
const BUILD: &str = env!("DEADSYNC_SONG_LUA_BAKE_BUILD");
const ENTRY_EXTENSION: &str = "bake";
const DEFAULT_ENTRIES_PER_SONG: usize = 8;
const COMPRESSION_LEVEL: i32 = 3;
/// Bounds applied before trusting sizes read from a file.
const MAX_INPUTS: u32 = 1 << 16;
const MAX_INPUT_NAME_BYTES: usize = 1 << 12;
const MAX_BAKE_BYTES: u64 = 1 << 30;
const MAX_FOLDER_DEPTH: usize = 16;
/// Larger "scripts" fall back to size and modification time.
const MAX_SCRIPT_BYTES: u64 = 16 << 20;
const STALE_TEMPORARY_AGE: Duration = Duration::from_secs(60 * 60);

/// Prefix of inputs recorded while the scripts ran, as opposed to inputs
/// known before the bake started.
const HOST_ANSWER: &str = "host.";
const EXTERNAL_FILE: &str = "file.";

/// A named input and a hash of its value.
pub type BakeInput = (String, u64);

/// Where bakes are kept, and which host wrote them.
#[derive(Debug, Clone)]
pub struct BakeCache {
    directory: PathBuf,
    host_build: String,
    entries_per_song: usize,
}

impl BakeCache {
    /// `host_build` identifies the code outside this crate that feeds or
    /// consumes a bake (chart parsing, noteskin resolution). This crate's own
    /// build identity is always part of the key.
    pub fn new(directory: impl Into<PathBuf>, host_build: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            host_build: host_build.into(),
            entries_per_song: DEFAULT_ENTRIES_PER_SONG,
        }
    }

    /// Keep at most this many sessions per song; the least recently used
    /// entry is dropped first. Zero disables storing.
    #[must_use]
    pub const fn with_entries_per_song(mut self, entries: usize) -> Self {
        self.entries_per_song = entries;
        self
    }

    fn song_directory(&self, song_dir: &Path) -> PathBuf {
        let mut hasher = XxHash64::with_seed(0);
        hasher.write(song_dir.to_string_lossy().as_bytes());
        self.directory.join(format!("{:016x}", hasher.finish()))
    }

    /// Everything that invalidates all of a song's entries at once.
    fn fixed_key(&self, song_dir: &Path) -> [u8; 16] {
        let mut low = XxHash64::with_seed(0);
        let mut high = XxHash64::with_seed(0x9e37_79b9_7f4a_7c15);
        for hasher in [&mut low, &mut high] {
            hasher.write_u32(FORMAT);
            write_text(hasher, BUILD);
            write_text(hasher, &self.host_build);
            hash_folder(hasher, song_dir, song_dir, 0);
        }
        let mut key = [0; 16];
        key[..8].copy_from_slice(&low.finish().to_le_bytes());
        key[8..].copy_from_slice(&high.finish().to_le_bytes());
        key
    }
}

/// What the cache did for one preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BakeCacheStatus {
    /// A stored bake matched this session; no script ran.
    Hit,
    /// The song was baked and the result stored.
    Stored,
    /// The song was baked. The result was not stored, for this reason.
    NotStored(String),
    /// The cache does not apply: runtime mode, or a song without Lua.
    Bypassed,
}

/// [`prepare_song_lua_with_runtime_fallback`] through the bake cache.
///
/// `host_inputs` names session inputs this crate cannot see, such as a
/// fingerprint of the selected noteskins. They are compared like every other
/// recorded input.
pub fn prepare_song_lua_cached<S>(
    cache: &BakeCache,
    song: &SongData,
    context: &SongLuaCompileContext,
    host_inputs: &[BakeInput],
    compile_song_lua: impl Fn(&Path, &SongLuaCompileContext) -> Result<CompiledSongLua<S>, String>,
    compile_song_lua_layers: impl Fn(
        &[&Path],
        usize,
        &SongLuaCompileContext,
    ) -> Result<Vec<CompiledSongLua<S>>, String>,
) -> (PreparedGameplaySongLua<S>, BakeCacheStatus)
where
    S: Clone + fmt::Debug + Encode + Decode<()> + 'static,
{
    if context.live
        || (song.background_lua_changes.is_empty() && song.foreground_lua_changes.is_empty())
    {
        let prepared = prepare_song_lua_with_runtime_fallback(
            song,
            context,
            compile_song_lua,
            compile_song_lua_layers,
        );
        return (prepared, BakeCacheStatus::Bypassed);
    }
    let lookup_started = Instant::now();
    let fixed = cache.fixed_key(&context.song_dir);
    let session = session_inputs(song, context, host_inputs);
    let directory = cache.song_directory(&context.song_dir);
    if let Some(prepared) = load(&directory, &fixed, &session) {
        log::info!(
            "Song Lua bake cache hit for '{}' in {:.1} ms",
            song.title,
            lookup_started.elapsed().as_secs_f64() * 1000.0,
        );
        return (prepared, BakeCacheStatus::Hit);
    }
    // A compile error may be transient (a file mid-copy), and its fallback
    // output is not what a clean bake of the same inputs produces.
    let compile_failed = Cell::new(false);
    let (prepared, reads) = bake_inputs::record(|| {
        prepare_song_lua_with_runtime_fallback(
            song,
            context,
            |path, context| {
                compile_song_lua(path, context).inspect_err(|_| compile_failed.set(true))
            },
            |paths, primary, context| {
                compile_song_lua_layers(paths, primary, context)
                    .inspect_err(|_| compile_failed.set(true))
            },
        )
    });
    let status = match unstorable(cache, &prepared, &reads, compile_failed.get()) {
        Some(reason) => BakeCacheStatus::NotStored(reason.to_owned()),
        None => {
            let inputs = entry_inputs(session, &reads, &context.song_dir);
            match store(cache, &directory, &fixed, &inputs, &prepared) {
                Ok(bytes) => {
                    log::info!(
                        "Song Lua bake for '{}' stored in the bake cache ({bytes} bytes)",
                        song.title,
                    );
                    BakeCacheStatus::Stored
                }
                Err(error) => BakeCacheStatus::NotStored(error),
            }
        }
    };
    if let BakeCacheStatus::NotStored(reason) = &status {
        log::info!(
            "Song Lua bake for '{}' not stored in the bake cache: {reason}",
            song.title,
        );
    }
    (prepared, status)
}

fn unstorable<S>(
    cache: &BakeCache,
    prepared: &PreparedGameplaySongLua<S>,
    reads: &BakeReads,
    compile_failed: bool,
) -> Option<&'static str> {
    if cache.entries_per_song == 0 {
        return Some("storing is disabled");
    }
    let mut layers = prepared
        .primary
        .iter()
        .map(|primary| &primary.compiled)
        .chain(
            prepared
                .background_layers
                .iter()
                .map(|layer| &layer.compiled),
        )
        .chain(
            prepared
                .foreground_layers
                .iter()
                .map(|layer| &layer.compiled),
        )
        .peekable();
    if layers.peek().is_none() {
        return Some("nothing was baked");
    }
    // Checked before compile errors: a song is moved to runtime execution
    // by first failing its baked attempt.
    if layers.any(|compiled| compiled.live_scene.is_some()) {
        return Some("the song runs its Lua during gameplay");
    }
    if compile_failed {
        return Some("a layer failed to compile");
    }
    reads.volatile
}

// ---------------------------------------------------------------------------
// Inputs

fn write_text(hasher: &mut XxHash64, text: &str) {
    hasher.write_u64(text.len() as u64);
    hasher.write(text.as_bytes());
}

/// Hash a value's `Debug` text without building the string. Derived `Debug`
/// prints every field, and floats in their shortest exact form, so two values
/// hash alike only when they print alike. The build identity pins the
/// formatting itself.
fn debug_hash(value: &impl fmt::Debug) -> u64 {
    struct Sink(XxHash64);
    impl fmt::Write for Sink {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            self.0.write(text.as_bytes());
            Ok(())
        }
    }
    let mut sink = Sink(XxHash64::with_seed(0));
    // The sink never fails, so neither does the write.
    let _ = write!(sink, "{value:?}");
    sink.0.finish()
}

macro_rules! record {
    ($inputs:ident, $prefix:expr, $($field:ident),+ $(,)?) => {
        $($inputs.push((format!("{}{}", $prefix, stringify!($field)), debug_hash($field)));)+
    };
}

/// Every input known before the bake starts.
fn session_inputs(
    song: &SongData,
    context: &SongLuaCompileContext,
    host_inputs: &[BakeInput],
) -> Vec<BakeInput> {
    // Naming every field makes a new context field a compile error here
    // rather than an input the cache silently ignores.
    let SongLuaCompileContext {
        // Reports progress only; never changes script execution or output.
        bake_progress: _,
        media_probes,
        live,
        defer_initial_live_update,
        screen_loading,
        background_layer_count,
        background_sources,
        layer_start_beats,
        background_activations,
        background_media_activations,
        background_upper_left_colors,
        foreground_names,
        song_dir,
        theme_combo_font_directory,
        song_banner_path,
        main_title,
        song_display_bpms,
        song_timing_bpms,
        song_timing,
        player_timing,
        song_step_timing,
        song_step_notes,
        song_step_row_beats,
        song_music_rate,
        // The seed is wall time. A bake that draws from the stream is never
        // stored, and one that does not draw cannot depend on the seed.
        random_seed: _,
        #[cfg(feature = "test-support")]
        message_actor_order,
        #[cfg(feature = "test-support")]
        trace_dynamic_commands,
        #[cfg(feature = "test-support")]
        initial_lua_gc_stopped,
        #[cfg(feature = "test-support")]
        capture_overlay_probe,
        #[cfg(feature = "test-support")]
        host_clock_inputs,
        #[cfg(feature = "test-support")]
        compile_clock_inputs,
        #[cfg(feature = "test-support")]
        compile_screen_actor_deltas,
        #[cfg(feature = "test-support")]
        compile_option_clock_deltas,
        music_length_seconds,
        song_note_bounds,
        style_name,
        versus_player_screen_x,
        song_steps,
        song_step_chart_indices,
        player_step_indices,
        event_mode,
        global_offset_seconds,
        pad_stick_seconds,
        screen_width,
        screen_height,
        display_width,
        display_height,
        display_aspect_ratio,
        render_to_texture,
        video_renderers,
        last_seen_video_driver,
        players,
        confusion_offset_available,
        confusion_available,
        amod_available,
    } = context;
    let mut inputs = Vec::with_capacity(128);
    // The probes read media files, which the fixed key already covers. Which
    // probes the host supplies still decides what a script is told.
    let sound_length_probe = &media_probes.sound_length_seconds.is_some();
    let movie_dimensions_probe = &media_probes.movie_dimensions.is_some();
    record!(inputs, "", sound_length_probe, movie_dimensions_probe);
    record!(
        inputs,
        "",
        live,
        defer_initial_live_update,
        screen_loading,
        background_layer_count,
        background_sources,
        layer_start_beats,
        background_activations,
        background_media_activations,
        background_upper_left_colors,
        foreground_names,
        song_dir,
        theme_combo_font_directory,
        song_banner_path,
        main_title,
        song_display_bpms,
        song_timing_bpms,
        song_timing,
        player_timing,
        song_step_timing,
        song_step_notes,
        song_step_row_beats,
        song_music_rate,
        music_length_seconds,
        song_note_bounds,
        style_name,
        versus_player_screen_x,
        song_steps,
        song_step_chart_indices,
        player_step_indices,
        event_mode,
        global_offset_seconds,
        pad_stick_seconds,
        screen_width,
        screen_height,
        display_width,
        display_height,
        display_aspect_ratio,
        render_to_texture,
        video_renderers,
        last_seen_video_driver,
        confusion_offset_available,
        confusion_available,
        amod_available,
    );
    #[cfg(feature = "test-support")]
    record!(
        inputs,
        "",
        message_actor_order,
        trace_dynamic_commands,
        initial_lua_gc_stopped,
        capture_overlay_probe,
        host_clock_inputs,
        compile_clock_inputs,
        compile_screen_actor_deltas,
        compile_option_clock_deltas,
    );
    for (index, player) in players.iter().enumerate() {
        let SongLuaPlayerContext {
            enabled,
            autoplay,
            difficulty,
            speedmod,
            mini,
            scroll,
            perspective,
            turn_option,
            column_take_from,
            display_bpms,
            noteskin_name,
            judgment_texture,
            column_cues,
            background_filter_percent,
            hide_lifebar,
            hide_danger,
            life_meter_type,
            nps_graph_at_top,
            step_statistics_enabled,
            timing_windows,
            screen_x,
            screen_y,
        } = player;
        let prefix = format!("p{}.", index + 1);
        record!(
            inputs,
            prefix,
            enabled,
            autoplay,
            difficulty,
            speedmod,
            mini,
            scroll,
            perspective,
            turn_option,
            column_take_from,
            display_bpms,
            noteskin_name,
            judgment_texture,
            column_cues,
            background_filter_percent,
            hide_lifebar,
            hide_danger,
            life_meter_type,
            nps_graph_at_top,
            step_statistics_enabled,
            timing_windows,
            screen_x,
            screen_y,
        );
    }
    // Preparation reads the layer declarations straight from the song.
    let SongData {
        title,
        background_changes,
        background_layer2_changes,
        background_lua_changes,
        foreground_lua_changes,
        ..
    } = song;
    record!(
        inputs,
        "song.",
        title,
        background_changes,
        background_layer2_changes,
        background_lua_changes,
        foreground_lua_changes,
    );
    // The month is exposed as constant globals no binding can observe being
    // read, so it is an input of every bake.
    let month = crate::song_lua_local_date_globals().month_of_year;
    inputs.push(("calendar.month".to_owned(), month as i64 as u64));
    for (name, value) in host_inputs {
        inputs.push((format!("supplied.{name}"), *value));
    }
    inputs
}

/// The full input list stored with an entry: the session plus what the
/// scripts were observed reading while they ran.
fn entry_inputs(mut inputs: Vec<BakeInput>, reads: &BakeReads, song_dir: &Path) -> Vec<BakeInput> {
    for (name, value) in &reads.answers {
        inputs.push((format!("{HOST_ANSWER}{name}"), *value));
    }
    let song_dir = lexically_normal(song_dir);
    for path in &reads.files {
        // Files under the song folder are already part of the fixed key.
        // `song/../shared.lua` is spelled under it without being inside it.
        let path = lexically_normal(path);
        if !path.starts_with(&song_dir) {
            inputs.push((
                format!("{EXTERNAL_FILE}{}", path.to_string_lossy()),
                file_stamp(&path),
            ));
        }
    }
    inputs
}

/// Resolve `.` and `..` by spelling alone. Unlike canonicalization this
/// works for a file that no longer exists and never follows a link.
fn lexically_normal(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normal.pop() {
                    normal.push(component);
                }
            }
            other => normal.push(other),
        }
    }
    normal
}

/// The current value of an input that was recorded while scripts ran.
fn observed_input(name: &str) -> Option<u64> {
    if let Some(path) = name.strip_prefix(EXTERNAL_FILE) {
        return Some(file_stamp(Path::new(path)));
    }
    let date = crate::song_lua_local_date_globals();
    let value = match name.strip_prefix(HOST_ANSWER)? {
        "Year" => date.year,
        "MonthOfYear" => date.month_of_year,
        "DayOfMonth" => date.day_of_month,
        "DayOfYear" => {
            use chrono::Datelike;
            let month = u32::try_from(date.month_of_year.checked_add(1)?).ok()?;
            let day = u32::try_from(date.day_of_month).ok()?;
            chrono::NaiveDate::from_ymd_opt(date.year, month, day)?.ordinal0() as i32
        }
        "Hour" => date.hour,
        _ => return None,
    };
    Some(value as i64 as u64)
}

fn inputs_match(stored: &[BakeInput], session: &HashMap<&str, u64>) -> bool {
    stored.iter().all(|(name, value)| {
        let current = if name.starts_with(HOST_ANSWER) || name.starts_with(EXTERNAL_FILE) {
            observed_input(name)
        } else {
            session.get(name.as_str()).copied()
        };
        current == Some(*value)
    })
}

fn modified_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos())
}

/// Files whose text the scripts execute or parse. These are identified by
/// content: an edit that keeps a script's size, saved within the filesystem's
/// timestamp granularity (two seconds on FAT), must still miss.
fn is_script_like(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            [
                "lua", "xml", "ini", "redir", "sprite", "sm", "ssc", "txt", "json",
            ]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Identity of one file's contents: the bytes of a script, or the size and
/// modification time of anything else. Media is too large to read on every
/// load and is replaced, not edited in place.
fn hash_file(hasher: &mut XxHash64, path: &Path, metadata: &fs::Metadata, by_content: bool) {
    if by_content
        && metadata.len() <= MAX_SCRIPT_BYTES
        && let Ok(bytes) = fs::read(path)
    {
        hasher.write_u8(4);
        hasher.write_u64(bytes.len() as u64);
        hasher.write(&bytes);
        return;
    }
    hasher.write_u8(1);
    hasher.write_u64(metadata.len());
    hasher.write_u128(modified_nanos(metadata));
}

/// Identity of a script the bake loaded from outside the song folder; a
/// missing file has its own stamp.
fn file_stamp(path: &Path) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => hash_file(&mut hasher, path, &metadata, true),
        Ok(_) => hasher.write_u8(2),
        Err(_) => hasher.write_u8(0),
    }
    hasher.finish()
}

/// Names and identities of everything under a folder, in a stable order.
/// Scripts can load any of it, so all of it is identity.
fn hash_folder(hasher: &mut XxHash64, root: &Path, directory: &Path, depth: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        hasher.write_u8(0);
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort_unstable();
    for path in entries {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let name = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        write_text(hasher, &name);
        if metadata.is_dir() {
            hasher.write_u8(2);
            // Bounded so a link cycle cannot walk forever.
            if depth < MAX_FOLDER_DEPTH {
                hash_folder(hasher, root, &path, depth + 1);
            }
        } else {
            hash_file(hasher, &path, &metadata, is_script_like(&path));
        }
    }
    hasher.write_u8(3);
}

// ---------------------------------------------------------------------------
// Entry files
//
// magic, format, fixed key, input count, inputs (name length, name, value),
// decoded length, packed length, hash of the decoded bytes, hash of all of the
// above; then the zstd-compressed bincode payload. Integers are little
// endian. Everything before the payload is uncompressed so an entry for
// another session is rejected without touching its payload.

struct Header {
    fixed: [u8; 16],
    inputs: Vec<BakeInput>,
    decoded_bytes: u64,
    packed_bytes: u64,
    decoded_hash: u64,
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// Reads fields while hashing exactly the bytes it consumed.
struct Checked<R> {
    reader: R,
    hasher: XxHash64,
}

impl<R: Read> Checked<R> {
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut bytes = [0; N];
        self.reader.read_exact(&mut bytes)?;
        self.hasher.write(&bytes);
        Ok(bytes)
    }

    fn u32(&mut self) -> io::Result<u32> {
        self.bytes().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> io::Result<u64> {
        self.bytes().map(u64::from_le_bytes)
    }
}

impl Header {
    fn read(reader: &mut impl Read) -> io::Result<Self> {
        let mut checked = Checked {
            reader,
            hasher: XxHash64::with_seed(0),
        };
        if &checked.bytes::<14>()? != MAGIC {
            return Err(invalid("not a bake cache entry"));
        }
        if checked.u32()? != FORMAT {
            return Err(invalid("unsupported bake cache format"));
        }
        let fixed = checked.bytes()?;
        let count = checked.u32()?;
        if count > MAX_INPUTS {
            return Err(invalid("too many recorded inputs"));
        }
        let mut inputs = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let length = checked.u32()? as usize;
            if length > MAX_INPUT_NAME_BYTES {
                return Err(invalid("recorded input name is too long"));
            }
            let mut name = vec![0; length];
            checked.reader.read_exact(&mut name)?;
            checked.hasher.write(&name);
            let name = String::from_utf8(name).map_err(|_| invalid("input name is not UTF-8"))?;
            inputs.push((name, checked.u64()?));
        }
        let decoded_bytes = checked.u64()?;
        let packed_bytes = checked.u64()?;
        let decoded_hash = checked.u64()?;
        if decoded_bytes > MAX_BAKE_BYTES || packed_bytes > MAX_BAKE_BYTES {
            return Err(invalid("bake is larger than the cache accepts"));
        }
        let expected = checked.hasher.finish();
        let mut stored = [0; 8];
        checked.reader.read_exact(&mut stored)?;
        if u64::from_le_bytes(stored) != expected {
            return Err(invalid("bake cache header is damaged"));
        }
        Ok(Self {
            fixed,
            inputs,
            decoded_bytes,
            packed_bytes,
            decoded_hash,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        out.extend_from_slice(&self.fixed);
        out.extend_from_slice(&(self.inputs.len() as u32).to_le_bytes());
        for (name, value) in &self.inputs {
            out.extend_from_slice(&(name.len() as u32).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.decoded_bytes.to_le_bytes());
        out.extend_from_slice(&self.packed_bytes.to_le_bytes());
        out.extend_from_slice(&self.decoded_hash.to_le_bytes());
        let mut hasher = XxHash64::with_seed(0);
        hasher.write(&out[start..]);
        out.extend_from_slice(&hasher.finish().to_le_bytes());
    }
}

fn bytes_hash(bytes: &[u8]) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(bytes);
    hasher.finish()
}

fn is_entry(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == ENTRY_EXTENSION)
}

enum Entry<T> {
    /// Valid, and for this session.
    Match(T),
    /// Valid, but for another session of the same song.
    OtherSession,
    /// From another build, an older song folder, or damaged.
    Stale(String),
}

fn read_entry<S>(
    path: &Path,
    fixed: &[u8; 16],
    session: &HashMap<&str, u64>,
) -> Entry<PreparedGameplaySongLua<S>>
where
    S: Decode<()> + 'static,
{
    let stale = |error: &dyn fmt::Display| Entry::Stale(error.to_string());
    let mut reader = match File::open(path) {
        Ok(file) => BufReader::new(file),
        Err(error) => return stale(&error),
    };
    let header = match Header::read(&mut reader) {
        Ok(header) => header,
        Err(error) => return stale(&error),
    };
    if header.fixed != *fixed {
        return stale(&"written by another build or for different song files");
    }
    if !inputs_match(&header.inputs, session) {
        return Entry::OtherSession;
    }
    let mut packed = Vec::new();
    if let Err(error) = reader.take(header.packed_bytes).read_to_end(&mut packed) {
        return stale(&error);
    }
    if packed.len() as u64 != header.packed_bytes {
        return stale(&"bake cache entry is truncated");
    }
    let decoded = match zstd::bulk::decompress(&packed, header.decoded_bytes as usize) {
        Ok(decoded) => decoded,
        Err(error) => return stale(&error),
    };
    if decoded.len() as u64 != header.decoded_bytes || bytes_hash(&decoded) != header.decoded_hash {
        return stale(&"bake cache payload is damaged");
    }
    match bincode::decode_from_slice::<PreparedGameplaySongLua<S>, _>(
        &decoded,
        bincode::config::standard(),
    ) {
        Ok((prepared, consumed)) if consumed == decoded.len() => Entry::Match(prepared),
        Ok(_) => stale(&"bake cache payload has trailing data"),
        Err(error) => stale(&error),
    }
}

fn load<S>(
    directory: &Path,
    fixed: &[u8; 16],
    session: &[BakeInput],
) -> Option<PreparedGameplaySongLua<S>>
where
    S: Decode<()> + 'static,
{
    let entries = fs::read_dir(directory).ok()?;
    let session: HashMap<&str, u64> = session
        .iter()
        .map(|(name, value)| (name.as_str(), *value))
        .collect();
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if !is_entry(&path) {
            continue;
        }
        match read_entry(&path, fixed, &session) {
            Entry::Match(prepared) => {
                // Eviction drops the least recently used entry.
                if let Ok(file) = File::options().write(true).open(&path) {
                    let _ = file.set_modified(SystemTime::now());
                }
                return Some(prepared);
            }
            Entry::OtherSession => {}
            Entry::Stale(reason) => {
                log::debug!("Removing bake cache entry {}: {reason}", path.display());
                let _ = fs::remove_file(&path);
            }
        }
    }
    None
}

fn store<S: Encode>(
    cache: &BakeCache,
    directory: &Path,
    fixed: &[u8; 16],
    inputs: &[BakeInput],
    prepared: &PreparedGameplaySongLua<S>,
) -> Result<usize, String> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);
    let decoded = bincode::encode_to_vec(prepared, bincode::config::standard())
        .map_err(|error| format!("the bake cannot be persisted: {error}"))?;
    if decoded.len() as u64 > MAX_BAKE_BYTES {
        return Err("the bake is larger than the cache accepts".to_owned());
    }
    let packed = zstd::bulk::compress(&decoded, COMPRESSION_LEVEL)
        .map_err(|error| format!("compression failed: {error}"))?;
    let header = Header {
        fixed: *fixed,
        inputs: inputs.to_vec(),
        decoded_bytes: decoded.len() as u64,
        packed_bytes: packed.len() as u64,
        decoded_hash: bytes_hash(&decoded),
    };
    let mut bytes = Vec::with_capacity(packed.len() + 64 * inputs.len() + 128);
    header.write(&mut bytes);
    bytes.extend_from_slice(&packed);

    // One file per distinct input list: the same session overwrites itself.
    let mut name = XxHash64::with_seed(0);
    for (input, value) in inputs {
        write_text(&mut name, input);
        name.write_u64(*value);
    }
    let path = directory.join(format!("{:016x}.{ENTRY_EXTENSION}", name.finish()));
    let temporary = directory.join(format!(
        ".{:016x}.{}.{}.tmp",
        name.finish(),
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed),
    ));
    let written = fs::create_dir_all(directory)
        .and_then(|()| fs::write(&temporary, &bytes))
        .and_then(|()| fs::rename(&temporary, &path));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot write {}: {error}", path.display()));
    }
    evict(directory, cache.entries_per_song);
    Ok(bytes.len())
}

/// Keep the most recently used entries and clear abandoned temporaries.
fn evict(directory: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    let mut stored = Vec::new();
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let Ok(modified) = fs::metadata(&path).and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if is_entry(&path) {
            stored.push((modified, path));
        } else if path.extension().is_some_and(|extension| extension == "tmp")
            && now
                .duration_since(modified)
                .is_ok_and(|age| age > STALE_TEMPORARY_AGE)
        {
            let _ = fs::remove_file(&path);
        }
    }
    if stored.len() > keep {
        stored.sort_unstable();
        for (_, path) in stored.drain(..stored.len() - keep) {
            let _ = fs::remove_file(path);
        }
    }
}
