use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
use crate::screens::components::evaluation::{FooterClock, eval_grades};
use crate::screens::components::shared::screen_bar::{
    ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::{
    banner as shared_banner, screen_bar, transitions, visual_style_bg,
};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
use crate::views::{PostSelectStageView, PostSongRuntimeView};
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadlib_present::space::{screen_center_x, screen_height, screen_width, widescale};
use deadsync_chart::ChartData;
use deadsync_chart::SongData;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_score as score_data;
use deadsync_score::stage_stats;
use std::sync::Arc;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const ROWS_PER_PAGE: usize = 4;

struct SummaryPlayerText {
    profile_name: Option<Arc<str>>,
    difficulty: Arc<str>,
    step_artist: Arc<str>,
}

struct SummaryRowText {
    banner_key: Arc<str>,
    full_title: Arc<str>,
    bpm_line: Arc<str>,
    players: [Option<SummaryPlayerText>; 2],
}

/// Retained Summary row text owned exclusively by the game thread.
///
/// The cache lives for one Summary screen, is warmed before actor construction,
/// and is capped by the shell-selected stage count. Stable frames compare a
/// dirty bit, the localization revision, and three scalar presentation values;
/// misses rebuild all selected rows at the screen/config transition boundary.
/// Rebuilds reuse vector capacity and replace old `Arc` values synchronously;
/// screen teardown drops the remaining rows. The boolean rebuild result is
/// available for tests, and worst-case work is one linear selected-stage pass.
struct SummaryRows {
    rows: Vec<SummaryRowText>,
    dirty: bool,
    i18n_revision: u64,
    active_color_index: i32,
    translated_titles: bool,
    zmod_rating_box_text: bool,
}

impl SummaryRows {
    const fn new() -> Self {
        Self {
            rows: Vec::new(),
            dirty: true,
            i18n_revision: u64::MAX,
            active_color_index: color::DEFAULT_COLOR_INDEX,
            translated_titles: false,
            zmod_rating_box_text: false,
        }
    }

    const fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn sync(
        &mut self,
        stages: PostSelectStageView<'_>,
        active_color_index: i32,
        translated_titles: bool,
        zmod_rating_box_text: bool,
    ) -> bool {
        let i18n_revision = crate::assets::i18n::revision();
        if !self.dirty
            && self.i18n_revision == i18n_revision
            && self.active_color_index == active_color_index
            && self.translated_titles == translated_titles
            && self.zmod_rating_box_text == zmod_rating_box_text
        {
            return false;
        }

        let show_profile_names = should_display_profile_names(stages);
        self.rows.clear();
        self.rows.reserve(stages.len());
        for stage in stages.iter() {
            self.rows.push(build_row_text(
                stage,
                show_profile_names,
                active_color_index,
                translated_titles,
                zmod_rating_box_text,
            ));
        }
        self.dirty = false;
        self.i18n_revision = i18n_revision;
        self.active_color_index = active_color_index;
        self.translated_titles = translated_titles;
        self.zmod_rating_box_text = zmod_rating_box_text;
        true
    }
}

/// Fixed-size localized label cache owned by the game thread.
///
/// Four `Arc` values live for the Summary screen and warm before actor
/// construction. Locale, page, or page-count changes replace them immediately;
/// there is no growth, eviction, locking beyond the localization lookup, or
/// background work. Sync's boolean result is test-visible, and worst-case work
/// is four bounded lookups plus one short page-label format.
struct SummaryLabels {
    screen_title: Arc<str>,
    no_stage_data: Arc<str>,
    itg_label: Arc<str>,
    page_label: Arc<str>,
    i18n_revision: u64,
    page: usize,
    pages: usize,
}

impl SummaryLabels {
    fn new() -> Self {
        Self {
            screen_title: Arc::from(""),
            no_stage_data: Arc::from(""),
            itg_label: Arc::from(""),
            page_label: Arc::from(""),
            i18n_revision: u64::MAX,
            page: 0,
            pages: 0,
        }
    }

    fn sync(&mut self, page: usize, pages: usize) -> bool {
        let i18n_revision = crate::assets::i18n::revision();
        if self.i18n_revision == i18n_revision && self.page == page && self.pages == pages {
            return false;
        }

        let page_text = TextContent::inline_u32(page as u32);
        let pages_text = TextContent::inline_u32(pages as u32);
        self.screen_title = tr("EvaluationSummary", "ScreenTitle");
        self.no_stage_data = tr("EvaluationSummary", "NoStageDataAvailable");
        self.itg_label = tr("EvaluationSummary", "ITGLabel");
        self.page_label = tr_fmt(
            "EvaluationSummary",
            "PageFormat",
            &[("page", page_text.as_str()), ("pages", pages_text.as_str())],
        );
        self.i18n_revision = crate::assets::i18n::revision();
        self.page = page;
        self.pages = pages;
        true
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    pub page: usize,
    pub elapsed: f32,
    pub return_to: Screen,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: [i8; 2],
    footer_clock: FooterClock,
    stage_rows: SummaryRows,
    labels: SummaryLabels,
    runtime: PostSongRuntimeView,
}

#[must_use]
pub fn init(runtime: PostSongRuntimeView) -> State {
    init_for_return(runtime, Screen::Initials)
}

#[must_use]
pub fn init_for_return(runtime: PostSongRuntimeView, return_to: Screen) -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        page: 1,
        elapsed: 0.0,
        return_to,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: [0; 2],
        footer_clock: FooterClock::new(),
        stage_rows: SummaryRows::new(),
        labels: SummaryLabels::new(),
        runtime,
    }
}

