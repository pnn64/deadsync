// Driver for scripts/profile_synthesis.py's isolated, instrumented decoder.
use deadlib_audio_decode_vorbis::*;
use std::{hint::black_box, time::Instant};

fn packets(bytes: &[u8], allow_zero_span_granules: bool) -> (Vec<Vec<u8>>, u64) {
    use deadlib_audio_decode_ogg::{Demuxer, PageReader, ValidationOptions};
    let mut pages = PageReader::new(bytes);
    let mut demux = Demuxer::with_validation(
        Default::default(),
        ValidationOptions {
            allow_zero_span_granules,
            ..Default::default()
        },
    );
    let mut out = Vec::new();
    let mut serial = None;
    while let Some(page) = pages.next_page().unwrap() {
        demux
            .push_page(page.page, page.offset, |packet| {
                assert_eq!(
                    *serial.get_or_insert(packet.info.serial),
                    packet.info.serial
                );
                out.push(packet.data.to_vec());
            })
            .unwrap();
    }
    demux.finish().unwrap();
    (out, demux.compatibility().zero_span_granules)
}
fn setup(packets: &[Vec<u8>]) -> Setup {
    let identification = Identification::parse(&packets[0]).unwrap();
    Setup::parse(&packets[2], &identification, &Limits::default()).unwrap()
}
fn production(decoder: &mut PacketDecoder, packets: &[Vec<u8>], iterations: usize) -> (u128, u64) {
    let mut samples = 0;
    let begin = Instant::now();
    for _ in 0..iterations {
        decoder.reset();
        for packet in packets {
            if let Some(pcm) = decoder.decode(black_box(packet)).unwrap() {
                samples += (pcm.frames * pcm.channels) as u64;
                for plane in pcm.planes() {
                    black_box(plane);
                }
            }
        }
    }
    (begin.elapsed().as_nanos(), samples)
}
fn profiled(
    decoder: &mut profiled_decoder::PacketDecoder,
    packets: &[Vec<u8>],
    iterations: usize,
) -> (u128, u64) {
    decoder.clear_profile();
    let mut samples = 0;
    let begin = Instant::now();
    for _ in 0..iterations {
        decoder.reset();
        for packet in packets {
            if let Some(pcm) = decoder.decode(black_box(packet)).unwrap() {
                samples += (pcm.frames * pcm.channels) as u64;
                for plane in pcm.planes() {
                    black_box(plane);
                }
            }
        }
    }
    (begin.elapsed().as_nanos(), samples)
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 5);
    let (packets, zero_span_granules) = packets(&std::fs::read(&args[0]).unwrap(), args[4] == "1");
    let mut real = PacketDecoder::new(setup(&packets)).unwrap();
    let mut copy = profiled_decoder::PacketDecoder::new(setup(&packets)).unwrap();
    let mut residue_kinds = [0usize; 3];
    let mut floor_kinds = [0usize; 2];
    for residue in real.setup().residues() {
        residue_kinds[residue.kind as usize] += 1;
    }
    for floor in real.setup().floors() {
        floor_kinds[usize::from(matches!(floor, Floor::Type1(_)))] += 1;
    }
    let channels = real.setup().channels();
    let block_sizes = real.setup().block_sizes();
    let audio = &packets[3..];
    let mut expected = 0u64;
    let mut discarded_packets = 0u64;
    let mut truncated_packets = 0u64;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for packet in audio {
        let a = real.decode(packet).unwrap();
        let b = copy.decode(packet).unwrap();
        assert_eq!(a.is_some(), b.is_some());
        if let (Some(a), Some(b)) = (a, b) {
            truncated_packets += u64::from(a.truncated);
            assert_eq!(
                (
                    a.frames,
                    a.channels,
                    a.start_frame,
                    a.block_size,
                    a.truncated
                ),
                (
                    b.frames,
                    b.channels,
                    b.start_frame,
                    b.block_size,
                    b.truncated
                )
            );
            expected += (a.frames * a.channels) as u64;
            for (a, b) in a.planes().zip(b.planes()) {
                for (&a, &b) in a.iter().zip(b) {
                    assert!(
                        a.is_finite() && a.to_bits() == b.to_bits(),
                        "instrumented PCM differs"
                    );
                    hash = (hash ^ a.to_bits() as u64).wrapping_mul(0x100_0000_01b3);
                }
            }
        } else {
            discarded_packets += 1;
        }
    }
    println!(
        "{{\"bit_exact\":true,\"raw_channel_samples\":{expected},\"audio_packets\":{},\"discarded_packets\":{discarded_packets},\"truncated_packets\":{truncated_packets},\"zero_span_granules\":{zero_span_granules},\"channels\":{channels},\"block_sizes\":{block_sizes:?},\"residue_setup_kinds\":{residue_kinds:?},\"floor_setup_kinds\":{floor_kinds:?},\"pcm_bits_fnv1a64\":\"{hash:016x}\"}}",
        audio.len()
    );
    let repeats: usize = args[1].parse().unwrap();
    let minimum: f64 = args[2].parse().unwrap();
    let mut seed: u64 = args[3].parse().unwrap();
    let counts = [
        production(&mut real, audio, 1).0,
        profiled(&mut copy, audio, 1).0,
    ]
    .map(|ns| ((minimum * 1e9 / ns as f64).ceil() as usize).clamp(1, 10000));
    for trial in 0..repeats {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let order = if seed >> 63 == 0 { [0, 1] } else { [1, 0] };
        for engine in order {
            let iterations = counts[engine];
            let (elapsed, samples) = if engine == 0 {
                production(&mut real, audio, iterations)
            } else {
                profiled(&mut copy, audio, iterations)
            };
            assert_eq!(samples, expected * iterations as u64);
            let stages = if engine == 0 { [0; 8] } else { copy.profile() };
            let engine = if engine == 0 {
                "production"
            } else {
                "profiled"
            };
            println!(
                "{{\"engine\":\"{engine}\",\"trial\":{trial},\"iterations\":{iterations},\"elapsed_ns\":{elapsed},\"samples\":{samples},\"packets\":{},\"profile_ns\":{stages:?}}}",
                audio.len() * iterations
            );
        }
    }
}
