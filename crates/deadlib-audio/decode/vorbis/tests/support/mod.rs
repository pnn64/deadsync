#![allow(dead_code)]
use deadlib_audio_decode_ogg::{Demuxer, PageReader};
use deadlib_audio_decode_vorbis::*;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

pub fn packets(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut pages = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::default();
    let mut out = Vec::new();
    let mut serial = None;
    while let Some(located) = pages.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |packet| {
                if let Some(serial) = serial {
                    assert_eq!(serial, packet.info.serial, "single-stream test helper");
                }
                serial = Some(packet.info.serial);
                out.push(packet.data.to_vec());
            })
            .unwrap();
    }
    demux.finish().unwrap();
    out
}
pub fn decoder(packets: &[Vec<u8>]) -> PacketDecoder {
    let id = Identification::parse(&packets[0]).unwrap();
    let setup = Setup::parse(&packets[2], &id, &Limits::default()).unwrap();
    PacketDecoder::new(setup).unwrap()
}
pub fn oracle(mode: &str, packets: &[&[u8]]) -> Vec<u8> {
    let mut input = Vec::new();
    for packet in packets {
        input.extend((packet.len() as u64).to_le_bytes());
        input.extend(*packet);
    }
    file_oracle(mode, &input)
}
pub fn file_oracle(mode: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let executable =
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set DEADLIB_LIBVORBIS_ORACLE");
    let file = std::env::temp_dir().join(format!(
        "deadlib-vorbis-pcm-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, input).unwrap();
    let output = Command::new(executable)
        .arg(mode)
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub fn take<'a>(input: &mut &'a [u8], count: usize) -> &'a [u8] {
    let (out, rest) = input.split_at(count);
    *input = rest;
    out
}
pub fn u32_in(input: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(input, 4).try_into().unwrap())
}
pub fn paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            out.extend(paths(&entry.path()));
        } else if entry
            .path()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ogg"))
        {
            out.push(entry.path());
        }
    }
    out.sort();
    out
}
#[derive(Default)]
pub struct Difference {
    pub samples: u64,
    pub peak_error: f64,
    pub error_energy: f64,
    pub peak_reference: f64,
}
impl Difference {
    pub fn merge(&mut self, other: &Self) {
        self.samples += other.samples;
        self.peak_error = self.peak_error.max(other.peak_error);
        self.error_energy += other.error_energy;
        self.peak_reference = self.peak_reference.max(other.peak_reference);
    }
    pub fn add(&mut self, native: f32, reference: f32) {
        assert!(native.is_finite() && reference.is_finite());
        let difference = native as f64 - reference as f64;
        self.samples += 1;
        self.peak_error = self.peak_error.max(difference.abs());
        self.error_energy += difference * difference;
        self.peak_reference = self.peak_reference.max(reference.abs() as f64);
    }
    pub fn rms(&self) -> f64 {
        (self.error_energy / self.samples.max(1) as f64).sqrt()
    }
    pub fn check(&self, label: &str) {
        self.check_with_bounds(label, 3.0e-6, 5.0e-7);
    }
    pub fn check_with_bounds(&self, label: &str, peak: f64, rms: f64) {
        eprintln!(
            "{label}: {} samples, peak error {:.9e}, RMS {:.9e}, signal peak {:.6}",
            self.samples,
            self.peak_error,
            self.rms(),
            self.peak_reference
        );
        assert!(
            self.peak_error <= peak * self.peak_reference.max(1.0),
            "{label}: PCM peak tolerance"
        );
        assert!(
            self.rms() <= rms * self.peak_reference.max(1.0),
            "{label}: PCM RMS tolerance"
        );
    }
}
