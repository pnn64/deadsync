pub(super) fn cached_song_result(cached: &CachedAnalysis) -> SimplyLoveSyncSongResult {
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
