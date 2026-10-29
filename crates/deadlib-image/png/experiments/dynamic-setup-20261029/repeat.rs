use std::hint::black_box;
mod cpu;

fn main() {
    for size in [1024, 65536, 1048576] {
        let raw = vec![73; size];
        let bytes = baseline::zlib::compress(&raw, baseline::zlib::Compression::Fast).unwrap();
        assert_eq!(bytes[2] >> 1 & 3, 1);
        assert_eq!(baseline::zlib::decompress(&bytes, size).unwrap(), raw);
        assert_eq!(selected::zlib::decompress(&bytes, size).unwrap(), raw);
        for output in [31, 258, 8192] {
            let mut buffer = vec![0; output];
            for current in [false, true] {
                let mut pos = 0;
                let mut actual = Vec::new();
                if current {
                    let mut inflater = selected::zlib::Inflater::new(size).unwrap();
                    loop {
                        let p = inflater.process(&bytes[pos..], &mut buffer).unwrap();
                        pos += p.consumed;
                        actual.extend_from_slice(&buffer[..p.written]);
                        if p.status == selected::zlib::StreamStatus::Done { break; }
                    }
                    inflater.finish().unwrap();
                } else {
                    let mut inflater = baseline::zlib::Inflater::new(size).unwrap();
                    loop {
                        let p = inflater.process(&bytes[pos..], &mut buffer).unwrap();
                        pos += p.consumed;
                        actual.extend_from_slice(&buffer[..p.written]);
                        if p.status == baseline::zlib::StreamStatus::Done { break; }
                    }
                    inflater.finish().unwrap();
                }
                assert_eq!(actual, raw);
                assert_eq!(pos, bytes.len());
            }
            cpu::compare(&format!("repeat-{size}-output-{output}"), if size == 1024 { 512 } else { 12 }, |current| {
                let mut pos = 0;
                if current {
                    let mut inflater = selected::zlib::Inflater::new(size).unwrap();
                    loop {
                        let p = inflater.process(black_box(&bytes[pos..]), black_box(&mut buffer)).unwrap();
                        pos += p.consumed;
                        if p.status == selected::zlib::StreamStatus::Done { break; }
                    }
                    black_box(inflater);
                } else {
                    let mut inflater = baseline::zlib::Inflater::new(size).unwrap();
                    loop {
                        let p = inflater.process(black_box(&bytes[pos..]), black_box(&mut buffer)).unwrap();
                        pos += p.consumed;
                        if p.status == baseline::zlib::StreamStatus::Done { break; }
                    }
                    black_box(inflater);
                }
            });
        }
    }
}
