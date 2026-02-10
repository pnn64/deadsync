use crate::act;
use crate::core::space::{is_wide, screen_height, screen_width, widescale};
use crate::ui::actors::Actor;

#[derive(Clone, Copy)]
pub struct PaneLayout {
    pub pane_top: f32,
    pub pane_width: f32,
    pub pane_height: f32,
    pub text_zoom: f32,
    pub cols: [f32; 4],
    pub rows: [f32; 3],
}

#[derive(Clone, Copy)]
pub struct StatsValues<'a> {
    pub steps: &'a str,
    pub mines: &'a str,
    pub jumps: &'a str,
    pub hands: &'a str,
    pub holds: &'a str,
    pub rolls: &'a str,
}

pub struct StatsPaneParams<'a> {
    pub pane_cx: f32,
    pub accent_color: [f32; 4],
    pub values: StatsValues<'a>,
    pub meter: Option<&'a str>,
}

#[inline(always)]
pub fn layout() -> PaneLayout {
    PaneLayout {
        pane_top: screen_height() - 92.0,
        pane_width: screen_width() / 2.0 - 10.0,
        pane_height: 60.0,
        text_zoom: widescale(0.8, 0.9),
        cols: [
            widescale(-104.0, -133.0),
            widescale(-36.0, -38.0),
            widescale(54.0, 76.0),
            widescale(150.0, 190.0),
        ],
        rows: [13.0, 31.0, 49.0],
    }
}

pub fn build_base(p: StatsPaneParams<'_>) -> Vec<Actor> {
    let l = layout();
    let mut out = Vec::with_capacity(16);
    out.push(act!(quad:
        align(0.5, 0.0):
        xy(p.pane_cx, l.pane_top):
        setsize(l.pane_width, l.pane_height):
        z(120):
        diffuse(p.accent_color[0], p.accent_color[1], p.accent_color[2], 1.0)
    ));

    let stats = [
        ("Steps", p.values.steps),
        ("Mines", p.values.mines),
        ("Jumps", p.values.jumps),
        ("Hands", p.values.hands),
        ("Holds", p.values.holds),
        ("Rolls", p.values.rolls),
    ];
    for (i, (label, value)) in stats.iter().enumerate() {
        let (c, r) = (i % 2, i / 2);
        out.push(act!(text:
            font("miso"):
            settext(*value):
            align(1.0, 0.5):
            horizalign(right):
            xy(p.pane_cx + l.cols[c], l.pane_top + l.rows[r]):
            zoom(l.text_zoom):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        ));
        out.push(act!(text:
            font("miso"):
            settext(*label):
            align(0.0, 0.5):
            xy(p.pane_cx + l.cols[c] + 3.0, l.pane_top + l.rows[r]):
            zoom(l.text_zoom):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        ));
    }

    if let Some(meter) = p.meter {
        let mut meter_actor = act!(text:
            font("wendy"):
            settext(meter):
            align(1.0, 0.5):
            horizalign(right):
            xy(p.pane_cx + l.cols[3], l.pane_top + l.rows[1]):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        );
        if !is_wide()
            && let Actor::Text { max_width, .. } = &mut meter_actor
        {
            *max_width = Some(66.0);
        }
        out.push(meter_actor);
    }
    out
}
