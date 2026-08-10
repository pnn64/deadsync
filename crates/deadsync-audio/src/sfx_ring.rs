use crate::QueuedSfx;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::{Arc, Mutex};

/// Game-thread endpoint for the bounded sound-effect command queue.
///
/// Clones share the producer lock because UI, game, and assist-tick producers
/// may enqueue concurrently. That contention never reaches the audio callback.
#[derive(Clone)]
pub struct SfxSender {
    producer: Arc<Mutex<Producer<QueuedSfx>>>,
}

/// Audio-callback endpoint for the bounded sound-effect command queue.
///
/// The callback is the sole consumer. Queue storage is allocated once during
/// backend startup, lives for the output-stream session, and is destroyed only
/// after the backend stops. A miss is one bounded atomic pop; a full queue
/// rejects the newest command on the producer thread. There is no callback-time
/// allocation, lock, wait, scan, eviction, or destruction of queue storage.
pub struct SfxReceiver {
    consumer: Consumer<QueuedSfx>,
}

pub struct SfxDrain<'a> {
    consumer: &'a mut Consumer<QueuedSfx>,
}

impl SfxSender {
    /// Enqueue one effect, returning it unchanged when the fixed queue is full.
    pub fn try_send(&self, queued: QueuedSfx) -> Result<(), QueuedSfx> {
        self.producer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(queued)
            .map_err(|PushError::Full(queued)| queued)
    }
}

impl SfxReceiver {
    /// Drain the commands available at callback entry in FIFO order.
    #[inline(always)]
    pub fn try_iter(&mut self) -> SfxDrain<'_> {
        SfxDrain {
            consumer: &mut self.consumer,
        }
    }
}

impl Iterator for SfxDrain<'_> {
    type Item = QueuedSfx;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.consumer.pop().ok()
    }
}

/// Create a fixed-capacity, session-lifetime SFX command transport.
pub fn sfx_transport(capacity: usize) -> (SfxSender, SfxReceiver) {
    assert!(capacity > 0, "SFX queue capacity must be nonzero");
    let (producer, consumer) = RingBuffer::new(capacity);
    (
        SfxSender {
            producer: Arc::new(Mutex::new(producer)),
        },
        SfxReceiver { consumer },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SfxLane;
    use std::sync::Arc;

    fn queued(frame: u64) -> QueuedSfx {
        QueuedSfx {
            data: Arc::from([frame as i16]),
            lane: SfxLane::Effect,
            stop_generation: frame + 1,
            target_stream_frame: frame,
        }
    }

    #[test]
    fn sfx_transport_preserves_fifo_and_reuses_capacity() {
        let (sender, mut receiver) = sfx_transport(3);
        for frame in 0..3 {
            assert!(sender.try_send(queued(frame)).is_ok());
        }
        let rejected = sender.try_send(queued(3)).unwrap_err();
        assert_eq!(rejected.target_stream_frame, 3);

        let first: Vec<_> = receiver
            .try_iter()
            .map(|queued| queued.target_stream_frame)
            .collect();
        assert_eq!(first, [0, 1, 2]);

        assert!(sender.try_send(queued(4)).is_ok());
        assert_eq!(
            receiver
                .try_iter()
                .next()
                .map(|queued| queued.target_stream_frame),
            Some(4)
        );
        assert!(receiver.try_iter().next().is_none());
    }
}
