use deadsync_online::score_compat as scores;
use deadsync_profile::PlayMode;
use deadsync_theme_simply_love::screens::{select_course, select_music};
use deadsync_theme_simply_love::views::{SelectCourseInitView, SelectMusicInitView};
use log::warn;
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

pub(crate) fn select_course_init_view() -> SelectCourseInitView {
    SelectCourseInitView {
        played_chart_counts: scores::played_chart_counts_for_machine(),
    }
}

fn prepare(request: PrepareRequest) -> PreparedState {
    match request {
        PrepareRequest::Music(init) => {
            scores::prewarm_select_music_score_caches();
            PreparedState::Music(select_music::init(init))
        }
        PrepareRequest::Course => {
            PreparedState::Course(select_course::init(select_course_init_view()))
        }
    }
}
