// Frozen from 55ca4c72e for exact-output comparisons.
fn original_scan<const LANES: usize>(
    mut data: &[u8],
    cap: Option<usize>,
    mut visit: impl FnMut(usize) -> bool,
) {
    // Empty-subdivision reduction cannot remove a step: a nonzero off-grid row
    // prevents that reduction level. The reduced density is this direct count.
    let mut measure_steps = 0usize;
    while !data.is_empty() {
        if data[0].is_ascii_whitespace() {
            data = skip_ws(data);
            continue;
        }
        // Minimized charts normally contain exactly one fixed-width row per line.
        if data.len() > LANES
            && (data[LANES] == b'\n' || data[LANES..].starts_with(b"\r\n"))
            && !data[..LANES].contains(&b'\n')
            && data[LANES - 1] != b'\r'
            && !matches!(data[0], b'/' | b',' | b';')
        {
            if cap.is_none_or(|limit| measure_steps < limit) {
                measure_steps += usize::from(original_row_has_step::<LANES>(data));
            }
            data = &data[LANES + 1 + usize::from(data[LANES] == b'\r')..];
            continue;
        }
        let (raw, rest) = data
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or((data, &[][..]), |end| (&data[..end], &data[end + 1..]));
        data = rest;
        let line = trim_cr(raw);
        if line.is_empty() || line[0] == b'/' {
            continue;
        }
        match line[0] {
            b',' => {
                if !visit(std::mem::take(&mut measure_steps)) {
                    return;
                }
            }
            b';' => {
                visit(measure_steps);
                return;
            }
            _ if line.len() >= LANES && cap.is_none_or(|limit| measure_steps < limit) => {
                measure_steps += usize::from(original_row_has_step::<LANES>(line));
            }
            _ => {}
        }
    }
    visit(measure_steps);
}

fn original_row_has_step<const LANES: usize>(line: &[u8]) -> bool {
    const IS_STEP: [bool; 256] = {
        let mut table = [false; 256];
        table[b'1' as usize] = true;
        table[b'2' as usize] = true;
        table[b'4' as usize] = true;
        table
    };
    line[..LANES].iter().any(|&byte| IS_STEP[byte as usize])
}
