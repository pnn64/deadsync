use super::super::*;

pub fn normalized_background_lua_color(field: Option<&str>) -> String {
    let Some(color) = field.and_then(parse_background_lua_color) else {
        return String::new();
    };
    let [r, g, b, a] =
        color.map(|component| (component * 255.0).round_ties_even().clamp(0.0, 255.0) as u8);
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut digits = [0; 8];
    for (pair, byte) in digits.as_chunks_mut::<2>().0.iter_mut().zip([r, g, b, a]) {
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 15)];
    }
    let len = if a == 255 { 6 } else { 8 };
    let mut output = String::new();
    output.push('#');
    output.push_str(std::str::from_utf8(&digits[..len]).expect("hex digits are ASCII"));
    output
}

pub fn resolve_background_changes_from_values<'a>(
    song_dir: &Path,
    values: impl IntoIterator<Item = &'a [u8]>,
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
    bg_animation_roots: &[PathBuf],
    allow_bg_animation: bool,
    apply_song_movie_fallback: bool,
) -> Vec<SongBackgroundChange> {
    let entries = list_song_dir_rel_entries(song_dir);
    let mut out: Vec<SongBackgroundChange> = Vec::new();
    let mut saw_no_song_bg = false;
    for raw in values {
        let decoded = decode_bytes(raw);
        let text = unescape_tag(decoded.as_ref());
        for fields in bgchange_sets(text.as_ref(), &entries) {
            let Some(change) = parse_background_change_set(
                song_dir,
                &fields,
                song_movie_roots,
                random_movie_roots,
                bg_animation_roots,
                allow_bg_animation,
            ) else {
                continue;
            };
            if matches!(change.target, SongBackgroundChangeTarget::NoSongBg) {
                saw_no_song_bg = true;
                continue;
            }
            upsert_background_change(&mut out, change);
        }
    }

    if !apply_song_movie_fallback {
        out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
        return out;
    }

    let BgchangeFallbackSummary {
        has_explicit_movie,
        beat_zero_still_ix,
        blocks_beat_zero,
        has_any_file,
    } = summarize_bgchange_fallbacks(&out);
    let mut movies = list_bgchange_song_movies(song_dir);
    if movies.len() == 1 && !has_explicit_movie {
        let movie = movies.pop().unwrap();
        if saw_no_song_bg {
            if let Some(ix) = beat_zero_still_ix {
                out[ix].target = SongBackgroundChangeTarget::File(movie);
            } else if !blocks_beat_zero {
                out.push(SongBackgroundChange::new(
                    0.0,
                    SongBackgroundChangeTarget::File(movie),
                ));
            }
        } else if !has_any_file && !blocks_beat_zero {
            out.push(SongBackgroundChange::new(
                0.0,
                SongBackgroundChangeTarget::File(movie),
            ));
        }
    }
    out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
    out
}

pub fn upsert_background_change(out: &mut Vec<SongBackgroundChange>, change: SongBackgroundChange) {
    if let Some(slot) = out
        .iter_mut()
        .find(|existing| existing.start_beat == change.start_beat)
    {
        *slot = change;
    } else {
        out.push(change);
    }
}
