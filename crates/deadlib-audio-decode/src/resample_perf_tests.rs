//! Resampler buffer and storage regressions.
use super::*;


#[test]
fn draining_large_packets_never_moves_the_unread_tail() {
    let packet: Vec<_> = (0..65536 * 2).map(|i| i as i16).collect();
    let mut queue = PlanarAccum::new(2, 65536);
    for _ in 0..3 {
        queue.push_i16_interleaved(&packet, 2);
        let pointers: Vec<_> = queue.channels.iter().map(Vec::as_ptr).collect();
        for consumed in (256..65536).step_by(256) {
            queue.consume_frames(256);
            assert_eq!(queue.start_frame, consumed);
            for (channel, &ptr) in queue.channels.iter().zip(&pointers) {
                assert_eq!(channel.as_ptr(), ptr);
                assert_eq!(channel.capacity(), 65536);
                assert_eq!(channel.len(), 65536);
            }
        }
        queue.consume_frames(256);
        assert!(queue.is_empty());
        assert_eq!(queue.start_frame, 0);
    }
}
