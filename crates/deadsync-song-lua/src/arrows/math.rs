//! The established native float algorithms, using rooted Rust host values.
use super::*;

fn indexed_n(
    call: &mut HostCall<'_>,
    state: Option<TableId>,
    prefix: &str,
    index: impl itoa::Integer,
) -> Result<f32, Error> {
    let mut number = itoa::Buffer::new();
    let suffix = number.format(index);
    let mut key = [0_u8; 40];
    let end = prefix.len() + suffix.len();
    key[..prefix.len()].copy_from_slice(prefix.as_bytes());
    key[prefix.len()..end].copy_from_slice(suffix.as_bytes());
    n_named(
        call,
        state,
        std::str::from_utf8(&key[..end]).expect("ASCII modifier key"),
    )
}
pub(super) fn x_pos(call: &mut HostCall<'_>, b: &Data, args: &[Value]) -> Result<f32, Error> {
    with_options(call, args, |call, options, state| {
        use std::f32::consts::PI;
        let player = player_for(call, options)?;
        let column = strict(call, args, 1, 1.0)? as i32 - 1;
        let y = strict(call, args, 2, 0.0)?;
        let style = style_info(call);
        let column = usize::try_from(column)
            .ok()
            .filter(|column| *column < style.columns)
            .ok_or_else(|| call.error(b"ArrowEffects.GetXPos column out of range"))?;
        let base_x = style.x_offsets[column];
        let two_sides = matches!(
            style.style_type,
            "StyleType_OnePlayerTwoSides" | "StyleType_TwoPlayersSharedSides"
        );
        // _fallback GameplayMargins reserves 272px per side, 544px when centered.
        // ScreenGameplay clamps the resulting style fit zoom to one.
        let field_zoom = field_zoom(&style);
        let cosecant = flag(call, state, "cosecant")?;
        let tangent = |angle: f32| {
            if cosecant {
                1.0 / angle.sin()
            } else {
                angle.tan()
            }
        };
        let scale = |x: f32, low: f32, high: f32, to_low: f32, to_high: f32| {
            (x - low) * (to_high - to_low) / (high - low) + to_low
        };
        let mut x = 0.0;
        for (amount, offset, period, tan) in [
            ("tornado", "tornadooffset", "tornadoperiod", false),
            ("tantornado", "tantornadooffset", "tantornadoperiod", true),
        ] {
            let amount = n(call, state, amount)?;
            if amount == 0.0 {
                continue;
            }
            let [low, high] = b.state.cache.lock().unwrap().bounds[player][column];
            let low = low * field_zoom;
            let high = high * field_zoom;
            let position = scale(base_x * field_zoom, low, high, -1.0, 1.0);
            let angle = position.acos()
                + (y + n(call, state, offset)?) * (n(call, state, period)? * 6.0 + 6.0) / b.height;
            let wave = if tan { tangent(angle) } else { angle.cos() };
            x += (scale(wave, -1.0, 1.0, low, high) - base_x * field_zoom) * amount;
        }
        for (amount, offset, period, tan) in [
            ("bumpyx", "bumpyxoffset", "bumpyxperiod", false),
            ("tanbumpyx", "tanbumpyxoffset", "tanbumpyxperiod", true),
        ] {
            let amount = n(call, state, amount)?;
            if amount == 0.0 {
                continue;
            }
            let angle =
                (y + 100.0 * n(call, state, offset)?) / (n(call, state, period)? * 16.0 + 16.0);
            x += amount * 40.0 * if tan { tangent(angle) } else { angle.sin() };
        }
        for (amount, speed, offset, period, tan) in [
            ("drunk", "drunkspeed", "drunkoffset", "drunkperiod", false),
            (
                "tandrunk",
                "tandrunkspeed",
                "tandrunkoffset",
                "tandrunkperiod",
                true,
            ),
        ] {
            let amount = n(call, state, amount)?;
            if amount == 0.0 {
                continue;
            }
            let time = query_time(call, b, state, "ArrowEffects.GetXPos")?;
            let angle = time * (1.0 + n(call, state, speed)?)
                + column as f32 * (n(call, state, offset)? * 0.2 + 0.2)
                + y * (n(call, state, period)? * 10.0 + 10.0) / b.height;
            let wave = if tan { tangent(angle) } else { angle.cos() };
            x += amount * (wave * 64.0 * 0.5);
        }
        if n(call, state, "flip")? != 0.0 {
            x += (style.x_offsets[style.columns - 1 - column] * field_zoom - base_x * field_zoom)
                * n(call, state, "flip")?;
        }
        if n(call, state, "invert")? != 0.0 {
            let distance = b.state.cache.lock().unwrap().invert[player][column];
            x += distance * n(call, state, "invert")?;
        }
        if n(call, state, "beat")? != 0.0 {
            let factor = b.state.cache.lock().unwrap().beat_x[player];
            x += n(call, state, "beat")?
                * (factor * (y / (n(call, state, "beatperiod")? * 15.0 + 15.0) + PI / 2.0).sin());
        }
        if n(call, state, "zigzag")? != 0.0 {
            let angle = PI
                * (1.0 / (n(call, state, "zigzagperiod")? + 1.0))
                * ((y + 100.0 * n(call, state, "zigzagoffset")?) / 64.0);
            let mut phase = angle % (PI * 2.0);
            if phase < 0.0 {
                phase += PI * 2.0;
            }
            let phase = f64::from(phase * (1.0 / PI));
            let wave = if phase < 0.5 {
                phase * 2.0
            } else if phase < 1.5 {
                1.0 - (phase - 0.5) * 2.0
            } else {
                -4.0 + phase * 2.0
            } as f32;
            x += (n(call, state, "zigzag")? * 64.0 / 2.0) * wave;
        }
        if n(call, state, "sawtooth")? != 0.0 {
            let phase = (0.5 / (n(call, state, "sawtoothperiod")? + 1.0) * y) / 64.0;
            x += (n(call, state, "sawtooth")? * 64.0) * (phase - phase.floor());
        }
        if n(call, state, "parabolax")? != 0.0 {
            x += n(call, state, "parabolax")? * (y / 64.0) * (y / 64.0);
        }
        if n(call, state, "attenuatex")? != 0.0 {
            x += n(call, state, "attenuatex")? * (y / 64.0) * (y / 64.0) * (base_x / 64.0);
        }
        for (amount, offset, period, steps, tan) in [
            (
                "digital",
                "digitaloffset",
                "digitalperiod",
                "digitalsteps",
                false,
            ),
            (
                "tandigital",
                "tandigitaloffset",
                "tandigitalperiod",
                "tandigitalsteps",
                true,
            ),
        ] {
            let amount = n(call, state, amount)?;
            if amount == 0.0 {
                continue;
            }
            let angle =
                PI * (y + n(call, state, offset)?) / (64.0 + n(call, state, period)? * 64.0);
            let wave = if tan { tangent(angle) } else { angle.sin() };
            x += (amount * 64.0 * 0.5) * ((n(call, state, steps)? + 1.0) * wave).round()
                / (n(call, state, steps)? + 1.0);
        }
        if n(call, state, "square")? != 0.0 {
            let angle = PI * (y + n(call, state, "squareoffset")?)
                / (64.0 + n(call, state, "squareperiod")? * 64.0);
            let mut phase = angle % (PI * 2.0);
            if phase < 0.01 {
                phase += PI * 2.0;
            }
            x += (n(call, state, "square")? * 64.0 * 0.5) * if phase >= PI { -1.0 } else { 1.0 };
        }
        if n(call, state, "bounce")? != 0.0 {
            x += n(call, state, "bounce")?
                * 64.0
                * 0.5
                * ((y + n(call, state, "bounceoffset")?)
                    / (60.0 + n(call, state, "bounceperiod")? * 60.0))
                    .sin()
                    .abs();
        }
        if n(call, state, "xmode")? != 0.0 {
            let negative = if two_sides {
                column >= style.columns / 2
            } else {
                player == 1
            };
            x += n(call, state, "xmode")? * if negative { -y } else { y };
        }
        x += base_x * field_zoom;
        if n(call, state, "tiny")? != 0.0 {
            x *= 0.5f32.powf(n(call, state, "tiny")?).min(1.0);
        }
        Ok(x)
    })
}
pub(super) fn zoom(call: &mut HostCall<'_>, _b: &Data, args: &[Value]) -> Result<f32, Error> {
    with_options(call, args, |call, _options, state| {
        let y = strict(call, args, 1, 0.0)?;
        let style = style_info(call);
        let column = strict(call, args, 2, 1.0)? as i32 - 1;
        let column = usize::try_from(column)
            .ok()
            .filter(|column| *column < style.columns)
            .ok_or_else(|| call.error(b"ArrowEffects.GetZoom column out of range"))?;
        let mut zoom = field_zoom(&style);
        let inner = n(call, state, "pulseinner")?;
        let outer = n(call, state, "pulseouter")?;
        if inner != 0.0 || outer != 0.0 {
            let sine = ((y + 100.0 * n(call, state, "pulseoffset")?)
                / (0.4 * (64.0 + n(call, state, "pulseperiod")? * 64.0)))
                .sin();
            let mut inner = inner * 0.5 + 1.0;
            if inner == 0.0 {
                inner = 0.01;
            }
            zoom *= sine * (outer * 0.5) + inner;
        }
        let shrink_mult = n(call, state, "shrinkmult")?;
        if shrink_mult != 0.0 && y >= 0.0 {
            zoom *= 1.0 / (1.0 + y * (shrink_mult / 100.0));
        }
        let shrink_linear = n(call, state, "shrinklinear")?;
        if shrink_linear != 0.0 && y >= 0.0 {
            zoom += y * (0.5 * shrink_linear / 64.0);
        }
        let tiny = n(call, state, "tiny")?;
        if tiny != 0.0 {
            zoom *= 0.5f32.powf(tiny);
        }
        let tiny = indexed_n(call, state, "tiny", column + 1)?;
        if tiny != 0.0 {
            zoom *= 0.5f32.powf(tiny);
        }
        Ok(zoom)
    })
}
// Keep the captured native float expression for the far-edge fade.
#[allow(clippy::neg_multiply)]
pub(super) fn alpha(call: &mut HostCall<'_>, b: &Data, args: &[Value]) -> Result<f32, Error> {
    with_options(call, args, |call, _options, state| {
        let column = strict(call, args, 1, 1.0)? as i32 - 1;
        let offset = strict(call, args, 2, 0.0)?;
        let fade_to_fail = strict(call, args, 3, -1.0)?;
        // Argument five controls reverse placement, which GetAlpha deliberately
        // disables in GetYPos. It still accepts the native optional argument.
        let _reverse_offset = strict(
            call,
            args,
            4,
            crate::THEME_RECEPTOR_Y_REV - crate::THEME_RECEPTOR_Y_STD,
        )?;
        let draw_distance = strict(call, args, 5, b.height * 1.5)?;
        let fade_in = strict(call, args, 6, 0.0)?;
        let (beat, time) = alpha_clock(call, state)?;
        let mut y = offset;
        let tipsy = n(call, state, "tipsy")?;
        if tipsy != 0.0 {
            let phase = time * (n(call, state, "tipsyspeed")? * 1.2 + 1.2)
                + column as f32 * (n(call, state, "tipsyoffset")? * 1.8 + 1.8);
            y += tipsy * (phase.cos() * (64.0 * 0.4));
        }
        let tan_tipsy = n(call, state, "tantipsy")?;
        if tan_tipsy != 0.0 {
            let phase = time * (n(call, state, "tantipsyspeed")? * 1.2 + 1.2)
                + column as f32 * (n(call, state, "tantipsyoffset")? * 1.8 + 1.8);
            let wave = if flag(call, state, "cosecant")? {
                1.0 / phase.sin()
            } else {
                phase.tan()
            };
            y += tan_tipsy * (wave * (64.0 * 0.4));
        }
        let attenuate = n(call, state, "attenuatey")?;
        if attenuate != 0.0 {
            let x = style_info(call)
                .x_offsets
                .get(column.max(0) as usize)
                .copied()
                .unwrap_or(0.0);
            y += attenuate * (offset / 64.0) * (offset / 64.0) * (x / 64.0);
        }
        let beat_y = n(call, state, "beaty")?;
        if beat_y != 0.0 {
            let mut pulse = (beat + 0.2 + n(call, state, "beatyoffset")?)
                * (n(call, state, "beatymult")? + 1.0);
            let odd = (pulse as i32) % 2 != 0;
            let mut factor = 0.0;
            if pulse >= 0.0 {
                pulse -= pulse.trunc();
                pulse += 1.0;
                pulse -= pulse.trunc();
                if pulse < 0.5 {
                    factor = if pulse < 0.2 {
                        let phase = pulse / 0.2;
                        phase * phase
                    } else {
                        let phase = 1.0 - (pulse - 0.2) / (0.5 - 0.2);
                        1.0 - (1.0 - phase) * (1.0 - phase)
                    };
                    if odd {
                        factor = -factor;
                    }
                    factor *= 20.0;
                }
            }
            y += beat_y
                * (factor
                    * (offset / (n(call, state, "beatyperiod")? * 15.0 + 15.0)
                        + std::f32::consts::PI / 2.0)
                        .sin());
        }
        let center = 160.0 / (1.0 - n(call, state, "mini")? * 0.5);
        let appearance_y = if flag(call, state, "stealthtype")? {
            offset
        } else {
            y
        };
        let mut visible = 1.0;
        if appearance_y >= 0.0 || flag(call, state, "stealthpastreceptors")? {
            let hidden = n(call, state, "hidden")?;
            let sudden = n(call, state, "sudden")?;
            let combined = hidden * sudden;
            let scale = |x: f32, low: f32, high: f32, from: f32, to: f32| {
                (x - low) / (high - low) * (to - from) + from
            };
            let mut adjust = 0.0;
            if hidden != 0.0 {
                let start =
                    center + 40.0 * (-0.25 * combined) + center * n(call, state, "hiddenoffset")?;
                let end = center
                    + 40.0 * (-1.0 - 0.25 * combined)
                    + center * n(call, state, "hiddenoffset")?;
                adjust += hidden * scale(appearance_y, start, end, 0.0, -1.0).clamp(-1.0, 0.0);
            }
            if sudden != 0.0 {
                let start = center
                    + 40.0 * (1.0 + 0.25 * combined)
                    + center * n(call, state, "suddenoffset")?;
                let end =
                    center + 40.0 * (0.25 * combined) + center * n(call, state, "suddenoffset")?;
                adjust += sudden * scale(appearance_y, start, end, -1.0, 0.0).clamp(-1.0, 0.0);
            }
            adjust -= n(call, state, "stealth")?;
            adjust -= indexed_n(call, state, "stealth", column + 1)?;
            if n(call, state, "blink")? != 0.0 {
                let wave = ((time * 10.0).sin() / 0.3333).round() * 0.3333;
                adjust += scale(wave, 0.0, 1.0, -1.0, 0.0);
            }
            let vanish = n(call, state, "randomvanish")?;
            if vanish != 0.0 {
                adjust += scale((y - center).abs(), 80.0, 160.0, -1.0, 0.0) * vanish;
            }
            visible = (1.0 + adjust).clamp(0.0, 1.0);
        }
        if fade_to_fail != -1.0 {
            visible = 1.0 - fade_to_fail;
        }
        let full_alpha_y = draw_distance * (1.0 - fade_in);
        if y > full_alpha_y {
            // Preserve the native raw float result: zero fade width divides by
            // zero, and positions beyond the far edge produce negative alpha.
            return Ok((y - full_alpha_y) / (draw_distance - full_alpha_y) * -1.0 + 1.0);
        }
        Ok(if visible > 0.5 { 1.0 } else { 0.0 })
    })
}
fn field_zoom(style: &crate::SongLuaStyleInfo) -> f32 {
    let two_sides = matches!(
        style.style_type,
        "StyleType_OnePlayerTwoSides" | "StyleType_TwoPlayersSharedSides"
    );
    ((if two_sides { 544.0 } else { 272.0 }) / style.width).min(1.0)
}

