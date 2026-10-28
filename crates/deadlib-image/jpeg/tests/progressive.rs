use deadlib_image_jpeg::{Error, Event, Limits, MarkerReader, decode_coefficients};

const JPEG: &[u8] = include_bytes!("fixtures/progressive/12-420.jpg");

fn scans(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    MarkerReader::new(bytes)
        .unwrap()
        .filter_map(|e| match e.unwrap() {
            Event::Scan {
                header,
                entropy,
                offset,
            } => Some((offset, header.len(), entropy.len())),
            _ => None,
        })
        .collect()
}

#[test]
fn refinement_precision_and_unused_table_selectors() {
    let original = decode_coefficients(JPEG, Limits::default()).unwrap();
    assert!(
        original
            .components
            .iter()
            .all(|c| c.successive_approximation == [0; 64])
    );
    let mut bytes = JPEG.to_vec();
    for (offset, len, _) in scans(JPEG) {
        let header = &mut bytes[offset + 4..offset + 4 + len];
        let n = header[0] as usize;
        let ss = header[1 + 2 * n];
        let ah = header[3 + 2 * n] >> 4;
        if ss == 0 && ah != 0 {
            for j in 0..n {
                header[2 + 2 * j] = 0x33;
            }
        } else if ss != 0 {
            header[2] |= 0x30;
        }
    }
    let changed = decode_coefficients(&bytes, Limits::default()).unwrap();
    for (a, b) in original.components.iter().zip(changed.components.iter()) {
        assert_eq!(a.blocks, b.blocks);
    }
}

#[test]
fn partial_progression_preserves_available_precision() {
    let (offset, len, entropy) = scans(JPEG)[0];
    let end = offset + 4 + len + entropy;
    let mut bytes = JPEG[..end].to_vec();
    bytes.extend_from_slice(&[255, 217]);
    let image = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert_eq!(image.scans, 1);
    for c in &image.components {
        assert_eq!(c.successive_approximation[0], 1);
        assert_eq!(&c.successive_approximation[1..], &[-1; 63]);
        assert!(c.blocks.iter().all(|b| b[1..] == [0; 63]));
    }
    image.reconstruct().unwrap();
}

#[test]
fn invalid_progression_order_and_quantizer_changes() {
    let (offset, len, entropy) = scans(JPEG)[0];
    let mut bytes = JPEG.to_vec();
    bytes[offset + 4 + len - 1] = 0x10;
    assert!(matches!(
        decode_coefficients(&bytes, Limits::default()),
        Err(Error::Invalid("progressive scan order"))
    ));
    let end = offset + 4 + len + entropy;
    let mut bytes = JPEG[..end].to_vec();
    bytes.extend_from_slice(&JPEG[offset..end]);
    bytes.extend_from_slice(&JPEG[end..]);
    assert!(matches!(
        decode_coefficients(&bytes, Limits::default()),
        Err(Error::Invalid("progressive scan order"))
    ));
    let gray = include_bytes!("fixtures/progressive/8-gray.jpg");
    let (offset, len, _) = scans(gray)[0];
    let mut bytes = gray.to_vec();
    bytes[offset + 4 + len - 3] = 1;
    bytes[offset + 4 + len - 2] = 63;
    let mut table = vec![16, 1];
    table.extend_from_slice(&[0; 15]);
    table.push(0);
    let mut combined = bytes[..offset].to_vec();
    segment(&mut combined, 196, &table);
    combined.extend_from_slice(&bytes[offset..]);
    assert!(matches!(
        decode_coefficients(&combined, Limits::default()),
        Err(Error::Invalid("AC scan precedes DC"))
    ));
    let mut changed = vec![255, 219, 0, 67, 0];
    changed.extend_from_slice(&[2; 64]);
    let (offset, len, entropy) = scans(JPEG)[0];
    let end = offset + 4 + len + entropy;
    let mut bytes = JPEG[..end].to_vec();
    bytes.extend_from_slice(&changed);
    bytes.extend_from_slice(&JPEG[end..]);
    assert!(matches!(
        decode_coefficients(&bytes, Limits::default()),
        Err(Error::Invalid(
            "quantizer changed between progressive scans"
        ))
    ));
}

fn segment(out: &mut Vec<u8>, marker: u8, data: &[u8]) {
    out.extend_from_slice(&[255, marker]);
    out.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(data);
}
fn excessive_eob(restart: bool) -> Vec<u8> {
    let mut bytes = vec![255, 216];
    let mut q = vec![0];
    q.extend_from_slice(&[1; 64]);
    segment(&mut bytes, 219, &q);
    let mut tables = vec![0, 1];
    tables.extend_from_slice(&[0; 15]);
    tables.push(0);
    tables.extend_from_slice(&[16, 1]);
    tables.extend_from_slice(&[0; 15]);
    tables.push(0x20);
    segment(&mut bytes, 196, &tables);
    segment(&mut bytes, 194, &[8, 0, 8, 0, 16, 1, 1, 0x11, 0]);
    segment(&mut bytes, 218, &[1, 1, 0, 0, 0, 0]);
    bytes.push(0x3f);
    if restart {
        segment(&mut bytes, 221, &[0, 1]);
    }
    segment(&mut bytes, 218, &[1, 1, 0, 1, 63, 0]);
    bytes.push(0x1f); // EOBRUN=4 for a two-block scan.
    if restart {
        bytes.extend_from_slice(&[255, 208]);
    }
    bytes.extend_from_slice(&[255, 217]);
    bytes
}

#[test]
fn eob_runs_cannot_cross_restart_or_scan_boundary() {
    assert!(matches!(
        decode_coefficients(&excessive_eob(false), Limits::default()),
        Err(Error::Invalid("EOB run exceeds scan"))
    ));
    assert!(matches!(
        decode_coefficients(&excessive_eob(true), Limits::default()),
        Err(Error::Invalid("EOB run crosses restart"))
    ));
}

#[test]
fn negative_dc_refinement_from_maximum_point_transform() {
    let mut bytes = vec![255, 216];
    let mut q = vec![0];
    q.extend_from_slice(&[1; 64]);
    segment(&mut bytes, 219, &q);
    let mut table = vec![0, 1];
    table.extend_from_slice(&[0; 15]);
    table.push(1);
    segment(&mut bytes, 196, &table);
    segment(&mut bytes, 194, &[8, 0, 8, 0, 8, 1, 1, 0x11, 0]);
    segment(&mut bytes, 218, &[1, 1, 0, 0, 0, 13]);
    bytes.push(0x3f); // DC=-1 << 13.
    for al in (0..13).rev() {
        segment(&mut bytes, 218, &[1, 1, 0x33, 0, 0, ((al + 1) << 4) | al]);
        bytes.extend_from_slice(&[255, 0]); // Refinement 1 followed by seven padding bits.
    }
    bytes.extend_from_slice(&[255, 217]);
    let image = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert_eq!(image.components[0].blocks[0][0], -1);
    assert_eq!(image.components[0].successive_approximation[0], 0);
    assert_eq!(image.scans, 14);
}
