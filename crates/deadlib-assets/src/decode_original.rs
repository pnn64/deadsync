// Frozen from main 1139884c for differential tests and paired benchmarks.
use super::*;

pub(crate) fn decode_texture_jobs_with_original<E>(
    jobs: Vec<TextureDecodeJob>,
    mut consume: impl FnMut(TextureDecodeResult) -> Result<(), E>,
) -> Result<(), E> {
    let job_count = jobs.len();
    if job_count == 0 {
        return Ok(());
    }

    let worker_count = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .min(job_count);
    if worker_count == 1 {
        for job in jobs {
            consume(decode_rgba(job))?;
        }
        return Ok(());
    }

    let jobs = Mutex::new(jobs.into_iter());
    let slot = DecodeSlot::new(worker_count);
    std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let jobs = &jobs;
            let slot = &slot;
            workers.push(scope.spawn(move || {
                let _worker = DecodeWorker(slot);
                let mut batch = Vec::with_capacity(DECODE_JOB_BATCH_SIZE);
                loop {
                    {
                        let mut jobs = jobs.lock().expect("texture decode job queue poisoned");
                        batch.extend(jobs.by_ref().take(DECODE_JOB_BATCH_SIZE));
                    }
                    if batch.is_empty() {
                        return;
                    }
                    for job in batch.drain(..) {
                        if !slot.send(decode_rgba(job)) {
                            return;
                        }
                    }
                }
            }));
        }

        let mut result = Ok(());
        while let Some(decoded) = slot.receive() {
            if let Err(error) = consume(decoded) {
                slot.cancel();
                result = Err(error);
                break;
            }
        }
        for worker in workers {
            worker.join().expect("texture decode worker panicked");
        }
        result
    })
}
