pub fn uv(frame_index: u32, frame_cols: usize, frame_rows: usize) -> [f32; 4] {
    let cols = frame_cols.max(1) as u32;
    let rows = frame_rows.max(1) as u32;
    let col = frame_index % cols;
    let row = (frame_index / cols).min(rows - 1);
    let cell = [1.0 / cols as f32, 1.0 / rows as f32];
    let left = col as f32 * cell[0];
    let top = row as f32 * cell[1];
    [left, top, left + cell[0], top + cell[1]]
}