pub fn update(state: &mut State, dt: f32) {
    state.elapsed = (state.elapsed + dt).max(0.0);
    state.footer_clock.update(dt);
}

pub const fn mark_stage_rows_dirty(state: &mut State) {
    state.stage_rows.mark_dirty();
}

pub fn sync_stage_rows(state: &mut State, stages: PostSelectStageView<'_>) -> bool {
    let pages = pages_for(stages.len());
    let page = state.page.clamp(1, pages);
    let labels_changed = state.labels.sync(page, pages);
    let rows_changed = state.stage_rows.sync(
        stages,
        state.active_color_index,
        state.runtime.translated_titles,
        state.runtime.zmod_rating_box_text,
    );
    labels_changed || rows_changed
}

#[inline(always)]
fn shift_page(state: &mut State, num_stages: usize, dir: i32) -> bool {
    let pages = pages_for(num_stages);
    let old_page = state.page;
    if dir < 0 {
        if pages > 1 && state.page > 1 {
            state.page = state.page.saturating_sub(1).max(1);
        }
    } else if pages > 1 {
        state.page = (state.page + 1).min(pages.max(1));
    }
    state.page != old_page
}

pub fn handle_input(state: &mut State, num_stages: usize, ev: &InputEvent) -> ThemeEffect {
    let chord_side = if state.runtime.three_key_navigation {
        state.menu_lr_chord.update(ev)
    } else {
        None
    };
    if !ev.pressed {
        if let Some(side) = screen_input::menu_lr_side(ev.action) {
            state.menu_lr_undo[profile_data::player_side_index(side)] = 0;
        }
        return ThemeEffect::None;
    }
    if let Some(side) = chord_side {
        let undo = state.menu_lr_undo[profile_data::player_side_index(side)];
        state.menu_lr_undo[profile_data::player_side_index(side)] = 0;
        if undo != 0 {
            let _ = shift_page(state, num_stages, i32::from(undo));
        }
        return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Media(
            crate::SimplyLoveMediaRequest::Screenshot(Some(side)),
        ));
    }
    match ev.action {
        VirtualAction::p1_back
        | VirtualAction::p1_start
        | VirtualAction::p2_back
        | VirtualAction::p2_start => ThemeEffect::Navigate(state.return_to),

        VirtualAction::p1_menu_left
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_up
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_up => {
            if let Some(side) = screen_input::menu_lr_side(ev.action) {
                state.menu_lr_undo[profile_data::player_side_index(side)] =
                    if shift_page(state, num_stages, -1) {
                        1
                    } else {
                        0
                    };
            } else {
                let _ = shift_page(state, num_stages, -1);
            }
            ThemeEffect::None
        }

        VirtualAction::p1_menu_right
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_down
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_down => {
            if let Some(side) = screen_input::menu_lr_side(ev.action) {
                state.menu_lr_undo[profile_data::player_side_index(side)] =
                    if shift_page(state, num_stages, 1) {
                        -1
                    } else {
                        0
                    };
            } else {
                let _ = shift_page(state, num_stages, 1);
            }
            ThemeEffect::None
        }

        _ => ThemeEffect::None,
    }
}

