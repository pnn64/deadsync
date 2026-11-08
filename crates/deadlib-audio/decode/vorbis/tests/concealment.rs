mod support;
use deadlib_audio_decode_vorbis::*;
use std::{fs, io::Cursor};

fn check_prefixes(label: &str, packets: &[Vec<u8>], selected: &[usize]) {
    let audio = &packets[3..];
    let mut cases = Vec::new();
    for &number in selected {
        let packet = &audio[number];
        for length in 0..=packet.len() {
            let mut case: Vec<&[u8]> = audio[number.saturating_sub(2)..number]
                .iter()
                .map(Vec::as_slice)
                .collect();
            case.push(&packet[..length]);
            case.extend(audio.iter().skip(number + 1).take(2).map(Vec::as_slice));
            cases.push((format!("{label}/packet-{number}/prefix-{length}"), case));
        }
    }
    check_cases(label, packets, &cases);
}

fn check_cases(label: &str, packets: &[Vec<u8>], cases: &[(String, Vec<&[u8]>)]) {
    let mut native = support::decoder(packets);
    let floor0 = native
        .setup()
        .floors()
        .iter()
        .any(|f| matches!(f, Floor::Type0(_)));
    let mut request = Vec::new();
    for packet in &packets[..3] {
        request.extend((packet.len() as u64).to_le_bytes());
        request.extend(packet);
    }
    request.extend((cases.len() as u64).to_le_bytes());
    for (_, case) in cases {
        request.extend((case.len() as u64).to_le_bytes());
        for packet in case {
            request.extend((packet.len() as u64).to_le_bytes());
            request.extend(*packet);
        }
    }
    let reference = support::file_oracle("pcm-cases", &request);
    let mut wire = reference.as_slice();
    assert_eq!(support::u32_in(&mut wire), native.setup().channels() as u32);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()),
        cases.len() as u64
    );
    let mut total = support::Difference::default();
    let mut discarded = 0;
    let mut truncated = 0;
    let mut packets_checked = 0;
    let mut reference_nonfinite = 0;
    let mut native_nonfinite_rejections = 0;
    let (peak, rms) = if floor0 {
        (5.0e-5, 3.0e-6)
    } else {
        (3.0e-6, 5.0e-7)
    };
    for (name, case) in cases {
        native.reset();
        let start = wire;
        let mut finite = true;
        for _ in case {
            match support::take(&mut wire, 1)[0] {
                b'S' => {}
                b'P' => {
                    let _block = support::u32_in(&mut wire);
                    let frames = support::u32_in(&mut wire) as usize;
                    let bytes =
                        support::take(&mut wire, frames * native.setup().channels() as usize * 4);
                    finite &= bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|b| f32::from_bits(u32::from_le_bytes(*b)).is_finite());
                }
                kind => panic!("unexpected reference record {kind}"),
            }
        }
        let mut input = &start[..start.len() - wire.len()];
        if !finite {
            reference_nonfinite += 1;
            let mut rejected = false;
            for packet in case {
                match native.decode(packet) {
                    Ok(Some(block)) => {
                        assert!(block.planes().flatten().all(|value| value.is_finite()))
                    }
                    Ok(None) => {}
                    Err(Error::Invalid(reason)) if reason.starts_with("nonfinite ") => {
                        native_nonfinite_rejections += 1;
                        rejected = true;
                        break;
                    }
                    Err(error) => panic!("{name}: unrelated native rejection: {error}"),
                }
            }
            eprintln!("reference-nonfinite: {name}; native rejected={rejected}");
            // These cases are not included in matched PCM/count totals.
            // Independently verify that an explicit reset restores native
            // synthesis after its finite-output guard rejects damaged data.
            native.reset();
            let mut fresh = support::decoder(packets);
            for packet in packets[3..].iter().take(3) {
                let a = native.decode(packet).unwrap().unwrap();
                let b = fresh.decode(packet).unwrap().unwrap();
                assert_eq!(a.start_frame, b.start_frame);
                assert_eq!(a.frames, b.frames);
                assert_eq!(
                    a.planes().collect::<Vec<_>>(),
                    b.planes().collect::<Vec<_>>()
                );
            }
            continue;
        }
        let mut position = 0;
        for (number, packet) in case.iter().enumerate() {
            packets_checked += 1;
            let kind = support::take(&mut input, 1)[0];
            let block = native
                .decode(packet)
                .unwrap_or_else(|error| panic!("{name}/step-{number}: {error}"));
            if kind == b'S' {
                assert!(block.is_none(), "{name}/step-{number}: libvorbis discarded");
                discarded += 1;
                assert_eq!(native.position(), position);
                continue;
            }
            assert_eq!(kind, b'P');
            let block = block.unwrap_or_else(|| panic!("{name}/step-{number}: native discarded"));
            assert_eq!(
                support::u32_in(&mut input) as usize,
                block.block_size,
                "{name}/step-{number}"
            );
            assert_eq!(
                support::u32_in(&mut input) as usize,
                block.frames,
                "{name}/step-{number}"
            );
            assert_eq!(block.start_frame, position, "{name}/step-{number}");
            position += block.frames as u64;
            truncated += usize::from(block.truncated);
            let mut difference = support::Difference::default();
            for plane in block.planes() {
                for &value in plane {
                    difference.add(value, f32::from_bits(support::u32_in(&mut input)));
                }
            }
            if difference.peak_error > peak * difference.peak_reference.max(1.0)
                || difference.rms() > rms * difference.peak_reference.max(1.0)
            {
                difference.check_with_bounds(&format!("{name}/step-{number}"), peak, rms);
            }
            total.merge(&difference);
        }
        assert!(input.is_empty());
    }
    assert!(wire.is_empty());
    eprintln!(
        "{label}: {} cases, {packets_checked} packets, {discarded} discarded, {truncated} truncated, {} samples, peak {}, RMS {}",
        cases.len() - reference_nonfinite,
        total.samples,
        total.peak_error,
        total.rms()
    );
    eprintln!(
        "{label}: reference-nonfinite {reference_nonfinite}, native-nonfinite-rejections {native_nonfinite_rejections}"
    );
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn every_fixture_audio_bit_flip_and_following_recovery_match_libvorbis() {
    for (label, bytes) in [
        ("mono", include_bytes!("fixtures/mono.ogg").as_slice()),
        ("stereo", include_bytes!("fixtures/stereo.ogg").as_slice()),
        (
            "eight-channel-silence",
            include_bytes!("fixtures/eight-channel.ogg").as_slice(),
        ),
        (
            "eight-channel-tones",
            include_bytes!("fixtures/surround.ogg").as_slice(),
        ),
    ] {
        let packets = support::packets(bytes);
        check_bit_flips(label, &packets, &(0..packets.len() - 3).collect::<Vec<_>>());
    }
}

