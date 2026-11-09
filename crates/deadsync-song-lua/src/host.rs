use chrono::{Datelike, Local, Timelike};

pub const SONG_LUA_STARTUP_MESSAGE: &str = "__songlua_startup";

#[inline(always)]
#[must_use]
pub fn is_compile_global_name(name: &str) -> bool {
    matches!(
        name,
        "prefix_globals" | "mods" | "mod_time" | "mods_ease" | "mod_perframes" | "mod_actions"
    )
}

pub struct SongLuaDateGlobals {
    pub year: i32,
    pub month_of_year: i32,
    pub day_of_month: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

#[must_use]
pub fn song_lua_local_date_globals() -> SongLuaDateGlobals {
    let now = Local::now();
    SongLuaDateGlobals {
        year: now.year(),
        month_of_year: now.month0() as i32,
        day_of_month: now.day() as i32,
        hour: now.hour() as i32,
        minute: now.minute() as i32,
        second: now.second() as i32,
    }
}

pub(crate) const fn arrow_effects_rotation_x(column: i32) -> f32 {
    // ITGmania's Lua binding reads the third argument as a one-based column.
    // Legacy multitap passes (ps, offset, 0, lane), so its column is zero.
    // PlayerOptions places m_SpeedfMovesZ[15] (default 1) immediately before
    // m_fConfusionX. Safely reproduce that legacy alias, not an out-of-bounds
    // read. ReceptorGetRotationX converts this one radian to degrees.
    if column == 0 {
        180.0 / std::f32::consts::PI
    } else {
        0.0
    }
}
