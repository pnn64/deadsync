use std::sync::Arc;

use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::attract::{AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation};
use deadsync_theme::views::{AttractCourseRecord, AttractRecord, AttractRecords};

use crate::demonstration::{push_training_underlay, training::TrainingCues};
use crate::presentation::{Layout, WIDTH, italic, push_red_backdrop, quad, sprite};
use crate::resources::{MENU_HEADING_FONT, MENU_NORMAL_FONT};

const RECORDS_PER_PAGE: usize = 5;
const RECORD_PAGE_SECONDS: f32 = 10.0;
const COURSE_SECONDS_PER_ROW: f32 = 0.4;
const COURSE_ROW_SPACING: f32 = 65.0;
const RANKING_TOP: f32 = 77.0;
const RANKING_BOTTOM: f32 = 403.0;
pub(crate) const LOGO_MOVIE: &str = "assets/media/openitg/logo.mpg";
pub(crate) const LOGO_POSTER: &str = "openitg/attract-logo-poster.png";
pub(crate) const LOGO_ENTRY_SECONDS: f32 = 1.53;

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Company,
    Intro,
    Logo,
    Demonstration(OpenItgDemonstration),
    Records,
    MarathonRecords,
    SurvivalRecords,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenItgDemonstration {
    Autoplay,
    Training,
}

#[derive(Clone, Copy, PartialEq)]
pub struct OpenItgIntroMedia {
    pub movie: Option<&'static str>,
    pub music: Option<&'static str>,
    pub loop_music: bool,
    rate: f32,
    poster: &'static str,
}

impl Phase {
    fn duration(self, records: &AttractRecords) -> f32 {
        match self {
            Self::Company | Self::Logo => 6.0,
            Self::Intro => 30.5,
            Self::Demonstration(OpenItgDemonstration::Autoplay) => 30.0,
            Self::Demonstration(OpenItgDemonstration::Training) => 41.0,
            Self::Records => {
                RECORD_PAGE_SECONDS * records.songs.len().div_ceil(RECORDS_PER_PAGE).max(1) as f32
            }
            Self::MarathonRecords => course_scroll_duration(&records.marathon),
            Self::SurvivalRecords => course_scroll_duration(&records.survival),
        }
    }

    const fn media(self) -> OpenItgIntroMedia {
        match self {
            Self::Company => OpenItgIntroMedia {
                movie: Some("assets/media/openitg/company.mpg"),
                music: Some("assets/media/openitg/company.ogg"),
                loop_music: false,
                rate: 1.0,
                poster: "openitg/attract-company-poster.png",
            },
            Self::Intro => OpenItgIntroMedia {
                movie: Some("assets/media/openitg/intro.mpg"),
                music: Some("assets/media/openitg/intro.ogg"),
                loop_music: false,
                rate: 1.0,
                poster: "openitg/attract-intro-poster.png",
            },
            Self::Logo => OpenItgIntroMedia {
                movie: Some(LOGO_MOVIE),
                music: None,
                loop_music: false,
                rate: 2.0,
                poster: LOGO_POSTER,
            },
            Self::Demonstration(_) => OpenItgIntroMedia {
                movie: None,
                music: None,
                loop_music: false,
                rate: 1.0,
                poster: "openitg/gameplay-fallback.png",
            },
            Self::Records | Self::MarathonRecords | Self::SurvivalRecords => OpenItgIntroMedia {
                movie: None,
                music: Some("assets/media/openitg/ranking.ogg"),
                loop_music: true,
                rate: 1.0,
                poster: "openitg/selection-backdrop.png",
            },
        }
    }
}

#[derive(Default)]
pub struct OpenItgIntro {
    phase: Phase,
    elapsed: f32,
    records: AttractRecords,
    training: Option<TrainingCues>,
}

impl OpenItgIntro {
    pub fn start_idle(&mut self, records: AttractRecords) {
        self.phase = Phase::Demonstration(OpenItgDemonstration::Autoplay);
        self.elapsed = 0.0;
        self.records = records;
    }

    pub const fn demonstration(&self) -> Option<OpenItgDemonstration> {
        match self.phase {
            Phase::Demonstration(demo) => Some(demo),
            _ => None,
        }
    }

    pub fn finish_demonstration(&mut self) {
        self.phase = match self.demonstration() {
            Some(OpenItgDemonstration::Autoplay) => Phase::Demonstration(OpenItgDemonstration::Training),
            Some(OpenItgDemonstration::Training) => Phase::Records,
            None => return,
        };
        self.elapsed = 0.0;
        self.training = None;
    }

