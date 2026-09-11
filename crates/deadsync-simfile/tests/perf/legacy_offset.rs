// Frozen from ec9914795 (0.5.1134) for behavior and old/new benchmarks.
#![allow(dead_code)]

#[inline(always)]
pub fn quantize_sync_offset_seconds(v: f32) -> f32 {
    (v / 0.001_f32).round() * 0.001_f32
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
