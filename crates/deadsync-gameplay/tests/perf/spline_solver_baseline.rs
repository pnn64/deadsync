// Frozen solver from d209a1380 (0.5.1682).
fn old_solve_song_lua_spline(points: &[[f32; 3]]) -> Vec<[[f32; 4]; 3]> {
    let size = points.len();
    let mut out = vec![[[0.0; 4]; 3]; size];
    let mut diagonals = vec![4.0_f32; size];
    let mut slopes = vec![0.0_f32; size];
    for axis in 0..3 {
        for (coefficients, point) in out.iter_mut().zip(points) {
            coefficients[axis][0] = point[axis];
        }
        if size < 2 || points.iter().all(|point| point[axis] == points[0][axis]) {
            continue;
        }
        if size == 2 {
            out[0][axis][1] = points[1][axis] - points[0][axis];
            out[1][axis][1] = -out[0][axis][1];
            continue;
        }
        diagonals.fill(4.0);
        diagonals[0] = 2.0;
        diagonals[size - 1] = 2.0;
        slopes[0] = 3.0 * (points[1][axis] - points[0][axis]);
        for i in 1..size - 1 {
            slopes[i] = 3.0 * (points[i + 1][axis] - points[i - 1][axis]);
        }
        slopes[size - 1] = 3.0 * (points[size - 1][axis] - points[size - 2][axis]);
        for i in 0..size - 1 {
            let multiple = 1.0 / diagonals[i];
            diagonals[i + 1] -= multiple;
            slopes[i + 1] -= slopes[i] * multiple;
        }
        for i in (1..size).rev() {
            slopes[i - 1] -= slopes[i] * (1.0 / diagonals[i]);
        }
        for i in 0..size {
            slopes[i] /= diagonals[i];
        }
        for i in 0..size {
            let next = (i + 1) % size;
            let diff = points[next][axis] - points[i][axis];
            out[i][axis][1..].copy_from_slice(&[
                slopes[i],
                3.0 * diff - 2.0 * slopes[i] - slopes[next],
                -2.0 * diff + slopes[i] + slopes[next],
            ]);
        }
    }
    out
}
