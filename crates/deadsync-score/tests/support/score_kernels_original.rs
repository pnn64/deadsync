// Frozen production functions from 57cdd5ec4 for behavioral comparison.
use deadsync_score::{GsCommentCounts, ghost::GhostData};

pub fn update_best(data: &mut GhostData, histories: &[Vec<f64>; 3]) -> bool {
    let mut changed = false;
    for (old, new) in [&mut data.itg, &mut data.ex, &mut data.hard_ex]
        .into_iter()
        .zip(histories)
    {
        if !new.is_empty()
            && new.iter().all(|point| point.is_finite())
            && (old.len() != new.len() || old.last() < new.last())
        {
            old.clone_from(new);
            changed = true;
        }
    }
    changed
}

pub fn parse_itl_points(chart_name: &str) -> Option<(u32, u32)> {
    let bytes = chart_name.as_bytes();
    let mut offset = 0;
    let mut values = [0_u32; 2];
    let mut found = 0;

    while offset < bytes.len() && found < values.len() {
        while offset < bytes.len() && !bytes[offset].is_ascii_digit() {
            offset += 1;
        }
        let mut value = (offset < bytes.len()).then_some(0_u32);
        while offset < bytes.len() && bytes[offset].is_ascii_digit() {
            value = value
                .and_then(|value| value.checked_mul(10))
                .and_then(|value| value.checked_add(u32::from(bytes[offset] - b'0')));
            offset += 1;
        }
        if let Some(value) = value {
            values[found] = value;
            found += 1;
        }
    }

    (found == values.len()).then_some((values[0], values[1]))
}

pub fn parse_gs_comment_counts(comment: &str) -> GsCommentCounts {
    let mut counts = GsCommentCounts::default();
    for part in comment.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }

        let mut value: u32 = 0;
        let mut idx = 0usize;
        for (i, ch) in s.char_indices() {
            if let Some(d) = ch.to_digit(10) {
                value = value.saturating_mul(10).saturating_add(d);
                idx = i + ch.len_utf8();
            } else {
                break;
            }
        }
        if value == 0 {
            continue;
        }

        match s[idx..].trim().as_bytes() {
            [b'w' | b'W'] => counts.w = value,
            [b'e' | b'E'] => counts.e = value,
            [b'g' | b'G'] => counts.g = value,
            [b'd' | b'D'] => counts.d = value,
            [b'w' | b'W', b'o' | b'O'] => counts.wo = value,
            [b'm' | b'M'] => counts.m = value,
            _ => {}
        }
    }
    counts
}
