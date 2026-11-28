import pathlib, shutil, sys

root = pathlib.Path(__file__).resolve().parent
source = pathlib.Path(sys.argv[1])
dest = root / "chroma-src"
shutil.copytree(source, dest, dirs_exist_ok=True)
p = dest / "chroma.rs"
s = p.read_text(encoding="utf-8")
needle = "        block[0] = dc;\n        inverse_dct_4x4(&mut block);"
assert s.count(needle) == 1
s = s.replace(needle, "        trace_chroma(&block, dc, has_ac);\n" + needle, 1)
s += """
fn trace_chroma(block: &[i32;16], dc: i32, has_ac: bool) {
    use std::{io::Write, sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}}};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    if COUNT.fetch_add(1, Ordering::Relaxed) % 64 != 0 { return; }
    let Some(file) = FILE.get_or_init(|| {
        std::env::var_os("H264_TRACE_PREFIX").map(|prefix| {
            let mut path = std::path::PathBuf::from(prefix);
            path.set_extension("chroma4");
            Mutex::new(std::fs::File::create(path).unwrap())
        })
    }) else { return };
    let mut record = [0u8; 69];
    record[..4].copy_from_slice(&dc.to_le_bytes());
    record[4] = u8::from(has_ac);
    for (bytes, value) in record[5..].chunks_exact_mut(4).zip(block) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    file.lock().unwrap().write_all(&record).unwrap();
}
"""
p.write_text(s, encoding="utf-8")
