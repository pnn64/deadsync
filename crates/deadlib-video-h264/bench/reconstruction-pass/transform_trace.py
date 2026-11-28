import pathlib, shutil, sys

root = pathlib.Path(__file__).resolve().parent
source = pathlib.Path(sys.argv[1])
dest = root / "transform-src"
shutil.copytree(source, dest, dirs_exist_ok=True)
p = dest / "residual.rs"
s = p.read_text(encoding="utf-8")
for needle, call in [
    ("pub fn inverse_dct_4x4(block: &mut [i32; 16]) {", "trace_transform(block, 0);"),
    ("pub fn inverse_dct_8x8(block: &mut [i32; 64]) {", "trace_transform(block, 1);"),
    ("pub fn dequant_4x4_full(block: &mut [i32; 16], qp: i32, scale: &[u8; 16]) {", "trace_dequant(block, qp, scale, 0);"),
    ("pub fn dequant_8x8(block: &mut [i32; 64], qp: i32, scale: &[u8; 64]) {", "trace_dequant(block, qp, scale, 1);"),
]:
    assert s.count(needle) == 1, needle
    s = s.replace(needle, needle + "\n    " + call, 1)
s += """
fn trace_transform<const N: usize>(block: &[i32;N], kind: usize) {
    use std::{io::Write, sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}}};
    static COUNTS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
    static FILES: [OnceLock<Option<Mutex<std::fs::File>>>; 2] = [const { OnceLock::new() }; 2];
    if COUNTS[kind].fetch_add(1, Ordering::Relaxed) % 64 != 0 { return; }
    let Some(file) = FILES[kind].get_or_init(|| {
        std::env::var_os("H264_TRACE_PREFIX").map(|prefix| {
            let mut path = std::path::PathBuf::from(prefix);
            path.set_extension(["idct4", "idct8"][kind]);
            Mutex::new(std::fs::File::create(path).unwrap())
        })
    }) else { return };
    let mut record = [0u8; 256];
    for (bytes, value) in record.chunks_exact_mut(4).zip(block) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    file.lock().unwrap().write_all(&record[..N*4]).unwrap();
}
fn trace_dequant<const N: usize>(block: &[i32;N], qp: i32, scale: &[u8;N], kind: usize) {
    use std::{io::Write, sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}}};
    static COUNTS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
    static FILES: [OnceLock<Option<Mutex<std::fs::File>>>; 2] = [const { OnceLock::new() }; 2];
    if COUNTS[kind].fetch_add(1, Ordering::Relaxed) % 64 != 0 { return; }
    let Some(file) = FILES[kind].get_or_init(|| {
        std::env::var_os("H264_TRACE_PREFIX").map(|prefix| {
            let mut path = std::path::PathBuf::from(prefix);
            path.set_extension(["dq4", "dq8"][kind]);
            Mutex::new(std::fs::File::create(path).unwrap())
        })
    }) else { return };
    let mut record = [0u8; 324];
    record[..4].copy_from_slice(&qp.to_le_bytes());
    record[4..4+N].copy_from_slice(scale);
    for (bytes, value) in record[4+N..].chunks_exact_mut(4).zip(block) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    file.lock().unwrap().write_all(&record[..4+N*5]).unwrap();
}
"""
p.write_text(s, encoding="utf-8")
