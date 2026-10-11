// Frozen from 8711fc76304beb7ec26866e1a7a99e5647469e64; benchmark and regression reference only.
use super::*;

pub(super) fn push_tabs(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let focused_zone = state.zone == Zone::Tabs;

    for (index, entry) in TABS.iter().enumerate() {
        let x = lo::tab_x(index);
        let selected = index == state.tab_index;

        // Three states, exactly as the original: focus (solid accent plate,
        // near-black ink), shown (accent plate at 0.26, accent ink), idle
        // (white 0.05 plate, grey label).
        // The original gives the icon its own grey, a shade darker than the
        // label's, in the idle state only.
        let (plate, ink, icon_ink) = if selected && focused_zone {
            (
                [accent[0], accent[1], accent[2], 1.0],
                TAB_FOCUS_INK,
                TAB_FOCUS_INK,
            )
        } else if selected {
            let lit = [accent[0], accent[1], accent[2], 1.0];
            ([accent[0], accent[1], accent[2], 0.26], lit, lit)
        } else {
            ([1.0, 1.0, 1.0, 0.05], TAB_IDLE_LABEL, TAB_IDLE_ICON)
        };

        actors.push(act!(quad:
            align(0.0, 0.5): xy(x - 3.0, lo::TABS_Y): zoomto(lo::TAB_W, lo::TAB_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_PANEL)
        ));
        // The icon takes the ink colour too, so a focused pill reads as one
        // object rather than as a bright label beside a grey glyph.
        actors.push(act!(sprite(entry.icon()):
            align(0.5, 0.5): xy(x + lo::TAB_ICON_INSET, lo::TABS_Y):
            setsize(lo::TAB_ICON_PX, lo::TAB_ICON_PX):
            diffuse(icon_ink[0], icon_ink[1], icon_ink[2], icon_ink[3]): z(Z_TEXT)
        ));
        actors.push(act!(text:
            font("miso"): settext(entry.label().to_owned()):
            align(0.0, 0.5): xy(x + lo::TAB_LABEL_INSET, lo::TABS_Y): zoom(lo::TAB_LABEL_ZOOM):
            maxwidth(lo::TAB_W - lo::TAB_LABEL_INSET - 4.0): horizalign(left):
            diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_TEXT)
        ));
    }

    // The strip's own spinner, at the far right, turning whenever a service
    // this view depends on is still working. Never in the year view: that one
    // has a spinner of its own beside the year being indexed, and two wheels
    // for one job is one too many.
    if services_busy(state) && !years_showing(state) {
        actors.push(spinner::accent_actor(
            lo::width() - lo::LIST_X - 12.0,
            lo::TABS_Y,
            20.0,
            Z_TEXT,
            accent,
        ));
    }
}

