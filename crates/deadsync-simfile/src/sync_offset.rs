use std::ffi::OsString;
use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
