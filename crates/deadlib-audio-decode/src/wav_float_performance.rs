use super::{Encoding, decode_packet_into};


// Expected PCM conversion for the packet test inputs.

fn reference(value: f64) -> i16 {
    if value.is_finite() {
        (value * 32767.0).round() as i16
    } else {
        0
    }
}

#[test]
fn float_packets_preserve_tails_offsets_storage_and_partial_errors() {
    let values = [
        0.0,
        -0.0,
        0.5 / 32767.0,
        -0.5 / 32767.0,
        0.25,
        -0.9,
        1.0,
        -1.0,
        1.5,
        -4.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::MAX,
        -f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0xfff0_0000_0000_0001),
    ];
    let mut cases = 0;
    let mut compared = 0;
    for encoding in [Encoding::Float32, Encoding::Float64] {
        for samples in (0..=97).chain([255, 256, 257, 8191, 8192, 8193]) {
            for offset in 0..32 {
                let mut bytes = vec![0xa5; offset];
                let mut expected = Vec::with_capacity(samples);
                for index in 0..samples {
                    let value = values[(index + offset) % values.len()];
                    if matches!(encoding, Encoding::Float32) {
                        bytes.extend_from_slice(&(value as f32).to_le_bytes());
                        expected.push(reference(f64::from(value as f32)));
                    } else {
                        bytes.extend_from_slice(&value.to_le_bytes());
                        expected.push(reference(value));
                    }
                }
                let end = bytes.len();
                bytes.extend_from_slice(&[0xa5; 8]);
                let mut output = Vec::with_capacity(samples.max(3));
                output.extend_from_slice(&[123; 3]);
                let storage = (output.as_ptr(), output.capacity());
                decode_packet_into(&bytes[offset..end], encoding, &mut output).unwrap();
                assert_eq!(output, expected, "samples={samples}, offset={offset}");
                assert_eq!((output.as_ptr(), output.capacity()), storage);
                assert!(bytes[..offset].iter().all(|&v| v == 0xa5));
                assert_eq!(&bytes[end..], &[0xa5; 8]);
                for extra in 1..encoding.sample_bytes() {
                    assert_eq!(
                        decode_packet_into(&bytes[offset..end + extra], encoding, &mut output),
                        Err("WAV packet ended mid-sample")
                    );
                    assert_eq!(output, expected);
                    assert_eq!((output.as_ptr(), output.capacity()), storage);
                }
                cases += 1;
                compared += samples;
            }
        }
    }
    println!("{cases} float packet/storage cases; {compared} exact PCM samples");
}
