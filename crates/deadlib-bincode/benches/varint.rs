use bincode::config;
use criterion::{criterion_group, criterion_main, Criterion};

fn slice_varint_u8(c: &mut Criterion) {
    let input: Vec<u8> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("slice_varint_u8", |b| {
        b.iter(|| {
            let _: (Vec<u8>, usize) = bincode::decode_from_slice(&bytes, config).unwrap();
        })
    });
}

fn slice_varint_u16(c: &mut Criterion) {
    let input: Vec<u16> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("slice_varint_u16", |b| {
        b.iter(|| {
            let _: (Vec<u16>, usize) = bincode::decode_from_slice(&bytes, config).unwrap();
        })
    });
}

fn slice_varint_u32(c: &mut Criterion) {
    let input: Vec<u32> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("slice_varint_u32", |b| {
        b.iter(|| {
            let _: (Vec<u32>, usize) = bincode::decode_from_slice(&bytes, config).unwrap();
        })
    });
}

fn slice_varint_u64(c: &mut Criterion) {
    let input: Vec<u64> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("slice_varint_u64", |b| {
        b.iter(|| {
            let _: (Vec<u64>, usize) = bincode::decode_from_slice(&bytes, config).unwrap();
        })
    });
}

fn bufreader_varint_u8(c: &mut Criterion) {
    let input: Vec<u8> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("bufreader_varint_u8", |b| {
        b.iter(|| {
            let _: Vec<u8> =
                bincode::decode_from_reader(&mut std::io::BufReader::new(&bytes[..]), config)
                    .unwrap();
        })
    });
}

fn bufreader_varint_u16(c: &mut Criterion) {
    let input: Vec<u16> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("bufreader_varint_u16", |b| {
        b.iter(|| {
            let _: Vec<u16> =
                bincode::decode_from_reader(&mut std::io::BufReader::new(&bytes[..]), config)
                    .unwrap();
        })
    });
}

fn bufreader_varint_u32(c: &mut Criterion) {
    let input: Vec<u32> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("bufreader_varint_u32", |b| {
        b.iter(|| {
            let _: Vec<u32> =
                bincode::decode_from_reader(&mut std::io::BufReader::new(&bytes[..]), config)
                    .unwrap();
        })
    });
}

fn bufreader_varint_u64(c: &mut Criterion) {
    let input: Vec<u64> = (0..10_000).map(|_| rand::random()).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(input, config).unwrap();

    c.bench_function("bufreader_varint_u64", |b| {
        b.iter(|| {
            let _: Vec<u64> =
                bincode::decode_from_reader(&mut std::io::BufReader::new(&bytes[..]), config)
                    .unwrap();
        })
    });
}

criterion_group!(
    benches,
    slice_varint_u8,
    slice_varint_u16,
    slice_varint_u32,
    slice_varint_u64,
    bufreader_varint_u8,
    bufreader_varint_u16,
    bufreader_varint_u32,
    bufreader_varint_u64,
);
criterion_main!(benches);
