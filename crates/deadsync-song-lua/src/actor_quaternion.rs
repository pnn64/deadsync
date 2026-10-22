//! Actor::AddRotationH/P/R and RageMath's quaternion arithmetic.

pub(crate) const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub(crate) fn rotate(a: [f32; 4], axis: usize, degrees: f32) -> [f32; 4] {
    let angle = -(degrees * (std::f32::consts::PI / 180.0) / 2.0);
    let mut b = [0.0, 0.0, 0.0, angle.cos()];
    b[axis] = angle.sin();
    multiply(a, b)
}

pub(crate) fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = a;
    let [bx, by, bz, bw] = b;
    let mut out = [
        w * bx + x * bw + y * bz - z * by,
        w * by + y * bw + z * bx - x * bz,
        w * bz + z * bw + x * by - y * bx,
        w * bw - x * bx - y * by - z * bz,
    ];
    let square = out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3];
    let inverse = if square > 0.0 {
        1.0 / square.sqrt()
    } else {
        1.0
    };
    for component in &mut out {
        *component *= inverse;
    }
    out
}

pub(crate) fn slerp(from: [f32; 4], mut to: [f32; 4], t: f32) -> [f32; 4] {
    let mut cosine = from[0] * to[0] + from[1] * to[1] + from[2] * to[2] + from[3] * to[3];
    if cosine < 0.0 {
        cosine = -cosine;
        to = to.map(|value| -value);
    }
    let (a, b) = if cosine < 0.9999 {
        let omega = cosine.acos();
        let sine = omega.sin();
        (((1.0 - t) * omega).sin() / sine, (t * omega).sin() / sine)
    } else {
        (1.0 - t, t)
    };
    std::array::from_fn(|i| a * from[i] + b * to[i])
}

pub(crate) fn matrix(q: [f32; 4]) -> [f32; 16] {
    let [x, y, z, w] = q;
    let (xx, xy, xz) = (x * (x + x), x * (y + y), x * (z + z));
    let (wx, wy, wz) = (w * (x + x), w * (y + y), w * (z + z));
    let (yy, yz, zz) = (y * (y + y), y * (z + z), z * (z + z));
    // Preserve RageMatrixFromQuat's constructor order (m[column][row]).
    [
        1.0 - (yy + zz),
        xy + wz,
        xz - wy,
        0.0,
        xy - wz,
        1.0 - (xx + zz),
        yz + wx,
        0.0,
        xz + wy,
        yz - wx,
        1.0 - (xx + yy),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}
