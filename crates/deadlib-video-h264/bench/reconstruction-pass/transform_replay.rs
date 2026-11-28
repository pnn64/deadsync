use std::{ffi::c_void, hint::black_box};
#[allow(dead_code)]
mod pixel {
    pub(crate) trait Pixel: Sized {
        fn store_row(_: &mut [u8], _: usize, _: &[Self]);
        fn add_residual_row(_: &mut [u8], _: usize, _: &[i32]);
        fn put_pred_residual_row(_: &mut [u8], _: usize, _: &[Self], _: &[i32]);
    }
}
#[allow(dead_code)]
mod residual;
mod chroma_fragment;
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
fn run<const N: usize>(data: &[u8], repeat: usize, transform: fn(&mut [i32;N])) {
    assert_eq!(data.len()%(N*4),0);
    let blocks: Vec<[i32;N]> = data.chunks_exact(N*4).map(|r| {
        core::array::from_fn(|i| i32::from_le_bytes(r[i*4..i*4+4].try_into().unwrap()))
    }).collect();
    assert!(!blocks.is_empty());
    let mut sig = 0u64;
    let start = cycles();
    for _ in 0..repeat {
        for b in black_box(&blocks) {
            let mut block = *b;
            transform(&mut block);
            sig = sig.wrapping_add(black_box(&block)[0] as u64);
        }
    }
    let elapsed = cycles()-start;
    println!("{},{sig},{elapsed},0,0", blocks.len()*repeat);
}
fn run_dequant<const N: usize>(data: &[u8], repeat: usize, transform: fn(&mut [i32;N], i32, &[u8;N])) {
    let size = 4+N*5;
    assert_eq!(data.len()%size,0);
    let blocks: Vec<(i32,[u8;N],[i32;N])> = data.chunks_exact(size).map(|r| {
        (i32::from_le_bytes(r[..4].try_into().unwrap()), r[4..4+N].try_into().unwrap(),
         core::array::from_fn(|i| i32::from_le_bytes(r[4+N+i*4..4+N+i*4+4].try_into().unwrap())))
    }).collect();
    assert!(!blocks.is_empty());
    let mut sig = 0u64;
    let start = cycles();
    for _ in 0..repeat {
        for (qp, scale, b) in black_box(&blocks) {
            let mut block = *b;
            transform(&mut block, *qp, scale);
            sig = sig.wrapping_add(black_box(&block)[0] as u64);
        }
    }
    let elapsed = cycles()-start;
    println!("{},{sig},{elapsed},0,0", blocks.len()*repeat);
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let cpu: usize = args[1].parse().unwrap();
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << cpu) }, 0);
    let data = std::fs::read(&args[3]).unwrap();
    let repeat = args[4].parse().unwrap();
    match args[2].as_str() {
        "idct4" => run(&data, repeat, residual::inverse_dct_4x4),
        "idct8" => run(&data, repeat, residual::inverse_dct_8x8),
        "dq4" => run_dequant(&data, repeat, residual::dequant_4x4_full),
        "dq8" => run_dequant(&data, repeat, residual::dequant_8x8),
        "chroma4" => {
            assert_eq!(data.len()%69,0);
            let blocks: Vec<(i32,bool,[i32;16])> = data.chunks_exact(69).map(|r| {
                (i32::from_le_bytes(r[..4].try_into().unwrap()), r[4]!=0,
                 core::array::from_fn(|i| i32::from_le_bytes(r[5+i*4..9+i*4].try_into().unwrap())))
            }).collect();
            assert!(!blocks.is_empty());
            let mut sig = 0u64;
            let start = cycles();
            for _ in 0..repeat {
                for (dc, has_ac, block) in black_box(&blocks) {
                    let result = chroma_fragment::transform(*block, *dc, *has_ac);
                    sig = sig.wrapping_add(black_box(&result)[0] as u64);
                }
            }
            println!("{},{sig},{},0,0", blocks.len()*repeat, cycles()-start);
        }
        _ => panic!("unknown mode"),
    }
}
