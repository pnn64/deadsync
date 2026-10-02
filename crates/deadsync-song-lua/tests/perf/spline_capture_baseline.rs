// Frozen from 14ea3817e (0.5.1681), before spline sample reuse.
use crate::lua_util::note_field_tables;
use mlua::{Lua, Table};
use std::collections::BTreeMap;

fn read_position_spline(
    column: &Table,
    key: &str,
) -> Result<Option<deadsync_gameplay::SongLuaSplineData>, String> {
    let Some(handler) = column
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<String>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?;
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Position") {
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
    let mut points = Vec::with_capacity(size);
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
        points.push(values);
    }
    Ok(Some(deadsync_gameplay::SongLuaSplineData {
        coefficients: deadsync_gameplay::solve_song_lua_spline(&points).into(),
        constant: points.iter().all(|point| *point == points[0]),
        beats_per_t,
        receptor_t,
        subtract_song_beat: handler
            .get::<bool>("__songlua_subtract_song_beat")
            .map_err(|err| err.to_string())?,
    }))
}

fn read_column_position_splines(
    lua: &Lua,
) -> Result<Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>, String> {
    let mut out = Vec::new();
    for field in note_field_tables(lua).map_err(|err| err.to_string())? {
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
            out.push((
                player,
                local_col,
                deadsync_gameplay::SongLuaColumnSplineFrame {
                    second: 0.0,
                    position: read_position_spline(&column, "__songlua_pos_handler")?,
                    zoom: read_position_spline(&column, "__songlua_zoom_handler")?,
                },
            ));
        }
    }
    Ok(out)
}

#[derive(Default)]
pub(super) struct ColumnSplineCapture {
    pub(super) lanes: BTreeMap<(usize, usize), Vec<deadsync_gameplay::SongLuaColumnSplineFrame>>,
    pub(super) bytes: usize,
}

impl ColumnSplineCapture {
    pub(super) fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {
        for (player, column, mut frame) in read_column_position_splines(lua)? {
            let frames = self.lanes.entry((player, column)).or_default();
            if frames
                .last()
                .is_some_and(|last| last.position == frame.position && last.zoom == frame.zoom)
            {
                continue;
            }
            self.bytes += [&frame.position, &frame.zoom]
                .into_iter()
                .flatten()
                .map(|spline| std::mem::size_of_val(spline.coefficients.as_ref()))
                .sum::<usize>();
            if self.bytes > 128 * 1024 * 1024 {
                return Err("Position spline tracks exceed 128 MiB per layer".into());
            }
            frame.second = second;
            frames.push(frame);
        }
        Ok(())
    }
}
