use super::{gameplay, sample_state, Result};
use deadlib_present::space::{self, Metrics};
use deadsync_chart::{ChartData, GameplayChartData, SongData};
use std::{
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) struct CaptureSpec {
    pub(super) layout: &'static str,
    pub(super) show_scorebox: bool,
    pub(super) center_single_notefield: bool,
    pub(super) path: std::path::PathBuf,
}

pub(super) struct PreparedCaptureState {
    pub(super) state: gameplay::State,
    pub(super) elapsed: Duration,
}

#[derive(Clone, Copy)]
pub(super) enum ProfilePhase {
    StateBatchWall,
    StateSetup,
    StateInit,
    StateData,
    TextureKeys,
    TextureUpload,
    Compose,
}

#[derive(Default)]
pub(super) struct ExportProfiling {
    pub(super) report_enabled: bool,
    state_batch_wall: AtomicU64,
    state_setup: AtomicU64,
    state_init: AtomicU64,
    state_data: AtomicU64,
    texture_keys: AtomicU64,
    texture_upload: AtomicU64,
    compose: AtomicU64,
}

impl ExportProfiling {
    pub(super) fn measure<T>(&self, phase: ProfilePhase, operation: impl FnOnce() -> T) -> T {
        let _scope = self.scope(phase);
        operation()
    }

    pub(super) fn scope(&self, phase: ProfilePhase) -> ProfileScope<'_> {
        ProfileScope {
            profiling: self,
            phase,
            started: Instant::now(),
        }
    }

    fn record(&self, phase: ProfilePhase, elapsed: Duration) {
        let counter = match phase {
            ProfilePhase::StateBatchWall => &self.state_batch_wall,
            ProfilePhase::StateSetup => &self.state_setup,
            ProfilePhase::StateInit => &self.state_init,
            ProfilePhase::StateData => &self.state_data,
            ProfilePhase::TextureKeys => &self.texture_keys,
            ProfilePhase::TextureUpload => &self.texture_upload,
            ProfilePhase::Compose => &self.compose,
        };
        let nanos = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        counter.fetch_add(nanos, Ordering::Relaxed);
    }

    fn seconds(&self, phase: ProfilePhase) -> f64 {
        let counter = match phase {
            ProfilePhase::StateBatchWall => &self.state_batch_wall,
            ProfilePhase::StateSetup => &self.state_setup,
            ProfilePhase::StateInit => &self.state_init,
            ProfilePhase::StateData => &self.state_data,
            ProfilePhase::TextureKeys => &self.texture_keys,
            ProfilePhase::TextureUpload => &self.texture_upload,
            ProfilePhase::Compose => &self.compose,
        };
        Duration::from_nanos(counter.load(Ordering::Relaxed)).as_secs_f64()
    }

    pub(super) fn report(&self, resolution: &str, workers: usize) {
        if !self.report_enabled {
            return;
        }
        eprintln!(
            concat!(
                "gameplay_prepare_parts resolution={} workers={} ",
                "state_batch_wall={:.3}s state_setup={:.3}s state_init={:.3}s ",
                "state_data={:.3}s texture_keys={:.3}s texture_upload={:.3}s ",
                "compose={:.3}s"
            ),
            resolution,
            workers,
            self.seconds(ProfilePhase::StateBatchWall),
            self.seconds(ProfilePhase::StateSetup),
            self.seconds(ProfilePhase::StateInit),
            self.seconds(ProfilePhase::StateData),
            self.seconds(ProfilePhase::TextureKeys),
            self.seconds(ProfilePhase::TextureUpload),
            self.seconds(ProfilePhase::Compose),
        );
    }
}

pub(super) struct ProfileScope<'a> {
    profiling: &'a ExportProfiling,
    phase: ProfilePhase,
    started: Instant,
}

impl Drop for ProfileScope<'_> {
    fn drop(&mut self) {
        self.profiling.record(self.phase, self.started.elapsed());
    }
}

pub(super) fn prepare_batch(
    captures: &[CaptureSpec],
    song: &Arc<SongData>,
    charts: &[Arc<ChartData>; 2],
    gameplay_charts: &[Arc<GameplayChartData>; 2],
    logical_width: f32,
    logical_height: f32,
    reverse: bool,
    metrics: Metrics,
    window_width: u32,
    window_height: u32,
    profiling: &ExportProfiling,
) -> Result<Vec<PreparedCaptureState>> {
    std::thread::scope(|scope| -> Result<_> {
        let mut handles = Vec::with_capacity(captures.len());
        for capture in captures {
            let worker_song = Arc::clone(song);
            let worker_charts = charts.clone();
            let worker_gameplay_charts = gameplay_charts.clone();
            handles.push(
                std::thread::Builder::new()
                    .name("gameplay-state-preparation".into())
                    .stack_size(32 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        space::set_current_metrics(metrics);
                        space::set_current_window_px(window_width, window_height);
                        let started = Instant::now();
                        let state = sample_state(
                            worker_song,
                            &worker_charts,
                            &worker_gameplay_charts,
                            logical_width,
                            logical_height,
                            capture.layout,
                            reverse,
                            capture.show_scorebox,
                            capture.center_single_notefield,
                            profiling,
                        )
                        .map_err(|error| error.to_string())?;
                        Ok::<_, String>(PreparedCaptureState {
                            state,
                            elapsed: started.elapsed(),
                        })
                    })?,
            );
        }

        let mut prepared = Vec::with_capacity(handles.len());
        let mut worker_error = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(state)) => prepared.push(state),
                Ok(Err(error)) => {
                    worker_error.get_or_insert_with(|| io::Error::other(error));
                }
                Err(_) => {
                    worker_error.get_or_insert_with(|| {
                        io::Error::other("gameplay preparation worker panicked")
                    });
                }
            }
        }
        if let Some(error) = worker_error {
            return Err(error.into());
        }
        Ok(prepared)
    })
}
