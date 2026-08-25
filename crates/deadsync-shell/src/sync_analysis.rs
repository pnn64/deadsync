use crate::sync_analysis_cache::{
    AnalysisOptions, Cache as AnalysisCache, CachedAnalysis, CachedPlot, CompletedTarget,
};
use deadsync_audio_decode as decode;
use deadsync_chart::SongData;
use deadsync_config::prelude as config;
use deadsync_simfile::app_runtime as song_loading;
use deadsync_theme_simply_love::{
    SimplyLoveSyncEvent, SimplyLoveSyncKernel, SimplyLoveSyncKernelTarget, SimplyLoveSyncOwner,
    SimplyLoveSyncPlotView, SimplyLoveSyncResult, SimplyLoveSyncSongResult,
    SimplyLoveSyncStreamEvent, SimplyLoveSyncTarget,
};
use null_or_die::{
    BiasCfg, BiasEstimateWithPlot, BiasKernel, BiasRuntime, BiasStreamCfg, BiasStreamEvent,
    KernelTarget, estimate_bias_with_beat_fn_stream_reuse,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PCM_INV_SCALE: f32 = 1.0 / 32768.0;
const PROGRESS_STEP_BEATS: usize = 4;
const SONG_PENDING_EVENTS: usize = 32;
const MAX_EVENTS_PER_FRAME: usize = 64;
const POLL_BUDGET: Duration = Duration::from_millis(3);

struct SyncAudio {
    sample_rate_hz: u32,
    mono: Vec<f32>,
}

struct Job {
    owner: SimplyLoveSyncOwner,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<SimplyLoveSyncEvent>,
}

/// Shell-owned sync-analysis workers and result queues.
///
/// The shell owns all worker lifetime and polling. Theme screens only emit
/// requests and consume the prepared events returned by [`Service::poll`].
pub(crate) struct Service {
    jobs: Vec<Job>,
    cache: Arc<AnalysisCache>,
}

impl Default for Service {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            cache: Arc::new(AnalysisCache::load(
                deadlib_platform::dirs::app_dirs().null_or_die_cache_file(),
            )),
        }
    }
}

impl Service {
    pub(crate) fn start(
        &mut self,
        owner: SimplyLoveSyncOwner,
        targets: Vec<SimplyLoveSyncTarget>,
        emit_freq_delta: bool,
    ) {
        self.cancel(owner);
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let rx = if owner == SimplyLoveSyncOwner::SelectMusicSong {
            let (tx, rx) = mpsc::sync_channel(SONG_PENDING_EVENTS);
            let cache = Arc::clone(&self.cache);
            std::thread::spawn(move || {
                run_song(targets, emit_freq_delta, thread_cancel, tx, cache);
            });
            rx
        } else {
            let (tx, rx) = mpsc::channel();
            let cache = Arc::clone(&self.cache);
            std::thread::spawn(move || run_pack(targets, thread_cancel, tx, cache));
            rx
        };
        self.jobs.push(Job { owner, cancel, rx });
    }

    pub(crate) fn refresh_applied(
        &self,
        changes: &[deadsync_simfile::sync_offset::SongOffsetSyncChange],
    ) {
        self.cache.refresh_applied(
            changes
                .iter()
                .map(|change| (change.simfile_path.as_path(), change.delta_seconds)),
        );
        self.cache.flush();
    }

