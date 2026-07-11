use deadsync_audio_decode as decode;
use deadsync_chart::SongData;
use deadsync_simfile::app_runtime as song_loading;
use null_or_die::{
    BiasCfg, BiasEstimateWithPlot, BiasRuntime, BiasStreamCfg, BiasStreamEvent,
    estimate_bias_with_beat_fn_stream_reuse,
};
use std::path::{Path, PathBuf};

const PCM_INV_SCALE: f32 = 1.0 / 32768.0;

struct SyncAudio {
    sample_rate_hz: u32,
    mono: Vec<f32>,
}

pub(crate) fn analyze_song_chart_stream<F>(
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
        2 => append_stereo_max(samples, out),
        n => append_frame_max(samples, n, out),
    }
}

fn append_stereo_max(samples: &[i16], out: &mut Vec<f32>) {
    out.reserve(samples.len() / 2);
    for frame in samples.chunks_exact(2) {
        out.push(f32::from(frame[0].max(frame[1])) * PCM_INV_SCALE);
    }
}

fn append_frame_max(samples: &[i16], channels: usize, out: &mut Vec<f32>) {
    out.reserve(samples.len() / channels);
    for frame in samples.chunks_exact(channels) {
        let Some(sample) = frame.iter().copied().max() else {
            continue;
        };
        out.push(f32::from(sample) * PCM_INV_SCALE);
    }
}
