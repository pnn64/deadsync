use std::ffi::c_void;
mod cabac_tables;
#[allow(dead_code)]
mod cabac {
    use std::hint::black_box;
    include!("cabac.rs");
    pub fn replay_bins(data: &[u8], mode: &str, repeat: usize) -> (u64, u64) {
        let kind = match mode { "bin" => 0, "bypass" => 1, "sign" => 2, _ => unreachable!() };
        let records: Vec<_> = data.chunks_exact(16).filter(|r| r[0] == kind).collect();
        assert!(!records.is_empty());
        let mut reader = CabacReader::new(&[], 0);
        let mut sig = 0u64;
        for _ in 0..repeat {
            for record in black_box(&records) {
                reader.low = u32::from_le_bytes(record[2..6].try_into().unwrap());
                reader.range = u32::from_le_bytes(record[6..10].try_into().unwrap());
                reader.pos = 0;
                reader.data = &record[14..16];
                let mut state = record[1];
                let v = match kind {
                    0 => reader.get_cabac(&mut state),
                    1 => reader.get_cabac_bypass(),
                    _ => reader.get_cabac_bypass_sign(i32::from_le_bytes(record[10..14].try_into().unwrap())) as u32,
                };
                sig = sig.wrapping_add(v as u64 + state as u64 + reader.low as u64 + reader.range as u64 + reader.pos as u64);
            }
        }
        ((records.len()*repeat) as u64, sig)
    }
    pub fn replay_residual(data: &[u8], repeat: usize) -> (u64, u64) {
        assert_eq!(data.len()%1547, 0);
        let mut reader = CabacReader::new(&[], 0);
        let mut sig = 0u64;
        for _ in 0..repeat {
            for r in black_box(data).chunks_exact(1547) {
                reader.low = u32::from_le_bytes(r[..4].try_into().unwrap());
                reader.range = u32::from_le_bytes(r[4..8].try_into().unwrap());
                reader.pos = 0;
                reader.data = &r[1035..];
                let mut states: [u8;1024] = r[11..1035].try_into().unwrap();
                let (coeffs, count) = reader.decode_residual_cabac_field(&mut states, r[8] as usize, r[9] as usize, r[10]!=0);
                for &(pos, level) in coeffs {
                    sig = sig.wrapping_add((pos as u64).wrapping_mul(31).wrapping_add(level as u64));
                }
                sig = sig.wrapping_add(count as u64 + reader.low as u64 + reader.range as u64 + reader.pos as u64);
                black_box(states);
            }
        }
        ((data.len()/1547*repeat) as u64, sig)
    }
    pub fn replay_init(data: &[u8], repeat: usize) -> (u64,u64) {
        assert_eq!(data.len()%9, 0);
        let mut sig = 0u64;
        for _ in 0..repeat {
            for r in black_box(data).chunks_exact(9) {
                let states = init_cabac_states(i32::from_le_bytes(r[..4].try_into().unwrap()), r[4]!=0, u32::from_le_bytes(r[5..9].try_into().unwrap()));
                sig = sig.wrapping_add(black_box(&states)[r[0] as usize] as u64);
            }
        }
        ((data.len()/9*repeat) as u64, sig)
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
}
fn cycles() -> u64 {
    let mut value = 0;
    assert_ne!(unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) }, 0);
    value
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let cpu: usize = args[1].parse().unwrap();
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << cpu) }, 0);
    let mode = &args[2];
    let data = std::fs::read(&args[3]).unwrap();
    let repeat = args[4].parse().unwrap();
    let start = cycles();
    let (units, signature) = match mode.as_str() {
        "bin" | "bypass" | "sign" => cabac::replay_bins(&data, mode, repeat),
        "cabres" => cabac::replay_residual(&data, repeat),
        "init" => cabac::replay_init(&data, repeat),
        _ => panic!("unknown mode"),
    };
    println!("{units},{signature},{},0,0", cycles()-start);
}
