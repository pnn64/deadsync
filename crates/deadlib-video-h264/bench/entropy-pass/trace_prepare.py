import pathlib, shutil, sys

root = pathlib.Path(__file__).resolve().parent
source = pathlib.Path(sys.argv[1])
dest = root / "trace-crate"
shutil.copytree(source, dest, dirs_exist_ok=True)
bits = dest / "src/bitstream.rs"
s = bits.read_text(encoding="utf-8")
s = s.replace("pub fn read_bits(&mut self, n: u8) -> Result<u32, &'static str> {", "pub fn read_bits(&mut self, n: u8) -> Result<u32, &'static str> {\n        self.record(2, 0, n);", 1)
s = s.replace("pub fn read_ue(&mut self) -> Result<u32, &'static str> {", "pub fn read_ue(&mut self) -> Result<u32, &'static str> {\n        self.record(1, 0, 0);", 1)
s += """
impl BitstreamReader<'_> {
    pub(crate) fn record(&self, tag: u8, param: i32, width: u8) {
        use std::{io::Write, sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}}};
        static COUNTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
        static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
        if COUNTS[tag as usize].fetch_add(1, Ordering::Relaxed) % 32 != 0 { return; }
        let Some(tail) = self.data.get(self.byte_offset..) else { return };
        let data = &tail[..tail.len().min(256)];
        let Some(file) = FILE.get_or_init(|| std::env::var_os("H264_TRACE_FILE").map(|p| Mutex::new(std::fs::File::create(p).unwrap()))) else { return; };
        let mut header = [0u8; 9];
        header[0] = tag;
        header[1..5].copy_from_slice(&param.to_le_bytes());
        header[5] = width;
        header[6] = self.bit_offset;
        header[7..9].copy_from_slice(&(data.len() as u16).to_le_bytes());
        let mut file = file.lock().unwrap();
        file.write_all(&header).unwrap();
        file.write_all(data).unwrap();
    }
}
"""
bits.write_text(s, encoding="utf-8")
cavlc = dest / "src/cavlc.rs"
s = cavlc.read_text(encoding="utf-8")
needle = ") -> Result<u8, &'static str> {\n    for c in coeffs.iter_mut() {"
assert needle in s
s = s.replace(needle, ") -> Result<u8, &'static str> {\n    reader.record(0, nc, max_num_coeff as u8);\n    for c in coeffs.iter_mut() {", 1)
cavlc.write_text(s, encoding="utf-8")
