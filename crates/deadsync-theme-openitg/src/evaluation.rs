use deadlib_present::actors::{Actor, InlineU32Text, TextContent};
use deadlib_present::anim::bounceend_p;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{EvaluationOverview, EvaluationPlayerOverview, Grade};

use crate::presentation::{HEIGHT, INK, Layout, WIDTH, quad};

const STATS_START_SECONDS: f32 = 3.0;
const STATS_ENTER_SECONDS: f32 = 0.3;
pub const EVALUATION_OVERVIEW_SETTLED_SECONDS: f32 = STATS_START_SECONDS + STATS_ENTER_SECONDS;

const GRADE_ENTER_SECONDS: f32 = 0.5;
const GRADE_SETTLE_START: f32 = 2.5;
const BANNER_SIZE: [f32; 2] = [210.0, 66.0];
const LIFE_SIZE: [f32; 2] = [271.0, 39.0];
const LIFE_Y: f32 = 239.0;
const COMBO_Y: f32 = 287.0;
const COUNT_FIRST_Y: f32 = 325.0;
const COUNT_PITCH: f32 = 16.0;

const WHITE: [f32; 4] = [INK[0], INK[1], INK[2], 1.0];
const QUIET: [f32; 4] = [0.67, 0.75, 0.83, 1.0];
const RAIL: [f32; 4] = [0.31, 0.44, 0.55, 1.0];
const SKY: [f32; 4] = [0.55, 0.90, 1.0, 1.0];
const FAILED: [f32; 4] = [1.0, 0.49, 0.53, 1.0];
const P1: [f32; 4] = [1.0, 0.77, 0.25, 1.0];
const P2: [f32; 4] = [0.48, 0.94, 0.51, 1.0];
const JUDGMENTS: [(&str, [f32; 4]); 6] = [
    ("Fantastic", SKY),
    ("Excellent", P1),
    ("Great", P2),
    ("Decent", [0.70, 0.65, 1.0, 1.0]),
    ("Way Off", [1.0, 0.65, 0.37, 1.0]),
    ("Miss", FAILED),
];

#[derive(Clone, Copy)]
struct Seat {
    panel_x: f32,
    grade_hold_x: f32,
    grade_x: f32,
    percent_x: f32,
    difficulty: [f32; 2],
    outward: f32,
    tint: [f32; 4],
}

impl Seat {
    fn new(p2: bool, solo_double: bool) -> Self {
        let (panel_x, grade_hold_x, grade_x, percent_x, difficulty_x, outward, tint) = if p2 {
            (329.0, 480.0, 376.0, 504.0, 558.0, 1.0, P2)
        } else {
            (39.0, 160.0, 86.0, 220.0, 80.0, -1.0, P1)
        };
        let (shift, difficulty) = if solo_double {
            (-147.0 * outward, [difficulty_x - 75.0 * outward, 90.0])
        } else {
            (0.0, [difficulty_x, 145.0])
        };
        Self {
            panel_x: panel_x + shift,
            grade_hold_x: grade_hold_x + shift,
            grade_x: grade_x + shift,
            percent_x: percent_x + shift,
            difficulty,
            outward,
            tint,
        }
    }
}

pub fn push_evaluation_overview(actors: &mut Vec<Actor>, page: EvaluationOverview<'_, Actor>) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    let elapsed = page.elapsed.max(0.0);
    let stats_progress = decelerate(elapsed - STATS_START_SECONDS, STATS_ENTER_SECONDS);
    let solo_double = page.double_side_style && page.players.iter().flatten().count() == 1;

    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.026, 0.039, 0.055, 1.0],
        -22,
    ));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.054, 0.084, 0.12, 1.0], -21));
    actors.push(layout.panel(16.0, 64.0, [608.0, 1.0], RAIL, -20));
    actors.push(layout.panel(16.0, 456.0, [608.0, 1.0], RAIL, -20));
    push_header(actors, layout, elapsed);

    if stats_progress > 0.0 {
        push_banner(actors, layout, &page, stats_progress);
        let footer_alpha = ((elapsed - STATS_START_SECONDS) / STATS_ENTER_SECONDS).clamp(0.0, 1.0);
        actors.push(layout.label(
            [320.0, 445.0],
            0.5,
            "PRESS START TO CONTINUE".into(),
            0.88,
            [QUIET[0], QUIET[1], QUIET[2], footer_alpha],
        ));
    }
    for (slot, player) in page.players.iter().enumerate() {
        let Some(player) = player else { continue };
        let seat = Seat::new(player.p2, solo_double);
        if stats_progress > 0.0 {
            let remaining = (1.0 - stats_progress) * seat.outward;
            let distance = if solo_double { WIDTH } else { WIDTH * 0.5 };
            let body_shift = remaining * distance;
            push_difficulty(actors, layout, seat, player, remaining * (WIDTH * 0.5));
            push_player_stats(actors, layout, seat, player, body_shift);
            if let Some(graph) = (page.life_graph)(
                slot,
                layout.position(seat.panel_x + body_shift, LIFE_Y),
                layout.size(LIFE_SIZE),
                seat.tint,
                8,
            ) {
                actors.push(graph);
            }
            push_course_boundaries(actors, layout, player, seat.panel_x + body_shift);
            push_combo_runs(actors, layout, player, seat, seat.panel_x + body_shift);
        }
        push_grade(actors, layout, seat, player.grade, elapsed);
    }
}

