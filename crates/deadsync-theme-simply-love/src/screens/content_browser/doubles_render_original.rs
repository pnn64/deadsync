// Frozen from 374c24c4c3c41631d3a8e50962c7fec3ed94bad3 for paired regression/benchmark checks.
use super::*;

pub(super) fn push_doubles(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let width = lo::dbl_w();
    let top = lo::dbl_top();
    let in_rows = state.zone == Zone::DoublesRows;
    let here = doubles_cursor(state);

    for column in 0..2usize {
        let x = lo::dbl_x(column);
        let list: Vec<usize> = doubles_column(state, column).to_vec();
        let picked = column == state.doubles_column;

        // The picker's focus is the whole column lit as one thing, so "you are
        // choosing between two lists" is something the screen says rather than
        // something the reader has to deduce from a cursor.
        if picked && !in_rows {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x - 4.0, lo::LIST_TOP_TIGHT):
                zoomto(width + 8.0, lo::DBL_HEAD + lo::DBL_ROWS as f32 * lo::ROW_H):
                diffuse(accent[0], accent[1], accent[2], 0.10): z(Z_PANEL)
            ));
        }

        let head = if column == 0 {
            "MADE FOR DOUBLES"
        } else {
            "SOME DOUBLES"
        };
        let heading = if list.is_empty() {
            head.to_owned()
        } else {
            format!("{head}   {}", list.len())
        };
        let head_rgba = if picked { accent } else { [0.6, 0.6, 0.6, 1.0] };
        actors.push(act!(text:
            font("miso"): settext(heading):
            align(0.0, 0.5): xy(x, lo::LIST_TOP_TIGHT + 7.0): zoom(0.5): horizalign(left):
            maxwidth(width - 8.0):
            diffuse(head_rgba[0], head_rgba[1], head_rgba[2], head_rgba[3]): z(Z_TEXT)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::LIST_TOP_TIGHT + 15.0): zoomto(width, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.13): z(Z_TEXT)
        ));

        // Each column is still coming on its own terms: the curated list is
        // complete the moment itgdb answers, while the candidates are not done
        // until the details walk is. One spinner for the whole tab left a
        // finished column looking busy.
        let building = if column == 0 {
            state.itgdb.phase == ItgdbPhase::Loading || state.doubles_left_waiting
        } else {
            state.details.phase == DetailsPhase::Loading
        };

        for slot in 0..lo::DBL_ROWS {
            let y = top + slot as f32 * lo::ROW_H;
            let at = state.doubles_window[column] + slot;
            let Some(index) = list.get(at) else {
                if building {
                    push_skeleton_row(actors, x, y, width, lo::DBL_ART_CX, accent, true);
                }
                continue;
            };
            let Some(pack) = state.snapshot.catalog.get(*index) else {
                continue;
            };
            push_row(
                actors,
                state,
                pack,
                &RowPlacement {
                    x,
                    y,
                    width,
                    art_cx: lo::DBL_ART_CX,
                    art_w: lo::DBL_ART_W,
                    text_x: lo::DBL_TEXT_X,
                    selected: picked && at == here,
                    focused: in_rows,
                    badges: false,
                },
                accent,
            );
        }

        // What a column says when it has nothing in it and nothing coming.
        if list.is_empty() && !building {
            let message = if column == 0 && state.itgdb.phase == ItgdbPhase::Error {
                "itgdb.net could not be reached, so this list is unavailable."
            } else {
                "Nothing here."
            };
            actors.push(act!(text:
                font("miso"): settext(message.to_owned()):
                align(0.0, 0.5): xy(x + 4.0, top + 14.0): zoom(0.5): horizalign(left):
                maxwidth(width - 12.0):
                diffuse(0.55, 0.55, 0.55, 1.0): z(Z_TEXT)
            ));
        }

        // One scrollbar per column, 5px off its right edge.
        if list.len() > lo::DBL_ROWS {
            let track_h = lo::DBL_ROWS as f32 * lo::ROW_H - 8.0;
            let frac = lo::DBL_ROWS as f32 / list.len() as f32;
            let thumb_h = (track_h * frac).max(14.0);
            let span = (list.len() - lo::DBL_ROWS) as f32;
            let progress = (state.doubles_window[column] as f32 / span).clamp(0.0, 1.0);
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x + width + 5.0, top): zoomto(lo::SCROLL_W, track_h):
                diffuse(1.0, 1.0, 1.0, 0.10): z(Z_PANEL)
            ));
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x + width + 5.0, top + (track_h - thumb_h) * progress):
                zoomto(lo::SCROLL_W, thumb_h):
                diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
            ));
        }
    }
}
