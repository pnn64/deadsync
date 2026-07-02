use log::info;
use std::path::Path;
use std::time::Instant;

pub struct SongLuaCompileTimer {
    compile_started: Instant,
    stage_started: Instant,
    stage_times: Vec<(&'static str, f64)>,
}

impl SongLuaCompileTimer {
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            compile_started: now,
            stage_started: now,
            stage_times: Vec::new(),
        }
    }

    pub fn push_stage(&mut self, stage: &'static str) {
        self.stage_times
            .push((stage, self.stage_started.elapsed().as_secs_f64() * 1000.0));
        self.stage_started = Instant::now();
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.compile_started.elapsed().as_secs_f64() * 1000.0
    }

    pub fn should_log(&self) -> bool {
        self.elapsed_ms() >= 1000.0
    }

    pub fn stage_summary(&self) -> String {
        song_lua_compile_stage_summary(&self.stage_times)
    }
}

pub fn song_lua_compile_stage_summary(stage_times: &[(&'static str, f64)]) -> String {
    let mut stages = String::new();
    for (stage, ms) in stage_times {
        if !stages.is_empty() {
            stages.push(' ');
        }
        stages.push_str(stage);
        stages.push_str("_ms=");
        stages.push_str(format!("{ms:.3}").as_str());
    }
    stages
}

pub fn log_song_lua_compile_timing(entry_path: &Path, compile_timer: &SongLuaCompileTimer) {
    if !compile_timer.should_log() {
        return;
    }
    let elapsed_ms = compile_timer.elapsed_ms();
    let stages = compile_timer.stage_summary();
    info!(
        "Song lua compile timing: entry='{}' elapsed_ms={elapsed_ms:.3} {}",
        entry_path.display(),
        stages
    );
}
