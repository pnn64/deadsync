//! The properties owned by Actor::TweenState, without unrelated render state.
use crate::{
    SongLuaOverlayState, SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value,
};

macro_rules! tween_pose {
    ($($target:ident => $field:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct TweenPose {
            $(pub $field: f32,)*
            pub diffuse: [f32; 4],
            pub vertex_colors: Option<[[f32; 4]; 4]>,
            pub glow: [f32; 4],
            pub quaternion: [f32; 4],
        }

        pub(crate) const TARGETS: [Target; 25] = [
            $(Target::$target,)* Target::Diffuse, Target::VertexColors, Target::Glow, Target::Quaternion,
        ];

        impl From<SongLuaOverlayState> for TweenPose {
            fn from(state: SongLuaOverlayState) -> Self {
                Self {
                    $($field: state.$field,)*
                    diffuse: state.diffuse,
                    vertex_colors: state.vertex_colors,
                    glow: state.glow,
                    quaternion: state.quaternion,
                }
            }
        }

        impl TweenPose {
            pub fn write_to(self, state: &mut SongLuaOverlayState) {
                // StretchTo's cached rectangle follows position at the export
                // boundary. It is not itself a member of the tween queue.
                crate::move_overlay(state, self.x, self.y);
                $(state.$field = self.$field;)*
                state.diffuse = self.diffuse;
                state.vertex_colors = self.vertex_colors;
                state.glow = self.glow;
                state.quaternion = self.quaternion;
            }

            pub fn set(&mut self, target: Target, value: &Value) {
                match (target, value) {
                    $((Target::$target, Value::F32(value)) => self.$field = *value,)*
                    (Target::Diffuse, Value::Vec4(value)) => self.diffuse = *value,
                    (Target::Quaternion, Value::Quaternion(value)) => self.quaternion = *value,
                    (Target::Glow, Value::Vec4(value)) => self.glow = *value,
                    (Target::VertexColors, Value::VertexColors(value)) => self.vertex_colors = Some(**value),
                    (Target::VertexColors, Value::None) => self.vertex_colors = None,
                    _ => {}
                }
            }

            pub fn get(self, target: Target) -> Value {
                match target {
                    $(Target::$target => Value::F32(self.$field),)*
                    Target::Diffuse => Value::Vec4(self.diffuse),
                    Target::Glow => Value::Vec4(self.glow),
                    Target::Quaternion => Value::Quaternion(self.quaternion),
                    Target::VertexColors => self.vertex_colors.map_or(Value::None,
                        |colors| Value::VertexColors(std::sync::Arc::new(colors))),
                    _ => unreachable!("non-tween property requested from a tween pose"),
                }
            }

            pub fn lerp(mut self, to: Self, factor: f32) -> Self {
                // Keep Actor::TweenState's subtract/multiply/add order and
                // unclamped easing, including overshoot and nonfinite values.
                $(self.$field = crate::actor_lerp(self.$field, to.$field, factor);)*
                self.quaternion = crate::actor_quaternion::slerp(self.quaternion, to.quaternion, factor);
                for (value, to) in self.diffuse.iter_mut().zip(to.diffuse) {
                    *value = crate::actor_lerp(*value, to, factor);
                }
                for (value, to) in self.glow.iter_mut().zip(to.glow) {
                    *value = crate::actor_lerp(*value, to, factor);
                }
                if let Some(to) = to.vertex_colors {
                    let colors = self.vertex_colors.get_or_insert([[1.0; 4]; 4]);
                    for (from, to) in colors.iter_mut().flatten().zip(to.into_iter().flatten()) {
                        *from = crate::actor_lerp(*from, to, factor);
                    }
                }
                self
            }
        }
    };
}

tween_pose! {
    Aux => aux, X => x, Y => y, Z => z,
    Zoom => zoom, ZoomX => zoom_x, ZoomY => zoom_y, ZoomZ => zoom_z,
    RotationX => rot_x_deg, RotationY => rot_y_deg, RotationZ => rot_z_deg,
    SkewX => skew_x, SkewY => skew_y,
    CropLeft => cropleft, CropRight => cropright, CropTop => croptop, CropBottom => cropbottom,
    FadeLeft => fadeleft, FadeRight => faderight, FadeTop => fadetop, FadeBottom => fadebottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SongLuaOverlayStateDelta;

    // The general render-delta evaluator is the independent pre-refactor path.
    fn delta(state: SongLuaOverlayState) -> SongLuaOverlayStateDelta {
        SongLuaOverlayStateDelta {
            aux: Some(state.aux),
            x: Some(state.x),
            y: Some(state.y),
            z: Some(state.z),
            zoom: Some(state.zoom),
            zoom_x: Some(state.zoom_x),
            zoom_y: Some(state.zoom_y),
            zoom_z: Some(state.zoom_z),
            rot_x_deg: Some(state.rot_x_deg),
            rot_y_deg: Some(state.rot_y_deg),
            rot_z_deg: Some(state.rot_z_deg),
            skew_x: Some(state.skew_x),
            skew_y: Some(state.skew_y),
            cropleft: Some(state.cropleft),
            cropright: Some(state.cropright),
            croptop: Some(state.croptop),
            cropbottom: Some(state.cropbottom),
            fadeleft: Some(state.fadeleft),
            faderight: Some(state.faderight),
            fadetop: Some(state.fadetop),
            fadebottom: Some(state.fadebottom),
            diffuse: Some(state.diffuse),
            vertex_colors: state.vertex_colors,
            glow: Some(state.glow),
            quaternion: Some(state.quaternion),
            ..Default::default()
        }
    }

    fn comparable_bits(value: f32) -> u32 {
        // Preserve exact finite values, infinities and signed zero. Arithmetic
        // can select a different NaN payload without changing its meaning.
        if value.is_nan() { f32::NAN.to_bits() } else { value.to_bits() }
    }

    fn compare_bits(actual: TweenPose, expected: SongLuaOverlayState) {
        for target in TARGETS {
            let actual = actual.get(target);
            let expected = crate::perframe::overlay_state_update_value(&expected, target);
            match (actual, expected) {
                (Value::F32(a), Value::F32(b)) => {
                    assert_eq!(comparable_bits(a), comparable_bits(b), "{target:?}")
                }
                (Value::Vec4(a), Value::Vec4(b)) | (Value::Quaternion(a), Value::Quaternion(b)) => {
                    assert_eq!(a.map(comparable_bits), b.map(comparable_bits), "{target:?}")
                }
                (Value::VertexColors(a), Value::VertexColors(b)) => assert_eq!(
                    a.map(|row| row.map(comparable_bits)),
                    b.map(|row| row.map(comparable_bits))
                ),
                (Value::None, Value::None) => {}
                _ => panic!("mismatched property {target:?}"),
            }
        }
    }

    #[test]
    fn compact_pose_matches_general_interpolation_and_export() {
        for start in [
            -0.0,
            1.25,
            -17.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0xffc0_1234),
        ] {
            for end in [
                0.0,
                -3.75,
                1e30,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::from_bits(0x7fc0_5678),
            ] {
                for corners in [false, true] {
                    let mut from = SongLuaOverlayState::default();
                    let mut to = SongLuaOverlayState::default();
                    for target in TARGETS {
                        let values = match target {
                            Target::Quaternion => (Value::Quaternion(crate::actor_quaternion::IDENTITY),
                                Value::Quaternion(crate::actor_quaternion::rotate(crate::actor_quaternion::IDENTITY, 1, 45.0))),
                            Target::Diffuse | Target::Glow => {
                                (Value::Vec4([start; 4]), Value::Vec4([end; 4]))
                            }
                            Target::VertexColors => {
                                if corners {
                                    (
                                        Value::None,
                                        Value::VertexColors(std::sync::Arc::new([[end; 4]; 4])),
                                    )
                                } else {
                                    (
                                        Value::VertexColors(std::sync::Arc::new([[start; 4]; 4])),
                                        Value::None,
                                    )
                                }
                            }
                            _ => (Value::F32(start), Value::F32(end)),
                        };
                        crate::perframe::set_overlay_state_update_value(
                            &mut from, target, &values.0,
                        );
                        crate::perframe::set_overlay_state_update_value(&mut to, target, &values.1);
                    }
                    for factor in [-0.25, 0.0, 0.375, 1.0, 1.25, f32::NAN] {
                        let mut expected = from;
                        crate::overlay_state_lerp(&mut expected, &delta(to), factor);
                        compare_bits(TweenPose::from(from).lerp(to.into(), factor), expected);
                    }
                    let mut exported = SongLuaOverlayState {
                        x: 8.0,
                        y: 11.0,
                        stretch_rect: Some([3.0, 7.0, 13.0, 17.0]),
                        effect_period: 9.0,
                        visible: false,
                        ..Default::default()
                    };
                    let mut expected = exported;
                    crate::apply_overlay_delta(&mut expected, &delta(to));
                    expected.vertex_colors = to.vertex_colors;
                    TweenPose::from(to).write_to(&mut exported);
                    compare_bits(exported.into(), expected);
                    assert_eq!(
                        exported.stretch_rect.map(|v| v.map(f32::to_bits)),
                        expected.stretch_rect.map(|v| v.map(f32::to_bits))
                    );
                    assert_eq!(exported.effect_period, 9.0);
                    assert!(!exported.visible);
                }
            }
        }
        println!(
            "tween pose: {} bytes; render state: {} bytes",
            size_of::<TweenPose>(),
            size_of::<SongLuaOverlayState>()
        );
        assert!(size_of::<TweenPose>() < size_of::<SongLuaOverlayState>());
    }
}
