//! Picture output in display order: the "output order DPB" operation of
//! spec C.5.2.
//!
//! Used when the decoder is created with `Decoder::with_display_order`.
//! Decoded pictures with PicOutputFlag = 1 wait here ("needed for output")
//! until the bumping process (C.5.2.4) releases them in POC order. Bumping
//! runs before each picture is decoded (C.5.2.2) and after it (C.5.2.3).
//! At an IRAP picture that starts a new coded video sequence the waiting
//! pictures are either all output or, when NoOutputOfPriorPicsFlag is 1,
//! discarded without output.

use std::collections::VecDeque;

use crate::decoder::Frame;
use crate::sps::Sps;

/// DPB limits of the highest temporal sub-layer (HighestTid) of the active
/// SPS, which is what the SPS parser stores.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OutputLimits {
    /// sps_max_num_reorder_pics
    max_num_reorder: usize,
    /// SpsMaxLatencyPictures, when sps_max_latency_increase_plus1 != 0.
    max_latency_pictures: Option<u32>,
    /// sps_max_dec_pic_buffering_minus1 + 1
    max_dec_pic_buffering: usize,
}

impl OutputLimits {
    pub(crate) fn from_sps(sps: &Sps) -> Self {
        let reorder = sps.sps_max_num_reorder_pics;
        let plus1 = sps.sps_max_latency_increase_plus1;
        Self {
            max_num_reorder: reorder as usize,
            // SpsMaxLatencyPictures = sps_max_num_reorder_pics +
            // sps_max_latency_increase_plus1 - 1 (spec 7.4.3.2.1).
            // (plus1 itself is any u32 below 2^32 - 1.)
            max_latency_pictures: (plus1 != 0).then(|| reorder.saturating_add(plus1 - 1)),
            max_dec_pic_buffering: sps.sps_max_dec_pic_buffering_minus1 as usize + 1,
        }
    }
}

/// A picture's frame as the output process holds it. With frame threading
/// a picture enters the output process when its last slice has been
/// handed to its thread, before its samples exist: its frame is `Pending`
/// until the thread finishes (`OutputQueue::resolve`).
enum Slot {
    Ready(Frame),
    Pending(u64),
    /// The picture's thread produced no frame (it failed); output skips it.
    Gone,
}

struct Waiting {
    poc: i32,
    slot: Slot,
    /// PicLatencyCount (spec C.5.2.3).
    latency: u32,
}

/// Pictures waiting for output plus the frames already released.
#[derive(Default)]
pub(crate) struct OutputQueue {
    /// Pictures marked "needed for output".
    waiting: Vec<Waiting>,
    /// Frames released by bumping, in output order, not yet taken.
    ready: VecDeque<Slot>,
}

impl OutputQueue {
    /// The bumping process (C.5.2.4): output the waiting picture with the
    /// smallest POC. Returns false when nothing is waiting.
    fn bump(&mut self) -> bool {
        let Some(i) = (0..self.waiting.len()).min_by_key(|&i| self.waiting[i].poc) else {
            return false;
        };
        let w = self.waiting.remove(i);
        self.ready.push_back(w.slot);
        true
    }

    fn latency_exceeded(&self, limits: &OutputLimits) -> bool {
        limits
            .max_latency_pictures
            .is_some_and(|max| self.waiting.iter().any(|w| w.latency >= max))
    }

    /// Output every waiting picture, in POC order.
    pub(crate) fn output_all(&mut self) {
        while self.bump() {}
    }

    /// C.5.2.2, before decoding an IRAP picture with NoRaslOutputFlag = 1
    /// that is not the first picture: the waiting pictures are discarded
    /// without output when NoOutputOfPriorPicsFlag is 1, and all output
    /// otherwise.
    pub(crate) fn start_new_sequence(&mut self, no_output_of_prior_pics: bool) {
        if no_output_of_prior_pics {
            self.waiting.clear();
        } else {
            self.output_all();
        }
    }

    /// C.5.2.2, before decoding any other picture, after its reference
    /// picture set has been applied. `is_referenced(poc)` says whether the
    /// DPB picture with that POC is still used for reference, and
    /// `referenced` is how many DPB pictures are. A waiting picture that is
    /// also referenced occupies one DPB slot, not two.
    pub(crate) fn before_picture(
        &mut self,
        limits: &OutputLimits,
        referenced: usize,
        is_referenced: impl Fn(i32) -> bool,
    ) {
        loop {
            let waiting_only = self
                .waiting
                .iter()
                .filter(|w| !is_referenced(w.poc))
                .count();
            let fullness = referenced + waiting_only;
            let must_bump = self.waiting.len() > limits.max_num_reorder
                || self.latency_exceeded(limits)
                || fullness >= limits.max_dec_pic_buffering;
            if !must_bump || !self.bump() {
                break;
            }
        }
    }

    /// C.5.2.3, once the current picture is decoded: `frame` is the picture
    /// when PicOutputFlag = 1, `None` otherwise ("not needed for output").
    pub(crate) fn after_picture(&mut self, limits: &OutputLimits, frame: Option<Frame>) {
        let entry = frame.map(|f| (f.pic_order_cnt, Slot::Ready(f)));
        self.add_picture(limits, entry);
    }

    /// As `after_picture`, for a picture still decoding on its thread
    /// (frame threading): its frame is filled in by `resolve(id, ..)`.
    pub(crate) fn after_picture_pending(&mut self, limits: &OutputLimits, id: u64, poc: i32) {
        self.add_picture(limits, Some((poc, Slot::Pending(id))));
    }