    pub fn advance(&mut self, delta_time: f32) -> bool {
        self.elapsed += delta_time.max(0.0);
        loop {
            let duration = self.phase.duration(&self.records);
            if self.elapsed < duration {
                return false;
            }
            let next = match self.phase {
                Phase::Company => Phase::Intro,
                Phase::Intro => Phase::Logo,
                Phase::Logo => return true,
                Phase::Demonstration(_) => {
                    self.finish_demonstration();
                    return false;
                }
                Phase::Records => Phase::MarathonRecords,
                Phase::MarathonRecords => Phase::SurvivalRecords,
                Phase::SurvivalRecords => Phase::Company,
            };
            self.elapsed -= duration;
            self.phase = next;
        }
    }

    pub const fn media(&self) -> OpenItgIntroMedia {
        self.phase.media()
    }

    pub const fn playback_time(&self) -> f32 {
        self.elapsed * self.media().rate
    }

    pub fn push_demonstration_overlay(
        &self,
        actors: &mut Vec<Actor>,
        music_time: f32,
    ) {
        if let Some(demo) = self.demonstration() {
            crate::demonstration::push_actors(actors, demo, self.elapsed, music_time);
        }
    }

    pub fn push_actors(&self, actors: &mut Vec<Actor>, video_key: Option<&Arc<str>>) {
        let width = screen_width();
        let height = screen_height();
        let [x, y] = [width * 0.5, height * 0.5];
        let layout = Layout::centered(width, height);
        match self.phase {
            Phase::Records => {
                push_records(actors, layout, &self.records.songs, self.elapsed);
                return;
            }
            Phase::MarathonRecords | Phase::SurvivalRecords => {
                let (title, courses) = match self.phase {
                    Phase::MarathonRecords => ("Marathon", &self.records.marathon),
                    _ => ("Survival", &self.records.survival),
                };
                push_course_records(actors, layout, title, courses, self.elapsed);
                return;
            }
            _ => {}
        }
        let alpha = if matches!(self.phase, Phase::Logo) {
            0.7
        } else {
            1.0
        };
        let size = [
            width,
            if matches!(self.phase, Phase::Intro) {
                height * 1.1
            } else {
                height
            },
        ];
        let mut backdrop = if let Some(key) = video_key {
            deadlib_assets::present_dsl::cover_sprite(
                Arc::clone(key),
                x,
                y,
                size[0],
                size[1],
                1.0,
                0,
            )
        } else {
            sprite(self.media().poster, [x, y], size, [1.0; 4], 0).build(0)
        };
        if let Actor::Sprite { tint, .. } = &mut backdrop {
            tint[3] = alpha;
        }
        actors.push(backdrop);
        match self.phase {
            Phase::Intro => push_intro_layers(actors, layout, self.elapsed),
            Phase::Logo => push_logo_layers(actors, layout, self.elapsed, 1.0, 0),
            _ => {}
        }
    }
}

impl AttractPresentation<Actor> for OpenItgIntro {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn record_limits(&self) -> (usize, usize) {
        (40, 15)
    }

    fn start_idle(&mut self, records: AttractRecords) {
        self.start_idle(records);
    }

    fn advance(&mut self, delta_time: f32) -> bool {
        self.advance(delta_time)
    }

    fn media(&self) -> AttractMedia {
        let media = self.media();
        AttractMedia { movie: media.movie, music: media.music, loop_music: media.loop_music }
    }

    fn playback_time(&self) -> f32 {
        self.playback_time()
    }

    fn demonstration(&self) -> Option<AttractDemoRequest> {
        let training = self.demonstration()? == OpenItgDemonstration::Training;
        Some(AttractDemoRequest {
            game: "dance",
            chart_type: "dance-single",
            max_difficulty: if training { 0 } else { 2 },
            force_uncentered_notefield: training,
            hide_gameplay_hud: training,
            two_players: false,
            play_music: true,
        })
    }

    fn prepare_demonstration(&mut self, chart: Option<AttractDemoChart<'_>>) {
        if self.demonstration() == Some(OpenItgDemonstration::Training) {
            self.training = chart.map(|chart| TrainingCues::new(chart.notes, chart.note_times, chart.timing));
        }
    }

    fn finish_demonstration(&mut self) {
        self.finish_demonstration();
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, video_key: Option<&Arc<str>>) {
        self.push_actors(actors, video_key);
    }