#[inline(always)]
fn pages_for(num_stages: usize) -> usize {
    let pages = num_stages.div_ceil(ROWS_PER_PAGE);
    pages.max(1)
}

fn format_rate_x(rate: f32) -> String {
    let r = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let s = format!("{r:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn stringify_display_bpms(song: &SongData, chart: Option<&ChartData>, music_rate: f32) -> String {
    // Handle Random display BPM — show "???" on eval
    if let Some(chart) = chart
        && matches!(
            chart.display_bpm,
            Some(deadsync_chart::ChartDisplayBpm::Random)
        )
    {
        return "???".to_string();
    }
    deadsync_chart::song::format_display_bpm_range(song.chart_display_bpm_range(chart), music_rate)
}

fn steps_type_label(chart_type: &str) -> Arc<str> {
    if chart_type.eq_ignore_ascii_case("dance-single") {
        tr("EvaluationSummary", "SingleLabel")
    } else if chart_type.eq_ignore_ascii_case("dance-double") {
        tr("EvaluationSummary", "DoubleLabel")
    } else {
        tr("EvaluationSummary", "UnknownLabel")
    }
}

fn difficulty_display_name(difficulty: &str, zmod_rating_box_text: bool) -> &'static str {
    color::difficulty_display_name(difficulty, zmod_rating_box_text)
}

fn build_player_text(
    player: &stage_stats::PlayerStageSummary,
    show_profile_name: bool,
    zmod_rating_box_text: bool,
) -> SummaryPlayerText {
    let style = steps_type_label(&player.chart.chart_type);
    let difficulty = difficulty_display_name(&player.chart.difficulty, zmod_rating_box_text);
    SummaryPlayerText {
        profile_name: show_profile_name.then(|| Arc::from(player.profile_name.as_str())),
        difficulty: tr_fmt(
            "EvaluationSummary",
            "DifficultyFormat",
            &[("style", &style), ("difficulty", difficulty)],
        ),
        step_artist: Arc::from(player.chart.step_artist.as_str()),
    }
}

fn build_row_text(
    stage: &stage_stats::StageSummary,
    show_profile_names: bool,
    active_color_index: i32,
    translated_titles: bool,
    zmod_rating_box_text: bool,
) -> SummaryRowText {
    let banner_key = stage
        .song
        .banner_path
        .as_deref()
        .map(crate::assets::media_path_key)
        .unwrap_or_else(|| {
            let banner_num = active_color_index.rem_euclid(12) + 1;
            Arc::from(format!("banner{banner_num}.png"))
        });
    let full_title = Arc::from(stage.song.display_full_title(translated_titles));
    let eval_chart = stage
        .players
        .iter()
        .flatten()
        .next()
        .map(|player| player.chart.as_ref());
    let bpm = stringify_display_bpms(&stage.song, eval_chart, stage.music_rate);
    let bpm_line = if bpm.is_empty() {
        Arc::from("")
    } else if (stage.music_rate - 1.0).abs() > 0.001 {
        tr_fmt(
            "EvaluationSummary",
            "BpmWithRate",
            &[("bpm", &bpm), ("rate", &format_rate_x(stage.music_rate))],
        )
    } else {
        tr_fmt("EvaluationSummary", "BpmDisplay", &[("bpm", &bpm)])
    };

    SummaryRowText {
        banner_key,
        full_title,
        bpm_line,
        players: std::array::from_fn(|index| {
            stage.players[index]
                .as_ref()
                .map(|player| build_player_text(player, show_profile_names, zmod_rating_box_text))
        }),
    }
}

