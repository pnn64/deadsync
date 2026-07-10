use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SongOffsetSyncChange {
    pub simfile_path: PathBuf,
    pub delta_seconds: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongOffsetSaveSummary {
    pub saved_files: usize,
    pub skipped_read_only: usize,
    pub changed_tags_total: usize,
    pub first_saved_path: Option<PathBuf>,
    pub first_skipped_path: Option<PathBuf>,
}

#[inline(always)]
pub fn quantize_sync_offset_seconds(v: f32) -> f32 {
    (v / 0.001_f32).round() * 0.001_f32
}

#[inline(always)]
pub fn sync_offset_delta_seconds(start: f32, new: f32) -> Option<f32> {
    let delta = quantize_sync_offset_seconds(new) - quantize_sync_offset_seconds(start);
    (delta.abs() >= 0.000_1_f32).then_some(delta)
}

#[inline(always)]
pub fn sync_offset_target_seconds(start: f32, new: f32) -> Option<f32> {
    sync_offset_delta_seconds(start, new).map(|_| quantize_sync_offset_seconds(new))
}

#[inline(always)]
pub fn sync_change_line(label: &str, start: f32, new: f32) -> Option<String> {
    let start_q = quantize_sync_offset_seconds(start);
    let new_q = quantize_sync_offset_seconds(new);
    let delta_q = sync_offset_delta_seconds(start, new)?;
    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "{label} from {start_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
}

#[inline(always)]
pub fn sync_offset_changed(start: f32, new: f32) -> bool {
    sync_offset_target_seconds(start, new).is_some()
}

#[inline(always)]
pub fn sync_offset_saveable_changed(start: f32, new: f32, writable: bool) -> bool {
    sync_offset_changed(start, new) && writable
}

#[inline(always)]
pub fn gameplay_sync_offset_saveable_changed(
    initial_global_offset_seconds: f32,
    global_offset_seconds: f32,
    initial_song_offset_seconds: f32,
    song_offset_seconds: f32,
    song_writable: bool,
) -> bool {
    sync_offset_changed(initial_global_offset_seconds, global_offset_seconds)
        || sync_offset_saveable_changed(
            initial_song_offset_seconds,
            song_offset_seconds,
            song_writable,
        )
}

pub struct GameplaySyncPromptText<'a> {
    pub song_title: &'a str,
    pub song_writable: bool,
    pub initial_global_offset_seconds: f32,
    pub global_offset_seconds: f32,
    pub initial_song_offset_seconds: f32,
    pub song_offset_seconds: f32,
}

pub fn gameplay_sync_prompt_text(input: GameplaySyncPromptText<'_>) -> String {
    let mut text = String::with_capacity(320);

    if let Some(line) = sync_change_line(
        "Global Offset",
        input.initial_global_offset_seconds,
        input.global_offset_seconds,
    ) {
        text.push_str(&line);
        text.push_str("\n\n");
    }

    if let Some(line) = sync_change_line(
        "Song offset",
        input.initial_song_offset_seconds,
        input.song_offset_seconds,
    ) {
        if input.song_writable {
            text.push_str("You have changed the timing of\n");
            text.push_str(input.song_title);
            text.push_str(":\n\n");
            text.push_str(&line);
            text.push_str("\n\n");
        } else {
            text.push_str("Song offset changes for\n");
            text.push_str(input.song_title);
            text.push_str("\nwill be discarded because the song folder is read-only.\n\n");
        }
    }

    text.push_str("Would you like to save these changes?\n");
    text.push_str("Choosing NO will discard your changes.");
    text
}

#[inline(always)]
pub fn format_offset_tag_value(value: f32) -> String {
    let mut v = quantize_sync_offset_seconds(value);
    if v.abs() < 0.000_5_f32 {
        v = 0.0;
    }
    format!("{v:.3}")
}

