

#[path = "projection.rs"]
mod projection;


fn chart(width: usize, measures: usize, rows: usize, pattern: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for owner in 0..2 {
        if owner != 0 {
            data.extend_from_slice(b"\n&\n");
        }
        for measure in 0..measures {
            if measure != 0 {
                data.extend_from_slice(b",\n");
            }
            for row in 0..rows {
                for col in 0..width {
                    let at = row + measure * rows + col * 7 + owner * 11;
                    data.push(match pattern {
                        0 if at.is_multiple_of(31) => b'1',
                        1 if at.is_multiple_of(3) => b'1',
                        2 if at.is_multiple_of(16) => b'2',
                        2 if at % 16 == 7 => b'3',
                        2 if at % 16 == 10 => b'M',
                        _ => b'0',
                    });
                }
                data.push(b'\n');
            }
        }
    }
    data
}


fn simfile(kind: &str, notes: &[u8], charts: usize, extension: &str) -> Vec<u8> {
    let mut data = b"#VERSION:0.83;#TITLE:Partners;#BPMS:0=120;".to_vec();
    for _ in 0..charts {
        if extension == "sm" {
            data.extend_from_slice(
                format!("#NOTES:{kind}:Partners:Hard:7:0,0,0,0,0:\n").as_bytes(),
            );
        } else {
            data.extend_from_slice(format!("#NOTEDATA:;#STEPSTYPE:{kind};#DESCRIPTION:Partners;#DIFFICULTY:Hard;#METER:7;#NOTES:\n").as_bytes());
        }
        data.extend_from_slice(notes);
        data.extend_from_slice(b";\n");
    }
    data
}
