mod support;
use deadlib_audio_decode_vorbis::*;
use std::{fs, io::Cursor};

fn check_prefixes(label: &str, packets: &[Vec<u8>], selected: &[usize]) {
    let mut native = support::decoder(packets);
    let floor0 = native
        .setup()
        .floors()
        .iter()
        .any(|f| matches!(f, Floor::Type0(_)));
    eprintln!(
        "{label}: residue kinds {:?}",
        native
            .setup()
            .residues()
            .iter()
            .map(|r| r.kind)
            .collect::<Vec<_>>()
    );
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
    let mut request = Vec::new();
    for packet in &packets[..3] {
        request.extend((packet.len() as u64).to_le_bytes());
        request.extend(packet);
    }
    request.extend((cases.len() as u64).to_le_bytes());
    for (_, case) in &cases {
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
    let (peak, rms) = if floor0 {
        (5.0e-5, 3.0e-6)
    } else {
        (3.0e-6, 5.0e-7)
    };
    for (name, case) in &cases {
        native.reset();
        let mut position = 0;
        for (number, packet) in case.iter().enumerate() {
            packets_checked += 1;
            let kind = support::take(&mut wire, 1)[0];
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
                support::u32_in(&mut wire) as usize,
                block.block_size,
                "{name}/step-{number}"
            );
            assert_eq!(
                support::u32_in(&mut wire) as usize,
                block.frames,
                "{name}/step-{number}"
            );
            assert_eq!(block.start_frame, position, "{name}/step-{number}");
            position += block.frames as u64;
            truncated += usize::from(block.truncated);
            let mut difference = support::Difference::default();
            for plane in block.planes() {
                for &value in plane {
                    difference.add(value, f32::from_bits(support::u32_in(&mut wire)));
                }
            }
            if difference.peak_error > peak * difference.peak_reference.max(1.0)
                || difference.rms() > rms * difference.peak_reference.max(1.0)
            {
                difference.check_with_bounds(&format!("{name}/step-{number}"), peak, rms);
            }
            total.merge(&difference);
        }
    }
    assert!(wire.is_empty());
    eprintln!(
        "{label}: {} cases, {packets_checked} packets, {discarded} discarded, {truncated} truncated, {} samples, peak {}, RMS {}",
        cases.len(),
        total.samples,
        total.peak_error,
        total.rms()
    );
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
    let path = std::env::var_os("DEADLIB_VORBIS_FLOOR0").expect("set DEADLIB_VORBIS_FLOOR0");
    let bytes = fs::read(path).unwrap();
    let mut reader = ogg::PacketReader::new(Cursor::new(bytes));
    let mut packets = Vec::new();
    while let Some(packet) = reader.read_packet().unwrap() {
        packets.push(packet.data);
    }
    let mut probe = support::decoder(&packets);
    assert!(
        probe
            .setup()
            .floors()
            .iter()
            .any(|f| matches!(f, Floor::Type0(_)))
    );
    let mut selected: Vec<_> = (0..6).collect();
    for (number, packet) in packets[3..].iter().enumerate() {
        if let Some(block) = probe.decode(packet).unwrap()
            && block.planes().flatten().any(|&value| value != 0.0)
        {
            if !selected.contains(&number) {
                selected.push(number);
            }
            if selected.len() == 14 {
                break;
            }
        }
    }
    assert_eq!(
        selected.len(),
        14,
        "include eight nonzero floor-zero packets"
    );
    check_prefixes("floor-zero", &packets, &selected);
}
