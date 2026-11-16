use crate::SongLuaColumnOffsetWindow;
use crate::SongLuaCompileInfo;
use crate::SongLuaEaseTarget;
use crate::SongLuaEaseWindow;

use crate::SongLuaOverlayEase;

pub struct SongLuaReadEasesResult {
    pub eases: Vec<SongLuaEaseWindow>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindow>,
    pub info: SongLuaCompileInfo,
    pub stats: SongLuaReadEasesStats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaReadEasesStats {
    pub entry_count: usize,
    pub function_targets: usize,
    pub overlay_capture_attempts: usize,
    pub overlay_capture_outputs: usize,
    pub probe_ms: f64,
    pub overlay_capture_ms: f64,
}

pub struct SongLuaFunctionEaseResult {
    pub decision: SongLuaFunctionEaseDecision,
    pub stats: SongLuaReadEasesStats,
}

pub enum SongLuaFunctionEaseDecision {
    ColumnOffsets(Vec<SongLuaColumnOffsetWindow>),
    OverlayEases(Vec<SongLuaOverlayEase>),
    Target(SongLuaEaseTarget),
    Skip,
}
