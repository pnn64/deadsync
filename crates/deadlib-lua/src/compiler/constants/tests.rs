use super::*;

fn equivalent(a: &Constant, b: &Constant) -> bool {
    match (a, b) {
        (Constant::Nil, Constant::Nil) => true,
        (Constant::Boolean(a), Constant::Boolean(b)) => a == b,
        (Constant::Number(a), Constant::Number(b)) => a == b,
        (Constant::String(a), Constant::String(b)) => a == b,
        _ => false,
    }
}
fn same_bits(a: &Constant, b: &Constant) -> bool {
    match (a, b) {
        (Constant::Number(a), Constant::Number(b)) => a.to_bits() == b.to_bits(),
        _ => equivalent(a, b),
    }
}

#[test]
fn indexed_pool_matches_append_ordered_linear_numeric_and_binary_pooling() {
    let mut actual = Constants::default();
    let mut expected: Vec<Constant> = Vec::new();
    let mut random = 0x123456789abcdef0u64;
    for i in 0..12000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let value = match i % 12 {
            0 => Constant::Nil,
            1 => Constant::Boolean(random & 1 != 0),
            2 => Constant::Number(0.0),
            3 => Constant::Number(-0.0),
            4 => Constant::Number(f64::INFINITY),
            5 => Constant::Number(f64::NEG_INFINITY),
            6 => Constant::Number(f64::from_bits(0x7ff8000000000000 | (random & 255))),
            7 => Constant::Number(f64::from_bits(random)),
            8 => Constant::Number((random % 1024) as f64),
            _ => {
                let mut bytes = vec![(random % 64) as u8; (random % 80) as usize];
                if let Some(last) = bytes.last_mut() {
                    *last = (random >> 16) as u8;
                }
                Constant::String(bytes.into_boxed_slice())
            }
        };
        let id = if let Some(i) = expected.iter().position(|v| equivalent(v, &value)) {
            i as u32
        } else {
            let id = expected.len() as u32;
            expected.push(value.clone());
            id
        };
        let got = match &value {
            Constant::String(bytes) if i % 2 == 0 => actual.string(bytes),
            _ => actual.intern(value),
        };
        assert_eq!(got, Some(id));
    }
    let values = actual.finish();
    assert_eq!(values.len(), expected.len());
    assert!(values.iter().zip(&expected).all(|(a, b)| same_bits(a, b)));
}

#[test]
fn collided_wrapped_clusters_keep_full_equality_and_stable_ids_after_growth() {
    let keys: Vec<Vec<u8>> = (0..)
        .map(|i| format!("constant-collision-{i}").into_bytes())
        .filter(|s| Key::String(s).hash().unwrap() & 1023 == 1023)
        .take(196)
        .collect();
    let mut pool = Constants::default();
    for (i, key) in keys.iter().enumerate() {
        assert_eq!(pool.string(key), Some(i as u32));
    }
    assert_ne!(pool.slots[0], 0);
    assert_ne!(*pool.slots.last().unwrap(), 0);
    for (i, key) in keys.iter().enumerate().rev() {
        assert_eq!(
            pool.intern(Constant::String(key.clone().into_boxed_slice())),
            Some(i as u32)
        );
    }
    let values = pool.finish();
    assert_eq!(values.len(), keys.len());
    assert!(
        values
            .iter()
            .zip(keys)
            .all(|(v, k)| matches!(v,Constant::String(s)if s.as_ref()==k))
    );
}

#[test]
fn borrowed_literals_copy_only_new_values_and_own_their_bytes() {
    let mut pool = Constants::default();
    let original = vec![255; 4097];
    let mut caller = original.clone();
    assert_eq!(pool.string(&caller), Some(0));
    caller[2048] = 0;
    assert_eq!(pool.string(&caller), Some(1));
    assert_eq!(pool.string(&original), Some(0));
    caller[0] = 17;
    for i in 0..256 {
        assert_eq!(pool.string(format!("extra-{i}").as_bytes()), Some(i + 2));
    }
    assert_eq!(pool.string(&original), Some(0));
    let mut changed = original.clone();
    changed[2048] = 0;
    assert_eq!(pool.string(&changed), Some(1));
    let values = pool.finish();
    assert!(matches!(&values[0],Constant::String(s)if s.as_ref()==original));
    assert!(matches!(&values[1],Constant::String(s)if s.as_ref()==changed));
}
