use super::*;

pub(super) struct ReloadUiState {
    pub(super) phase: crate::views::SimplyLoveContentReloadPhase,
    pub(super) line2: String,
    pub(super) line3: String,
    pub(super) songs_done: usize,
    pub(super) songs_total: usize,
    pub(super) courses_done: usize,
    pub(super) courses_total: usize,
    pub(super) done: bool,
    pub(super) started_at: Instant,
}

impl ReloadUiState {
    pub(super) fn new() -> Self {
        Self {
            phase: crate::views::SimplyLoveContentReloadPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            songs_done: 0,
            songs_total: 0,
            courses_done: 0,
            courses_total: 0,
            done: false,
            started_at: Instant::now(),
        }
    }
}

pub(super) fn start_reload_songs_and_courses(state: &mut State) -> ThemeEffect {
    if state.reload_ui.is_some() {
        return ThemeEffect::None;
    }

    // Clear navigation holds so the menu can't "run away" after reload finishes.
    clear_navigation_holds(state);

    state.reload_ui = Some(ReloadUiState::new());
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
        crate::SimplyLoveContentRequest::ReloadLibrary {
            songs_root: state.app_paths.songs.path.clone(),
            courses_root: state.app_paths.courses.path.clone(),
        },
    ))
}

pub(super) fn start_reload_song_dirs(state: &mut State, pack_dirs: Vec<PathBuf>) -> ThemeEffect {
    if state.reload_ui.is_some() || pack_dirs.is_empty() {
        return ThemeEffect::None;
    }

    clear_navigation_holds(state);
    state.reload_ui = Some(ReloadUiState::new());
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
        crate::SimplyLoveContentRequest::ReloadSongDirs {
            songs_root: state.app_paths.songs.path.clone(),
            pack_dirs,
        },
    ))
}

pub fn sync_reload_events(
    state: &mut State,
    events: Vec<crate::views::SimplyLoveContentReloadEvent>,
) {
    let Some(reload) = state.reload_ui.as_mut() else {
        return;
    };
    for event in events {
        match event {
            crate::views::SimplyLoveContentReloadEvent::Phase(phase) => {
                reload.phase = phase;
                reload.line2.clear();
                reload.line3.clear();
            }
            crate::views::SimplyLoveContentReloadEvent::Song {
                done,
                total,
                pack,
                song,
            } => {
                reload.phase = crate::views::SimplyLoveContentReloadPhase::Songs;
                reload.songs_done = done;
                reload.songs_total = total;
                reload.line2 = pack;
                reload.line3 = song;
            }
            crate::views::SimplyLoveContentReloadEvent::Course {
                done,
                total,
                group,
                course,
            } => {
                reload.phase = crate::views::SimplyLoveContentReloadPhase::Courses;
                reload.courses_done = done;
                reload.courses_total = total;
                reload.line2 = group;
                reload.line3 = course;
            }
            crate::views::SimplyLoveContentReloadEvent::Artwork { .. }
            | crate::views::SimplyLoveContentReloadEvent::Noteskins { .. } => {}
            crate::views::SimplyLoveContentReloadEvent::Finished { .. } => {
                reload.done = true;
            }
        }
    }
}

#[inline(always)]
pub(super) fn reload_progress(reload: &ReloadUiState) -> (usize, usize, f32) {
    let done = reload.songs_done.saturating_add(reload.courses_done);
    let mut total = reload.songs_total.saturating_add(reload.courses_total);
    if total < done {
        total = done;
    }
    let mut progress = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !reload.done && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

pub(super) fn reload_detail_lines(reload: &ReloadUiState) -> (String, String) {
    (reload.line2.clone(), reload.line3.clone())
}

pub(super) fn build_reload_overlay_actors(
    reload: &ReloadUiState,
    active_color_index: i32,
) -> Vec<Actor> {
    let (done, total, progress) = reload_progress(reload);
    let elapsed = reload.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0;
    let speed_text = if elapsed > 0.0 && show_speed_row {
        tr_fmt(
            "SelectMusic",
            "LoadingSpeed",
            &[("speed", &format!("{:.1}", done as f32 / elapsed))],
        )
        .to_string()
    } else if show_speed_row {
        tr_fmt("SelectMusic", "LoadingSpeed", &[("speed", "0.0")]).to_string()
    } else {
        String::new()
    };
    let (line2, line3) = reload_detail_lines(reload);
    let fill = color::decorative_rgba(active_color_index);

    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_width() * 0.5;
    let bar_cy = screen_height() * 0.5 + 34.0;
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    let mut out: Vec<Actor> = Vec::with_capacity(7);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));
    let phase_label = match reload.phase {
        crate::views::SimplyLoveContentReloadPhase::Songs => tr("Init", "LoadingSongsText"),
        crate::views::SimplyLoveContentReloadPhase::Courses => tr("Init", "LoadingCoursesText"),
        crate::views::SimplyLoveContentReloadPhase::Artwork => tr("Init", "CachingArtworkText"),
        crate::views::SimplyLoveContentReloadPhase::Noteskins => {
            tr("Init", "CompilingNoteskinsText")
        }
    };
    out.push(act!(text:
        font("miso"):
        settext(if total == 0 { tr("Init", "InitializingText") } else { phase_label }):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));
    if !line2.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(line2):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 74.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }
    if !line3.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(line3):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 50.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    out.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [actors::SizeSpec::Px(bar_w), actors::SizeSpec::Px(bar_h)],
        background: None,
        z: 301,
        children: bar_children,
    });

    if show_speed_row {
        out.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(301)
        ));
    }
    out
}
