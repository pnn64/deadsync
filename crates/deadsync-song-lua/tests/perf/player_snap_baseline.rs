// Retired production heuristic retained only by frozen performance baselines.
use crate::SongLuaPerframePlayerState;
use mlua::Table;

fn transform_distance(left: f32, right: f32, cyclic: bool) -> f32 {
    let distance = (left - right).abs();
    if cyclic {
        let wrapped = distance.rem_euclid(360.0);
        wrapped.min(360.0 - wrapped)
    } else {
        distance
    }
}

fn transform_tail_closes(
    current: Option<f32>,
    prior: Option<f32>,
    baseline: Option<f32>,
    default: f32,
    cyclic: bool,
) -> bool {
    let current = current.unwrap_or(default);
    let prior = prior.unwrap_or(default);
    let baseline = baseline.unwrap_or(default);
    // Crossing the baseline can be an explicit new destination. Preserve it
    // rather than treating the smaller distance as an unfinished return tween.
    if !cyclic
        && ((current < baseline && prior > baseline) || (current > baseline && prior < baseline))
    {
        return false;
    }
    let remaining = transform_distance(current, baseline, cyclic);
    let prior_remaining = transform_distance(prior, baseline, cyclic);
    let last_step = transform_distance(current, prior, cyclic);
    remaining < prior_remaining && remaining <= last_step + f32::EPSILON
}

pub(crate) fn snap_ended_transforms(
    actor: &Table,
    current: &mut SongLuaPerframePlayerState,
    prior: SongLuaPerframePlayerState,
    baseline: SongLuaPerframePlayerState,
    ended: u16,
) -> Result<(), String> {
    macro_rules! snap {
        ($index:expr, $field:ident, $state_key:literal, $default:expr, $cyclic:expr) => {
            if ended & (1 << $index) != 0
                && transform_tail_closes(
                    current.$field,
                    prior.$field,
                    baseline.$field,
                    $default,
                    $cyclic,
                )
            {
                current.$field = baseline.$field;
                actor
                    .set($state_key, baseline.$field)
                    .map_err(|err| err.to_string())?;
            }
        };
    }

    snap!(0, x, "__songlua_state_x", 0.0, false);
    snap!(1, y, "__songlua_state_y", 0.0, false);
    snap!(2, z, "__songlua_state_z", 0.0, false);
    snap!(3, rotation_x, "__songlua_state_rot_x_deg", 0.0, true);
    snap!(4, rotation_z, "__songlua_state_rot_z_deg", 0.0, true);
    snap!(5, rotation_y, "__songlua_state_rot_y_deg", 0.0, true);
    snap!(6, zoom_x, "__songlua_state_zoom_x", 1.0, false);
    snap!(7, zoom_y, "__songlua_state_zoom_y", 1.0, false);
    snap!(8, zoom_z, "__songlua_state_zoom_z", 1.0, false);
    snap!(9, skew_x, "__songlua_state_skew_x", 0.0, false);
    snap!(10, skew_y, "__songlua_state_skew_y", 0.0, false);
    Ok(())
}