fn should_display_profile_names(stages: PostSelectStageView<'_>) -> bool {
    (0..2).any(|side| {
        profile_name_changed(
            stages
                .iter()
                .filter_map(|stage| stage.players.get(side)?.as_ref())
                .map(|player| player.profile_name.as_str()),
        )
    })
}

#[inline]
fn profile_name_changed<'a>(mut names: impl Iterator<Item = &'a str>) -> bool {
    let Some(first) = names.next() else {
        return false;
    };
    names.any(|name| name != first)
}

#[inline]
fn fixed_2_text(value: f64) -> TextContent {
    TextContent::inline_format(format_args!("{value:.2}"))
        .unwrap_or_else(|| TextContent::Owned(format!("{value:.2}")))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn benchmark_profile_name_changed(sides: [&[&str]; 2]) -> bool {
    sides
        .into_iter()
        .any(|names| profile_name_changed(names.iter().copied()))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn benchmark_eval_numeric_text(percent: f64, ex: f64, counts: &[u32; 8]) -> usize {
    let mut bytes = fixed_2_text(percent).as_str().len() + fixed_2_text(ex).as_str().len();
    for count in counts {
        bytes += TextContent::inline_u32(*count).as_str().len();
    }
    bytes
}

fn build_player_stats(
    side: profile_data::PlayerSide,
    p: &stage_stats::PlayerStageSummary,
    text: &SummaryPlayerText,
    active_color_index: i32,
    difficulty_color_scheme: deadsync_config::prelude::DifficultyColorScheme,
    elapsed: f32,
    machine_font: deadsync_config::prelude::MachineFont,
    judgment_palette: JudgmentPalette,
) -> Vec<Actor> {
    let (col1x, col2x, grade_x, align1_x, align2_x, align1_text, align2_text, col1_eps) = match side
    {
        profile_data::PlayerSide::P1 => (
            -90.0,
            -(screen_width() / 2.5),
            -widescale(194.0, 250.0),
            1.0,
            0.0,
            deadlib_present::actors::TextAlign::Right,
            deadlib_present::actors::TextAlign::Left,
            -1.0,
        ),
        profile_data::PlayerSide::P2 => (
            90.0,
            screen_width() / 2.5,
            widescale(194.0, 250.0),
            0.0,
            1.0,
            deadlib_present::actors::TextAlign::Left,
            deadlib_present::actors::TextAlign::Right,
            1.0,
        ),
    };

    let mut out = Vec::with_capacity(24);

    // Profile name (only if there were any profile switches this session)
    if let Some(profile_name) = text.profile_name.as_ref() {
        let mut a = act!(text:
            font("miso"):
            settext(Arc::clone(profile_name)):
            align(align1_x, 0.5):
            xy(col1x, -43.0):
            zoom(0.5):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    let show_w0 = p.show_w0;
    let showex = p.show_ex_score;

    // Percent score (trim '%' and remove leading whitespace, like Simply Love)
    let percent_text = fixed_2_text((p.score_percent * 100.0).max(0.0));
    let percent_rgba = if p.grade == score_data::Grade::Failed {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        [1.0; 4]
    };

    let (percent_zoom, percent_y) = if showex {
        (0.38, -12.0)
    } else if show_w0 {
        (0.48, -32.0)
    } else {
        (0.5, -24.0)
    };
    let mut percent_actor = act!(text:
        font(machine_font_key(machine_font, FontRole::Header)):
        settext(percent_text):
        align(align1_x, 0.5):
        xy(col1x, percent_y):
        zoom(percent_zoom):
        z(3):
        diffuse(percent_rgba[0], percent_rgba[1], percent_rgba[2], percent_rgba[3])
    );
    if let Actor::Text { align_text, .. } = &mut percent_actor {
        *align_text = align1_text;
    }
    out.push(percent_actor);

    // EX score (only if W0 is enabled)
    if show_w0 {
        let ex_color = judgment_palette.color(Role::FantasticBlue);
        let ex_text = fixed_2_text(p.ex_score_percent.max(0.0));
        let (ex_zoom, ex_y) = if showex { (0.48, -32.0) } else { (0.38, -12.0) };
        let mut ex_actor = act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(ex_text):
            align(align1_x, 0.5):
            xy(col1x, ex_y):
            zoom(ex_zoom):
            z(3):
            diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
        );
        if let Actor::Text { align_text, .. } = &mut ex_actor {
            *align_text = align1_text;
        }
        out.push(ex_actor);
    }

    // Stepchart style + difficulty text
    {
        let mut a = act!(text:
            font("miso"):
            settext(Arc::clone(&text.difficulty)):
            align(align1_x, 0.5):
            xy(col1x + col1_eps, 17.0):
            zoom(0.65):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Difficulty meter
    {
        let diff_color = color::difficulty_rgba_with_scheme(
            &p.chart.difficulty,
            active_color_index,
            difficulty_color_scheme,
        );
        let (meter_zoom, meter_y) = if show_w0 { (0.3, 5.0) } else { (0.4, -1.0) };
        let mut a = act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(TextContent::inline_u32(p.chart.meter)):
            align(align1_x, 0.5):
            xy(col1x, meter_y):
            zoom(meter_zoom):
            z(3):
            diffuse(diff_color[0], diff_color[1], diff_color[2], 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Step artist
    {
        let mut a = act!(text:
            font("miso"):
            settext(Arc::clone(&text.step_artist)):
            align(align1_x, 0.5):
            xy(col1x, 32.0):
            zoom(0.65):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Letter grade
    eval_grades::push_actors(
        &mut out,
        p.grade,
        eval_grades::EvalGradeParams {
            x: grade_x,
            y: -6.0,
            z: 4,
            zoom: widescale(0.275, 0.3),
            elapsed,
            ..Default::default()
        },
    );

    // Judgment numbers: W0..W5, Miss
    let wc = p.window_counts;
    let mut counts: [u32; 7] = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
    if !show_w0 {
        counts[1] = counts[0].saturating_add(counts[1]); // W1 includes W0 when FA+/EX is disabled
    }
    let y_base = if show_w0 { -58.0 } else { -63.0 };

    for (i, count) in counts.iter().copied().enumerate() {
        if i == 0 && !show_w0 {
            continue;
        }
        let y = ((i as f32) + 1.0).mul_add(13.0, y_base);
        let rgba = match i {
            0 => judgment_palette.color(Role::FantasticBlue), // W0
            1 => {
                if show_w0 {
                    judgment_palette.color(Role::FantasticWhite)
                } else {
                    judgment_palette.color(Role::FantasticBlue)
                }
            }
            2 => judgment_palette.color(Role::Excellent),
            3 => judgment_palette.color(Role::Great),
            4 => judgment_palette.color(Role::Decent),
            5 => judgment_palette.color(Role::WayOff),
            _ => judgment_palette.color(Role::Miss),
        };

        let mut a = act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(TextContent::inline_u32(count)):
            align(align2_x, 0.5):
            xy(col2x, y):
            zoom(0.28):
            z(3):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3])
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align2_text;
        }
        out.push(a);
    }

    out
}

fn build_row(
    row_pos: usize,
    stage: &stage_stats::StageSummary,
    text: &SummaryRowText,
    active_color_index: i32,
    difficulty_color_scheme: deadsync_config::prelude::DifficultyColorScheme,
    elapsed: f32,
    machine_font: deadsync_config::prelude::MachineFont,
    judgment_palettes: [JudgmentPalette; 2],
) -> Actor {
    let cx = screen_center_x();
    let y = (screen_height() / 4.75) * (row_pos as f32);

    let mut children: Vec<Actor> = Vec::with_capacity(64);

    // Black quad background
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, -6.0):
        zoomto(screen_width() - 40.0, 94.0):
        diffuse(0.0, 0.0, 0.0, 0.5):
        z(0)
    ));

    // Banner
    children.push(shared_banner::sprite(
        Arc::clone(&text.banner_key),
        0.0,
        -6.0,
        418.0,
        164.0,
        0.333,
        1,
    ));

    // Song title
    children.push(act!(text:
        font("miso"):
        settext(Arc::clone(&text.full_title)):
        align(0.5, 0.5):
        xy(0.0, -43.0):
        zoom(0.8):
        maxwidth(350.0):
        z(2):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    // BPM(s)
    children.push(act!(text:
        font("miso"):
        settext(Arc::clone(&text.bpm_line)):
        align(0.5, 0.5):
        xy(0.0, 32.0):
        zoom(0.65):
        maxwidth(350.0):
        z(2):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    for (idx, side) in [
        (0, profile_data::PlayerSide::P1),
        (1, profile_data::PlayerSide::P2),
    ] {
        let Some((player, player_text)) = stage
            .players
            .get(idx)
            .and_then(|player| player.as_ref())
            .zip(text.players.get(idx).and_then(|text| text.as_ref()))
        else {
            continue;
        };
        children.extend(build_player_stats(
            side,
            player,
            player_text,
            active_color_index,
            difficulty_color_scheme,
            elapsed,
            machine_font,
            judgment_palettes[idx],
        ));
    }

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [cx, y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 50,
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    stages: PostSelectStageView<'_>,
    _asset_manager: &AssetManager,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(32);

    // Background
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    // Top Bar
    actors.push(screen_bar::build_cached(ScreenBarParams {
        visual_policy,
        title: &state.labels.screen_title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    if stages.is_empty() {
        actors.push(act!(text:
            font(machine_font_key(state.runtime.machine_font, FontRole::Header)):
            settext(Arc::clone(&state.labels.no_stage_data)):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_height() * 0.5):
            zoom(0.8):
            z(100):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(center)
        ));
        return;
    }
    debug_assert_eq!(state.stage_rows.rows.len(), stages.len());

    let pages = pages_for(stages.len());
    let page = state.page.clamp(1, pages);

    // Centered "Page x/y"
    actors.push(act!(text:
        font(machine_font_key(state.runtime.machine_font, FontRole::Header)):
        settext(Arc::clone(&state.labels.page_label)):
        align(0.5, 0.5):
        xy(screen_center_x(), 15.0):
        zoom(widescale(0.5, 0.6)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    // --- "ITG" text (top right, no pads) ---
    {
        let itg_text_x = screen_width() - 10.0;
        actors.push(act!(text:
                font(machine_font_key(state.runtime.machine_font, FontRole::Header)):
                settext(Arc::clone(&state.labels.itg_label)):
                align(1.0, 0.5):
            xy(itg_text_x, 15.0):
            zoom(widescale(0.5, 0.6)):
            z(121):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ));
    }

    for row in 1..=ROWS_PER_PAGE {
        let stage_index = (page - 1) * ROWS_PER_PAGE + (row - 1);
        let Some((stage, text)) = stages
            .get(stage_index)
            .zip(state.stage_rows.rows.get(stage_index))
        else {
            continue;
        };
        actors.push(build_row(
            row,
            stage,
            text,
            state.active_color_index,
            state.runtime.difficulty_color_scheme,
            state.elapsed,
            state.runtime.machine_font,
            [
                state.runtime.players[0].judgment_palette,
                state.runtime.players[1].judgment_palette,
            ],
        ));
    }

    // --- Footer decorations (avatars + date/time) ---
    {
        let p1 = &state.runtime.players[0];
        let p2 = &state.runtime.players[1];
        let p1_avatar_key = if p1.joined && !p1.guest {
            p1.avatar_texture_key.as_deref()
        } else {
            None
        };
        let p2_avatar_key = if p2.joined && !p2.guest {
            p2.avatar_texture_key.as_deref()
        } else {
            None
        };

        let (left_avatar, right_avatar) = if state.runtime.play_style.is_versus() {
            (p1_avatar_key, p2_avatar_key)
        } else {
            match state.runtime.player_side {
                profile_data::PlayerSide::P1 => (p1_avatar_key, None),
                profile_data::PlayerSide::P2 => (None, p2_avatar_key),
            }
        };

        if let Some(key) = left_avatar {
            actors.push(act!(sprite(key):
                align(0.0, 1.0):
                xy(0.0, screen_height()):
                setsize(32.0, 32.0):
                z(121)
            ));
        }
        if let Some(key) = right_avatar {
            actors.push(act!(sprite(key):
                align(1.0, 1.0):
                xy(screen_width(), screen_height()):
                setsize(32.0, 32.0):
                z(121)
            ));
        }

        actors.push(act!(text:
            font(machine_font_key(state.runtime.machine_font, FontRole::Numbers)):
            settext(Arc::clone(state.footer_clock.text())):
            align(0.5, 1.0):
            xy(screen_center_x(), screen_height() - 14.0):
            zoom(0.18):
            horizalign(center):
            z(121)
        ));
    }
}

pub fn get_actors(
    state: &mut State,
    stages: PostSelectStageView<'_>,
    asset_manager: &AssetManager,
) -> Vec<Actor> {
    sync_stage_rows(state, stages);
    let mut actors = Vec::with_capacity(32);
    push_actors(
        &mut actors,
        state,
        stages,
        asset_manager,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_rows_rebuild_only_when_dirty_or_policy_changes() {
        let stages: [stage_stats::StageSummary; 0] = [];
        let indices: [usize; 0] = [];
        let view = PostSelectStageView::new(&stages, &indices);
        let mut state = init(PostSongRuntimeView::default());

        assert!(sync_stage_rows(&mut state, view));
        assert!(!sync_stage_rows(&mut state, view));

        mark_stage_rows_dirty(&mut state);
        assert!(sync_stage_rows(&mut state, view));
        assert!(!sync_stage_rows(&mut state, view));

        state.active_color_index += 1;
        assert!(sync_stage_rows(&mut state, view));
        assert!(!sync_stage_rows(&mut state, view));
    }

    #[test]
    fn summary_labels_reformat_only_when_page_or_locale_changes() {
        let mut labels = SummaryLabels::new();

        assert!(labels.sync(1, 3));
        let first_page = Arc::clone(&labels.page_label);
        assert!(!labels.sync(1, 3));
        assert!(Arc::ptr_eq(&labels.page_label, &first_page));

        assert!(labels.sync(2, 3));
        assert!(!Arc::ptr_eq(&labels.page_label, &first_page));
        assert!(labels.page_label.contains('2'));
        assert!(!labels.sync(2, 3));
    }

    #[test]
    fn profile_change_scan_matches_distinct_name_behavior() {
        assert!(!profile_name_changed(std::iter::empty()));
        assert!(!profile_name_changed(["Player"].into_iter()));
        assert!(!profile_name_changed(["Player", "Player"].into_iter()));
        assert!(profile_name_changed(["Player", "Guest"].into_iter()));
        assert!(profile_name_changed(
            ["Player", "Player", "Guest"].into_iter()
        ));
    }

    #[test]
    fn inline_fixed_precision_matches_standard_formatting() {
        for value in [0.0, -0.0, 0.004, 98.7654, 100.0, f64::INFINITY] {
            assert_eq!(fixed_2_text(value).as_str(), format!("{value:.2}"));
        }

        let large = f64::MAX;
        assert_eq!(fixed_2_text(large).as_str(), format!("{large:.2}"));
    }
}
