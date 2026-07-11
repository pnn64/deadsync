use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use log::trace;
use std::time::{Duration, Instant};

const TRACE_INTERVAL: Duration = Duration::from_secs(1);
const SLOW_BATCH_US: u32 = 1_000;
const BURST_KEYS: u32 = 8;

#[derive(Clone, Copy)]
struct EventBatchTrace {
    started_at: Instant,
    gameplay_seen: bool,
    key_events: u32,
    key_repeat_events: u32,
    pad_events: u32,
    queued_events: u32,
    app_handler_sum_us: u64,
    app_handler_max_us: u32,
}

impl EventBatchTrace {
    #[inline(always)]
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            gameplay_seen: false,
            key_events: 0,
            key_repeat_events: 0,
            pad_events: 0,
            queued_events: 0,
            app_handler_sum_us: 0,
            app_handler_max_us: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

#[derive(Clone, Copy)]
struct EventTrace {
    started_at: Instant,
    batches: u32,
    key_events: u32,
    key_repeat_events: u32,
    pad_events: u32,
    queued_events: u32,
    batch_sum_us: u64,
    batch_max_us: u32,
    app_handler_sum_us: u64,
    app_handler_max_us: u32,
    dispatch_overhead_sum_us: u64,
    dispatch_overhead_max_us: u32,
    slow_batches: u32,
}

impl EventTrace {
    #[inline(always)]
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            batches: 0,
            key_events: 0,
            key_repeat_events: 0,
            pad_events: 0,
            queued_events: 0,
            batch_sum_us: 0,
            batch_max_us: 0,
            app_handler_sum_us: 0,
            app_handler_max_us: 0,
            dispatch_overhead_sum_us: 0,
            dispatch_overhead_max_us: 0,
            slow_batches: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

pub struct GameplayInputTrace {
    batch: EventBatchTrace,
    summary: EventTrace,
}

impl GameplayInputTrace {
    pub fn new(now: Instant) -> Self {
        Self {
            batch: EventBatchTrace::new(now),
            summary: EventTrace::new(now),
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, now: Instant) {
        self.batch.reset(now);
        self.summary.reset(now);
    }

    #[inline(always)]
    pub fn note_new_events(&mut self, now: Instant) {
        self.batch.reset(now);
    }

    #[inline(always)]
    pub fn note_key_handler(&mut self, gameplay_screen: bool, repeat: bool, handler_us: u32) {
        if !gameplay_screen {
            return;
        }
        self.batch.gameplay_seen = true;
        self.batch.key_events = self.batch.key_events.saturating_add(1);
        self.batch.key_repeat_events = self.batch.key_repeat_events.saturating_add(repeat as u32);
        self.note_handler_time(handler_us);
    }

    #[inline(always)]
    pub fn note_pad_handler(&mut self, gameplay_screen: bool, handler_us: u32) {
        if !gameplay_screen {
            return;
        }
        self.batch.gameplay_seen = true;
        self.batch.pad_events = self.batch.pad_events.saturating_add(1);
        self.note_handler_time(handler_us);
    }

    #[inline(always)]
    pub fn note_queued_input(&mut self) {
        self.batch.gameplay_seen = true;
        self.batch.queued_events = self.batch.queued_events.saturating_add(1);
    }

    #[inline(always)]
    fn note_handler_time(&mut self, handler_us: u32) {
        self.batch.app_handler_sum_us = self
            .batch
            .app_handler_sum_us
            .saturating_add(u64::from(handler_us));
        self.batch.app_handler_max_us = self.batch.app_handler_max_us.max(handler_us);
    }

    pub fn finish_batch(&mut self, now: Instant, screen: Screen) {
        let batch = &mut self.batch;
        if !batch.gameplay_seen
            || (batch.key_events == 0 && batch.pad_events == 0 && batch.queued_events == 0)
        {
            if now.duration_since(self.summary.started_at) >= TRACE_INTERVAL {
                self.summary.reset(now);
            }
            batch.reset(now);
            return;
        }

        let batch_us = deadsync_config::frame_pacing::elapsed_us_between(now, batch.started_at);
        let app_handler_sum_us = batch.app_handler_sum_us.min(u64::from(u32::MAX)) as u32;
        let dispatch_overhead_us = batch_us.saturating_sub(app_handler_sum_us);
        if batch_us >= SLOW_BATCH_US || batch.key_events >= BURST_KEYS {
            trace!(
                "Gameplay event batch: screen={:?} keys={} repeats={} pads={} queued={} batch_ms={:.3} app_ms={:.3} dispatch_ms={:.3} app_max_ms={:.3}",
                screen,
                batch.key_events,
                batch.key_repeat_events,
                batch.pad_events,
                batch.queued_events,
                batch_us as f32 / 1000.0,
                app_handler_sum_us as f32 / 1000.0,
                dispatch_overhead_us as f32 / 1000.0,
                batch.app_handler_max_us as f32 / 1000.0
            );
        }

        let summary = &mut self.summary;
        summary.batches = summary.batches.saturating_add(1);
        summary.key_events = summary.key_events.saturating_add(batch.key_events);
        summary.key_repeat_events = summary
            .key_repeat_events
            .saturating_add(batch.key_repeat_events);
        summary.pad_events = summary.pad_events.saturating_add(batch.pad_events);
        summary.queued_events = summary.queued_events.saturating_add(batch.queued_events);
        summary.batch_sum_us = summary.batch_sum_us.saturating_add(u64::from(batch_us));
        summary.batch_max_us = summary.batch_max_us.max(batch_us);
        summary.app_handler_sum_us = summary
            .app_handler_sum_us
            .saturating_add(batch.app_handler_sum_us);
        summary.app_handler_max_us = summary.app_handler_max_us.max(batch.app_handler_max_us);
        summary.dispatch_overhead_sum_us = summary
            .dispatch_overhead_sum_us
            .saturating_add(u64::from(dispatch_overhead_us));
        summary.dispatch_overhead_max_us =
            summary.dispatch_overhead_max_us.max(dispatch_overhead_us);
        summary.slow_batches = summary
            .slow_batches
            .saturating_add((batch_us >= SLOW_BATCH_US) as u32);

        if now.duration_since(summary.started_at) >= TRACE_INTERVAL {
            let batches = summary.batches.max(1);
            trace!(
                "Gameplay raw input: batches={} keys={} repeats={} pads={} queued={} batch_ms=[avg:{:.3} max:{:.3}] app_ms=[avg:{:.3} max:{:.3}] dispatch_ms=[avg:{:.3} max:{:.3}] slow_batches={}",
                summary.batches,
                summary.key_events,
                summary.key_repeat_events,
                summary.pad_events,
                summary.queued_events,
                summary.batch_sum_us as f32 / batches as f32 / 1000.0,
                summary.batch_max_us as f32 / 1000.0,
                summary.app_handler_sum_us as f32 / batches as f32 / 1000.0,
                summary.app_handler_max_us as f32 / 1000.0,
                summary.dispatch_overhead_sum_us as f32 / batches as f32 / 1000.0,
                summary.dispatch_overhead_max_us as f32 / 1000.0,
                summary.slow_batches
            );
            summary.reset(now);
        }
        batch.reset(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_aggregates_gameplay_input_only() {
        let started = Instant::now();
        let mut trace = GameplayInputTrace::new(started);
        trace.note_key_handler(false, false, 50);
        trace.note_key_handler(true, true, 100);
        trace.note_pad_handler(true, 200);
        trace.note_queued_input();
        trace.finish_batch(started + Duration::from_micros(800), Screen::Gameplay);

        assert_eq!(trace.summary.batches, 1);
        assert_eq!(trace.summary.key_events, 1);
        assert_eq!(trace.summary.key_repeat_events, 1);
        assert_eq!(trace.summary.pad_events, 1);
        assert_eq!(trace.summary.queued_events, 1);
        assert_eq!(trace.summary.app_handler_sum_us, 300);
        assert_eq!(trace.summary.dispatch_overhead_sum_us, 500);
    }
}
