use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) struct Progress {
    started: Instant,
    stage: Mutex<String>,
    pub(super) checks: AtomicUsize,
    pub(super) failed: AtomicUsize,
    pub(super) frame: AtomicUsize,
    pub(super) frames: AtomicUsize,
}

impl Progress {
    pub(super) fn stage(&self, name: impl Into<String>) {
        let name = name.into();
        eprintln!("[{:.1}s] {name}", self.started.elapsed().as_secs_f64());
        *self.stage.lock().unwrap() = name;
    }

    fn heartbeat(&self) {
        let stage = self.stage.lock().unwrap();
        let checks = self.checks.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let frames = self.frames.load(Ordering::Relaxed);
        let frame = self.frame.load(Ordering::Relaxed);
        if frames > 0 && frame < frames {
            eprintln!(
                "[{:.1}s] {stage}: frame {frame}/{frames} ({:.1}%)",
                self.started.elapsed().as_secs_f64(),
                100.0 * frame as f64 / frames as f64
            );
        } else {
            eprintln!(
                "[{:.1}s] {stage}: {checks} checks completed, {failed} failed",
                self.started.elapsed().as_secs_f64()
            );
        }
    }
}

pub(super) struct Reporter {
    pub(super) progress: Arc<Progress>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Reporter {
    pub(super) fn start() -> Self {
        let progress = Arc::new(Progress {
            started: Instant::now(),
            stage: Mutex::new(String::new()),
            checks: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            frame: AtomicUsize::new(0),
            frames: AtomicUsize::new(0),
        });
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_progress = Arc::clone(&progress);
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            let (lock, wake) = &*worker_stop;
            let mut stopped = lock.lock().unwrap();
            while !*stopped {
                let (next, _) = wake.wait_timeout(stopped, Duration::from_secs(2)).unwrap();
                stopped = next;
                if !*stopped {
                    worker_progress.heartbeat();
                }
            }
        });
        Self {
            progress,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        let (lock, wake) = &*self.stop;
        *lock.lock().unwrap() = true;
        wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