pub fn rewrite_simfile_offset_tags(
    simfile_bytes: &[u8],
    delta: f32,
) -> Result<(Vec<u8>, usize), String> {
    const TAG: &[u8] = b"#OFFSET:";
    let len = simfile_bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len.saturating_add(64));
    let mut changed = 0usize;
    let mut cursor = 0usize;
    let mut i = 0usize;

    while i + TAG.len() <= len {
        if simfile_bytes[i..i + TAG.len()].eq_ignore_ascii_case(TAG) {
            out.extend_from_slice(&simfile_bytes[cursor..i + TAG.len()]);
            let mut value_start = i + TAG.len();
            while value_start < len
                && simfile_bytes[value_start].is_ascii_whitespace()
                && simfile_bytes[value_start] != b';'
            {
                value_start += 1;
            }
            out.extend_from_slice(&simfile_bytes[i + TAG.len()..value_start]);

            let mut value_end = value_start;
            while value_end < len && simfile_bytes[value_end] != b';' {
                value_end += 1;
            }
            if value_end >= len {
                return Err("Malformed #OFFSET tag: missing ';' terminator".to_string());
            }

            let raw = &simfile_bytes[value_start..value_end];
            let Some(trim_start) = raw.iter().position(|b| !b.is_ascii_whitespace()) else {
                return Err("Malformed #OFFSET tag: empty value".to_string());
            };
            let Some(trim_end_inclusive) = raw.iter().rposition(|b| !b.is_ascii_whitespace())
            else {
                return Err("Malformed #OFFSET tag: empty value".to_string());
            };
            let trim_end = trim_end_inclusive + 1;
            let value_bytes = &raw[trim_start..trim_end];
            let value_str = std::str::from_utf8(value_bytes)
                .map_err(|_| "Malformed #OFFSET tag: value is not valid UTF-8".to_string())?;
            let parsed_value = value_str
                .parse::<f32>()
                .map_err(|_| format!("Malformed #OFFSET tag value: '{value_str}'"))?;
            let new_value = parsed_value + delta;

            out.extend_from_slice(&raw[..trim_start]);
            out.extend_from_slice(format_offset_tag_value(new_value).as_bytes());
            out.extend_from_slice(&raw[trim_end..]);
            out.push(b';');

            changed = changed.saturating_add(1);
            i = value_end.saturating_add(1);
            cursor = i;
            continue;
        }
        i += 1;
    }

    out.extend_from_slice(&simfile_bytes[cursor..]);
    Ok((out, changed))
}

#[inline(always)]
pub fn simfile_backup_path(simfile_path: &Path) -> PathBuf {
    let mut backup = OsString::from(simfile_path.as_os_str());
    backup.push(".old");
    PathBuf::from(backup)
}

pub fn save_song_offset_delta_to_simfile(simfile_path: &Path, delta: f32) -> Result<usize, String> {
    let simfile_bytes = std::fs::read(simfile_path)
        .map_err(|e| format!("Failed to read simfile '{}': {e}", simfile_path.display()))?;
    let (rewritten, changed_tags) = rewrite_simfile_offset_tags(&simfile_bytes, delta)?;
    if changed_tags == 0 {
        return Err(format!(
            "No #OFFSET tags found in simfile '{}'",
            simfile_path.display()
        ));
    }

    let backup_path = simfile_backup_path(simfile_path);
    std::fs::copy(simfile_path, &backup_path).map_err(|e| {
        format!(
            "Failed to create backup '{}': {e}",
            backup_path.to_string_lossy()
        )
    })?;
    std::fs::write(simfile_path, rewritten)
        .map_err(|e| format!("Failed to write simfile '{}': {e}", simfile_path.display()))?;
    Ok(changed_tags)
}

