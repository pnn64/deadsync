#[derive(Debug, Clone, Default)]
pub struct SpriteDefinition {
    pub src: [i32; 2],
    pub size: [i32; 2],
    pub rotation_deg: i32,
    pub mirror_h: bool,
    pub mirror_v: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationRate {
    FramesPerSecond(f32),
    FramesPerBeat(f32),
}

#[inline(always)]
pub fn neg_rot_sin_cos(rotation_deg: i32) -> [f32; 2] {
    match rotation_deg.rem_euclid(360) {
        0 => [0.0, 1.0],
        90 => [-1.0, 0.0],
        180 => [0.0, -1.0],
        270 => [1.0, 0.0],
        _ => {
            let (sin_r, cos_r) = (-(rotation_deg as f32)).to_radians().sin_cos();
            [sin_r, cos_r]
        }
    }
}

#[inline(always)]
pub fn frame_duration_total(durations: &[f32], frames: usize) -> Option<f32> {
    let total = durations.iter().take(frames).fold(0.0, |sum, duration| {
        if *duration > f32::EPSILON {
            sum + *duration
        } else {
            sum
        }
    });
    (total > f32::EPSILON && total.is_finite()).then_some(total)
}

#[inline(always)]
pub fn duration_frame_index(durations: &[f32], frames: usize, mut position: f32) -> Option<usize> {
    let mut last = None;
    for (idx, duration) in durations.iter().take(frames).enumerate() {
        let span = (*duration).max(0.0);
        if span <= f32::EPSILON {
            continue;
        }
        last = Some(idx);
        if position < span {
            return Some(idx);
        }
        position -= span;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::{duration_frame_index, frame_duration_total, neg_rot_sin_cos};

    #[test]
    fn neg_rotation_uses_exact_cardinal_values() {
        assert_eq!(neg_rot_sin_cos(0), [0.0, 1.0]);
        assert_eq!(neg_rot_sin_cos(90), [-1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(180), [0.0, -1.0]);
        assert_eq!(neg_rot_sin_cos(270), [1.0, 0.0]);
        assert_eq!(neg_rot_sin_cos(-90), [1.0, 0.0]);
    }

    #[test]
    fn frame_duration_total_skips_non_positive_spans() {
        assert_eq!(frame_duration_total(&[0.1, 0.0, -1.0, 0.2], 4), Some(0.3));
        assert_eq!(frame_duration_total(&[0.0, -1.0], 2), None);
    }

    #[test]
    fn duration_frame_index_uses_last_positive_span_as_fallback() {
        let durations = [0.1, 0.0, 0.2];
        assert_eq!(duration_frame_index(&durations, 3, 0.05), Some(0));
        assert_eq!(duration_frame_index(&durations, 3, 0.15), Some(2));
        assert_eq!(duration_frame_index(&durations, 3, 9.0), Some(2));
        assert_eq!(duration_frame_index(&[0.0], 1, 0.0), None);
    }
}
