import pathlib, shutil, sys

root = pathlib.Path(__file__).resolve().parent
source = pathlib.Path(sys.argv[1])
dest = root / "trace-src"
shutil.copytree(source, dest, dirs_exist_ok=True)
shutil.copyfile(source.parent / "README.md", root / "README.md")
p = dest / "cabac.rs"
s = p.read_text(encoding="utf-8")
for needle, call in [
    ("pub fn get_cabac(&mut self, state: &mut u8) -> u32 {", "self.trace_bin(0, *state, 0);"),
    ("pub fn get_cabac_bypass(&mut self) -> u32 {", "self.trace_bin(1, 0, 0);"),
    ("pub fn get_cabac_bypass_sign(&mut self, val: i32) -> i32 {", "self.trace_bin(2, 0, val);"),
    (") -> (&[(usize, i32)], u8) {", "self.trace_residual(state, cat, max_coeff, field_coded);"),
    ("pub fn init_cabac_states(slice_qp: i32, is_i_slice: bool, cabac_init_idc: u32) -> [u8; 1024] {", "trace_init(slice_qp, is_i_slice, cabac_init_idc);"),
]:
    assert s.count(needle) == 1, needle
    s = s.replace(needle, needle + "\n        " + call, 1)
s += """
fn trace_write(kind: usize, bytes: &[u8]) {
    use std::{io::Write, sync::{Mutex, OnceLock}};
    static FILES: [OnceLock<Option<Mutex<std::fs::File>>>; 3] = [const { OnceLock::new() }; 3];
    let Some(file) = FILES[kind].get_or_init(|| {
        std::env::var_os("H264_TRACE_PREFIX").map(|prefix| {
            let mut path = std::path::PathBuf::from(prefix);
            path.set_extension(["bins", "residual", "init"][kind]);
            Mutex::new(std::fs::File::create(path).unwrap())
        })
    }) else { return };
    file.lock().unwrap().write_all(bytes).unwrap();
}
fn trace_init(qp: i32, intra: bool, idc: u32) {
    let mut record = [0; 9];
    record[..4].copy_from_slice(&qp.to_le_bytes());
    record[4] = intra as u8;
    record[5..].copy_from_slice(&idc.to_le_bytes());
    trace_write(2, &record);
}
impl CabacReader<'_> {
    fn trace_bin(&self, kind: u8, state: u8, value: i32) {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static COUNTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
        if COUNTS[kind as usize].fetch_add(1, Ordering::Relaxed) % 512 != 0 { return; }
        let mut record = [0u8; 16];
        record[0] = kind;
        record[1] = state;
        record[2..6].copy_from_slice(&self.low.to_le_bytes());
        record[6..10].copy_from_slice(&self.range.to_le_bytes());
        record[10..14].copy_from_slice(&value.to_le_bytes());
        record[14] = self.data.get(self.pos).copied().unwrap_or(0);
        record[15] = self.data.get(self.pos + 1).copied().unwrap_or(0);
        trace_write(0, &record);
    }
    fn trace_residual(&self, state: &[u8;1024], cat: usize, count: usize, field: bool) {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        if COUNT.fetch_add(1, Ordering::Relaxed) % 128 != 0 { return; }
        let mut record = [0u8; 1547];
        record[..4].copy_from_slice(&self.low.to_le_bytes());
        record[4..8].copy_from_slice(&self.range.to_le_bytes());
        record[8] = cat as u8;
        record[9] = count as u8;
        record[10] = field as u8;
        record[11..1035].copy_from_slice(state);
        if let Some(tail) = self.data.get(self.pos..) {
            let n = tail.len().min(512);
            record[1035..1035+n].copy_from_slice(&tail[..n]);
        }
        trace_write(1, &record);
    }
}
"""
p.write_text(s, encoding="utf-8")
