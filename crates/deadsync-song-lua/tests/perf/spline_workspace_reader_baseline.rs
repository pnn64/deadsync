// Frozen reader from 98d79dc83 (0.5.1683).
use super::*;
include!("../../../deadsync-gameplay/tests/perf/spline_workspace_baseline.rs");
pub(super) fn read_position_spline(
    column: &Table,
    key: &str,
    previous: Option<&deadsync_gameplay::SongLuaSplineData>,
    points: &mut Vec<[f32; 3]>,
) -> Result<Option<deadsync_gameplay::SongLuaSplineData>, String> {
    let Some(handler) = column
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<LuaFieldText<32>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?;
    if !mode
        .as_str()
        .eq_ignore_ascii_case("NoteColumnSplineMode_Position")
    {
        return Ok(None);
    }
    let spline = handler
        .get::<Table>("__songlua_spline")
        .map_err(|err| err.to_string())?;
    let size = spline
        .get::<usize>("__songlua_spline_size")
        .map_err(|err| err.to_string())?;
    if size == 0 {
        return Ok(None);
    }
    if size > 65536 {
        return Err("Position spline exceeds 65536 points".into());
    }
    let beats_per_t = handler
        .get::<f32>("__songlua_beats_per_t")
        .map_err(|err| err.to_string())?;
    let receptor_t = handler
        .get::<f32>("__songlua_receptor_t")
        .map_err(|err| err.to_string())?;
    if !beats_per_t.is_finite() || beats_per_t == 0.0 || !receptor_t.is_finite() {
        return Err("Position spline has an invalid beat conversion".into());
    }
    let table = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    points.clear();
    points.reserve_exact(size);
    let mut unchanged = previous.is_some_and(|spline| spline.coefficients.len() == size);
    for index in 1..=size {
        let point = table
            .raw_get::<Table>(index)
            .map_err(|err| err.to_string())?;
        let mut values = [0.0; 3];
        for (axis, value) in values.iter_mut().enumerate() {
            *value = point
                .raw_get::<Option<f32>>(axis + 1)
                .map_err(|err| err.to_string())?
                .unwrap_or(0.0);
            if !value.is_finite() {
                return Err("Position spline contains a nonfinite point".into());
            }
        }
        // Compare authored coordinates, not table identity: Lua can mutate a
        // point in place. Bits preserve signed zero even when metadata changes.
        unchanged = unchanged
            && previous
                .and_then(|spline| spline.coefficients.get(index - 1))
                .is_some_and(|coefficients| {
                    values
                        .iter()
                        .zip(coefficients)
                        .all(|(value, axis)| value.to_bits() == axis[0].to_bits())
                });
        points.push(values);
    }
    Ok(Some(deadsync_gameplay::SongLuaSplineData {
        coefficients: if unchanged {
            std::sync::Arc::clone(&previous.expect("matching spline exists").coefficients)
        } else {
            old_solve_song_lua_spline(points).into()
        },
        constant: points.iter().all(|point| *point == points[0]),
        beats_per_t,
        receptor_t,
        subtract_song_beat: handler
            .get::<bool>("__songlua_subtract_song_beat")
            .map_err(|err| err.to_string())?,
    }))
}

/// Reuse capture scratch and the preceding frame's immutable coefficients.
/// Every Lua field and coordinate is still read and validated on every sample.
pub(crate) fn read_column_position_splines<'a>(
    lua: &Lua,
    mut previous: impl FnMut(usize, usize) -> Option<&'a deadsync_gameplay::SongLuaColumnSplineFrame>,
    points: &mut Vec<[f32; 3]>,
    out: &mut Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
) -> Result<(), String> {
    out.clear();
    let mut fields = smallvec::SmallVec::<[Table; LUA_PLAYERS]>::new();
    for_each_note_field(lua, |field| {
        fields.push(field);
        Ok(())
    })
    .map_err(|err| err.to_string())?;
    for field in fields {
        let Some(columns) = field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let player = column
                .get::<usize>("__songlua_player_index")
                .map_err(|err| err.to_string())?;
            let local_col = column
                .get::<usize>("__songlua_column_index")
                .map_err(|err| err.to_string())?;
            let last = previous(player, local_col);
            out.push((
                player,
                local_col,
                deadsync_gameplay::SongLuaColumnSplineFrame {
                    second: 0.0,
                    position: read_position_spline(
                        &column,
                        "__songlua_pos_handler",
                        last.and_then(|frame| frame.position.as_ref()),
                        points,
                    )?,
                    zoom: read_position_spline(
                        &column,
                        "__songlua_zoom_handler",
                        last.and_then(|frame| frame.zoom.as_ref()),
                        points,
                    )?,
                },
            ));
        }
    }
    Ok(())
}
