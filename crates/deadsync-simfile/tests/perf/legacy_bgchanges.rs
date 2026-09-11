// Frozen from ec9914795 (0.5.1134) for behavior and old/new benchmarks.
#![allow(dead_code)]

pub fn split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    split_bgchange_sets(changes, entries)
}

fn split_bgchange_sets(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    if changes.is_empty() {
        return Vec::new();
    }
    let mut content_start = 0;
    skip_newline_sequence(changes.as_bytes(), &mut content_start);
    if content_start == changes.len() {
        return Vec::new();
    }
    let set_capacity = changes
        .as_bytes()
        .iter()
        .filter(|&&byte| byte == b',')
        .count()
        .saturating_add(1);
    let mut out = Vec::with_capacity(set_capacity);
    let mut start = 0usize;
    let mut pnum = 0u8;
    while start <= changes.len() {
        if matches!(pnum, 1 | 7)
            && let Some(end) = match_bgchange_entry_end(changes, start, entries)
        {
            push_bgchange_field_range(out.last_mut().unwrap(), changes, start, end);
            start = end;
            if let Some(&delim) = changes.as_bytes().get(start) {
                pnum = if delim == b'=' { pnum + 1 } else { 0 };
                start += 1;
            }
            continue;
        }
        if pnum == 0 {
            out.push(Vec::with_capacity(4));
        }
        let Some((end, delim)) = next_bgchange_delimiter(changes, start) else {
            push_bgchange_field_range(out.last_mut().unwrap(), changes, start, changes.len());
            break;
        };
        push_bgchange_field_range(out.last_mut().unwrap(), changes, start, end);
        start = end + 1;
        pnum = if delim == b'=' { pnum + 1 } else { 0 };
    }
    out
}

#[inline]
fn next_bgchange_delimiter(changes: &str, start: usize) -> Option<(usize, u8)> {
    changes.as_bytes()[start..]
        .iter()
        .position(|byte| matches!(byte, b'=' | b','))
        .map(|offset| {
            let index = start + offset;
            (index, changes.as_bytes()[index])
        })
}

fn push_bgchange_field_range(fields: &mut Vec<String>, changes: &str, start: usize, end: usize) {
    if fields.len() == 4 && fields.capacity() == 4 {
        fields.reserve_exact(7);
    }
    let field = &changes[start..end];
    fields.push(if field.as_bytes().contains(&b'\n') {
        strip_newlines_owned(field)
    } else {
        field.to_string()
    });
}

fn match_bgchange_entry_end(changes: &str, start: usize, entries: &[String]) -> Option<usize> {
    entries
        .iter()
        .find_map(|entry| match_bgchange_entry_end_one(changes, start, entry))
}

fn match_bgchange_entry_end_one(changes: &str, start: usize, entry: &str) -> Option<usize> {
    let input = changes.as_bytes();
    let mut input_index = start;
    for &expected in entry.as_bytes() {
        skip_newline_sequence(input, &mut input_index);
        let actual = *input.get(input_index)?;
        if !actual.eq_ignore_ascii_case(&expected) {
            return None;
        }
        input_index += 1;
    }
    skip_newline_sequence(input, &mut input_index);
    matches!(input.get(input_index), None | Some(b'=') | Some(b',')).then_some(input_index)
}

#[inline]
fn skip_newline_sequence(input: &[u8], index: &mut usize) {
    loop {
        match input.get(*index..) {
            Some([b'\r', b'\n', ..]) => *index += 2,
            Some([b'\n', ..]) => *index += 1,
            _ => break,
        }
    }
}

fn strip_newlines_owned(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(line);
    }
    out
}