fn push_course_boundaries(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    x: f32,
) {
    let [first, last] = player.graph_seconds;
    if last <= first {
        return;
    }
    let stages = player.course_graph_stages;
    let mut elapsed = 0.0;
    for stage in &stages[..stages.len().saturating_sub(1)] {
        elapsed += stage.song_last_second.max(0.0);
        if elapsed > first && elapsed < last {
            let center = x + LIFE_SIZE[0] * ((elapsed - first) / (last - first));
            actors.push(layout.panel(
                center - 1.0,
                LIFE_Y,
                [2.0, LIFE_SIZE[1]],
                [1.0, 1.0, 1.0, 0.25],
                9,
            ));
        }
    }
}

fn push_combo_runs(
    actors: &mut Vec<Actor>,
    layout: Layout,
    player: &EvaluationPlayerOverview<'_>,
    seat: Seat,
    x: f32,
) {
    let [first, last] = player.graph_seconds;
    let max_count = player
        .combo_runs
        .iter()
        .map(|run| run.count)
        .max()
        .unwrap_or(0);
    if max_count < 5 || last <= first {
        return;
    }
    let width = LIFE_SIZE[0];
    let bar_x = |second: f32| width * ((second - first) / (last - first)).clamp(0.0, 1.0);
    actors.push(layout.panel(
        x,
        COMBO_Y - 9.0,
        [width, 18.0],
        [0.027, 0.054, 0.081, 1.0],
        2,
    ));
    for run in player.combo_runs.iter().filter(|run| run.count >= 5) {
        let start = bar_x(run.start_second);
        let end = bar_x(run.end_second);
        let max_run = run.count == max_count;
        actors.push(layout.panel(
            x + start,
            COMBO_Y - 4.0,
            [(end - start).max(1.0), 8.0],
            if max_run { seat.tint } else { RAIL },
            3,
        ));
        if max_run {
            let midpoint = x + f32::midpoint(start, end).clamp(12.0, width - 12.0);
            actors.push(layout.panel(
                midpoint - 12.0,
                COMBO_Y - 8.0,
                [24.0, 12.0],
                [0.027, 0.054, 0.081, 1.0],
                4,
            ));
            actors.push(layout.bounded_label(
                [midpoint, COMBO_Y - 2.0],
                0.5,
                22.0,
                unsigned(run.count),
                0.83,
                WHITE,
            ));
        }
    }
}

fn decelerate(elapsed: f32, duration: f32) -> f32 {
    let remaining = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
    remaining.mul_add(-remaining, 1.0)
}

fn push_header(actors: &mut Vec<Actor>, layout: Layout, elapsed: f32) {
    const START: f32 = 2.0;
    if elapsed <= START {
        return;
    }
    let reveal = bounceend_p(((elapsed - START) / 0.3).min(1.0));
    let mut headline = layout.text([36.0, 38.0], 0.0, "CHECK YOUR STATS".into(), 1.6, SKY);
    let [zoom_x, zoom_y] = layout.size([1.6 * reveal, 1.6 * (6.0 - 5.0 * reveal).max(0.6)]);
    headline.zoomx(zoom_x);
    headline.zoomy(zoom_y);
    actors.push(headline.build(0));
}

