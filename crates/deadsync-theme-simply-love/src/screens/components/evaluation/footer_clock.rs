use chrono::{DateTime, Local};
use std::sync::Arc;

/// Retained footer clock shared by the Evaluation screen family.
///
/// The game thread owns one fixed-capacity value for a screen lifetime. It is
/// warmed at screen initialization, reads the wall clock at most once per
/// second, and allocates only when the displayed minute changes. There are no
/// misses, eviction, locking, or background work; screen teardown drops the
/// retained string. Cadence tests provide instrumentation, and worst-case
/// update cost is one clock read plus one short string allocation.
#[derive(Clone)]
pub(crate) struct FooterClock {
    check_elapsed: f32,
    minute: i64,
    text: Arc<str>,
}

impl FooterClock {
    pub(crate) fn new() -> Self {
        Self::at(Local::now())
    }

    fn at(now: DateTime<Local>) -> Self {
        Self {
            check_elapsed: 0.0,
            minute: now.timestamp().div_euclid(60),
            text: Arc::from(now.format("%Y/%m/%d %H:%M").to_string()),
        }
    }

    pub(crate) fn update(&mut self, dt: f32) {
        self.update_with(dt, Local::now);
    }

    fn update_with(&mut self, dt: f32, now: impl FnOnce() -> DateTime<Local>) {
        if dt.is_finite() && dt > 0.0 {
            self.check_elapsed += dt;
        }
        if self.check_elapsed < 1.0 {
            return;
        }
        self.check_elapsed %= 1.0;

        let now = now();
        let minute = now.timestamp().div_euclid(60);
        if minute == self.minute {
            return;
        }
        self.minute = minute;
        self.text = Arc::from(now.format("%Y/%m/%d %H:%M").to_string());
    }

    #[inline(always)]
    pub(crate) fn text(&self) -> &Arc<str> {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn reads_once_per_second_and_formats_once_per_minute() {
        let first = Local.timestamp_opt(0, 0).single().expect("valid timestamp");
        let same_minute = Local
            .timestamp_opt(30, 0)
            .single()
            .expect("valid timestamp");
        let next_minute = Local
            .timestamp_opt(60, 0)
            .single()
            .expect("valid timestamp");
        let mut clock = FooterClock::at(first);
        let first_text = Arc::clone(clock.text());

        clock.update_with(0.5, || panic!("clock read before cadence elapsed"));
        assert!(Arc::ptr_eq(clock.text(), &first_text));

        let mut reads = 0;
        clock.update_with(0.5, || {
            reads += 1;
            same_minute
        });
        assert_eq!(reads, 1);
        assert!(Arc::ptr_eq(clock.text(), &first_text));

        clock.update_with(10.0, || {
            reads += 1;
            next_minute
        });
        assert_eq!(reads, 2);
        assert!(!Arc::ptr_eq(clock.text(), &first_text));
        assert_eq!(
            clock.text().as_ref(),
            next_minute.format("%Y/%m/%d %H:%M").to_string()
        );
    }

    #[test]
    fn ignores_non_positive_and_invalid_deltas() {
        let first = Local.timestamp_opt(0, 0).single().expect("valid timestamp");
        let mut clock = FooterClock::at(first);

        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            clock.update_with(dt, || panic!("invalid delta triggered a clock read"));
        }
    }
}
