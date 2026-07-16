use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile::PlayMode;
use deadsync_profile::compat as profile;
use deadsync_simfile::runtime_cache::{get_course_cache, get_song_cache};
use deadsync_theme_simply_love::screens::{select_course, select_music};
use deadsync_theme_simply_love::views::{
    SelectCourseContextView, SelectCourseInitView, SelectCoursePolicyView, SelectMusicInitView,
};
use log::warn;
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Clone)]
enum PrepareRequest {
    Music(SelectMusicInitView),
    Course,
}

pub(crate) enum PreparedState {
    Music(select_music::State),
    Course(select_course::State),
}

/// Shell-owned worker for the expensive screen preparation behind Profile Load.
#[derive(Default)]
pub(crate) struct Service {
    rx: Option<mpsc::Receiver<PreparedState>>,
    fallback: Option<PrepareRequest>,
}

impl Service {
    pub(crate) fn start(&mut self, play_mode: PlayMode, select_music: SelectMusicInitView) {
        let request = match play_mode {
            PlayMode::Regular => PrepareRequest::Music(select_music),
            PlayMode::Marathon => PrepareRequest::Course,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.fallback = Some(request.clone());
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(prepare(request));
        });
    }

    pub(crate) fn poll(&mut self) -> Option<PreparedState> {
        let result = self.rx.as_ref()?.try_recv();
        match result {
            Ok(prepared) => {
                self.rx = None;
                self.fallback = None;
                Some(prepared)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                let request = self.fallback.take()?;
                warn!("Profile Load worker disconnected; preparing synchronously.");
                Some(prepare(request))
            }
        }
    }
}

pub(crate) fn select_course_context_view(config: &config::Config) -> SelectCourseContextView {
    let session = profile::get_session_snapshot();
    SelectCourseContextView {
        policy: SelectCoursePolicyView {
            show_random_courses: config.show_random_courses,
            show_most_played_courses: config.show_most_played_courses,
            music_wheel_switch_speed: config.music_wheel_switch_speed,
            global_offset_seconds: config.global_offset_seconds,
            dedicated_three_key_nav: config.three_key_navigation
                && config.only_dedicated_menu_buttons,
        },
        play_style: session.play_style,
        player_side: session.player_side,
        music_rate: session.music_rate,
    }
}

pub(crate) fn select_course_init_view() -> SelectCourseInitView {
    let config = config::get();
    let context = select_course_context_view(&config);
    let translated_titles = config.translated_titles;
    let last_course = profile::get()
        .last_played_course(context.play_style)
        .clone();
    let song_packs = get_song_cache().clone();
    let courses = get_course_cache().clone();
    SelectCourseInitView {
        song_packs,
        courses,
        played_chart_counts: scores::played_chart_counts_for_machine(),
        translated_titles,
        last_course_path: last_course.course_path.map(PathBuf::from),
        last_course_difficulty: last_course.difficulty_name,
        context,
    }
}

fn prepare(request: PrepareRequest) -> PreparedState {
    match request {
        PrepareRequest::Music(init) => {
            scores::prewarm_select_music_score_caches();
            let init = crate::select_music::prepare_init_view(init);
            PreparedState::Music(select_music::init(init))
        }
        PrepareRequest::Course => {
            PreparedState::Course(select_course::init(select_course_init_view()))
        }
    }
}
