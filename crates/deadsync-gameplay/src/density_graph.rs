/// Display geometry and scrolling chosen by the theme for one stage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DensityGraphLayout {
    pub width: f32,
    pub height: f32,
    pub visible_seconds: f32,
    pub cursor_fraction: f32,
    pub top_height: f32,
    pub top_width: [f32; MAX_PLAYERS],
    pub top_scale_y: [f32; MAX_PLAYERS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DensityGraphWindow {
    pub first_second: f32,
    pub last_second: f32,
    pub duration: f32,
    pub graph_w: f32,
    pub graph_h: f32,
    pub scaled_width: f32,
    pub u_window: f32,
    pub cursor_fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayDensityGraphView {
    pub first_second: f32,
    pub last_second: f32,
    pub duration: f32,
    pub graph_w: f32,
    pub graph_h: f32,
    pub scaled_width: f32,
    pub u0: f32,
    pub u_window: f32,
    pub top_h: f32,
    pub top_w: [f32; MAX_PLAYERS],
    pub top_scale_y: [f32; MAX_PLAYERS],
}

impl GameplayDensityGraphView {
    #[inline(always)]
    #[must_use]
    pub fn top_mesh_h(self, player: usize) -> f32 {
        self.top_h * self.top_scale_y[player].clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug)]
pub struct GameplayDensityGraphState {
    pub first_second: f32,
    pub last_second: f32,
    pub duration: f32,
    pub graph_w: f32,
    pub graph_h: f32,
    pub scaled_width: f32,
    pub u0: f32,
    pub u_window: f32,
    pub cursor_fraction: f32,
    pub life_update_rate: f32,
    pub life_next_update_elapsed: f32,
    pub life_points: [Vec<[f32; 2]>; MAX_PLAYERS],
    pub life_dirty: [bool; MAX_PLAYERS],
    pub top_h: f32,
    pub top_w: [f32; MAX_PLAYERS],
    pub top_scale_y: [f32; MAX_PLAYERS],
}

impl Default for GameplayDensityGraphState {
    fn default() -> Self {
        Self {
            first_second: 0.0,
            last_second: 0.0,
            duration: 0.001,
            graph_w: 0.0,
            graph_h: 0.0,
            scaled_width: 0.0,
            u0: 0.0,
            u_window: 1.0,
            cursor_fraction: 0.0,
            life_update_rate: 0.25,
            life_next_update_elapsed: 0.0,
            life_points: std::array::from_fn(|_| Vec::new()),
            life_dirty: [false; MAX_PLAYERS],
            top_h: 0.0,
            top_w: [0.0; MAX_PLAYERS],
            top_scale_y: [1.0; MAX_PLAYERS],
        }
    }
}

#[must_use]
pub fn density_graph_u0_for_time(window: DensityGraphWindow, current_music_time: f32) -> f32 {
    if window.graph_w <= 0.0_f32 || window.graph_h <= 0.0_f32 || window.scaled_width <= 0.0_f32 {
        return 0.0;
    }

    let duration = window.duration.max(0.001_f32);
    let u_window = window.u_window.clamp(0.0_f32, 1.0_f32);
    let max_u0 = (1.0_f32 - u_window).max(0.0_f32);
    if max_u0 <= 0.0_f32 {
        return 0.0;
    }

    let max_seconds = (u_window * duration).max(0.0_f32);
    if max_seconds <= 0.0_f32 {
        return 0.0;
    }
    let cursor_fraction = window.cursor_fraction;
    if current_music_time > max_seconds.mul_add(cursor_fraction - 1.0, window.last_second) {
        return max_u0;
    }

    let seconds_past_cursor =
        max_seconds.mul_add(-cursor_fraction, current_music_time - window.first_second);
    if seconds_past_cursor > 0.0_f32 {
        return (seconds_past_cursor / duration).clamp(0.0_f32, max_u0);
    }
    0.0
}

#[must_use]
pub fn density_graph_life_catch_up_steps(
    total_elapsed: f32,
    next_update_elapsed: f32,
    update_rate: f32,
) -> u32 {
    if !update_rate.is_finite()
        || update_rate <= 0.0_f32
        || !total_elapsed.is_finite()
        || total_elapsed < next_update_elapsed
    {
        return 0;
    }
    let elapsed = (total_elapsed - next_update_elapsed).max(0.0_f32);
    ((elapsed / update_rate) as u32).saturating_add(1).min(64)
}

/// Maximum number of life samples the gameplay density graph can retain for a song.
///
/// The buffer is owned by the gameplay thread for the song lifetime and is
/// populated at `life_update_rate`. Two spare entries cover the inclusive first
/// sample and floating-point boundary rounding, so a live song never has to grow
/// this buffer. The simplifier may retain fewer points, but never more than the
/// number of scheduled samples.
#[must_use]
pub fn density_graph_life_capacity(duration: f32, life_update_rate: f32) -> usize {
    if !duration.is_finite()
        || duration <= 0.0
        || !life_update_rate.is_finite()
        || life_update_rate <= 0.0
    {
        return 0;
    }
    ((duration / life_update_rate).ceil() as usize).saturating_add(2)
}

#[must_use]
pub fn density_graph_life_sample_x(
    current_music_time: f32,
    first_second: f32,
    last_second: f32,
    duration: f32,
    scaled_width: f32,
) -> Option<f32> {
    if current_music_time <= 0.0_f32 || current_music_time > last_second {
        return None;
    }
    let x = (((current_music_time - first_second) / duration.max(0.001_f32)) * scaled_width)
        .clamp(0.0_f32, scaled_width);
    x.is_finite().then_some(x)
}

#[inline(always)]
pub fn push_density_life_point(points: &mut Vec<[f32; 2]>, x: f32, y: f32) -> bool {
    const EPS: f32 = 0.000_1_f32;
    const ANGLE_SIN2_MAX: f32 = 0.032_f32; // sin(0.18rad)^2

    if let Some(last) = points.last_mut()
        && x <= last[0] + EPS
    {
        if (y - last[1]).abs() <= EPS {
            return false;
        }
        last[1] = y;
        return true;
    }

    if points.len() >= 2 {
        let a = points[points.len() - 2];
        let b = points[points.len() - 1];
        let abx = b[0] - a[0];
        let aby = b[1] - a[1];
        let bcx = x - b[0];
        let bcy = y - b[1];
        let ab_len_sq = abx.mul_add(abx, aby * aby);
        let bc_len_sq = bcx.mul_add(bcx, bcy * bcy);
        let dot = abx.mul_add(bcx, aby * bcy);
        if dot > 0.0_f32 && ab_len_sq > EPS && bc_len_sq > EPS {
            let cross = abx.mul_add(bcy, -(aby * bcx));
            let cross_sq = cross * cross;
            if cross_sq <= ANGLE_SIN2_MAX * ab_len_sq * bc_len_sq {
                let last_ix = points.len() - 1;
                points[last_ix] = [x, y];
                return true;
            }
        }
    }

    points.push([x, y]);
    true
}

#[must_use]
pub fn reference_bpm_from_display_tag(
    chart_display_bpm: Option<&ChartDisplayBpm>,
    song_display_bpm: &str,
) -> Option<f32> {
    match chart_display_bpm {
        Some(ChartDisplayBpm::Specified { max, .. }) => {
            let value = *max as f32;
            if value.is_finite() && value > 0.0 {
                return Some(value);
            }
        }
        Some(ChartDisplayBpm::Random) => return None,
        None => {}
    }

    let tag = song_display_bpm.trim();
    if tag.is_empty() || tag == "*" {
        return None;
    }
    if let Some((_, max_tag)) = tag.split_once(':') {
        return max_tag.trim().parse::<f32>().ok();
    }
    tag.parse::<f32>().ok()
}
