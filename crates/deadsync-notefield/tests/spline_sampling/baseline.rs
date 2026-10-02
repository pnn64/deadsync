// Frozen from 14ea3817e (0.5.1681), before frame/path sample reuse.
use super::PreparedNotefield;
pub(super) struct Baseline<'p, 'a, S>(pub(super) &'p PreparedNotefield<'a, S>);

impl<S> Baseline<'_, '_, S> {
    pub(crate) fn spline_position(&self, col: usize, beat: f32, base: [f32; 3]) -> [f32; 3] {
        let spline = self.0.column_position_splines[col];
        if spline.enabled && spline.absolute {
            let position = spline.sample(self.0.current_beat, beat).0;
            [
                self.0.field.playfield_center_x + position[0] * self.0.field_zoom,
                self.0.spline_origin_y + position[1] * self.0.field_zoom,
                position[2] * self.0.field_zoom,
            ]
        } else {
            let offset = self.spline_offsets(col, beat).0;
            std::array::from_fn(|axis| base[axis] + offset[axis])
        }
    }

    pub(crate) fn spline_zoom(&self, col: usize, beat: f32, base: f32) -> f32 {
        let spline = self.0.column_zoom_splines[col];
        if spline.enabled {
            spline.sample(self.0.current_beat, beat).0[0]
        } else {
            base
        }
    }

    pub(crate) fn spline_offsets(&self, col: usize, beat: f32) -> ([f32; 3], [f32; 3]) {
        let spline = self.0.column_position_splines[col];
        let (position, derivative) = spline.sample(self.0.current_beat, beat);
        let receptor = spline.receptor(self.0.current_beat);
        (
            std::array::from_fn(|axis| {
                (position[axis] - if axis < 2 { receptor[axis] } else { 0.0 }) * self.0.field_zoom
            }),
            derivative,
        )
    }
}
