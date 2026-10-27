//! Independent C raw-packet comparisons shared by procedural setup matrices.
use super::support;
use deadlib_audio_decode_vorbis::{DecodeOptions, Identification, PacketDecoder, Setup};
pub struct Case {
    pub name: String,
    pub packets: Vec<Vec<u8>>,
}
pub fn request(headers: &[Vec<u8>], cases: &[Case]) -> Vec<u8> {
    let mut wire = Vec::new();
    for header in headers {
        wire.extend((header.len() as u64).to_le_bytes());
        wire.extend(header);
    }
    wire.extend((cases.len() as u64).to_le_bytes());
    for case in cases {
        wire.extend((case.packets.len() as u64).to_le_bytes());
        for packet in &case.packets {
            wire.extend((packet.len() as u64).to_le_bytes());
            wire.extend(packet);
        }
    }
    wire
}
pub fn reference(label: &str, headers: &[Vec<u8>], cases: &[Case]) -> Vec<u8> {
    let wire = request(headers, cases);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_CONFIGURATION_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join(format!("{label}.bin")),
            &wire,
        )
        .unwrap();
    }
    support::file_oracle("pcm-cases", &wire)
}
#[derive(Default)]
pub struct Counts {
    cases: usize,
    packets: usize,
    discarded: usize,
    pcm: support::Difference,
}
pub fn compare(headers: &[Vec<u8>], cases: &[Case], reference: &[u8]) -> Counts {
    compare_with_options(headers, cases, reference, DecodeOptions::default())
}
pub fn compare_with_options(
    headers: &[Vec<u8>],
    cases: &[Case],
    reference: &[u8],
    options: DecodeOptions,
) -> Counts {
    let id = Identification::parse(&headers[0]).unwrap();
    let setup = Setup::parse(&headers[2], &id, &options.limits).unwrap();
    let mut native = PacketDecoder::with_options(setup, options).unwrap();
    let mut wire = reference;
    assert_eq!(support::u32_in(&mut wire), native.setup().channels() as u32);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()),
        cases.len() as u64
    );
    let mut total = Counts {
        cases: cases.len(),
        ..Counts::default()
    };
    for case in cases {
        native.reset();
        let mut position = 0;
        for (i, packet) in case.packets.iter().enumerate() {
            total.packets += 1;
            let kind = support::take(&mut wire, 1)[0];
            let block = native
                .decode(packet)
                .unwrap_or_else(|e| panic!("{}/step-{i}: {e}", case.name));
            if kind == b'S' {
                assert!(block.is_none(), "{}/step-{i}", case.name);
                assert_eq!(native.position(), position);
                total.discarded += 1;
                continue;
            }
            assert_eq!(kind, b'P');
            let block = block.unwrap_or_else(|| panic!("{}/step-{i}: native discarded", case.name));
            assert_eq!(
                support::u32_in(&mut wire) as usize,
                block.block_size,
                "{}/step-{i}",
                case.name
            );
            assert_eq!(
                support::u32_in(&mut wire) as usize,
                block.frames,
                "{}/step-{i}",
                case.name
            );
            assert_eq!(block.start_frame, position, "{}/step-{i}", case.name);
            position += block.frames as u64;
            let mut difference = support::Difference::default();
            for plane in block.planes() {
                for &value in plane {
                    difference.add(value, f32::from_bits(support::u32_in(&mut wire)));
                }
            }
            if difference.peak_error > 3e-6 * difference.peak_reference.max(1.0)
                || difference.rms() > 5e-7 * difference.peak_reference.max(1.0)
            {
                difference.check(&format!("{}/step-{i}", case.name));
            }
            total.pcm.merge(&difference);
        }
    }
    assert!(wire.is_empty());
    total
}
pub fn report(label: &str, counts: Counts) {
    eprintln!(
        "{label}: {} cases, {} packets, {} discarded",
        counts.cases, counts.packets, counts.discarded
    );
    counts.pcm.check(label);
}
