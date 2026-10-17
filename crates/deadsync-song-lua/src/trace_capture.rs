//! Structured trace delivery for diagnostic replay tools.
use std::cell::Cell;

thread_local! {
    static SINK: Cell<Option<fn(serde_json::Value)>> = const { Cell::new(None) };
}

/// Select a trace consumer on this replay thread; `None` restores text logging.
pub fn set_sink(sink: Option<fn(serde_json::Value)>) {
    SINK.set(sink);
}

pub(crate) fn emit(event: serde_json::Value) {
    if let Some(sink) = SINK.get() {
        sink(event);
    } else {
        log::trace!(target: "deadsync_song_lua::trace", "{event}");
    }
}
