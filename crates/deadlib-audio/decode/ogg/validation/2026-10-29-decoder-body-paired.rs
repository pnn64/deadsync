extern crate body_before;
extern crate body_empty;
extern crate body_partial;
extern crate body_status;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;
use std::hint::black_box;
fn frame(size: usize) -> Vec<u8> {
    let mut lace = vec![255; size / 255];
    if lace.len() < 255 {
        lace.push((size % 255) as u8);
    }
    body_before::encode_page(
        body_before::PageHeader {
            flags: 0,
            granule_position: 0,
            serial: 42,
            sequence: 0,
        },
        &lace,
        &vec![37; size],
    )
    .unwrap()
}
fn header_frame(segments: usize) -> Vec<u8> {
    body_before::encode_page(
        body_before::PageHeader {
            flags: 0,
            granule_position: u64::MAX,
            serial: 42,
            sequence: 0,
        },
        &vec![255; segments],
        &vec![37; segments * 255],
    )
    .unwrap()
}
macro_rules! decoder {
    ($a:ident,$b:ident,$label:expr,$bytes:expr,$chunk:expr,$iter:expr) => {{
        cpu::compare($label, $iter, |new| {
            if new {
                let mut decoder = $b::PageDecoder::new();
                let mut count = 0;
                for input in black_box(&$bytes).chunks($chunk) {
                    let p = decoder.push(input).unwrap();
                    assert_eq!(p.consumed, input.len());
                    count += p.page.is_some() as usize;
                    black_box(p.page);
                }
                decoder.finish().unwrap();
                assert_eq!(count, 1);
            } else {
                let mut decoder = $a::PageDecoder::new();
                let mut count = 0;
                for input in black_box(&$bytes).chunks($chunk) {
                    let p = decoder.push(input).unwrap();
                    assert_eq!(p.consumed, input.len());
                    count += p.page.is_some() as usize;
                    black_box(p.page);
                }
                decoder.finish().unwrap();
                assert_eq!(count, 1);
            }
        });
    }};
}
macro_rules! headers {
    ($a:ident,$b:ident,$label:expr,$header:expr,$chunk:expr) => {{
        let mut old = $a::PageDecoder::new();
        let mut new = $b::PageDecoder::new();
        old.push($header).unwrap();
        new.push($header).unwrap();
        cpu::compare($label, 1000, |after| {
            if after {
                new.reset();
                for input in black_box($header).chunks($chunk) {
                    let p = new.push(input).unwrap();
                    assert_eq!(p.consumed, input.len());
                    assert!(p.page.is_none());
                }
            } else {
                old.reset();
                for input in black_box($header).chunks($chunk) {
                    let p = old.push(input).unwrap();
                    assert_eq!(p.consumed, input.len());
                    assert!(p.page.is_none());
                }
            }
        });
    }};
}
macro_rules! poll {
    ($a:ident,$b:ident,$label:expr,$state:expr,$bytes:expr,$small:expr) => {{
        let mut old = $a::PageDecoder::new();
        let mut new = $b::PageDecoder::new();
        let prefix = match $state {
            1 => 1,
            2 => 27,
            3 => 283,
            _ => 0,
        };
        if prefix != 0 {
            old.push(&$bytes[..prefix]).unwrap();
            new.push(&$bytes[..prefix]).unwrap();
        }
        if $state == 4 {
            old.push(&$small).unwrap();
            new.push(&$small).unwrap();
        }
        if $state == 5 {
            assert!(old.push(&[0; 27]).is_err());
            assert!(new.push(&[0; 27]).is_err());
        }
        macro_rules! step {
            ($decoder:expr) => {{
                let _ = black_box($decoder.push(black_box(&[])));
                if $state == 4 {
                    let _ = black_box($decoder.push(black_box(&$small)));
                }
            }};
        }
        cpu::compare($label, 65536, |after| {
            if after {
                step!(new);
            } else {
                step!(old);
            }
        });
        let (_, before) = heap::measure(|| {
            step!(old);
        });
        let (_, after) = heap::measure(|| {
            step!(new);
        });
        assert_eq!(before, heap::Churn::default());
        assert_eq!(after, before);
        println!(
            "{}: before allocation {:?}; after allocation {:?}",
            $label, before, after
        );
    }};
}
fn main() {
    for size in [17, 4096, 65025] {
        let bytes = frame(size);
        for chunk in [1, 7, 4096, usize::MAX] {
            decoder!(
                body_before,
                body_partial,
                &format!("partial/{size}/{chunk}"),
                bytes,
                chunk,
                if size >= 4096 { 8 } else { 500 }
            );
            decoder!(
                body_partial,
                body_empty,
                &format!("empty/{size}/{chunk}"),
                bytes,
                chunk,
                if size >= 4096 { 8 } else { 500 }
            );
            decoder!(
                body_empty,
                body_status,
                &format!("status/{size}/{chunk}"),
                bytes,
                chunk,
                if size >= 4096 { 8 } else { 500 }
            );
        }
    }
    for segments in [1, 17, 128, 255] {
        let bytes = header_frame(segments);
        let header = &bytes[..27 + segments];
        for chunk in [1, 7, 27] {
            headers!(
                body_before,
                body_partial,
                &format!("partial-header/{segments}/{chunk}"),
                header,
                chunk
            );
            headers!(
                body_partial,
                body_empty,
                &format!("empty-header/{segments}/{chunk}"),
                header,
                chunk
            );
            headers!(
                body_empty,
                body_status,
                &format!("status-header/{segments}/{chunk}"),
                header,
                chunk
            );
        }
    }
    let bytes = frame(65025);
    let small = frame(17);
    for state in 0..6 {
        poll!(
            body_partial,
            body_empty,
            &format!("empty-poll/{state}"),
            state,
            bytes,
            small
        );
        poll!(
            body_empty,
            body_status,
            &format!("status-poll/{state}"),
            state,
            bytes,
            small
        );
    }
}
