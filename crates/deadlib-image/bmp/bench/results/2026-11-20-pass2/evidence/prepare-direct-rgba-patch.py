from pathlib import Path
import difflib
import hashlib
import json

root = Path.cwd()
task = root / "target/bmp-decode-opt-20261120-pass2"
paths = ["crates/deadlib-image/bmp/src/decode.rs", "crates/deadlib-image/bmp/tests/bmp_optimized.rs"]
before = {name: (root / name).read_text(encoding="utf-8") for name in paths}
after = dict(before)

anchor = "        Format::Rgb24 | Format::Masked { .. } => {\n"
insert = """        Format::Masked {
            red,
            green,
            blue,
            alpha: Some(alpha),
            opaque_if_zero: false,
        } if header.info.bit_depth == 32
            && [red.mask, green.mask, blue.mask, alpha.mask]
                == [0xff, 0xff00, 0xff0000, 0xff000000] =>
        {
            let mut samples = Vec::new();
            reserve(&mut samples, header.decoded_bytes)?;
            // These explicit 32-bit pixels already have RGBA byte order and
            // no row padding. Append initialized bytes once in logical order.
            if header.info.top_down {
                samples.extend_from_slice(source);
            } else {
                for row in source.chunks_exact(header.row_pitch).rev() {
                    samples.extend_from_slice(row);
                }
            }
            Pixels::Rgba(samples)
        }
"""
assert after[paths[0]].count(anchor) == 1
after[paths[0]] = after[paths[0]].replace(anchor, insert + anchor, 1)
old = """            [0xff, 0xff00, 0xff0000, 0xff000000] => {
                output.copy_from_slice(&row[..output.len()]);
            }
"""
assert after[paths[0]].count(old) == 1
after[paths[0]] = after[paths[0]].replace(old, "", 1)

test_anchor = "#[test]\nfn bgra_zero_alpha_repair_covers_row_boundaries_and_final_pixels() {\n"
test = """#[test]
fn rgba_byte_order_copy_preserves_explicit_alpha_and_source_extent() {
    for header in [56, 108, 124] {
        let format = Format {
            bits: 32,
            header,
            compression: 3,
            masks: [0xff, 0xff00, 0xff0000, 0xff000000],
        };
        for width in [1usize, 3, 17, 257] {
            for zero_alpha in [false, true] {
                let mut expected = Vec::new();
                for i in 0..width * 3 {
                    expected.extend([
                        i as u8,
                        (i * 71 + 19) as u8,
                        (i * 139 + 41) as u8,
                        if zero_alpha { 0 } else { (i * 43) as u8 },
                    ]);
                }
                for top_down in [false, true] {
                    let original = wire(format, width, 3, top_down, &[], &expected);
                    let offset = u32::from_le_bytes(original[10..14].try_into().unwrap()) as usize;
                    for gap in [0, 1, 7] {
                        let mut bytes = original[..offset].to_vec();
                        bytes.resize(offset + gap, 0xff);
                        bytes.extend_from_slice(&original[offset..]);
                        let source_end = bytes.len();
                        bytes.extend_from_slice(&[0xa5; 9]);
                        bytes[10..14].copy_from_slice(&((offset + gap) as u32).to_le_bytes());
                        let limits = Limits {
                            max_decoded_bytes: expected.len(),
                            ..Limits::default()
                        };
                        let bitmap = deadlib_image_bmp::decode_with_limits(&bytes, limits).unwrap();
                        assert_eq!((bitmap.width, bitmap.height), (width as u32, 3));
                        assert_eq!(bitmap.pixels, Pixels::Rgba(expected.clone()));
                        assert_eq!(decode(&bytes[..source_end - 1]), Err(Error::Truncated));
                        assert_eq!(
                            deadlib_image_bmp::decode_with_limits(
                                &bytes,
                                Limits {
                                    max_decoded_bytes: expected.len() - 1,
                                    ..limits
                                }
                            ),
                            Err(Error::LimitExceeded)
                        );
                    }
                }
            }
        }
    }
}

"""
assert after[paths[1]].count(test_anchor) == 1
after[paths[1]] = after[paths[1]].replace(test_anchor, test + test_anchor, 1)

patch = "".join(
    "".join(difflib.unified_diff(before[name].splitlines(True), after[name].splitlines(True),
                                  fromfile="a/" + name, tofile="b/" + name))
    for name in paths
)
patch_path = task / "candidate3-direct-rgba.patch"
with patch_path.open("x", encoding="utf-8", newline="\n") as output:
    output.write(patch)
record = {
    "status": "source-only proposal; not applied, built, tested or timed",
    "prepared_on_stage": "candidate2-rgba4444",
    "source_files": [
        {"path": name, "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}
        for name in paths
    ],
    "patch_sha256": hashlib.sha256(patch_path.read_bytes()).hexdigest(),
    "production_added_lines": len(insert.splitlines()),
    "production_removed_lines": len(old.splitlines()),
}
with (task / "candidate3-direct-rgba.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
print(json.dumps(record, indent=2))

