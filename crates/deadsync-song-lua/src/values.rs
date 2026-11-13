pub const SONG_LUA_EASING_NAME_KEY: &str = "__songlua_easing_name";

#[inline(always)]
#[must_use]
pub fn player_number_name(player: usize) -> &'static str {
    match player {
        0 => "PlayerNumber_P1",
        1 => "PlayerNumber_P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}
