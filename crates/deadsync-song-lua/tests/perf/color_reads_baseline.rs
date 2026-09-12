// Frozen from e7501fe239507bde3eaa96cd30c4affaacad10db; only test visibility/formatting differs.

use super::*;

pub(super) fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = method_arg(args, 0).cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = method_arg(args, 0).cloned().and_then(read_f32)?;
    let g = method_arg(args, 1).cloned().and_then(read_f32)?;
    let b = method_arg(args, 2).cloned().and_then(read_f32)?;
    let a = method_arg(args, 3)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(1.0);
    Some([r, g, b, a])
}

pub(super) fn read_color_call(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = args.front().cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = args.front().cloned().and_then(read_f32)?;
    let g = args.get(1).cloned().and_then(read_f32)?;
    let b = args.get(2).cloned().and_then(read_f32)?;
    let a = args.get(3).cloned().and_then(read_f32).unwrap_or(1.0);
    Some([r, g, b, a])
}