    fn push_demo_underlay(&self, actors: &mut Vec<Actor>, music_time: f32) {
        if self.demonstration() == Some(OpenItgDemonstration::Training) {
            let feedback = self.training.as_ref().map_or_else(Default::default, |cues| cues.sample(music_time));
            push_training_underlay(actors, feedback);
        }
    }

    fn push_demo_overlay(&self, actors: &mut Vec<Actor>, music_time: f32) {
        self.push_demonstration_overlay(actors, music_time);
    }
}

fn push_ranking_backdrop(actors: &mut Vec<Actor>, layout: Layout, elapsed: f32) {
    push_red_backdrop(actors, [screen_width(), screen_height()], elapsed, 0);
    for y in [RANKING_TOP, RANKING_BOTTOM] {
        actors.push(
            sprite(
                "openitg/attract-ranking-rule.png",
                layout.position(320.0, y),
                layout.size([626.0, 2.0]),
                [1.0; 4],
                2,
            )
            .build(0),
        );
    }
}

fn push_records(actors: &mut Vec<Actor>, layout: Layout, records: &[AttractRecord], elapsed: f32) {
    push_ranking_backdrop(actors, layout, elapsed);
    let offset = (elapsed / RECORD_PAGE_SECONDS) as usize * RECORDS_PER_PAGE;
    let page_elapsed = elapsed % RECORD_PAGE_SECONDS;
    push_record_label(
        actors,
        layout,
        [30.0, 42.0],
        0.0,
        "RECENT SCORES".into(),
        0.8,
        [1.0; 4],
        320.0,
    );
    if records.is_empty() {
        push_record_label(
            actors,
            layout,
            [320.0, 240.0],
            0.5,
            "NO SCORES".into(),
            0.65,
            [1.0; 4],
            180.0,
        );
    }
    for (index, record) in records
        .iter()
        .skip(offset)
        .take(RECORDS_PER_PAGE)
        .enumerate()
    {
        let y = 110.0 + 60.0 * index as f32;
        let (alpha, color) = (
            pulse(
                page_elapsed,
                0.2 + 0.1 * index as f32,
                0.3,
                8.8 - 0.1 * index as f32,
                0.3,
            ),
            crate::select_music::difficulty_style(&record.difficulty).2,
        );
        let white = [1.0, 1.0, 1.0, alpha];
        let difficulty = [color[0], color[1], color[2], alpha];
        push_record_label(
            actors,
            layout,
            [36.0, y],
            0.0,
            (&record.title).into(),
            0.75,
            white,
            298.0,
        );
        push_record_label(
            actors,
            layout,
            [372.0, y],
            0.5,
            (&record.difficulty).into(),
            0.55,
            difficulty,
            76.0,
        );
        push_record_label(
            actors,
            layout,
            [462.0, y],
            0.5,
            TextContent::inline_str(&record.initials).unwrap_or_else(|| "-----".into()),
            0.65,
            white,
            86.0,
        );
        if let Some(percent) = TextContent::inline_format(format_args!("{:.2}%", record.percent)) {
            push_record_label(actors, layout, [603.0, y], 1.0, percent, 0.75, white, 96.0);
        }
    }
}

fn course_scroll_duration(courses: &[AttractCourseRecord]) -> f32 {
    if courses.is_empty() {
        0.0
    } else {
        (courses.len() + RECORDS_PER_PAGE) as f32 * COURSE_SECONDS_PER_ROW
    }
}

