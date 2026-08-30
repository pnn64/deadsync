use super::*;

use crate::views::SimplyLoveApplyReplayGainEvent;

/// Modal overlay state for the Sound options "Apply `ReplayGain`" action. Drives
/// a full-library EBU R128 loudness analysis with a progress bar, ETA readout,
/// and cancellation.
pub(super) struct ApplyReplayGainUiState {
    pub(super) total: usize,
    pub(super) done: usize,
    /// Smoothed `done` used for the bar fill and speed readout so progress
    /// eases instead of jumping when songs land in bursts across threads.
    pub(super) displayed_done: f32,
    pub(super) line2: String,
    pub(super) line3: String,
    /// Set once the worker reports a terminal event; the overlay then waits for
    /// the user to dismiss it.
    pub(super) finished: bool,
    pub(super) cancelled: bool,
    /// Set when the user asks to cancel but the worker hasn't confirmed yet.
    pub(super) cancel_requested: bool,
    pub(super) started_at: Instant,
    /// Game-thread-owned, one-entry result-tree cache. The first finished
    /// frame warms it; worker/progress mutations, color, or viewport changes
    /// replace it, and dismissal drops it. Its fixed bound needs no eviction
    /// policy or instrumentation.
    presentation_revision: u64,
    presentation: RefCell<Option<ReplayGainPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReplayGainPresentationKey {
    revision: u64,
    active_color_index: i32,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

struct ReplayGainPresentation {
    key: ReplayGainPresentationKey,
    children: Arc<[Actor]>,
}

impl ApplyReplayGainUiState {
    pub(super) fn new() -> Self {
        Self {
            total: 0,
            done: 0,
            displayed_done: 0.0,
            line2: String::new(),
            line3: tr("OptionsSound", "ApplyReplayGainPreparing").to_string(),
            finished: false,
            cancelled: false,
            cancel_requested: false,
            started_at: Instant::now(),
            presentation_revision: 0,
            presentation: RefCell::new(None),
        }
    }

    #[inline(always)]
    fn invalidate_presentation(&mut self) {
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        self.presentation.get_mut().take();
    }
}

/// Time constant (seconds) for the exponential ease applied to `displayed_done`.
const APPLY_REPLAYGAIN_PROGRESS_TAU: f32 = 0.4;

/// Start the bulk analysis: show the overlay and ask the shell to spawn the
/// worker. No-op when an overlay is already up.
pub(super) fn begin_apply_replaygain(state: &mut State) -> ThemeEffect {
    if state.apply_replaygain_ui.is_some() {
        return ThemeEffect::None;
    }
    clear_navigation_holds(state);
    state.apply_replaygain_ui = Some(ApplyReplayGainUiState::new());
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
        crate::SimplyLoveContentRequest::ApplyReplayGain,
    ))
}

pub(super) fn apply_apply_replaygain_event(
    state: &mut State,
    event: SimplyLoveApplyReplayGainEvent,
) {
    let Some(ui) = state.apply_replaygain_ui.as_mut() else {
        return;
    };
    ui.invalidate_presentation();
    match event {
        SimplyLoveApplyReplayGainEvent::Started { total } => {
            ui.total = total;
            if total == 0 {
                ui.line3 = tr("OptionsSound", "ApplyReplayGainNoSongs").to_string();
            }
        }
        SimplyLoveApplyReplayGainEvent::Progress {
            done,
            total,
            line2,
            line3,
        } => {
            ui.done = done;
            ui.total = total;
            ui.line2 = line2;
            ui.line3 = line3;
        }
        SimplyLoveApplyReplayGainEvent::Finished {
            done,
            total,
            cancelled,
        } => {
            if total > 0 {
                ui.total = total;
            }
            ui.done = done.max(ui.done);
            ui.displayed_done = ui.done as f32;
            ui.finished = true;
            ui.cancelled = cancelled;
            ui.line2.clear();
            ui.line3.clear();
        }
    }
}