fn push_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    page: &EvaluationOverview<'_, Actor>,
    progress: f32,
) {
    let y = 114.0 - (1.0 - progress) * (WIDTH * 0.5);
    actors.push(layout.panel(212.0, y - 36.0, [216.0, 72.0], RAIL, 5));
    if let Some(key) = page.banner_key {
        actors.push(layout.image([320.0, y], BANNER_SIZE, key, 6));
    } else {
        actors.push(layout.panel(215.0, y - 33.0, BANNER_SIZE, [0.08, 0.16, 0.23, 1.0], 6));
        actors.push(layout.panel(215.0, y - 33.0, [210.0, 3.0], SKY, 7));
    }
    let text_y = if page.banner_key.is_some() {
        y + 44.0
    } else {
        y - 7.0
    };
    let text_width = if page.banner_key.is_some() {
        272.0
    } else {
        194.0
    };
    actors.push(layout.bounded_label(
        [320.0, text_y],
        0.5,
        text_width,
        TextContent::from(page.title),
        1.04,
        WHITE,
    ));
    if !page.artist.is_empty() {
        actors.push(layout.bounded_label(
            [320.0, text_y + 15.0],
            0.5,
            text_width,
            TextContent::from(page.artist),
            0.75,
            QUIET,
        ));
    }
    if page.course_summary {
        actors.push(layout.label([36.0, y - 38.0], 0.0, "COURSE RESULTS".into(), 0.85, SKY));
    }
}

fn push_difficulty(
    actors: &mut Vec<Actor>,
    layout: Layout,
    seat: Seat,
    player: &EvaluationPlayerOverview<'_>,
    shift: f32,
) {
    let [x, y] = seat.difficulty;
    let x = x + shift;
    actors.push(layout.panel(
        x - 54.0,
        y - 10.0,
        [108.0, 32.0],
        [0.10, 0.18, 0.25, 1.0],
        2,
    ));
    actors.push(layout.panel(x - 54.0, y - 10.0, [3.0, 32.0], seat.tint, 3));
    actors.push(layout.bounded_label(
        [x, y],
        0.5,
        92.0,
        value(format_args!("{}", player.difficulty)),
        0.88,
        WHITE,
    ));
    actors.push(layout.label([x, y + 14.0], 0.5, unsigned(player.meter), 0.8, seat.tint));
}

fn push_grade(actors: &mut Vec<Actor>, layout: Layout, seat: Seat, grade: Grade, elapsed: f32) {
    let arrived = decelerate(elapsed, GRADE_ENTER_SECONDS);
    let settle = decelerate(elapsed - GRADE_SETTLE_START, GRADE_ENTER_SECONDS);
    let grade_x = (seat.grade_x - seat.grade_hold_x).mul_add(settle, seat.grade_hold_x);
    let x = (1.0 - arrived).mul_add(seat.outward * 300.0, grade_x);
    let y = 22.0_f32.mul_add(settle, 180.0);
    let scale = (-1.5_f32).mul_add(settle, 2.5);
    let ink = if grade == Grade::Failed {
        FAILED
    } else {
        seat.tint
    };
    let face = match grade {
        Grade::Quint => "*****",
        Grade::Tier01 => "****",
        Grade::Tier02 => "***",
        Grade::Tier03 => "**",
        Grade::Tier04 => "*",
        _ => grade.gif_suffix(),
    };
    let half_w = 45.0 * scale;
    let half_h = 26.0 * scale;
    actors.push(layout.panel(
        x - half_w,
        y - half_h,
        [2.0 * half_w, 2.0 * half_h],
        ink,
        100,
    ));
    actors.push(layout.panel(
        x - half_w + 2.0 * scale,
        y - half_h + 2.0 * scale,
        [2.0 * half_w - 4.0 * scale, 2.0 * half_h - 4.0 * scale],
        [0.078, 0.117, 0.17, 1.0],
        101,
    ));
    let mut label = layout.text(
        [x, y],
        0.5,
        face.into(),
        if face.len() > 3 {
            1.75 * scale
        } else {
            2.65 * scale
        },
        ink,
    );
    label.maxwidth(layout.size([80.0 * scale, 0.0])[0]);
    label.z(102);
    actors.push(label.build(0));
}