pub fn save_song_offset_changes<W, A>(
    changes: &[SongOffsetSyncChange],
    mut is_writable: W,
    mut after_save: A,
) -> Result<SongOffsetSaveSummary, String>
where
    W: FnMut(&Path) -> bool,
    A: FnMut(&Path) -> Result<(), String>,
{
    let mut summary = SongOffsetSaveSummary::default();

    for change in changes {
        if change.delta_seconds.abs() < 0.000_001_f32 {
            continue;
        }
        let path = change.simfile_path.as_path();
        if !is_writable(path) {
            summary.skipped_read_only = summary.skipped_read_only.saturating_add(1);
            if summary.first_skipped_path.is_none() {
                summary.first_skipped_path = Some(path.to_path_buf());
            }
            continue;
        }

        summary.changed_tags_total +=
            save_song_offset_delta_to_simfile(path, change.delta_seconds)?;
        after_save(path)?;
        summary.saved_files = summary.saved_files.saturating_add(1);
        if summary.first_saved_path.is_none() {
            summary.first_saved_path = Some(path.to_path_buf());
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn sync_offset_change_uses_millisecond_quantization() {
        assert_eq!(sync_offset_delta_seconds(-0.0421, -0.0424), None);
        assert_eq!(sync_change_line("Global Offset", -0.0421, -0.0424), None);

        let delta = sync_offset_delta_seconds(-0.0424, -0.0426).expect("changed by one ms");
        assert!((delta + 0.001).abs() < f32::EPSILON);
        assert_eq!(sync_offset_target_seconds(-0.0424, -0.0426), Some(-0.043));
        assert_eq!(
            sync_change_line("Global Offset", -0.0424, -0.0426).as_deref(),
            Some("Global Offset from -0.042 to -0.043 (notes later)")
        );
    }

    #[test]
    fn gameplay_sync_offset_saveable_change_ignores_readonly_song_only_edits() {
        assert!(!gameplay_sync_offset_saveable_changed(
            0.0, 0.0, -0.042, -0.043, false,
        ));
        assert!(gameplay_sync_offset_saveable_changed(
            0.0, 0.001, -0.042, -0.043, false,
        ));
        assert!(gameplay_sync_offset_saveable_changed(
            0.0, 0.0, -0.042, -0.043, true,
        ));
    }

    #[test]
    fn gameplay_sync_prompt_text_reports_writable_changes() {
        let text = gameplay_sync_prompt_text(GameplaySyncPromptText {
            song_title: "Test Song",
            song_writable: true,
            initial_global_offset_seconds: 0.0,
            global_offset_seconds: 0.001,
            initial_song_offset_seconds: -0.042,
            song_offset_seconds: -0.043,
        });

        assert!(text.contains("Global Offset from +0.000 to +0.001"));
        assert!(text.contains("You have changed the timing of\nTest Song"));
        assert!(text.contains("Song offset from -0.042 to -0.043"));
        assert!(text.contains("Would you like to save these changes?"));
    }

    #[test]
    fn gameplay_sync_prompt_text_marks_readonly_song_changes_discarded() {
        let text = gameplay_sync_prompt_text(GameplaySyncPromptText {
            song_title: "Read Only Song",
            song_writable: false,
            initial_global_offset_seconds: 0.0,
            global_offset_seconds: 0.0,
            initial_song_offset_seconds: -0.042,
            song_offset_seconds: -0.043,
        });

        assert!(text.contains("Song offset changes for\nRead Only Song"));
        assert!(text.contains("will be discarded because the song folder is read-only"));
    }

    #[test]
    fn rewrite_simfile_offset_tags_updates_all_tags() {
        let input = b"#TITLE:test;\n#OFFSET: -0.0424 ;\n#NOTES:\n#offset:0.100;\n";
        let (out, changed) = rewrite_simfile_offset_tags(input, -0.001).expect("rewrite");

        assert_eq!(changed, 2);
        let out = std::str::from_utf8(&out).expect("utf8");
        assert!(out.contains("#OFFSET: -0.043 ;"));
        assert!(out.contains("#offset:0.099;"));
    }

    #[test]
    fn rewrite_simfile_offset_tags_rejects_missing_terminator() {
        let err = rewrite_simfile_offset_tags(b"#OFFSET:0.000", 0.001).expect_err("error");
        assert!(err.contains("missing ';'"));
    }

    #[test]
    fn save_song_offset_changes_tracks_skips_and_writes() {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "deadsync_sync_offset_test_{}_{}",
            std::process::id(),
            id
        ));
        std::fs::create_dir_all(&root).expect("create temp dir");
        let writable = root.join("writable.ssc");
        let read_only = root.join("read_only.ssc");
        std::fs::write(&writable, b"#TITLE:test;\n#OFFSET:0.100;\n").expect("write simfile");
        std::fs::write(&read_only, b"#TITLE:test;\n#OFFSET:0.200;\n").expect("write simfile");

        let changes = vec![
            SongOffsetSyncChange {
                simfile_path: writable.clone(),
                delta_seconds: 0.001,
            },
            SongOffsetSyncChange {
                simfile_path: read_only.clone(),
                delta_seconds: 0.001,
            },
            SongOffsetSyncChange {
                simfile_path: writable.clone(),
                delta_seconds: 0.000_000_1,
            },
        ];
        let mut reloaded = Vec::new();
        let summary = save_song_offset_changes(
            &changes,
            |path| path != read_only.as_path(),
            |path| {
                reloaded.push(path.to_path_buf());
                Ok(())
            },
        )
        .expect("save changes");

        assert_eq!(summary.saved_files, 1);
        assert_eq!(summary.skipped_read_only, 1);
        assert_eq!(summary.changed_tags_total, 1);
        assert_eq!(
            summary.first_saved_path.as_deref(),
            Some(writable.as_path())
        );
        assert_eq!(
            summary.first_skipped_path.as_deref(),
            Some(read_only.as_path())
        );
        assert_eq!(reloaded, vec![writable.clone()]);
        let rewritten = std::fs::read_to_string(&writable).expect("read rewritten");
        assert!(rewritten.contains("#OFFSET:0.101;"));

        let _ = std::fs::remove_file(root.join("writable.ssc.old"));
        let _ = std::fs::remove_file(writable);
        let _ = std::fs::remove_file(read_only);
        let _ = std::fs::remove_dir(root);
    }
}
