// Keep one line buffer and measure borrowed slices when hard-breaking a word.
// Measurements retain the font's integer advances and the caller's zoom.
pub(super) fn wrap_text_with_measure(
    raw_text: &str,
    max_width_px: f32,
    measure: impl Fn(&str) -> f32,
) -> String {
    let mut out = String::with_capacity(raw_text.len());
    let mut current_line = String::new();
    let mut is_first_output_line = true;
    let emit_line = |line: &str, is_first: &mut bool, out: &mut String| {
        if !*is_first {
            out.push('\n');
        }
        out.push_str(line);
        *is_first = false;
    };
    for segment in raw_text.split('\n') {
        let trimmed = segment.trim_end();
        if trimmed.is_empty() {
            if !is_first_output_line {
                out.push('\n');
            }
            continue;
        }
        current_line.clear();
        for word in trimmed.split_whitespace() {
            let previous_len = current_line.len();
            if previous_len != 0 {
                current_line.push(' ');
            }
            current_line.push_str(word);
            if previous_len != 0 && measure(&current_line) > max_width_px {
                emit_line(
                    &current_line[..previous_len],
                    &mut is_first_output_line,
                    &mut out,
                );
                current_line.clear();
                current_line.push_str(word);
            }
            if measure(&current_line) > max_width_px {
                let mut start = 0;
                for (end, ch) in current_line.char_indices() {
                    if end > start
                        && measure(&current_line[start..end + ch.len_utf8()]) > max_width_px
                    {
                        emit_line(
                            &current_line[start..end],
                            &mut is_first_output_line,
                            &mut out,
                        );
                        start = end;
                    }
                }
                current_line.drain(..start);
            }
        }
        if !current_line.is_empty() {
            emit_line(&current_line, &mut is_first_output_line, &mut out);
        }
    }
    if out.is_empty() {
        out.push_str(raw_text);
    }
    out
}