    pub(crate) fn cancel(&mut self, owner: SimplyLoveSyncOwner) {
        for job in self.jobs.iter().filter(|job| job.owner == owner) {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.jobs.retain(|job| job.owner != owner);
    }

    /// Returns `None` without reading a clock or constructing scratch vectors
    /// when no analysis jobs exist. Terminal events are last on each job's
    /// FIFO channel, so removing a finished job cannot strand later work.
    pub(crate) fn poll(&mut self) -> Option<Vec<(SimplyLoveSyncOwner, SimplyLoveSyncEvent)>> {
        if self.jobs.is_empty() {
            return None;
        }
        Some(self.drain_events())
    }

    fn drain_events(&mut self) -> Vec<(SimplyLoveSyncOwner, SimplyLoveSyncEvent)> {
        let started = Instant::now();
        let mut events = Vec::new();
        let mut finished = Vec::new();

        'jobs: for job in &self.jobs {
            while events.len() < MAX_EVENTS_PER_FRAME && started.elapsed() < POLL_BUDGET {
                match job.rx.try_recv() {
                    Ok(event) => {
                        let is_finished = matches!(
                            event,
                            SimplyLoveSyncEvent::SongFinished(_)
                                | SimplyLoveSyncEvent::Finished
                                | SimplyLoveSyncEvent::Disconnected
                        );
                        events.push((job.owner, event));
                        if is_finished {
                            finished.push(job.owner);
                            continue 'jobs;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => continue 'jobs,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        events.push((job.owner, SimplyLoveSyncEvent::Disconnected));
                        finished.push(job.owner);
                        continue 'jobs;
                    }
                }
            }
            break;
        }
        self.jobs.retain(|job| !finished.contains(&job.owner));
        events
    }
}

fn run_song(
    mut targets: Vec<SimplyLoveSyncTarget>,
    emit_freq_delta: bool,
    cancel: Arc<AtomicBool>,
    tx: mpsc::SyncSender<SimplyLoveSyncEvent>,
    cache: Arc<AnalysisCache>,
) {
    let Some(target) = targets.pop() else {
        let _ = tx.send(SimplyLoveSyncEvent::SongFinished(Err(
            "No sync-analysis target was provided".to_owned(),
        )));
        return;
    };
    let cfg = config::null_or_die_bias_cfg();
    let options = AnalysisOptions::new(&cfg, config::get().null_or_die_confidence_percent);
    let prepared = sync_music_path(target.song.as_ref(), target.chart_ix)
        .ok()
        .map(|music_path| {
            cache.prepare(
                &target.song.simfile_path,
                &music_path,
                target.chart_ix,
                options,
                true,
            )
        });
    if let Some(cached) = prepared
        .as_ref()
        .and_then(|prepared| prepared.cached_analysis())
    {
        if !cancel.load(Ordering::Relaxed) {
            cache.flush();
            let _ = tx.send(SimplyLoveSyncEvent::SongFinished(Ok(cached_song_result(
                cached,
            ))));
        }
        return;
    }
    let prepared = prepared.and_then(|prepared| prepared.into_prepared());
    let stream_cfg = BiasStreamCfg {
        emit_freq_delta,
        orientation: config::get().null_or_die_graph_orientation,
    };
    let kernel = cfg.kernel_type;
    let result = analyze_song_chart_stream(
        target.song.as_ref(),
        target.chart_ix,
        &cfg,
        stream_cfg,
        |event| {
            if !cancel.load(Ordering::Relaxed) {
                let event = sync_stream_event(event, kernel);
                let _ = tx.send(SimplyLoveSyncEvent::SongStream(event));
            }
        },
    )
    .map(sync_song_result);
    if !cancel.load(Ordering::Relaxed) {
        if let (Ok(result), Some(prepared)) = (&result, prepared) {
            cache.record_completed(vec![CompletedTarget::with_plot(
                prepared,
                result.estimate.bias_ms,
                result.estimate.confidence,
                CachedPlot {
                    freq_rows: result.plot.freq_rows,
                    digest_rows: result.plot.digest_rows,
                    cols: result.plot.cols,
                    post_rows: result.plot.post_rows,
                    freq_domain: result.plot.freq_domain.clone(),
                    beat_digest: result.plot.beat_digest.clone(),
                    post_kernel: result.plot.post_kernel.clone(),
                    times_ms: result.plot.times_ms.clone(),
                    convolution: result.plot.convolution.clone(),
                    edge_discard: result.plot.edge_discard,
                },
            )]);
            cache.flush();
        }
        let _ = tx.send(SimplyLoveSyncEvent::SongFinished(result));
    }
}

fn cached_song_result(cached: &CachedAnalysis) -> SimplyLoveSyncSongResult {
    let bias_ms = if cached.applied { 0.0 } else { cached.bias_ms };
    let plot = cached.plot.as_ref().filter(|_| !cached.applied);
    SimplyLoveSyncSongResult {
        estimate: SimplyLoveSyncResult {
            bias_ms,
            confidence: cached.confidence,
        },
        plot: SimplyLoveSyncPlotView {
            freq_rows: plot.map_or(0, |plot| plot.freq_rows),
            digest_rows: plot.map_or(0, |plot| plot.digest_rows),
            cols: plot.map_or(0, |plot| plot.cols.max(plot.times_ms.len())),
            post_rows: plot.map_or(0, |plot| plot.post_rows),
            freq_domain: plot.map_or_else(Vec::new, |plot| plot.freq_domain.clone()),
            beat_digest: plot.map_or_else(Vec::new, |plot| plot.beat_digest.clone()),
            post_kernel: plot.map_or_else(Vec::new, |plot| plot.post_kernel.clone()),
            convolution: plot.map_or_else(Vec::new, |plot| plot.convolution.clone()),
            times_ms: plot.map_or_else(Vec::new, |plot| plot.times_ms.clone()),
            edge_discard: plot.map_or(0, |plot| plot.edge_discard),
        },
        cached: true,
    }
}

#[inline(always)]
const fn sync_kernel_target(target: KernelTarget) -> SimplyLoveSyncKernelTarget {
    match target {
        KernelTarget::Digest => SimplyLoveSyncKernelTarget::Digest,
        KernelTarget::Accumulator => SimplyLoveSyncKernelTarget::Accumulator,
    }
}

#[inline(always)]
const fn sync_kernel(kernel: BiasKernel) -> SimplyLoveSyncKernel {
    match kernel {
        BiasKernel::Rising => SimplyLoveSyncKernel::Rising,
        BiasKernel::Loudest => SimplyLoveSyncKernel::Loudest,
    }
}

fn sync_stream_event(event: BiasStreamEvent, kernel: BiasKernel) -> SimplyLoveSyncStreamEvent {
    match event {
        BiasStreamEvent::Init(init) => SimplyLoveSyncStreamEvent::Init {
            cols: init.cols,
            freq_rows: init.freq_rows,
            planned_beats: init.planned_beats,
            kernel_target: sync_kernel_target(init.kernel_target),
            kernel: sync_kernel(kernel),
            times_ms: init.times_ms,
        },
        BiasStreamEvent::Beat(beat) => SimplyLoveSyncStreamEvent::Beat {
            beat_seq: beat.beat_seq,
            digest_row: beat.digest_row,
            freq_delta: beat.freq_delta,
        },
        BiasStreamEvent::Convolution(conv) => SimplyLoveSyncStreamEvent::Convolution {
            rows: conv.rows,
            post_kernel: conv.post_kernel,
            convolution: conv.convolution,
            edge_discard: conv.edge_discard,
        },
        BiasStreamEvent::Done(estimate) => SimplyLoveSyncStreamEvent::Done(SimplyLoveSyncResult {
            bias_ms: estimate.bias_ms,
            confidence: estimate.confidence,
        }),
    }
}

fn sync_song_result(result: BiasEstimateWithPlot) -> SimplyLoveSyncSongResult {
    SimplyLoveSyncSongResult {
        estimate: SimplyLoveSyncResult {
            bias_ms: result.estimate.bias_ms,
            confidence: result.estimate.confidence,
        },
        plot: SimplyLoveSyncPlotView {
            freq_rows: result.plot.freq_rows,
            digest_rows: result.plot.digest_rows,
            cols: result.plot.cols,
            post_rows: result.plot.post_rows,
            freq_domain: result.plot.freq_domain,
            beat_digest: result.plot.beat_digest,
            post_kernel: result.plot.post_kernel,
            convolution: result.plot.convolution,
            times_ms: result.plot.times_ms,
            edge_discard: result.plot.edge_discard,
        },
        cached: false,
    }
}

fn run_pack(
    targets: Vec<SimplyLoveSyncTarget>,
    cancel: Arc<AtomicBool>,
    tx: mpsc::Sender<SimplyLoveSyncEvent>,
    cache: Arc<AnalysisCache>,
) {
    let worker_count = pack_worker_count(targets.len());
    let cfg = Arc::new(config::null_or_die_bias_cfg());
    let options = AnalysisOptions::new(&cfg, config::get().null_or_die_confidence_percent);
    let stream_cfg = BiasStreamCfg {
        emit_freq_delta: false,
        orientation: config::get().null_or_die_graph_orientation,
    };
    let (job_tx, job_rx) = mpsc::channel::<(usize, SimplyLoveSyncTarget)>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let completed = Arc::new(Mutex::new(Vec::new()));
    let mut workers = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let cancel = Arc::clone(&cancel);
        let cfg = Arc::clone(&cfg);
        let job_rx = Arc::clone(&job_rx);
        let tx = tx.clone();
        let cache = Arc::clone(&cache);
        let completed = Arc::clone(&completed);
        workers.push(std::thread::spawn(move || {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let job = {
                    let Ok(rx) = job_rx.lock() else { return };
                    rx.recv()
                };
                let Ok((index, target)) = job else { return };
                if cancel.load(Ordering::Relaxed) {
                    return;
                }

                let prepared = sync_music_path(target.song.as_ref(), target.chart_ix)
                    .ok()
                    .map(|music_path| {
                        cache.prepare(
                            &target.song.simfile_path,
                            &music_path,
                            target.chart_ix,
                            options,
                            false,
                        )
                    });
                if prepared
                    .as_ref()
                    .is_some_and(|prepared| prepared.is_cached())
                {
                    let _ = tx.send(SimplyLoveSyncEvent::RowCached { index });
                    continue;
                }
                let prepared = prepared.and_then(|prepared| prepared.into_prepared());

                let _ = tx.send(SimplyLoveSyncEvent::RowStarted { index });
                let mut total_beats = 0usize;
                let mut last_sent = 0usize;
                let result = analyze_song_chart_stream(
                    target.song.as_ref(),
                    target.chart_ix,
                    cfg.as_ref(),
                    stream_cfg,
                    |event| match event {
                        BiasStreamEvent::Init(init) => {
                            total_beats = init.planned_beats;
                            let _ = tx.send(SimplyLoveSyncEvent::RowInit { index, total_beats });
                        }
                        BiasStreamEvent::Beat(beat) => {
                            let beats_processed = beat.beat_seq.saturating_add(1);
                            let is_last = total_beats > 0 && beats_processed >= total_beats;
                            if beats_processed == 1
                                || is_last
                                || beats_processed.saturating_sub(last_sent) >= PROGRESS_STEP_BEATS
                            {
                                last_sent = beats_processed;
                                let _ = tx.send(SimplyLoveSyncEvent::RowBeat {
                                    index,
                                    beats_processed,
                                    total_beats,
                                });
                            }
                        }
                        BiasStreamEvent::Convolution(_) | BiasStreamEvent::Done(_) => {}
                    },
                )
                .map(|result| SimplyLoveSyncResult {
                    bias_ms: result.estimate.bias_ms,
                    confidence: result.estimate.confidence,
                });
                if let (Ok(result), Some(prepared)) = (&result, prepared)
                    && let Ok(mut completed) = completed.lock()
                {
                    completed.push(CompletedTarget::new(
                        prepared,
                        result.bias_ms,
                        result.confidence,
                    ));
                }
                let _ = tx.send(SimplyLoveSyncEvent::RowFinished { index, result });
            }
        }));
    }

    for (index, target) in targets.into_iter().enumerate() {
        if job_tx.send((index, target)).is_err() {
            break;
        }
    }
    drop(job_tx);
    for worker in workers {
        let _ = worker.join();
    }
    if !cancel.load(Ordering::Relaxed)
        && let Ok(mut completed) = completed.lock()
    {
        cache.record_completed(std::mem::take(&mut *completed));
    }
    cache.flush();
    let _ = tx.send(SimplyLoveSyncEvent::Finished);
}