fn push_course_records(
    actors: &mut Vec<Actor>,
    layout: Layout,
    title: &'static str,
    courses: &[AttractCourseRecord],
    elapsed: f32,
) {
    push_ranking_backdrop(actors, layout, elapsed);
    let mut heading = layout.text(
        [110.0, 30.0],
        0.0,
        title.into(),
        0.8,
        [1.0, 1.0, 1.0, ramp(elapsed, 0.0, 0.5)],
    );
    heading.font(MENU_HEADING_FONT);
    heading.shadowlength(4.0);
    heading.z(3);
    actors.push(heading.build(0));

    // The reference starts one row beyond the half-window mask.
    let center = elapsed / COURSE_SECONDS_PER_ROW - RECORDS_PER_PAGE as f32 / 2.0 - 1.0;
    let [x, y] = layout.position(0.0, RANKING_TOP);
    let [w, h] = layout.size([WIDTH, RANKING_BOTTOM - RANKING_TOP]);
    for (index, course) in courses.iter().enumerate() {
        let row_y = 240.0 + (index as f32 - center) * COURSE_ROW_SPACING;
        let top = row_y - 31.0;
        let bottom = row_y + 31.0;
        if bottom <= RANKING_TOP || top >= RANKING_BOTTOM {
            continue;
        }
        let mut frame = sprite(
            "openitg/attract-ranking-frame.png",
            layout.position(320.0, row_y),
            layout.size([620.0, 62.0]),
            [1.0; 4],
            2,
        );
        frame.croptop(((RANKING_TOP - top) / 62.0).max(0.0));
        frame.cropbottom(((bottom - RANKING_BOTTOM) / 62.0).max(0.0));
        actors.push(frame.build(0));

        let (name, percent) = course
            .best
            .as_ref()
            .map_or(("-----", 0.0), |(name, percent)| (name.as_str(), *percent));
        let first_label = actors.len();
        push_record_label(
            actors,
            layout,
            [110.0, row_y],
            0.5,
            (&course.title).into(),
            1.0,
            [1.0; 4],
            160.0,
        );
        push_record_label(
            actors,
            layout,
            [266.0, row_y - 9.0],
            0.5,
            TextContent::inline_str(name).unwrap_or_else(|| "-----".into()),
            0.7,
            [0.8, 0.8, 1.0, 1.0],
            100.0,
        );
        if let Some(score) = TextContent::inline_format(format_args!("{percent:.2}%")) {
            push_record_label(
                actors,
                layout,
                [266.0, row_y + 9.0],
                0.5,
                score,
                0.7,
                [0.8, 0.8, 1.0, 1.0],
                100.0,
            );
        }
        for actor in &mut actors[first_label..] {
            if let Actor::Text { clip, .. } = actor {
                *clip = Some([x, y, w, h]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_record_label(
    actors: &mut Vec<Actor>,
    layout: Layout,
    position: [f32; 2],
    align: f32,
    content: TextContent,
    zoom: f32,
    rgba: [f32; 4],
    width: f32,
) {
    let mut text = layout.text(position, align, content, zoom, rgba);
    text.font(MENU_NORMAL_FONT);
    text.maxwidth(layout.size([width, 0.0])[0]);
    text.shadowlength(3.0);
    text.z(3);
    actors.push(text.build(0));
}

fn ramp(elapsed: f32, start: f32, duration: f32) -> f32 {
    ((elapsed - start) / duration).clamp(0.0, 1.0)
}

fn pulse(elapsed: f32, start: f32, fade_in: f32, hold: f32, fade_out: f32) -> f32 {
    ramp(elapsed, start, fade_in) * (1.0 - ramp(elapsed, start + fade_in + hold, fade_out))
}

fn push_intro_layers(actors: &mut Vec<Actor>, layout: Layout, elapsed: f32) {
    if elapsed < 3.5 {
        let alpha = pulse(elapsed, 0.0, 1.0, 2.0, 0.3);
        let zoom = 3.0 - 1.9 * ramp(elapsed, 0.0, 1.0) - 0.1 * ramp(elapsed, 1.0, 2.0);
        let mut ring = sprite(
            "openitg/attract-intro-ring.png",
            layout.position(320.0, 240.0),
            layout.size([470.0 * zoom, 90.0 * zoom]),
            [1.0, 1.0, 1.0, alpha],
            1,
        );
        ring.rotationz(180.0 * (1.0 - ramp(elapsed, 0.0, 1.0)));
        ring.zoomy(-1.0);
        actors.push(ring.build(0));
        let alpha = pulse(elapsed, 0.5, 0.5, 2.0, 0.5);
        if alpha > 0.0 {
            push_caption(
                actors,
                layout,
                [320.0, 240.0],
                "Roxor Games presents",
                alpha,
                false,
            );
        }
    }

    for (start, label, key, size, y) in [
        (
            3.0,
            "In The Groove 2",
            "openitg/attract-intro-2.png",
            [342.0, 346.0],
            240.0,
        ),
        (
            11.0,
            "135 songs",
            "openitg/attract-intro-135.png",
            [494.0, 230.0],
            290.0,
        ),
        (
            15.0,
            "1,000 step charts",
            "openitg/attract-intro-1000.png",
            [558.0, 190.0],
            290.0,
        ),
        (
            19.0,
            "45 courses",
            "openitg/attract-intro-45.png",
            [412.0, 230.0],
            290.0,
        ),
        (
            23.0,
            "50 modifiers",
            "openitg/attract-intro-50.png",
            [418.0, 230.0],
            290.0,
        ),
    ] {
        let t = elapsed - start;
        if !(0.0..4.0).contains(&t) {
            continue;
        }
        let enter = 1.0 - (1.0 - ramp(t, 0.0, 0.5)).powi(2);
        let exit = ramp(t, 3.5, 0.5).powi(2);
        let x = 960.0 * (1.0 - enter) + 100.0 * ramp(t, 0.5, 3.0) - WIDTH * exit;
        let mut ribbon = deadlib_present::dsl::SpriteBuilder::solid();
        let [x, y_pos] = layout.position(x, 110.0);
        let [w, h] = layout.size([WIDTH, 60.0]);
        ribbon.xy(x, y_pos);
        ribbon.size(w, h);
        ribbon.diffuse([1.0, 1.0, 1.0, 0.5]);
        ribbon.fadeleft(0.3);
        ribbon.faderight(0.3);
        ribbon.z(1);
        actors.push(ribbon.build(0));
        let alpha = pulse(t, 0.5, 0.3, 3.0, 0.3);
        if alpha > 0.0 {
            push_caption(actors, layout, [200.0, 110.0], label, alpha, true);
        }
        let alpha = pulse(t, 0.0, 0.5, 3.0, 0.5) * 0.5;
        actors.push(
            sprite(
                key,
                layout.position(WIDTH - size[0] * 0.5 + 30.0 * (t / 4.0), y),
                layout.size(size),
                [1.0, 1.0, 1.0, alpha],
                2,
            )
            .build(0),
        );
    }
}

fn push_caption(
    actors: &mut Vec<Actor>,
    layout: Layout,
    position: [f32; 2],
    caption: &'static str,
    alpha: f32,
    skewed: bool,
) {
    let mut text = layout.text(
        position,
        0.5,
        TextContent::from(caption),
        1.0,
        [1.0, 1.0, 1.0, alpha],
    );
    text.font(MENU_NORMAL_FONT);
    text.shadowlength(4.0);
    text.z(3);
    let actor = text.build(0);
    actors.push(if skewed { italic(actor, -0.2) } else { actor });
}

pub(crate) fn push_logo_layers(
    actors: &mut Vec<Actor>,
    layout: Layout,
    elapsed: f32,
    opacity: f32,
    depth: i16,
) {
    let company_alpha = opacity * ramp(elapsed, 0.5, 0.5);
    if company_alpha > 0.0 {
        let x = layout.size([90.0, 0.0])[0];
        actors.push(
            sprite(
                "openitg/attract-logo-roxor.png",
                [x, layout.position(0.0, 30.0)[1]],
                layout.size([146.0, 36.0]),
                [1.0, 1.0, 1.0, company_alpha],
                depth + 2,
            )
            .build(0),
        );
    }

    let band_progress = ramp(elapsed, 0.6, 0.3);
    if band_progress > 0.0 {
        let mut band = sprite(
            "openitg/attract-logo-otherred.png",
            layout.position(415.0, 270.0),
            layout.size([430.0, 76.8]),
            [1.0, 1.0, 1.0, opacity],
            depth + 1,
        );
        band.cropright(1.0 - band_progress);
        band.faderight(0.9);
        actors.push(band.build(0));
    }

    let remaining = (1.0 - ramp(elapsed, 0.2, 0.5)).powi(2);
    let [x, y] = layout.position(130.0, 215.0);
    let mut arrow = sprite(
        "openitg/attract-logo-arrow.png",
        [
            x + screen_width() * 0.5 * remaining,
            y + screen_height() / 1.2 * remaining,
        ],
        layout.size([240.0, 241.6]),
        [1.0, 1.0, 1.0, opacity],
        depth + 2,
    );
    arrow.glow([1.0, 1.0, 1.0, opacity * remaining]);
    actors.push(arrow.build(0));

    let reveal = ramp(elapsed, 0.7, 0.3);
    if reveal > 0.0 {
        let glow = 0.8 * (1.0 - ramp(elapsed, 1.03, 0.5));
        for (key, size, alpha) in [
            ("openitg/attract-logo-oitg.png", [340.8, 52.8], 1.0),
            ("openitg/attract-logo-oitgglow.png", [352.0, 64.0], glow),
        ] {
            let mut logo = sprite(
                key,
                layout.position(410.0, 270.0),
                layout.size(size),
                [1.0, 1.0, 1.0, opacity * alpha],
                depth + 3,
            );
            logo.cropright(1.0 - reveal);
            logo.faderight(0.1 * (1.0 - ramp(elapsed, 1.0, 0.03)));
            actors.push(logo.build(0));
        }
    }

    let flash = opacity * (1.0 - ramp(elapsed, 0.1, 0.5).powi(2));
    if flash > 0.0 {
        actors.push(quad(
            [0.0, 0.0],
            [screen_width(), screen_height()],
            [1.0, 1.0, 1.0, flash],
            depth + 5,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intro_advances_through_the_media_at_the_reference_times() {
        let mut intro = OpenItgIntro::default();
        assert_eq!(
            intro.media().movie,
            Some("assets/media/openitg/company.mpg")
        );
        assert!(!intro.advance(6.5));
        assert_eq!(intro.media().movie, Some("assets/media/openitg/intro.mpg"));
        assert_eq!(intro.playback_time(), 0.5);
        let mut actors = Vec::new();
        intro.push_actors(&mut actors, None);
        assert!(actors.len() > 1, "intro should add overlays over the poster");
        assert!(!intro.advance(31.0));
        assert_eq!(intro.media().movie, Some("assets/media/openitg/logo.mpg"));
        assert_eq!(intro.playback_time(), 2.0);
        actors.clear();
        intro.push_actors(&mut actors, None);
        assert!(actors.len() > 1, "logo should add overlays over the poster");
        assert!(intro.advance(5.0));
    }

    #[test]
    fn idle_records_use_the_ranking_music_then_return_to_attract() {
        let mut intro = OpenItgIntro::default();
        intro.start_idle(AttractRecords::default());
        assert_eq!(intro.demonstration(), Some(OpenItgDemonstration::Autoplay));
        assert!(!intro.advance(29.9));
        assert_eq!(intro.demonstration(), Some(OpenItgDemonstration::Autoplay));
        assert!(!intro.advance(0.1));
        assert_eq!(intro.demonstration(), Some(OpenItgDemonstration::Training));
        assert!(!intro.advance(41.0));
        assert_eq!(intro.demonstration(), None);
        assert!(intro.media().loop_music);
        assert_eq!(
            intro.media().music,
            Some("assets/media/openitg/ranking.ogg")
        );
        assert!(!intro.advance(10.0));
        assert_eq!(
            intro.media().movie,
            Some("assets/media/openitg/company.mpg")
        );
    }

    #[test]
    fn idle_records_render_the_supplied_chart_and_score() {
        use deadlib_present::space::{Metrics, with_current_metrics};
        let mut intro = OpenItgIntro::default();
        intro.start_idle(AttractRecords {
            songs: vec![AttractRecord {
                title: Arc::from("Quartz"),
                chart_type: Arc::from("dance-single"),
                difficulty: Arc::from("Hard"),
                initials: String::from("ABC"),
                percent: 98.76,
                leaderboard: Vec::new(),
            }],
            ..AttractRecords::default()
        });
        intro.finish_demonstration();
        intro.finish_demonstration();
        assert!(!intro.advance(1.0));
        let mut actors = Vec::new();
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            intro.push_actors(&mut actors, None)
        });
        for expected in ["Quartz", "Hard", "ABC", "98.76%"] {
            assert!(actors.iter().any(
                |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == expected)
            ));
        }
    }

    #[test]
    fn idle_course_pages_scroll_in_order_without_switching_music() {
        use deadlib_present::space::{Metrics, with_current_metrics};
        let mut intro = OpenItgIntro::default();
        intro.start_idle(AttractRecords {
            marathon: vec![AttractCourseRecord {
                title: Arc::from("Quartz Marathon"),
                best: Some(("ABC".into(), 91.5)),
            }],
            survival: vec![AttractCourseRecord {
                title: Arc::from("Quartz Survival"),
                best: None,
            }],
            ..AttractRecords::default()
        });
        intro.finish_demonstration();
        intro.finish_demonstration();
        let ranking_media = intro.media();
        let mut actors = Vec::new();
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            for (delta, name, value) in [
                (11.0, "Quartz Marathon", "91.50%"),
                (2.4, "Quartz Survival", "0.00%"),
            ] {
                assert!(!intro.advance(delta));
                assert!(intro.media() == ranking_media);
                actors.clear();
                intro.push_actors(&mut actors, None);
                for label in [name, value] {
                    assert!(actors.iter().any(|actor| matches!(
                        actor, Actor::Text { content, clip: Some(_), .. } if content.as_str() == label
                    )));
                }
            }
        });
        assert!(!intro.advance(2.0));
        assert_eq!(
            intro.media().movie,
            Some("assets/media/openitg/company.mpg")
        );
    }
}