pub(super) fn push_context_band(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let accent = accent(state);
    let Some((title, blurb)) = band(state) else {
        return;
    };
    actors.push(act!(quad:
        align(0.0, 0.0): xy(lo::LIST_X, lo::BAND_Y): zoomto(w - 2.0 * lo::LIST_X, lo::BAND_H):
        diffuse(1.0, 1.0, 1.0, 0.05): z(Z_PANEL)
    ));
    actors.push(act!(text:
        font("wendy"): settext(title.to_owned()):
        align(0.0, 0.5): xy(lo::BAND_TITLE_X, lo::BAND_TITLE_Y): zoom(0.42): horizalign(left):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    // A search quotes the query; every other view states its rule and then
    // says how many packs matched it. Two spaces before the count, as the
    // original's format string has.
    let busy = band_busy(state) || search_running(state);
    let subtitle = if state.query.is_empty() {
        if busy && tab(state) == Tab::Beginner {
            // The count matters here: the walk reads about four packs for
            // every one it keeps, so "checking..." alone reads as stuck.
            format!(
                "{blurb}  Checking pack {} of {}{}",
                state.beginner.checked.max(1),
                state.beginner.candidates.max(1),
                spinner::ellipsis()
            )
        } else if busy {
            format!("{blurb}  Checking packs{}", spinner::ellipsis())
        } else {
            format!("{blurb}  {} found.", band_count(state))
        }
    } else {
        format!("{:?} {blurb}", state.query)
    };
    actors.push(act!(text:
        font("miso"): settext(subtitle):
        align(0.0, 0.5): xy(lo::BAND_TITLE_X, lo::BAND_SUB_Y): zoom(0.55): horizalign(left):
        maxwidth(w - 2.0 * lo::LIST_X - 120.0):
        diffuse(0.78, 0.78, 0.78, 1.0): z(Z_TEXT)
    ));

    if busy {
        actors.push(spinner::accent_actor(
            w - lo::LIST_X - 34.0,
            lo::BAND_Y + lo::BAND_H * 0.5,
            32.0,
            Z_TEXT,
            accent,
        ));
    }
}

pub(super) fn push_footer(actors: &mut Vec<Actor>, state: &State, w: f32) {
    actors.push(act!(text:
        font("miso"): settext(footer_hint(state)):
        align(0.5, 0.5): xy(w * 0.5, lo::VERSION_Y): zoom(0.5): horizalign(center):
        maxwidth(w - 160.0):
        diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]): z(Z_TEXT)
    ));
    // Centred, under the hint line.
    //
    // Both bottom corners belong to the engine's version watermark, and which
    // corner it takes is a setting -- so a credit anchored to either one
    // overlaps it for half the people who see it.
    actors.push(act!(text:
        font("miso"): settext("pack data from stepmaniaonline.net".to_owned()):
        align(0.5, 0.5): xy(w * 0.5, lo::VERSION_Y + 14.0): zoom(lo::VERSION_ZOOM):
        horizalign(center):
        diffuse(0.55, 0.55, 0.55, 0.75): z(Z_TEXT)
    ));
}

pub(super) fn commify(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub(super) fn readout_text(state: &State) -> String {
    match tab(state) {
        Tab::Installed => format!("{} installed", state.installed.len()),
        // Never a status here. The year view says it is indexing with the
        // progress bar beside this line, and a second indicator that flips
        // between "indexing..." and a count reads as a glitch rather than as
        // progress.
        Tab::Years => format!("{} packs", commify(state.results.len())),
        // The doubles view is not paged and has two counts, so it reports
        // neither here; its columns say how long they are themselves.
        Tab::Doubles if state.query.is_empty() => String::new(),
        // Nor the beginner list: it stops after a helping and waits to be
        // asked, so it has no page count worth reporting.
        Tab::Beginner if state.query.is_empty() => String::new(),
        _ if !state.query.is_empty() => {
            // The "+" is the only thing on screen that says more matched than
            // the search will show.
            let more = if search_capped(state) { "+" } else { "" };
            let still = if search_running(state) {
                spinner::ellipsis()
            } else {
                ""
            };
            format!(
                "{}{more} results for {:?}{still}",
                commify(state.results.len()),
                state.query
            )
        }
        _ if state.snapshot.phase == CatalogPhase::Loading => {
            format!("loading{}", spinner::ellipsis())
        }
        // Nor a count over skeleton rows, which is a count of what is not
        // on screen yet.
        _ if state.results.is_empty() || awaiting_order(state) => String::new(),
        _ => format!(
            "Page {}/{}   -   {} packs",
            list_page(state) + 1,
            total_pages(state),
            commify(state.results.len())
        ),
    }
}

pub(super) fn push_readout(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let text = readout_text(state);
    if text.is_empty() {
        return;
    }
    actors.push(act!(text:
        font("miso"): settext(text):
        align(1.0, 0.5): xy(w - lo::LIST_X - 16.0, lo::FEAT_LABEL_Y): zoom(0.5):
        horizalign(right):
        diffuse(READOUT_RGBA[0], READOUT_RGBA[1], READOUT_RGBA[2], READOUT_RGBA[3]): z(Z_TEXT)
    ));
}
