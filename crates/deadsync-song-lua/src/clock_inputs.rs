//! Diagnostic replay of external clocks, independent of song/frame time.
//! Only raw host reads enter this tape; actor/query observations never do.
use std::collections::{HashMap, VecDeque};

use mlua::{Function, Lua, MultiValue};

use crate::SongLuaCompileContext;

struct ClockInputs {
    song_root: String,
    reads: HashMap<(String, String, i32), VecDeque<f64>>,
}

fn canonical_source(source: &str, song_root: &str) -> String {
    let source = source.trim_start_matches('@').replace('\\', "/").replace("/./", "/");
    if let Some(relative) = source.strip_prefix(song_root) {
        return format!("song:/{}", relative.trim_start_matches('/'));
    }
    if let Some((_, relative)) = source.split_once("Themes/") {
        if let Some((_, relative)) = relative.split_once('/') {
            return format!("theme:/{relative}");
        }
    }
    source
}

pub(crate) fn install(lua: &Lua, context: &SongLuaCompileContext) {
    let song_root = context.song_dir.to_string_lossy().replace('\\', "/");
    let mut inputs = ClockInputs { song_root, reads: HashMap::new() };
    for input in &context.host_clock_inputs {
        let source = canonical_source(&input.source, &inputs.song_root);
        inputs.reads.entry((input.clock.clone(), source, input.line)).or_default().push_back(input.value);
    }
    lua.set_app_data(inputs);
}

pub(crate) fn next(lua: &Lua, clock: &str) -> Option<f64> {
    if !lua.app_data_ref::<ClockInputs>().is_some_and(|inputs| !inputs.reads.is_empty()) {
        return None;
    }
    let (source, line) = (1..32).find_map(|level| lua.inspect_stack(level, |frame| {
        let info = frame.source();
        (info.what != "C").then(|| info.source.map(|source| (
            source.into_owned(), frame.current_line().map_or(0, |line| line as i32)
        )))
    }).flatten().flatten())?;
    let mut inputs = lua.app_data_mut::<ClockInputs>()?;
    let source = canonical_source(&source, &inputs.song_root);
    inputs.reads.get_mut(&(clock.to_owned(), source, line))?.pop_front()
}

pub(crate) fn install_calendar(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["Year", "MonthOfYear", "DayOfMonth", "DayOfYear", "Hour", "Minute", "Second"] {
        let original: Function = globals.get(name)?;
        globals.set(name, lua.create_function(move |lua, args: MultiValue| {
            if let Some(value) = next(lua, name) { return Ok(value); }
            original.call::<f64>(args)
        })?)?;
    }
    Ok(())
}
