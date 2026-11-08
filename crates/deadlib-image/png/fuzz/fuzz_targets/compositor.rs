#![no_main]
use deadlib_image_png::{DisposeOp, Error, Limits, Samples, decode, decode_reader};
use libfuzzer_sys::fuzz_target;
#[allow(dead_code)]
#[path = "../../tests/support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "../../tests/support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "../../tests/support/compositor.rs"]
mod reference;

fuzz_target!(|data: &[u8]| {
    if !(16..=4096).contains(&data.len()) {
        return;
    }
    let case = reference::Scenario::from_bytes(data);
    let answers = reference::render(&case.png);
    let wire = case.wire();
    let decoded = decode(&wire).unwrap();
    assert_eq!(decoded, case.png);
    assert_eq!(
        decode_reader(wire.as_slice(), &Default::default()).unwrap(),
        case.png
    );
    for png in [&case.png, &decoded] {
        assert_eq!(
            png.to_rgba16(Limits::default()).unwrap().samples,
            Samples::U16(
                reference::expand(png, &png.image)
                    .into_iter()
                    .flatten()
                    .collect()
            )
        );
        let mut renderer = png.compositor(Limits::default()).unwrap();
        for _ in 0..2 {
            for (frame, answer) in png.animation.as_ref().unwrap().frames.iter().zip(&answers) {
                let result = renderer.next_frame().unwrap().unwrap();
                assert_eq!(
                    (result.width, result.height),
                    (png.image.width, png.image.height)
                );
                assert_eq!(result.control, frame.control);
                assert_eq!(result.samples, answer);
            }
            assert!(renderer.next_frame().unwrap().is_none());
            renderer.reset();
        }
    }
    let canvas = case.png.image.width as usize * case.png.image.height as usize * 8;
    let max_output_bytes = canvas - 1 + usize::from(data[data.len() - 1]) % (canvas + 2);
    let limits = Limits {
        max_output_bytes,
        ..Default::default()
    };
    if max_output_bytes < canvas {
        assert!(matches!(
            case.png.compositor(limits),
            Err(Error::LimitExceeded)
        ));
    } else {
        let mut renderer = case.png.compositor(limits).unwrap();
        for (index, frame) in case
            .png
            .animation
            .as_ref()
            .unwrap()
            .frames
            .iter()
            .enumerate()
        {
            let previous = if index != 0 && frame.control.dispose == DisposeOp::Previous {
                frame.control.width as usize * frame.control.height as usize * 8
            } else {
                0
            };
            if canvas + previous > max_output_bytes {
                assert!(matches!(renderer.next_frame(), Err(Error::LimitExceeded)));
                assert!(matches!(renderer.next_frame(), Err(Error::LimitExceeded)));
                break;
            }
            assert_eq!(
                renderer.next_frame().unwrap().unwrap().samples,
                answers[index]
            );
        }
        renderer.reset();
        assert_eq!(renderer.next_frame().unwrap().unwrap().samples, answers[0]);
    }
});
