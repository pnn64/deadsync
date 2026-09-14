//! Pure evaluation graph preparation from caller-owned chart and life records.
use crate::views::CourseGraphStage;
use deadlib_render_core::MeshVertex;
use std::sync::Arc;
pub(super) const GRAPH_LIFE_SAMPLE_COUNT: usize = 100;

#[inline(always)]
pub(super) const fn course_graph_stage_seconds(stage: &CourseGraphStage) -> f32 {
    if stage.song_last_second.is_finite() {
        stage.song_last_second.max(0.0)
    } else {
        0.0
    }
}

pub(super) fn course_graph_raw_seconds(stages: &[CourseGraphStage]) -> f32 {
    stages.iter().map(course_graph_stage_seconds).sum()
}

pub(super) fn course_graph_peak_nps(stages: &[CourseGraphStage]) -> f64 {
    stages.iter().fold(0.0, |peak, stage| {
        let stage_peak = stage.chart.max_nps;
        if stage_peak.is_finite() {
            peak.max(stage_peak)
        } else {
            peak
        }
    })
}

#[inline(always)]
pub(super) fn valid_music_rate(music_rate: f32) -> f32 {
    if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    }
}

pub(super) fn build_course_density_graph_mesh(
    stages: &[CourseGraphStage],
    graph_width: f32,
    graph_height: f32,
    music_rate: f32,
) -> Option<Arc<[MeshVertex]>> {
    let total = course_graph_raw_seconds(stages);
    let width = graph_width.max(0.0);
    let height = graph_height.max(0.0);
    if total <= 0.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }

    let rate = valid_music_rate(music_rate);
    // Simply Love measures both values in rate-adjusted seconds.  Keeping
    // both divisions here makes the cancellation explicit and prevents the
    // numerator-only rate scaling that previously shortened the course graph.
    let display_total = total / rate;
    let peak_nps = course_graph_peak_nps(stages);
    let mut x = 0.0_f32;
    let mut out = Vec::new();
    let mut scratch = crate::screens::components::shared::density::DensityHistScratch::default();
    for stage in stages {
        let stage_seconds = course_graph_stage_seconds(stage);
        let stage_width = (stage_seconds / rate) / display_total * width;
        if stage_width <= 0.0 {
            continue;
        }

        let first = stage.chart.first_second;
        let last = stage_seconds.max(first + 0.001);
        scratch.append_mesh(
            &mut out,
            &stage.chart.measure_nps_vec,
            peak_nps,
            &stage.chart.measure_seconds_vec,
            first,
            last,
            stage_width,
            height,
            x,
            Some(0.5),
            0.65,
        );
        x += stage_width;
    }

    // The final immutable copy only needs the assembled vertices.
    drop(scratch);
    (!out.is_empty()).then(|| Arc::from(out.into_boxed_slice()))
}

/// A borrowed, batch-local view of the same upper-bound interpolation as the
/// scalar life lookup. Only the fixed chart-start search and anchor clamp are
/// prepared; per-sample searches retain their behavior for duplicate,
/// nonmonotonic and nonfinite records as well as ordinary sorted histories.
pub(super) struct LifeRecordSampler<'a> {
    records: &'a [(f32, f32)],
    record_start: f32,
    start_life: f32,
}

impl<'a> LifeRecordSampler<'a> {
    pub(super) fn new(life_history: &'a [(f32, f32)], record_start: f32) -> Self {
        let start_life = life_history
            .first()
            .map_or(0.0, |&(_, life)| life.clamp(0.0, 1.0));
        let first = life_history.partition_point(|&(t, _)| t < record_start);
        Self {
            records: &life_history[first..],
            record_start,
            start_life,
        }
    }

    #[inline(always)]
    pub(super) fn sample(&self, sample_time: f32) -> f32 {
        if sample_time <= self.record_start {
            return self.start_life;
        }
        let later_ix = self.records.partition_point(|&(t, _)| t <= sample_time);
        let (earlier_t, earlier_life) = if later_ix == 0 {
            (self.record_start, self.start_life)
        } else {
            self.records[later_ix - 1]
        };
        let Some(&(later_t, later_life)) = self.records.get(later_ix) else {
            return earlier_life.clamp(0.0, 1.0);
        };
        let dt = later_t - earlier_t;
        if dt.abs() <= f32::EPSILON {
            return earlier_life.clamp(0.0, 1.0);
        }
        let alpha = ((sample_time - earlier_t) / dt).clamp(0.0, 1.0);
        (later_life - earlier_life)
            .mul_add(alpha, earlier_life)
            .clamp(0.0, 1.0)
    }
}

#[cfg(test)]
pub(super) fn life_record_lerp_at(
    life_history: &[(f32, f32)],
    record_start: f32,
    sample_time: f32,
) -> f32 {
    LifeRecordSampler::new(life_history, record_start).sample(sample_time)
}

pub(super) fn graph_display_life_points(
    life_history: &[(f32, f32)],
    record_start: f32,
    graph_first: f32,
    graph_last: f32,
    graph_width: f32,
    graph_height: f32,
) -> Option<[[f32; 2]; GRAPH_LIFE_SAMPLE_COUNT]> {
    if life_history.is_empty() || graph_height <= 0.0 {
        return None;
    }

    let (graph_last, line_left, sample_x_step) =
        graph_display_life_layout(record_start, graph_first, graph_last, graph_width)?;
    let record_duration = graph_last - record_start;
    let sample_time_step = record_duration / GRAPH_LIFE_SAMPLE_COUNT as f32;

    let record = LifeRecordSampler::new(life_history, record_start);
    let mut points = [[0.0_f32; 2]; GRAPH_LIFE_SAMPLE_COUNT];
    for i in 0..GRAPH_LIFE_SAMPLE_COUNT {
        // GraphDisplay samples life with a denominator of 100 but lays those
        // samples across its width with a denominator of 99.
        let sample_time = (i as f32).mul_add(sample_time_step, record_start);
        let life = record.sample(sample_time);
        points[i] = [
            (i as f32).mul_add(sample_x_step, line_left),
            (1.0 - life).mul_add(graph_height, 1.0),
        ];
    }
    Some(points)
}

pub(super) fn graph_display_life_layout(
    record_start: f32,
    graph_first: f32,
    graph_last: f32,
    graph_width: f32,
) -> Option<(f32, f32, f32)> {
    if !record_start.is_finite()
        || !graph_first.is_finite()
        || !graph_last.is_finite()
        || graph_width <= 0.0
    {
        return None;
    }
    let graph_last = graph_last.max(graph_first + 0.001);
    if graph_last <= record_start {
        return None;
    }
    let line_left = ((record_start - graph_first) / (graph_last - graph_first)) * graph_width;
    let sample_x_step = (graph_width - line_left) / (GRAPH_LIFE_SAMPLE_COUNT - 1) as f32;
    Some((graph_last, line_left, sample_x_step))
}
