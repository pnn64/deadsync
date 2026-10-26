//! Frozen production parser bodies from 2e4e32cc4.
use deadsync_config::audio::NoteScrollClock;
use deadsync_config::theme::{DefaultFailType, LogLevel};

pub fn clock(value: &str) -> Result<NoteScrollClock, ()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "rawaudio" | "raw" | "off" | "0" | "false" => Ok(NoteScrollClock::RawAudio),
        "framestable" | "frame-stable" | "frame_stable" | "frame stable" | "smooth"
        | "smoothed" | "itgdestep" | "itg-destep" | "itgdestepped" | "itg-de-stepped"
        | "itg_de_stepped" | "itg" | "on" | "1" | "true" => Ok(NoteScrollClock::FrameStable),
        _ => Err(()),
    }
}

pub fn log(s: &str) -> Result<LogLevel, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "error" | "err" => Ok(LogLevel::Error),
        "warn" | "warning" => Ok(LogLevel::Warn),
        "info" => Ok(LogLevel::Info),
        "debug" => Ok(LogLevel::Debug),
        "trace" => Ok(LogLevel::Trace),
        _ => Err(()),
    }
}

pub fn fail(s: &str) -> Result<DefaultFailType, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "immediate" => Ok(DefaultFailType::Immediate),
        "immediatecontinue" | "immediate_continue" => Ok(DefaultFailType::ImmediateContinue),
        _ => Err(()),
    }
}
