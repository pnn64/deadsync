use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Shared, observational progress for a load worker's finite Lua preparation.
/// Cloned contexts and compiler retries publish to the same monotonic counter.
#[derive(Clone, Debug, Default)]
pub struct SongLuaBakeProgress(Arc<AtomicU32>);

impl SongLuaBakeProgress {
    #[must_use]
    pub fn fraction(&self) -> f32 {
        self.0.load(Ordering::Relaxed) as f32 / 10_000.0
    }

    /// Called after all layers and their resources have been prepared.
    pub fn complete(&self) {
        self.0.store(10_000, Ordering::Relaxed);
    }

    pub(crate) fn report(&self, fraction: f32) {
        if fraction.is_finite() {
            // A compiler pass cannot declare the enclosing preparation complete.
            let value = (fraction.clamp(0.0, 0.99) * 10_000.0) as u32;
            self.0.fetch_max(value, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_progress_is_shared_monotonic_and_completed_by_the_caller() {
        let progress = SongLuaBakeProgress::default();
        let worker = progress.clone();
        std::thread::spawn(move || {
            for value in [0.25, 0.1, f32::NAN, -1.0, 0.75, f32::INFINITY] {
                worker.report(value);
            }
        })
        .join()
        .unwrap();
        assert_eq!(progress.fraction(), 0.75);
        progress.report(1.0);
        assert_eq!(progress.fraction(), 0.99);
        progress.complete();
        progress.report(0.5);
        assert_eq!(progress.fraction(), 1.0);
    }
}