fn check_bit_flips(label: &str, packets: &[Vec<u8>], selected: &[usize]) {
    let audio = &packets[3..];
    for &number in selected {
        let packet = &audio[number];
        // Bound reference output memory while testing every bit, including
        // packet kind, mode/window fields, floors and residue payloads.
        for first in (0..packet.len() * 8).step_by(64) {
            let end = (first + 64).min(packet.len() * 8);
            let mutations: Vec<_> = (first..end)
                .map(|bit| {
                    let mut bytes = packet.clone();
                    bytes[bit / 8] ^= 1 << (bit & 7);
                    (bit, bytes)
                })
                .collect();
            let cases: Vec<_> = mutations
                .iter()
                .map(|(bit, bytes)| {
                    let mut case: Vec<_> = audio[number.saturating_sub(2)..number]
                        .iter()
                        .map(Vec::as_slice)
                        .collect();
                    case.push(bytes.as_slice());
                    case.extend(audio.iter().skip(number + 1).take(2).map(Vec::as_slice));
                    (format!("{label}/packet-{number}/bit-{bit}"), case)
                })
                .collect();
            check_cases(
                &format!("{label}/packet-{number}/bits-{first}-{end}"),
                packets,
                &cases,
            );
        }
    }
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn every_fixture_packet_byte_prefix_and_following_recovery_match_libvorbis() {
    for (name, bytes) in [
        ("mono", include_bytes!("fixtures/mono.ogg").as_slice()),
        ("stereo", include_bytes!("fixtures/stereo.ogg").as_slice()),
        (
            "eight-channel-silence",
            include_bytes!("fixtures/eight-channel.ogg").as_slice(),
        ),
        (
            "eight-channel-tones",
            include_bytes!("fixtures/surround.ogg").as_slice(),
        ),
    ] {
        let packets = support::packets(bytes);
        check_prefixes(name, &packets, &(0..packets.len() - 3).collect::<Vec<_>>());
    }
}

#[test]
#[ignore = "requires external libvorbis oracle and DEADLIB_VORBIS_FLOOR0"]
fn historical_floor_zero_packet_prefixes_and_recovery_match_libvorbis() {
    let (packets, selected) = historical_packets();
    check_prefixes("floor-zero", &packets, &selected);
}

#[test]
#[ignore = "requires external libvorbis oracle and DEADLIB_VORBIS_FLOOR0"]
fn historical_floor_zero_audio_bit_flips_check_finite_reference_and_recovery() {
    let (packets, selected) = historical_packets();
    check_bit_flips("floor-zero-bits", &packets, &selected);
}

fn historical_packets() -> (Vec<Vec<u8>>, Vec<usize>) {
    let path = std::env::var_os("DEADLIB_VORBIS_FLOOR0").expect("set DEADLIB_VORBIS_FLOOR0");
    let bytes = fs::read(path).unwrap();
    let mut reader = ogg::PacketReader::new(Cursor::new(bytes));
    let mut packets = Vec::new();
    while let Some(packet) = reader.read_packet().unwrap() {
        packets.push(packet.data);
    }
    let probe = support::decoder(&packets);
    assert!(
        probe
            .setup()
            .floors()
            .iter()
            .any(|f| matches!(f, Floor::Type0(_)))
    );
    // Select nonzero cases through the independent reference, so a native
    // silence/reconstruction regression cannot change which inputs are tested.
    let count = packets.len().min(3 + 128);
    let reference = support::oracle(
        "pcm",
        &packets[..count]
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>(),
    );
    let mut wire = reference.as_slice();
    let channels = support::u32_in(&mut wire) as usize;
    assert_eq!(channels, probe.setup().channels() as usize);
    let mut selected: Vec<_> = (0..6).collect();
    for number in 0..count - 3 {
        let kind = support::take(&mut wire, 1)[0];
        if kind == b'S' {
            continue;
        }
        assert_eq!(kind, b'P');
        let _block_size = support::u32_in(&mut wire);
        let frames = support::u32_in(&mut wire) as usize;
        let values = support::take(&mut wire, frames * channels * 4);
        let nonzero = values
            .as_chunks::<4>()
            .0
            .iter()
            .any(|b| f32::from_bits(u32::from_le_bytes(*b)) != 0.0);
        if nonzero && selected.len() < 14 && !selected.contains(&number) {
            selected.push(number);
        }
    }
    assert!(wire.is_empty());
    assert_eq!(
        selected.len(),
        14,
        "include eight nonzero floor-zero packets"
    );
    eprintln!("historical independently selected packets: {selected:?}");
    (packets, selected)
}
