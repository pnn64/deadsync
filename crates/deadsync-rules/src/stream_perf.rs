use super::*;

fn chart_data(lanes: usize, densities: &[usize], crlf: bool, decorated: bool) -> Vec<u8> {
    let mut data = Vec::new();
    for (measure, &density) in densities.iter().enumerate() {
        if decorated {
            data.extend_from_slice(b" // ignored 1111,;\n");
        }
        for row in 0..density.max(16) {
            if decorated {
                data.extend_from_slice(b" \t");
            }
            for column in 0..lanes {
                data.push(if row < density && column == (row + measure) % lanes {
                    b'1'
                } else {
                    b'0'
                });
            }
            if crlf {
                data.push(b'\r');
            }
            data.push(b'\n');
        }
        data.extend_from_slice(if measure + 1 == densities.len() {
            b";\n"
        } else {
            b",\n"
        });
    }
    data
}

#[test]
fn fixed_row_scan_preserves_irregular_lines_and_lane_fallbacks() {
    for lanes in [4, 5, 8, 10] {
        for crlf in [false, true] {
            for decorated in [false, true] {
                let densities = [0, 1, 15, 16, 17, 31, 32, 192];
                let data = chart_data(lanes, &densities, crlf, decorated);
                assert_eq!(measure_densities(&data, lanes), densities);
                assert_eq!(
                    stream_measure_densities(&data, lanes),
                    [0, 1, 15, 16, 17, 31, 32, 32]
                );
            }
        }
    }
}

#[test]
fn capped_progress_preserves_thresholds_run_bounds_and_early_end() {
    let data = chart_data(4, &[0, 16, 192, 24, 0, 16], false, false);
    assert_eq!(stream_run_progress(&data, 4, 16, 2), Some((2, 3)));
    assert_eq!(stream_run_progress(&data, 4, 16, 4), None);
    assert_eq!(stream_run_progress(b";\n1111\n", 4, 1, 0), None);
}
