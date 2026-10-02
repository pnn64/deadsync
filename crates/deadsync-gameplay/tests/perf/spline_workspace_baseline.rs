// Frozen solver from 98d79dc83 (0.5.1683).
fn old_solve_song_lua_spline(points: &[[f32; 3]]) -> Vec<[[f32; 4]; 3]> {
    let size = points.len();
    let mut out = vec![[[0.0; 4]; 3]; size];
    for (coefficients, point) in out.iter_mut().zip(points) {
        for axis in 0..3 {
            coefficients[axis][0] = point[axis];
        }
    }
    if size < 2 {
        return out;
    }
    let active = std::array::from_fn::<_, 3, _>(|axis| {
        !points.iter().all(|point| point[axis] == points[0][axis])
    });
    if size == 2 {
        for axis in 0..3 {
            if active[axis] {
                out[0][axis][1] = points[1][axis] - points[0][axis];
                out[1][axis][1] = -out[0][axis][1];
            }
        }
        return out;
    }
    if !active.iter().any(|&axis| axis) {
        return out;
    }
    // The three axes share the same tridiagonal matrix. Factor it once,
    // keeping each axis's slopes in its already allocated output coefficient.
    let mut diagonals = vec![4.0_f32; size];
    diagonals[0] = 2.0;
    diagonals[size - 1] = 2.0;
    if active.iter().filter(|&&axis| axis).count() == 1 {
        let axis = active.iter().position(|&axis| axis).unwrap();
        // A single varying axis needs no inner axis loop. Keep its original
        // arithmetic order while still using output storage for the slopes.
        out[0][axis][1] = 3.0 * (points[1][axis] - points[0][axis]);
        for i in 1..size - 1 {
            out[i][axis][1] = 3.0 * (points[i + 1][axis] - points[i - 1][axis]);
        }
        out[size - 1][axis][1] = 3.0 * (points[size - 1][axis] - points[size - 2][axis]);
        for i in 0..size - 1 {
            let multiple = 1.0 / diagonals[i];
            diagonals[i + 1] -= multiple;
            out[i + 1][axis][1] -= out[i][axis][1] * multiple;
        }
        for i in (1..size).rev() {
            out[i - 1][axis][1] -= out[i][axis][1] * (1.0 / diagonals[i]);
        }
        for i in 0..size {
            out[i][axis][1] /= diagonals[i];
        }
        for i in 0..size {
            let next = (i + 1) % size;
            let diff = points[next][axis] - points[i][axis];
            let slope = out[i][axis][1];
            let next_slope = out[next][axis][1];
            out[i][axis][2] = 3.0 * diff - 2.0 * slope - next_slope;
            out[i][axis][3] = -2.0 * diff + slope + next_slope;
        }
        return out;
    }
    for axis in 0..3 {
        if !active[axis] {
            continue;
        }
        out[0][axis][1] = 3.0 * (points[1][axis] - points[0][axis]);
        for i in 1..size - 1 {
            out[i][axis][1] = 3.0 * (points[i + 1][axis] - points[i - 1][axis]);
        }
        out[size - 1][axis][1] = 3.0 * (points[size - 1][axis] - points[size - 2][axis]);
    }
    for i in 0..size - 1 {
        let multiple = 1.0 / diagonals[i];
        diagonals[i + 1] -= multiple;
        for axis in 0..3 {
            if active[axis] {
                out[i + 1][axis][1] -= out[i][axis][1] * multiple;
            }
        }
    }
    for i in (1..size).rev() {
        let multiple = 1.0 / diagonals[i];
        for axis in 0..3 {
            if active[axis] {
                out[i - 1][axis][1] -= out[i][axis][1] * multiple;
            }
        }
    }
    for i in 0..size {
        for axis in 0..3 {
            if active[axis] {
                out[i][axis][1] /= diagonals[i];
            }
        }
    }
    for i in 0..size {
        let next = (i + 1) % size;
        for axis in 0..3 {
            if active[axis] {
                let diff = points[next][axis] - points[i][axis];
                let slope = out[i][axis][1];
                let next_slope = out[next][axis][1];
                out[i][axis][2] = 3.0 * diff - 2.0 * slope - next_slope;
                out[i][axis][3] = -2.0 * diff + slope + next_slope;
            }
        }
    }
    out
}
