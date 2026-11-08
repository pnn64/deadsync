use deadlib_image_jpeg::{Error, Event, FrameHeader, MarkerReader, Process};

#[test]
fn scan_stuffing_restart_fill_and_trailing_data() {
    let bytes = [
        0xff, 0xd8, 0xff, 0xfe, 0, 3, 42, 0xff, 0xda, 0, 2, 1, 0xff, 0, 2, 0xff, 0xff, 0xd0, 3,
        0xff, 0xff, 0xd9, 99,
    ];
    let mut reader = MarkerReader::new(&bytes).unwrap();
    assert!(matches!(
        reader.next().unwrap().unwrap(),
        Event::Segment {
            marker: 0xfe,
            data: [42],
            offset: 2
        }
    ));
    assert!(
        matches!(reader.next().unwrap().unwrap(), Event::Scan { header: [], entropy, offset: 7 } if entropy == &bytes[11..19])
    );
    assert_eq!(reader.next().unwrap().unwrap(), Event::End { offset: 19 });
    assert_eq!(reader.consumed(), 22);
    assert!(reader.next().is_none());
}

#[test]
fn truncations_and_bad_lengths_are_terminal() {
    let bytes = [
        0xff, 0xd8, 0xff, 0xe0, 0, 4, 1, 2, 0xff, 0xda, 0, 2, 0xff, 0, 0xff, 0xd9,
    ];
    for end in 0..bytes.len() {
        let result = MarkerReader::new(&bytes[..end]).and_then(|mut r| {
            loop {
                match r.next() {
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        assert!(r.next().is_none());
                        return Err(e);
                    }
                    None => return Ok(()),
                }
            }
        });
        assert_eq!(result, Err(Error::Truncated), "prefix {end}");
    }
    for marker in [0, 0xd0, 0xd7, 0xd8] {
        let b = [0xff, 0xd8, 0xff, marker];
        assert!(MarkerReader::new(&b).unwrap().next().unwrap().is_err());
    }
    for length in [0, 1] {
        let b = [0xff, 0xd8, 0xff, 0xe0, 0, length];
        assert!(MarkerReader::new(&b).unwrap().next().unwrap().is_err());
    }
}

#[test]
fn all_sof_modes_and_precision_rules() {
    for marker in [
        0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
    ] {
        let p = Process::try_from(marker).unwrap();
        for precision in 0..=17 {
            let b = [precision, 0, 8, 0, 9, 1, 1, 0x11, 0];
            let valid = if p == Process::Baseline {
                precision == 8
            } else if p.is_lossless() {
                (2..=16).contains(&precision)
            } else {
                precision == 8 || precision == 12
            };
            assert_eq!(FrameHeader::parse(marker, &b).is_ok(), valid);
        }
    }
    assert!(FrameHeader::parse(0xc4, &[8, 0, 8, 0, 8, 1, 1, 0x11, 0]).is_err());
    assert!(FrameHeader::parse(0xc0, &[8, 0, 8, 0, 8, 2, 1, 0x11, 0, 1, 0x11, 0]).is_err());
}