    /// The frame of pending picture `id` (`None`: it has none).
    pub(crate) fn resolve(&mut self, id: u64, frame: Option<Frame>) {
        let mut frame = Some(frame);
        let slots = self
            .waiting
            .iter_mut()
            .map(|w| &mut w.slot)
            .chain(self.ready.iter_mut());
        for slot in slots {
            if matches!(slot, Slot::Pending(p) if *p == id) {
                *slot = match frame.take().flatten() {
                    Some(f) => Slot::Ready(f),
                    None => Slot::Gone,
                };
                return;
            }
        }
    }

    fn add_picture(&mut self, limits: &OutputLimits, entry: Option<(i32, Slot)>) {
        if let Some((poc, slot)) = entry {
            for w in &mut self.waiting {
                if w.poc > poc {
                    w.latency += 1;
                }
            }
            self.waiting.push(Waiting {
                poc,
                slot,
                latency: 0,
            });
        }
        while (self.waiting.len() > limits.max_num_reorder || self.latency_exceeded(limits))
            && self.bump()
        {}
    }

    /// Take the next frame released for output. `None` also while the next
    /// one is still decoding on its thread.
    pub(crate) fn pop(&mut self) -> Option<Frame> {
        loop {
            match self.ready.front()? {
                Slot::Pending(_) => return None,
                Slot::Gone => {
                    self.ready.pop_front();
                }
                Slot::Ready(_) => match self.ready.pop_front() {
                    Some(Slot::Ready(f)) => return Some(f),
                    _ => unreachable!(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::PixelData;

    fn frame(poc: i32) -> Frame {
        Frame {
            y: PixelData::U8(vec![]),
            u: PixelData::U8(vec![]),
            v: PixelData::U8(vec![]),
            width: 0,
            height: 0,
            pic_order_cnt: poc,
            bit_depth: 8,
            chroma_format_idc: 1,
        }
    }

    fn limits(reorder: usize, dpb: usize) -> OutputLimits {
        OutputLimits {
            max_num_reorder: reorder,
            max_latency_pictures: None,
            max_dec_pic_buffering: dpb,
        }
    }

    fn drain(q: &mut OutputQueue) -> Vec<i32> {
        std::iter::from_fn(|| q.pop())
            .map(|f| f.pic_order_cnt)
            .collect()
    }

    #[test]
    fn reorder_limit_releases_in_poc_order() {
        // Decoding order 0 2 1 4 3 with one picture of reorder delay: each
        // new picture releases the smallest waiting POC.
        let (l, mut q) = (limits(1, 16), OutputQueue::default());
        let mut out = Vec::new();
        for poc in [0, 2, 1, 4, 3] {
            q.after_picture(&l, Some(frame(poc)));
            out.extend(drain(&mut q));
        }
        assert_eq!(out, vec![0, 1, 2, 3]);
        q.output_all();
        assert_eq!(drain(&mut q), vec![4]);
    }

    #[test]
    fn full_dpb_bumps_before_decoding() {
        // The reorder limit is not reached, but 3 referenced pictures (POC
        // 4, 6, 8) plus POC 2, which is only waiting for output, fill a
        // 4-picture DPB. A referenced picture that is also waiting (8)
        // counts once. Bumping POC 2 frees its slot, after which the DPB is
        // no longer full and POC 8 keeps waiting.
        let (l, mut q) = (limits(8, 4), OutputQueue::default());
        for poc in [8, 2] {
            q.after_picture(&l, Some(frame(poc)));
        }
        let referenced = |poc| matches!(poc, 4 | 6 | 8);
        q.before_picture(&l, 3, referenced);
        assert_eq!(drain(&mut q), vec![2]);
        q.before_picture(&l, 3, referenced);
        assert_eq!(drain(&mut q), Vec::<i32>::new());
    }

    #[test]
    fn no_output_of_prior_pics_discards_waiting() {
        let (l, mut q) = (limits(4, 16), OutputQueue::default());
        for poc in [0, 3, 1] {
            q.after_picture(&l, Some(frame(poc)));
        }
        q.start_new_sequence(true);
        assert_eq!(drain(&mut q), Vec::<i32>::new());
        q.after_picture(&l, Some(frame(0)));
        q.output_all();
        assert_eq!(drain(&mut q), vec![0]);
    }

    #[test]
    fn new_sequence_outputs_waiting_otherwise() {
        let (l, mut q) = (limits(4, 16), OutputQueue::default());
        for poc in [0, 3, 1] {
            q.after_picture(&l, Some(frame(poc)));
        }
        q.start_new_sequence(false);
        assert_eq!(drain(&mut q), vec![0, 1, 3]);
    }

    #[test]
    fn pending_frames_keep_output_order() {
        // POC 2 is still decoding when it is bumped: POC 4 behind it waits,
        // and a picture that fails (no frame) is skipped.
        let (l, mut q) = (limits(0, 16), OutputQueue::default());
        q.after_picture_pending(&l, 7, 2);
        q.after_picture(&l, Some(frame(4)));
        q.after_picture_pending(&l, 8, 6);
        assert_eq!(drain(&mut q), Vec::<i32>::new());
        q.resolve(7, Some(frame(2)));
        assert_eq!(drain(&mut q), vec![2, 4]);
        q.resolve(8, None);
        q.after_picture(&l, Some(frame(8)));
        assert_eq!(drain(&mut q), vec![8]);
    }

    #[test]
    fn latency_limit_releases_old_pictures() {
        let l = OutputLimits {
            max_num_reorder: 8,
            max_latency_pictures: Some(2),
            max_dec_pic_buffering: 16,
        };
        let mut q = OutputQueue::default();
        // POC 10 waits while two pictures that precede it in output order
        // are decoded; at a latency count of 2 it is bumped, taking every
        // picture with a smaller POC out first.
        for poc in [10, 2, 4] {
            q.after_picture(&l, Some(frame(poc)));
        }
        assert_eq!(drain(&mut q), vec![2, 4, 10]);
    }
}