pub(super) fn update_apply_replaygain_ui(ui: &mut ApplyReplayGainUiState, dt: f32) {
    let previous_displayed_done = ui.displayed_done.to_bits();
    let target = ui.done as f32;
    if ui.finished {
        ui.displayed_done = target;
        if ui.displayed_done.to_bits() != previous_displayed_done {
            ui.invalidate_presentation();
        }
        return;
    }
    let dt = dt.max(0.0);
    let alpha = 1.0 - (-dt / APPLY_REPLAYGAIN_PROGRESS_TAU).exp();
    ui.displayed_done =
        (target - ui.displayed_done).mul_add(alpha.clamp(0.0, 1.0), ui.displayed_done);
    if ui.displayed_done.to_bits() != previous_displayed_done {
        ui.invalidate_presentation();
    }
}

#[inline(always)]
fn apply_replaygain_progress(ui: &ApplyReplayGainUiState) -> (usize, usize, f32) {
    let done = ui.done;
    let mut total = ui.total;
    if total < done {
        total = done;
    }
    let smoothed = ui.displayed_done.clamp(0.0, total.max(done) as f32);
    let mut progress = if total > 0 {
        (smoothed / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !ui.finished && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

pub(super) fn push_apply_replaygain_overlay_actors(
    out: &mut Vec<Actor>,
    ui: &ApplyReplayGainUiState,
    active_color_index: i32,
) {
    // Running progress has elapsed-time speed/ETA text. The finished result is
    // stable while it waits for dismissal, so retain only that immutable tree.
    if !ui.finished {
        push_apply_replaygain_overlay_actors_unreserved(out, ui, active_color_index);
        return;
    }
    let key = ReplayGainPresentationKey {
        revision: ui.presentation_revision,
        active_color_index,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = ui
        .presentation
        .borrow()
        .as_ref()
        .filter(|presentation| presentation.key == key)
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(8);
        push_apply_replaygain_overlay_actors_unreserved(&mut children, ui, active_color_index);
        let children = Arc::<[Actor]>::from(children);
        *ui.presentation.borrow_mut() = Some(ReplayGainPresentation {
            key,
            children: Arc::clone(&children),
        });
        children
    });
    crate::screens::components::select_music::push_retained_overlay(out, children);
}

fn push_apply_replaygain_overlay_actors_unreserved(
    out: &mut Vec<Actor>,
    ui: &ApplyReplayGainUiState,
    active_color_index: i32,
) {
    let (done, total, progress) = apply_replaygain_progress(ui);
    let elapsed = ui.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0 || done > 0;
    let speed_text = if show_speed_row {
        let rate = if ui.finished {
            0.0
        } else if elapsed > 0.0 {
            ui.displayed_done.max(0.0) / elapsed
        } else {
            0.0
        };
        let mut text = tr_fmt(
            "SelectMusic",
            "LoadingSpeed",
            &[("speed", &format!("{rate:.0}"))],
        )
        .to_string();
        if !ui.finished && total > 0 && rate > 0.0 {
            let remaining = total.saturating_sub(done) as f32;
            if remaining > 0.0 {
                let eta_secs = (remaining / rate).round() as u64;
                text = format!(
                    "{text}  \u{2022}  {}",
                    tr_fmt(
                        "OptionsScoreImport",
                        "ImportEta",
                        &[("eta", &format_eta(eta_secs))],
                    ),
                );
            }
        }
        text
    } else {
        String::new()
    };

    let header = if ui.finished {
        if ui.cancelled {
            tr("OptionsSound", "ApplyReplayGainCancelled")
        } else {
            tr("OptionsSound", "ApplyReplayGainComplete")
        }
    } else if ui.cancel_requested {
        tr("OptionsSound", "ApplyReplayGainCancelling")
    } else {
        tr("OptionsSound", "ApplyReplayGainRunning")
    };

    let fill = color::decorative_rgba(active_color_index);
    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_width() * 0.5;
    let bar_cy = screen_height().mul_add(0.5, 34.0);
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    out.reserve(8);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));
    out.push(act!(text:
        font("miso"):
        settext(header):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));
    if !ui.line2.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(ui.line2.clone()):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 74.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }
    if !ui.line3.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(ui.line3.clone()):
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

    if show_speed_row && !speed_text.is_empty() {
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

    let footer = if ui.finished {
        tr("OptionsScoreImport", "PressStartToDismiss")
    } else {
        tr("OptionsSound", "ApplyReplayGainCancelHint")
    };
    out.push(act!(text:
        font("miso"):
        settext(footer):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy + 66.0):
        zoom(0.9):
        horizalign(center):
        z(301)
    ));
}

