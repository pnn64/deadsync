// Frozen from 374c24c4c3c41631d3a8e50962c7fec3ed94bad3 for paired regression/benchmark checks.
use super::*;

pub(super) fn push_fact_table(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    left_w: f32,
) {
    let accent = accent(state);
    let table_w = left_w - 2.0 * (lo::DET_TAB_X - lo::LIST_X);
    let facts = facts(state, pack);

    for (row, (label, value)) in facts.iter().enumerate() {
        if row.is_multiple_of(2) {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(lo::DET_TAB_X - 6.0, lo::DET_TAB_Y + row as f32 * lo::DET_TAB_H):
                zoomto(table_w, lo::DET_TAB_H):
                diffuse(1.0, 1.0, 1.0, 0.035): z(Z_ROW_BG)
            ));
        }
        let y = lo::det_tab_y(row);
        actors.push(act!(text:
            font("miso"): settext((*label).to_owned()):
            align(0.0, 0.5): xy(lo::DET_TAB_X, y): zoom(0.46): horizalign(left):
            maxwidth(lo::DET_TAB_VX - lo::DET_TAB_X - 6.0):
            diffuse(0.52, 0.52, 0.52, 1.0): z(Z_TEXT)
        ));

        // A fact that is still coming says so where its value will land,
        // rather than showing an empty row that reads as "none".
        if value.is_empty() && pending(state, pack) {
            actors.push(spinner::accent_actor(
                lo::DET_TAB_VX + 8.0,
                y,
                15.0,
                Z_TEXT,
                accent,
            ));
            continue;
        }
        actors.push(act!(text:
            font("miso"): settext(value.clone()):
            align(0.0, 0.5): xy(lo::DET_TAB_VX, y): zoom(0.5): horizalign(left):
            maxwidth(table_w - (lo::DET_TAB_VX - lo::DET_TAB_X) - 4.0):
            diffuse(0.86, 0.86, 0.86, 1.0): z(Z_TEXT)
        ));
    }
}

fn facts(state: &State, pack: &PackInfo) -> [(&'static str, String); 8] {
    let details = details_for(state, pack);
    let page = page_for(state, pack);
    let lost = state.page.pack_id == pack.id && state.page.phase == PagePhase::Error;
    let unknown = || if lost { "--".to_owned() } else { String::new() };

    let charts = page
        .and_then(|p| p.chart_count.clone())
        .unwrap_or_else(unknown);
    let difficulty =
        page.and_then(PackPage::difficulty_span)
            .map_or_else(unknown, |(low, high)| {
                if low == high {
                    low.to_string()
                } else {
                    format!("{low} - {high}")
                }
            });
    // The pack page knows this for certain, per song. Everything else is a
    // guess from the catalogue's one coarse column, which is what made a pack
    // with a whole doubles set in it report "Singles".
    let style = match page.and_then(PackPage::style_label) {
        Some(label) => label.to_owned(),
        None => match pack.pack_type.as_deref().filter(|v| meaningful(v)) {
            Some(kind) if kind.eq_ignore_ascii_case("keyboard") => "Keyboard".to_owned(),
            _ if super::super::state::is_dedicated_doubles(state, pack) => "Doubles".to_owned(),
            _ => unknown(),
        },
    };
    // What the site says, and nothing more.
    //
    // This row used to claim "the engine corrects ITG sync", which is not
    // true: `MachinePackIniOffsets` is off by default and `DefaultSyncOffset`
    // is Null, so a stock install applies no pack correction at all. Saying
    // otherwise here told a player their old ITG-synced packs were handled
    // when they were about to play nine milliseconds early.
    let sync = smo_sync_of(pack).short().to_owned();
    let added = details
        .and_then(|d| d.date_added.as_deref())
        .map_or_else(unknown, |date| format_date(date, false));
    let authors = match page {
        Some(page) if !page.authors.is_empty() => {
            let shown = page.authors.len().min(2);
            let mut names = page.authors[..shown].join(", ");
            if page.authors.len() > shown {
                names.push_str(" and more");
            }
            names
        }
        _ => unknown(),
    };

    [
        ("Songs", commify(pack.song_count as usize)),
        ("Charts", charts),
        ("Size", format_bytes(pack.size_bytes)),
        ("Difficulty", difficulty),
        ("Style", style),
        ("Sync", sync),
        ("Added", added),
        ("Charts by", authors),
    ]
}