fn push_player_stats(
    actors: &mut Vec<Actor>,
    layout: Layout,
    seat: Seat,
    player: &EvaluationPlayerOverview<'_>,
    shift: f32,
) {
    let x = seat.panel_x + shift;
    let percent_x = seat.percent_x + shift;
    actors.push(layout.panel(x - 3.0, 177.0, [277.0, 262.0], [0.076, 0.117, 0.17, 1.0], 1));
    actors.push(layout.panel(x, 177.0, [271.0, 2.0], seat.tint, 2));
    actors.push(layout.label(
        [x + 109.0, 188.0],
        0.0,
        if player.p2 { "P2" } else { "P1" }.into(),
        0.76,
        QUIET,
    ));
    actors.push(layout.bounded_label(
        [percent_x, 212.0],
        0.5,
        156.0,
        value(format_args!("{:.2}%", player.percent)),
        2.0,
        seat.tint,
    ));
    let accompanying = [
        ("EX", player.ex_percent),
        ("H.EX", player.hard_ex_percent),
        ("OWN", player.individual_percent),
    ];
    let count = accompanying
        .iter()
        .filter(|(_, percent)| percent.is_some())
        .count();
    for (index, (name, percent)) in accompanying
        .into_iter()
        .filter_map(|(name, percent)| percent.map(|percent| (name, percent)))
        .enumerate()
    {
        let dx = (index as f32 - (count - 1) as f32 * 0.5) * 55.0;
        actors.push(layout.bounded_label(
            [percent_x + dx, 230.0],
            0.5,
            52.0,
            value(format_args!("{name} {percent:.2}%")),
            0.7,
            QUIET,
        ));
    }

    actors.push(layout.panel(x, LIFE_Y, LIFE_SIZE, [0.027, 0.054, 0.081, 1.0], 2));
    actors.push(layout.panel(x, LIFE_Y + LIFE_SIZE[1] - 1.0, [LIFE_SIZE[0], 1.0], RAIL, 3));
    for fraction in [0.25, 0.5, 0.75] {
        actors.push(layout.panel(
            x + LIFE_SIZE[0] * fraction,
            LIFE_Y,
            [1.0, LIFE_SIZE[1]],
            [0.12, 0.19, 0.25, 1.0],
            3,
        ));
    }

    if player.disqualified {
        actors.push(layout.label([x + 6.0, 302.0], 0.0, "DISQUALIFIED".into(), 0.8, FAILED));
    } else {
        for (rank, prefix, label_x) in [
            (player.personal_rank, "PERSONAL", x + 6.0),
            (player.machine_rank, "MACHINE", x + 144.0),
        ] {
            if let Some(rank) = rank.filter(|rank| *rank > 0) {
                actors.push(layout.bounded_label(
                    [label_x, 302.0],
                    0.0,
                    122.0,
                    value(format_args!("{prefix} #{rank}")),
                    0.8,
                    seat.tint,
                ));
            }
        }
    }
    push_counts(actors, layout, x, seat.tint, player);
    if !player.mods.is_empty() {
        actors.push(layout.bounded_label(
            [x + 6.0, 429.0],
            0.0,
            258.0,
            TextContent::from(player.mods),
            0.78,
            QUIET,
        ));
    }
}

fn push_counts(
    actors: &mut Vec<Actor>,
    layout: Layout,
    x: f32,
    tint: [f32; 4],
    player: &EvaluationPlayerOverview<'_>,
) {
    actors.push(layout.panel(x + 135.0, 319.0, [1.0, 93.0], RAIL, 2));
    for (index, ((label, color), count)) in JUDGMENTS.into_iter().zip(player.judgments).enumerate()
    {
        let y = (index as f32).mul_add(COUNT_PITCH, COUNT_FIRST_Y);
        actors.push(layout.label([x + 6.0, y], 0.0, label.into(), 0.85, color));
        actors.push(layout.bounded_label(
            [x + 127.0, y],
            1.0,
            43.0,
            value(format_args!("{count:03}")),
            0.92,
            WHITE,
        ));
    }
    for (index, (label, [hit, total])) in [
        ("Jumps", player.jumps),
        ("Holds", player.holds),
        ("Mines", player.mines),
        ("Hands", player.hands),
        ("Rolls", player.rolls),
    ]
    .into_iter()
    .enumerate()
    {
        let y = (index as f32).mul_add(COUNT_PITCH, COUNT_FIRST_Y);
        actors.push(layout.label([x + 144.0, y], 0.0, label.into(), 0.85, QUIET));
        actors.push(layout.bounded_label(
            [x + 263.0, y],
            1.0,
            65.0,
            value(format_args!("{hit:03}/{total:03}")),
            0.92,
            WHITE,
        ));
    }
    let y = 5.0_f32.mul_add(COUNT_PITCH, COUNT_FIRST_Y);
    actors.push(layout.label([x + 144.0, y], 0.0, "Max Combo".into(), 0.85, tint));
    actors.push(layout.bounded_label(
        [x + 265.0, y],
        1.0,
        44.0,
        value(format_args!("{:05}", player.max_combo)),
        0.92,
        WHITE,
    ));
}

