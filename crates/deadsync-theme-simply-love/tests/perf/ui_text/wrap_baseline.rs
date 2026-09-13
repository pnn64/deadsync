// Frozen wrapping body from fcb00c347 (0.5.1201).

pub(super) fn wrap_text_with_measure(
    raw_text: &str,
    max_width_px: f32,
    measure: impl Fn(&str) -> f32,
) -> String {
    let mut out = String::new();
    let mut is_first_output_line = true;

    // Break a single "word" that doesn't fit on its own line into
    // smaller chunks that each fit within max_width_px. Without
    // this, long unbreakable strings (e.g. filesystem paths with
    // no spaces) overflow the description box.
    let break_long_word = |word: &str| -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for ch in word.chars() {
            let mut candidate = current.clone();
            candidate.push(ch);
            if !current.is_empty() && measure(&candidate) > max_width_px {
                chunks.push(std::mem::take(&mut current));
                current.push(ch);
            } else {
                current = candidate;
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        if chunks.is_empty() {
            chunks.push(word.to_owned());
        }
        chunks
    };

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

        let mut current_line = String::new();
        for word in trimmed.split_whitespace() {
            let candidate = if current_line.is_empty() {
                word.to_owned()
            } else {
                let mut tmp = current_line.clone();
                tmp.push(' ');
                tmp.push_str(word);
                tmp
            };

            if !current_line.is_empty() && measure(&candidate) > max_width_px {
                emit_line(&current_line, &mut is_first_output_line, &mut out);
                current_line.clear();
                current_line.push_str(word);
            } else {
                current_line = candidate;
            }

            // If the word alone still doesn't fit, hard-break it
            // by character so it never overflows the box.
            if measure(&current_line) > max_width_px {
                let pieces = break_long_word(&current_line);
                current_line.clear();
                if let Some((last, leading)) = pieces.split_last() {
                    for piece in leading {
                        emit_line(piece, &mut is_first_output_line, &mut out);
                    }
                    current_line.push_str(last);
                }
            }
        }

        if !current_line.is_empty() {
            emit_line(&current_line, &mut is_first_output_line, &mut out);
        }
    }

    if out.is_empty() {
        raw_text.to_string()
    } else {
        out
    }
}
