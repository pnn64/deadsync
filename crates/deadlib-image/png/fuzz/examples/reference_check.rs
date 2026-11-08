//! Deterministic positive-oracle regression, independent of libFuzzer.
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};
#[path = "../support/reference.rs"]
mod reference;
#[path = "../../tests/support/huffman.rs"]
mod wire;

fn main() -> std::io::Result<()> {
    let cases = wire::all_cases();
    let (mut valid, mut extended) = (0, 0);
    let mut malformed = 0;
    let mut permissive = BTreeMap::<String, usize>::new();
    for case in &cases {
        let Some(expected) = &case.output else {
            assert!(deadlib_image_png::zlib::decompress(&case.bytes, 1 << 20).is_err());
            let mut probe = [0; 64];
            if libdeflater::Decompressor::new()
                .zlib_decompress(&case.bytes, &mut probe)
                .is_ok()
            {
                *permissive
                    .entry(case.name.split('/').next().unwrap().to_owned())
                    .or_default() += 1;
            }
            malformed += 1;
            continue;
        };
        let actual = deadlib_image_png::zlib::decompress(&case.bytes, expected.len()).unwrap();
        assert!(actual == *expected, "native fixture {}", case.name);
        reference::assert_zlib(&case.bytes, expected);
        valid += 1;
        if case.extended_distance {
            let mut old = Vec::new();
            assert!(
                flate2::read::ZlibDecoder::new(case.bytes.as_slice())
                    .read_to_end(&mut old)
                    .is_err(),
                "old oracle policy changed: {}",
                case.name
            );
            extended += 1;
        }
    }
    assert_eq!((valid, extended), (3091, 68));
    assert_eq!(malformed, 16554);
    let base = cases
        .iter()
        .find(|c| c.name == "counts-with-body/257/1")
        .unwrap();
    for window in 0..8 {
        for level in 0..4 {
            let mut input = base.bytes.clone();
            let cmf = (window << 4) | 8;
            let flags = level << 6;
            let header = (u16::from(cmf) << 8) | u16::from(flags);
            input[0] = cmf;
            input[1] = flags | ((31 - header % 31) % 31) as u8;
            let expected = base.output.as_ref().unwrap();
            assert!(
                deadlib_image_png::zlib::decompress(&input, expected.len()).unwrap() == *expected
            );
            reference::assert_zlib(&input, expected);
        }
    }
    println!(
        "libdeflate verified {valid} valid fixtures, all {extended} extended-distance definitions, and 32 zlib window/level headers"
    );
    println!(
        "Native rejected {malformed} malformed fixtures; libdeflate accepts these within a 64-byte probe: {permissive:?}"
    );
    if let Some(path) = std::env::args_os().nth(1) {
        let case = cases.iter().find(|c| c.extended_distance).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&case.bytes)?;
        println!("Wrote raw regression fixture {}", case.name);
    }
    Ok(())
}