fn value(args: std::fmt::Arguments<'_>) -> TextContent {
    TextContent::inline_format(args).unwrap_or_else(|| TextContent::Owned(args.to_string()))
}

fn unsigned(number: u32) -> TextContent {
    TextContent::InlineU32(InlineU32Text::new(number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use std::cell::RefCell;
    use std::sync::Arc;

    fn player(mods: &Arc<str>) -> EvaluationPlayerOverview<'_> {
        EvaluationPlayerOverview {
            p2: false,
            grade: Grade::Tier08,
            percent: 97.65,
            individual_percent: None,
            ex_percent: None,
            hard_ex_percent: None,
            difficulty: "Expert",
            meter: 11,
            mods,
            judgments: [963, 104, 27, 4, 2, 8],
            jumps: [18, 20],
            holds: [72, 79],
            mines: [32, 36],
            hands: [15, 16],
            rolls: [7, 9],
            max_combo: 611,
            combo_runs: &[],
            graph_seconds: [0.0, 120.0],
            course_graph_stages: &[],
            personal_rank: Some(1),
            machine_rank: Some(3),
            disqualified: false,
        }
    }

    fn has_text(actors: &[Actor], text: &str, position: [f32; 2]) -> bool {
        actors.iter().any(|actor| matches!(actor,
            Actor::Text { content, offset, .. } if content.as_str() == text && *offset == position
        ))
    }

    #[test]
    fn native_and_wide_overviews_place_the_banner_and_both_canonical_life_graphs() {
        use deadsync_theme::views::ComboRun;
        let title = Arc::from("DeadSync Stage");
        let artist = Arc::from("Artist");
        let mods = Arc::from("C500, Overhead");
        let banner = Arc::from("stage-banner");
        let combo_runs = [
            ComboRun { start_second: 3.0, end_second: 7.0, count: 4, carried: 500 },
            ComboRun { start_second: 15.0, end_second: 45.0, count: 9, carried: 0 },
            ComboRun { start_second: 75.0, end_second: 105.0, count: 9, carried: 0 },
        ];
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                let graphs = RefCell::new(Vec::new());
                push_evaluation_overview(
                    &mut actors,
                    EvaluationOverview {
                        title: &title,
                        artist: &artist,
                        banner_key: Some(&banner),
                        course_summary: false,
                        double_side_style: false,
                        elapsed: EVALUATION_OVERVIEW_SETTLED_SECONDS,
                        players: [
                            Some(EvaluationPlayerOverview { combo_runs: &combo_runs, ..player(&mods) }),
                            Some(EvaluationPlayerOverview {
                                p2: true,
                                grade: Grade::Failed,
                                percent: 65.0,
                                disqualified: true,
                                ..player(&mods)
                            }),
                        ],
                        life_graph: &|slot, origin, size, tint, z| {
                            graphs.borrow_mut().push((slot, origin, size, tint, z));
                            None
                        },
                    },
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { align, offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *align == [0.5, 0.5] && *offset == [320.0 + delta, 114.0]
                            && *w == 210.0 && *h == 66.0
                )));
                assert_eq!(
                    graphs.borrow().as_slice(),
                    &[
                        (0, [39.0 + delta, 239.0], LIFE_SIZE, P1, 8),
                        (1, [329.0 + delta, 239.0], LIFE_SIZE, P2, 8),
                    ]
                );
                assert!(has_text(&actors, "97.65%", [220.0 + delta, 212.0]));
                assert!(has_text(&actors, "65.00%", [504.0 + delta, 212.0]));
                assert!(has_text(&actors, "A+", [86.0 + delta, 202.0]));
                assert!(has_text(&actors, "F", [376.0 + delta, 202.0]));
                assert!(has_text(&actors, "DISQUALIFIED", [335.0 + delta, 302.0]));
                assert!(has_text(&actors, "018/020", [302.0 + delta, 325.0]));
                assert!(has_text(&actors, "018/020", [592.0 + delta, 325.0]));
                assert!(has_text(&actors, "9", [106.75 + delta, 285.0]));
                assert!(has_text(&actors, "9", [242.25 + delta, 285.0]));
                assert_eq!(actors.iter().filter(|actor| matches!(actor,
                    Actor::Sprite { offset, size: [_, SizeSpec::Px(8.0)], .. }
                        if offset[1] == COMBO_Y - 4.0
                )).count(), 2);
                assert!(has_text(
                    &actors,
                    "PRESS START TO CONTINUE",
                    [320.0 + delta, 445.0]
                ));
            });
        }
    }

    #[test]
    fn grade_enters_holds_and_settles_before_the_stats_slide() {
        let title = Arc::from("Stage");
        let empty = Arc::from("");
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            for (elapsed, grade_xy) in [
                (0.0, [-140.0, 180.0]),
                (0.5, [160.0, 180.0]),
                (2.49, [160.0, 180.0]),
                (2.75, [104.5, 196.5]),
                (3.0, [86.0, 202.0]),
            ] {
                let mut actors = Vec::new();
                push_evaluation_overview(
                    &mut actors,
                    EvaluationOverview {
                        title: &title,
                        artist: &empty,
                        banner_key: None,
                        course_summary: false,
                        double_side_style: false,
                        elapsed,
                        players: [Some(player(&empty)), None],
                        life_graph: &|_, _, _, _, _| {
                            panic!("life graph should wait for the stats slide")
                        },
                    },
                );
                assert!(has_text(&actors, "A+", grade_xy));
                assert_eq!(
                    has_text(&actors, "CHECK YOUR STATS", [36.0, 38.0]),
                    elapsed > 2.0
                );
                assert!(!has_text(&actors, "97.65%", [220.0, 212.0]));
            }
            let mut actors = Vec::new();
            let graph_x = std::cell::Cell::new(0.0);
            push_evaluation_overview(
                &mut actors,
                EvaluationOverview {
                    title: &title,
                    artist: &empty,
                    banner_key: None,
                    course_summary: false,
                    double_side_style: false,
                    elapsed: 3.15,
                    players: [Some(player(&empty)), None],
                    life_graph: &|_, origin, _, _, _| {
                        graph_x.set(origin[0]);
                        None
                    },
                },
            );
            assert!((graph_x.get() - (-41.0)).abs() < 0.001);
        });
    }

    #[test]
    fn p2_solo_double_centers_the_panel_and_keeps_runtime_slot_and_prepared_stats() {
        let title = Arc::from("Course Name");
        let mods = Arc::from("2.30x");
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            push_evaluation_overview(
                &mut actors,
                EvaluationOverview {
                    title: &title,
                    artist: &Arc::from(""),
                    banner_key: None,
                    course_summary: true,
                    double_side_style: true,
                    elapsed: 5.0,
                    players: [
                        Some(EvaluationPlayerOverview {
                            p2: true,
                            grade: Grade::Quint,
                            individual_percent: Some(93.44),
                            ex_percent: Some(91.25),
                            hard_ex_percent: Some(89.64),
                            ..player(&mods)
                        }),
                        None,
                    ],
                    life_graph: &|slot, origin, size, tint, _| {
                        assert_eq!(
                            (slot, origin, size, tint),
                            (0, [182.0, 239.0], LIFE_SIZE, P2)
                        );
                        None
                    },
                },
            );
            assert!(has_text(&actors, "COURSE RESULTS", [36.0, 76.0]));
            assert!(has_text(&actors, "Course Name", [320.0, 107.0]));
            assert!(has_text(&actors, "Expert", [483.0, 90.0]));
            assert!(has_text(&actors, "*****", [229.0, 202.0]));
            assert!(has_text(&actors, "EX 91.25%", [302.0, 230.0]));
            assert!(has_text(&actors, "H.EX 89.64%", [357.0, 230.0]));
            assert!(has_text(&actors, "OWN 93.44%", [412.0, 230.0]));
            assert!(has_text(&actors, "963", [309.0, 325.0]));
            assert!(has_text(&actors, "072/079", [445.0, 341.0]));
            assert!(has_text(&actors, "00611", [447.0, 405.0]));
            assert!(has_text(&actors, "MACHINE #3", [326.0, 302.0]));
        });
    }
}
