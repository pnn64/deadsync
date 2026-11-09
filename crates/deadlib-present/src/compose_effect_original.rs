// Frozen effect arithmetic from cb214e495, before runtime FMA dispatch.
use super::super::anim;

#[inline(always)]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
pub(super) fn apply_effect_to_sprite(
    effect: anim::EffectState,
    elapsed: f32,
    tint: &mut [f32; 4],
    scale: &mut [f32; 2],
    rot_deg: &mut [f32; 3],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if matches!(effect.mode, anim::EffectMode::Spin) {
        // ITGmania spin uses effect delta from clock and does not use effectoffset.
        let units = anim::effect_clock_units(effect, elapsed, beat);
        rot_deg[0] = effect.magnitude[0]
            .mul_add(units, rot_deg[0])
            .rem_euclid(360.0);
        rot_deg[1] = effect.magnitude[1]
            .mul_add(units, rot_deg[1])
            .rem_euclid(360.0);
        rot_deg[2] = effect.magnitude[2]
            .mul_add(units, rot_deg[2])
            .rem_euclid(360.0);
    }

    if matches!(
        effect.mode,
        anim::EffectMode::DiffuseBlink
            | anim::EffectMode::DiffuseRamp
            | anim::EffectMode::DiffuseShift
            | anim::EffectMode::Pulse
    ) && let Some(percent) = anim::effect_mix(effect, elapsed, beat)
    {
        match effect.mode {
            anim::EffectMode::DiffuseBlink => {
                let alpha = tint[3];
                *tint = if percent > 0.5 {
                    effect.color1
                } else {
                    effect.color2
                };
                tint[3] *= alpha;
            }
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = ((percent + 0.25) * 2.0 * std::f32::consts::PI)
                    .sin()
                    .mul_add(0.5, 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowBlink
            | anim::EffectMode::GlowRamp
            | anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
pub(super) fn apply_effect_to_text(
    effect: anim::EffectState,
    elapsed: f32,
    color: &mut [f32; 4],
    scale: &mut [f32; 2],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if matches!(
        effect.mode,
        anim::EffectMode::DiffuseBlink
            | anim::EffectMode::DiffuseRamp
            | anim::EffectMode::DiffuseShift
            | anim::EffectMode::Pulse
    ) && let Some(percent) = anim::effect_mix(effect, elapsed, beat)
    {
        match effect.mode {
            anim::EffectMode::DiffuseBlink => {
                let alpha = color[3];
                *color = if percent > 0.5 {
                    effect.color1
                } else {
                    effect.color2
                };
                color[3] *= alpha;
            }
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = ((percent + 0.25) * 2.0 * std::f32::consts::PI)
                    .sin()
                    .mul_add(0.5, 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowBlink
            | anim::EffectMode::GlowRamp
            | anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    color[0] = color[0].clamp(0.0, 1.0);
    color[1] = color[1].clamp(0.0, 1.0);
    color[2] = color[2].clamp(0.0, 1.0);
    color[3] = color[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}