pub(super) fn y_pos(call: &mut HostCall<'_>, b: &Data, args: &[Value]) -> Result<f32, Error> {
    let y_offset = loose(call, args, 2, 0.);
    with_options(call, args, |call, options, state| {
        let style = style_info(call);
        let column = loose(call, args, 1, 1.0) as i32 - 1;
        let column = usize::try_from(column)
            .ok()
            .filter(|column| *column < style.columns)
            .ok_or_else(|| call.error(b"ArrowEffects.GetYPos column out of range"))?;
        let player = player_for(call, options)?;
        let reverse = reverse_percent(call, args)?;
        let reverse_offset = loose(
            call,
            args,
            3,
            crate::THEME_RECEPTOR_Y_REV - crate::THEME_RECEPTOR_Y_STD,
        );
        let zoom = 1.0 - n(call, state, "mini")? * 0.5;
        let zoom = if zoom.abs() < 0.01 { 0.01 } else { zoom };
        let half = reverse_offset / zoom / 2.0;
        let shift = reverse * (half + half) - half;
        let shift = n(call, state, "centered")? * -shift + shift;
        let mut y = y_offset * (reverse * -2.0 + 1.0) + shift;
        let cached = *b.state.cache.lock().unwrap();
        y += n(call, state, "tipsy")? * cached.tipsy[player][column];
        y += n(call, state, "tantipsy")? * cached.tan_tipsy[player][column];
        if n(call, state, "attenuatey")? != 0.0 {
            y += n(call, state, "attenuatey")?
                * (y_offset / 64.0)
                * (y_offset / 64.0)
                * (style.x_offsets[column] / 64.0);
        }
        if n(call, state, "beaty")? != 0.0 {
            let factor = cached.beat_y[player];
            let shift = factor
                * (y_offset / (n(call, state, "beatyperiod")? * 15.0 + 15.0)
                    + std::f32::consts::PI / 2.0)
                    .sin();
            y += n(call, state, "beaty")? * shift;
        }
        // Captured _fallback QuantizeArrowYPosition is false.
        Ok(y)
    })
}
pub(super) fn rotation_z(call: &mut HostCall<'_>, _b: &Data, args: &[Value]) -> Result<f32, Error> {
    use std::f32::consts::PI;
    with_options(call, args, |call, options, state| {
        let player = player_for(call, options)?;
        let t = runtime(call, true)?;
        let beat = call.with_roots(&[Value::Table(t)], |call| {
            let v = raw(call, t, PLAYERS)?;
            let positions = optional_table(call, v)?;
            position_beat(call, t, positions, player)
        })?;
        let column = loose(call, args, 3, 1.0) as i32;
        let mut rotation = 0.0_f32;
        // ArrowEffects::ReceptorGetRotationZ uses radians for both offset levels.
        let column_offset = indexed_n(call, state, "confusionoffset", column)?;
        if column_offset != 0.0 {
            rotation += column_offset * 180.0 / PI;
        }
        let offset = n(call, state, "confusionoffset")?;
        if offset != 0.0 {
            rotation += offset * 180.0 / PI;
        }
        let confusion = n(call, state, "confusion")?;
        if confusion != 0.0 {
            let mut phase = beat * confusion;
            phase %= 2.0 * PI;
            phase *= -180.0 / PI;
            rotation += phase;
        }
        let hold_head = !matches!(args.get(2), None | Some(Value::Nil | Value::Boolean(false)));
        let dizzy_holds = flag(call, state, "dizzyholds")?;
        let dizzy = n(call, state, "dizzy")?;
        if dizzy != 0.0 && (dizzy_holds || !hold_head) {
            let note_beat = loose(call, args, 1, 0.0);
            let mut phase = note_beat - beat;
            phase *= dizzy;
            phase %= 2.0 * PI;
            phase *= 180.0 / PI;
            rotation += phase;
        }
        Ok(rotation)
    })
}
