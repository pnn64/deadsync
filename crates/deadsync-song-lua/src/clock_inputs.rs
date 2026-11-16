//! Diagnostic replay of external clocks, independent of song/frame time.
//! Only raw host reads enter this tape; actor/query observations never do.

#[derive(Clone, Copy)]
pub(crate) struct ArrowFrameClocks {
    pub tipsy: [Option<f64>; crate::LUA_PLAYERS],
    pub tan_tipsy: [Option<f64>; crate::LUA_PLAYERS],
}