#[cfg(any(test, feature = "bench-support"))]
fn replaygain_overlay_checksum(actors: &[Actor]) -> u64 {
    let semantic_actors = match actors {
        [Actor::SharedFrame { children, .. }] => children.as_ref(),
        _ => actors,
    };
    semantic_actors
        .iter()
        .fold(semantic_actors.len() as u64, |checksum, actor| {
            let value = match actor {
                Actor::Text { content, z, .. } => content
                    .as_str()
                    .bytes()
                    .fold(u64::from(*z as u16), |hash, byte| {
                        hash.rotate_left(7) ^ u64::from(byte)
                    }),
                Actor::Frame { children, z, .. } => {
                    replaygain_overlay_checksum(children) ^ u64::from(*z as u16)
                }
                Actor::SharedFrame { children, z, .. } => {
                    replaygain_overlay_checksum(children) ^ u64::from(*z as u16)
                }
                _ => 1,
            };
            checksum.rotate_left(11) ^ value
        })
}

/// Stable completed-analysis frame used to compare the former per-frame
/// builder with the retained result tree.
#[cfg(any(test, feature = "bench-support"))]
pub struct ReplayGainOverlayBenchmark {
    ui: ApplyReplayGainUiState,
}

#[cfg(any(test, feature = "bench-support"))]
impl ReplayGainOverlayBenchmark {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ui: ApplyReplayGainUiState {
                total: 512,
                done: 512,
                displayed_done: 512.0,
                line2: String::new(),
                line3: String::new(),
                finished: true,
                cancelled: false,
                cancel_requested: false,
                started_at: Instant::now(),
                presentation_revision: 0,
                presentation: RefCell::new(None),
            },
        }
    }

    #[must_use]
    pub fn actor_count(&self) -> usize {
        let mut actors = Vec::with_capacity(8);
        push_apply_replaygain_overlay_actors_unreserved(&mut actors, &self.ui, 2);
        actors.len()
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_apply_replaygain_overlay_actors_unreserved(out, &self.ui, 2);
        std::hint::black_box(&*out);
        replaygain_overlay_checksum(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_apply_replaygain_overlay_actors(out, &self.ui, 2);
        std::hint::black_box(&*out);
        replaygain_overlay_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for ReplayGainOverlayBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod overlay_staging_tests {
    use super::*;

    #[test]
    fn retained_replaygain_result_matches_builder_and_reuses_tree() {
        crate::assets::i18n::init_for_tests();
        let benchmark = ReplayGainOverlayBenchmark::new();
        let mut legacy = Vec::with_capacity(8);
        let mut direct = Vec::with_capacity(8);

        assert_eq!(
            benchmark.legacy_frame(&mut legacy),
            benchmark.direct_frame(&mut direct)
        );
        assert_eq!(legacy.len(), benchmark.actor_count());
        let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
            panic!("expected a retained ReplayGain result tree");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));
        let children = Arc::clone(children);
        let _ = benchmark.direct_frame(&mut direct);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = direct.as_slice()
        else {
            panic!("expected a retained ReplayGain result tree");
        };
        assert!(Arc::ptr_eq(&children, repeated));
    }

    #[test]
    fn replaygain_result_mutation_invalidates_retained_tree() {
        crate::assets::i18n::init_for_tests();
        let mut benchmark = ReplayGainOverlayBenchmark::new();
        let mut actors = Vec::with_capacity(8);
        let _ = benchmark.direct_frame(&mut actors);
        let old_checksum = replaygain_overlay_checksum(&actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected a retained ReplayGain result tree");
        };
        let old = Arc::clone(children);

        benchmark.ui.cancelled = true;
        benchmark.ui.invalidate_presentation();
        let _ = benchmark.direct_frame(&mut actors);
        let [Actor::SharedFrame { children, .. }] = actors.as_slice() else {
            panic!("expected a retained ReplayGain result tree");
        };
        assert!(!Arc::ptr_eq(&old, children));
        assert_ne!(old_checksum, replaygain_overlay_checksum(&actors));
    }
}
