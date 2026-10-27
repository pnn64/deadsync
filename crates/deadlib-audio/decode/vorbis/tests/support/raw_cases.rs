//! Independent C raw-packet comparisons shared by procedural setup matrices.
use super::support;
use deadlib_audio_decode_vorbis::{
    DecodeOptions, Floor, Identification, PacketDecoder, ParseOptions, Setup,
};
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
    peak_bound: f64,
    rms_bound: f64,
    nonfinite_samples: u64,
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
    compare_with_parse_options(
        headers,
        cases,
        reference,
        options,
        ParseOptions {
            limits: options.limits,
            ..ParseOptions::default()
        },
    )
}
pub fn compare_with_parse_options(
    headers: &[Vec<u8>],
    cases: &[Case],
    reference: &[u8],
    options: DecodeOptions,
    parsing: ParseOptions,
) -> Counts {
    let id = Identification::parse(&headers[0]).unwrap();
    let setup = Setup::parse_with_options(&headers[2], &id, parsing).unwrap();
    let mut native = PacketDecoder::with_options(setup, options).unwrap();
    let (peak, rms) = if native
        .setup()
        .floors()
        .iter()
        .any(|f| matches!(f, Floor::Type0(_)))
    {
        (5e-5, 3e-6)
    } else {
        (3e-6, 5e-7)
    };
    let mut wire = reference;
    assert_eq!(support::u32_in(&mut wire), native.setup().channels() as u32);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()),
        cases.len() as u64
    );
    let mut total = Counts {
        cases: cases.len(),
        peak_bound: peak,
        rms_bound: rms,
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
                    let expected = f32::from_bits(support::u32_in(&mut wire));
                    if expected.is_nan() {
                        assert!(value.is_nan(), "{}/step-{i}: expected NaN", case.name);
                        total.nonfinite_samples += 1;
                    } else if expected.is_infinite() {
                        assert_eq!(value, expected, "{}/step-{i}: expected infinity", case.name);
                        total.nonfinite_samples += 1;
                    } else {
                        difference.add(value, expected);
                    }
                }
            }
            if difference.peak_error > peak * difference.peak_reference.max(1.0)
                || difference.rms() > rms * difference.peak_reference.max(1.0)
            {
                difference.check_with_bounds(&format!("{}/step-{i}", case.name), peak, rms);
            }
            total.pcm.merge(&difference);
        }
    }
    assert!(wire.is_empty());
    total
}
pub fn report(label: &str, counts: Counts) {
    if counts.nonfinite_samples != 0 {
        eprintln!(
            "{label}: {} nonfinite samples with matching IEEE classes",
            counts.nonfinite_samples
        );
    }
    eprintln!(
        "{label}: {} cases, {} packets, {} discarded",
        counts.cases, counts.packets, counts.discarded
    );
    counts
        .pcm
        .check_with_bounds(label, counts.peak_bound, counts.rms_bound);
}