fn pack_worker_count(target_count: usize) -> usize {
    if target_count == 0 {
        return 0;
    }
    let available = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let configured = match config::get().null_or_die_pack_sync_threads {
        0 => available,
        1 => 1,
        count => usize::from(count).min(available).max(1),
    };
    configured.min(target_count).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_poll_only_runs_while_a_job_is_active() {
        let mut service = Service::default();
        assert!(service.poll().is_none());

        let (tx, rx) = mpsc::channel();
        service.jobs.push(Job {
            owner: SimplyLoveSyncOwner::SelectMusicSong,
            cancel: Arc::new(AtomicBool::new(false)),
            rx,
        });
        assert!(service.poll().is_some_and(|events| events.is_empty()));
        tx.send(SimplyLoveSyncEvent::Finished)
            .expect("the service owns the matching receiver");
        let events = service.poll().expect("the job is active");

        assert!(matches!(
            events.as_slice(),
            [(
                SimplyLoveSyncOwner::SelectMusicSong,
                SimplyLoveSyncEvent::Finished
            )]
        ));
        assert!(service.poll().is_none());
    }

    #[test]
    fn cached_song_result_restores_estimate_and_visuals() {
        let result = cached_song_result(&CachedAnalysis {
            bias_ms: -4.0,
            confidence: 0.93,
            applied: false,
            plot: Some(CachedPlot {
                freq_rows: 1,
                digest_rows: 1,
                cols: 3,
                post_rows: 1,
                freq_domain: vec![0.2, 0.4, 0.6],
                beat_digest: vec![0.3, 0.5, 0.7],
                post_kernel: vec![0.4, 0.6, 0.8],
                times_ms: vec![-1.0, 0.0, 1.0],
                convolution: vec![0.1, 0.9, 0.2],
                edge_discard: 1,
            }),
        });

        assert!(result.cached);
        assert_eq!(result.estimate.bias_ms, -4.0);
        assert_eq!(result.estimate.confidence, 0.93);
        assert_eq!(result.plot.cols, 3);
        assert_eq!(result.plot.freq_domain, [0.2, 0.4, 0.6]);
        assert_eq!(result.plot.beat_digest, [0.3, 0.5, 0.7]);
        assert_eq!(result.plot.post_kernel, [0.4, 0.6, 0.8]);
        assert_eq!(result.plot.convolution, [0.1, 0.9, 0.2]);
    }

    #[test]
    fn cached_applied_result_cannot_apply_the_same_delta_twice() {
        let result = cached_song_result(&CachedAnalysis {
            bias_ms: -4.0,
            confidence: 0.93,
            applied: true,
            plot: None,
        });

        assert!(result.cached);
        assert_eq!(result.estimate.bias_ms, 0.0);
        assert!(result.plot.convolution.is_empty());
    }
}

