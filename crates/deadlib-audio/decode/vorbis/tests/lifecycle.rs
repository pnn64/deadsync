mod support;
use deadlib_audio_decode_vorbis::{
    bitpack::{BitWriter, ilog},
    *,
};
use support::*;

fn silence(setup: &Setup, long: bool, previous: bool, next: bool) -> Vec<u8> {
    let mode = setup
        .modes()
        .iter()
        .position(|m| m.long_block == long)
        .unwrap();
    let mut bits = BitWriter::new();
    bits.write(0, 1).unwrap();
    bits.write(mode as u32, ilog(setup.modes().len() as u32 - 1))
        .unwrap();
    if long {
        bits.write(previous as u32, 1).unwrap();
        bits.write(next as u32, 1).unwrap();
    }
    for _ in 0..setup.channels() {
        bits.write(0, 1).unwrap();
    }
    bits.finish().unwrap()
}

#[test]
fn window_transitions_produce_exact_raw_frame_positions() {
    let packets = packets(include_bytes!("fixtures/stereo.ogg"));
    let mut decoder = decoder(&packets);
    let modes = [false, true, true, false, false, true, false];
    let sizes = decoder.setup().block_sizes();
    for _ in 0..3 {
        decoder.reset();
        let mut prior = None;
        let mut position = 0;
        for (i, &long) in modes.iter().enumerate() {
            let packet = silence(
                decoder.setup(),
                long,
                i != 0 && modes[i - 1],
                modes.get(i + 1).copied().unwrap_or(false),
            );
            let n = sizes[long as usize];
            let expected = prior.map_or(0, |previous| (previous + n) / 4);
            let block = decoder.decode(&packet).unwrap().unwrap();
            assert_eq!(block.frames, expected);
            assert_eq!(block.start_frame, position);
            assert!(!block.truncated);
            assert!(block.planes().flatten().all(|&x| x == 0.0));
            assert!(block.plane(block.channels).is_none());
            position += expected as u64;
            prior = Some(n);
        }
        assert_eq!(decoder.position(), position);
    }
}

#[test]
fn rejected_packets_preserve_overlap_and_limits_fail_before_use() {
    let packets = packets(include_bytes!("fixtures/stereo.ogg"));
    let mut native = decoder(&packets);
    let mut control = decoder(&packets);
    native.decode(&packets[3]).unwrap();
    control.decode(&packets[3]).unwrap();
    assert!(native.decode(&[]).unwrap().is_none());
    assert!(native.decode(b"\x01not audio").unwrap().is_none());
    let expected = control.decode(&packets[4]).unwrap().unwrap();
    let actual = native.decode(&packets[4]).unwrap().unwrap();
    assert_eq!(
        actual.planes().collect::<Vec<_>>(),
        expected.planes().collect::<Vec<_>>()
    );
    let identification = Identification::parse(&packets[0]).unwrap();
    let setup = Setup::parse(&packets[2], &identification, &Limits::default()).unwrap();
    assert!(matches!(
        PacketDecoder::with_limits(
            setup,
            Limits {
                max_decode_memory: 0,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded("decode memory"))
    ));
}

#[test]
fn every_short_packet_prefix_is_bounded_and_reset_restores_synthesis() {
    let packets = packets(include_bytes!("fixtures/stereo.ogg"));
    let mut native = decoder(&packets);
    for packet in &packets[3..] {
        for len in 0..packet.len() {
            native.reset();
            let _ = native.decode(&packet[..len]);
        }
    }
    native.reset();
    let mut control = decoder(&packets);
    for packet in &packets[3..] {
        let actual = native.decode(packet).unwrap().unwrap();
        let expected = control.decode(packet).unwrap().unwrap();
        assert_eq!(
            actual.planes().collect::<Vec<_>>(),
            expected.planes().collect::<Vec<_>>()
        );
    }
}

#[test]
fn seeded_malformed_packets_cannot_escape_buffers_or_emit_nonfinite_pcm() {
    let packets = packets(include_bytes!("fixtures/surround.ogg"));
    let mut decoder = decoder(&packets);
    let mut seed = 0x9135_e1a5u32;
    for i in 0..1000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut bytes = if i % 2 == 0 {
            packets[3 + i % (packets.len() - 3)].clone()
        } else {
            vec![0; seed as usize % 512]
        };
        for value in &mut bytes {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            if i % 2 != 0 || seed & 7 == 0 {
                *value ^= (seed >> 16) as u8;
            }
        }
        decoder.reset();
        decoder.decode(&packets[3]).unwrap();
        if let Ok(Some(block)) = decoder.decode(&bytes) {
            assert!(block.frames <= 4096);
            assert!(block.planes().flatten().all(|x| x.is_finite()));
        }
    }
}