fn analyze_song_chart_stream<F>(
    song: &SongData,
    chart_ix: usize,
    cfg: &BiasCfg,
    stream_cfg: BiasStreamCfg,
    on_event: F,
) -> Result<BiasEstimateWithPlot, String>
where
    F: FnMut(BiasStreamEvent),
{
    let music_path = sync_music_path(song, chart_ix)?;
    let gameplay_chart = song_loading::load_sync_analysis_chart(song, chart_ix)?;
    let audio = decode_sync_audio(music_path.as_path())?;
    let mut runtime = BiasRuntime::default();
    estimate_bias_with_beat_fn_stream_reuse(
        &audio.mono,
        audio.sample_rate_hz,
        cfg,
        &mut runtime,
        stream_cfg,
        on_event,
        |beat| f64::from(gameplay_chart.timing.get_time_for_beat(beat as f32)),
    )
}

fn sync_music_path(song: &SongData, chart_ix: usize) -> Result<PathBuf, String> {
    let chart = song
        .charts
        .get(chart_ix)
        .ok_or_else(|| format!("Chart index {chart_ix} out of range"))?;
    chart
        .music_path
        .as_ref()
        .or(song.music_path.as_ref())
        .cloned()
        .ok_or_else(|| format!("No music path for '{}'", song.display_full_title(false)))
}

fn decode_sync_audio(path: &Path) -> Result<SyncAudio, String> {
    let opened = decode::open_file(path)
        .map_err(|e| format!("Cannot open sync audio '{}': {e}", path.display()))?;
    if opened.channels == 0 {
        return Err(format!("Sync audio '{}' has no channels", path.display()));
    }
    if opened.sample_rate_hz == 0 {
        return Err(format!(
            "Sync audio '{}' has no sample rate",
            path.display()
        ));
    }

    let channels = opened.channels;
    let sample_rate_hz = opened.sample_rate_hz;
    let mut reader = opened.reader;
    let mut packet = Vec::new();
    let mut mono = Vec::new();
    while reader
        .read_dec_packet_into(&mut packet)
        .map_err(|e| format!("Cannot decode sync audio '{}': {e}", path.display()))?
    {
        append_sync_mono(&packet, channels, &mut mono);
    }
    if mono.is_empty() {
        return Err(format!(
            "Sync audio '{}' contained no decoded samples",
            path.display()
        ));
    }
    Ok(SyncAudio {
        sample_rate_hz,
        mono,
    })
}

fn append_sync_mono(samples: &[i16], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => out.extend(
            samples
                .iter()
                .map(|&sample| f32::from(sample) * PCM_INV_SCALE),
        ),
        2 => {
            out.reserve(samples.len() / 2);
            for frame in samples.chunks_exact(2) {
                out.push(f32::from(frame[0].max(frame[1])) * PCM_INV_SCALE);
            }
        }
        count => {
            out.reserve(samples.len() / count);
            for frame in samples.chunks_exact(count) {
                if let Some(sample) = frame.iter().copied().max() {
                    out.push(f32::from(sample) * PCM_INV_SCALE);
                }
            }
        }
    }
}
