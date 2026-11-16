use deadlib_lua::{Value, Vm, compile};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::probe;

// Exact decimal expansion of a dyadic rational, using test-only base-10
// arithmetic independent of the runtime conversion/normalization algorithm.
fn dyadic(numerator: u64, power: i32) -> String {
    let mut digits: Vec<u8> = numerator
        .to_string()
        .bytes()
        .rev()
        .map(|b| b - b'0')
        .collect();
    let multiplier = if power < 0 { 5 } else { 2 };
    for _ in 0..power.unsigned_abs() {
        let mut carry = 0;
        for digit in &mut digits {
            let value = *digit * multiplier + carry;
            *digit = value % 10;
            carry = value / 10;
        }
        if carry != 0 {
            digits.push(carry);
        }
    }
    let fractional = (-power).max(0) as usize;
    digits.resize(digits.len().max(fractional + 1), 0);
    digits.reverse();
    let mut text: String = digits.into_iter().map(|n| char::from(b'0' + n)).collect();
    if fractional != 0 {
        text.insert(text.len() - fractional, '.');
    }
    text
}

fn near_midpoint(lower: u64) -> [(String, u64); 3] {
    let exponent = (lower >> 52) as i32;
    let mantissa = (lower & ((1 << 52) - 1)) | if exponent == 0 { 0 } else { 1 << 52 };
    let power = if exponent == 0 {
        -1075
    } else {
        exponent - 1023 - 53
    };
    let mut middle = dyadic(2 * mantissa + 1, power);
    if !middle.contains('.') {
        middle.push('.');
    }
    // Put the distinguishing digit beyond the runtime's entire retained
    // significand, including for a midpoint near the smallest subnormal.
    middle.push_str(&"0".repeat(1200));
    let mut above = middle.clone().into_bytes();
    *above.last_mut().unwrap() = b'1';
    let mut below = middle.clone().into_bytes();
    for digit in below.iter_mut().rev() {
        if *digit == b'.' {
            continue;
        }
        if *digit == b'0' {
            *digit = b'9';
        } else {
            *digit -= 1;
            break;
        }
    }
    [
        (String::from_utf8(below).unwrap(), lower),
        (middle, lower + (lower & 1)),
        (String::from_utf8(above).unwrap(), lower + 1),
    ]
}

#[test]
fn runtime_conversion_keeps_exact_rounding_without_allocating() {
    let locale = std::env::var("DEADLUA_TEST_LOCALE").unwrap_or_else(|_| {
        if cfg!(windows) {
            "French_France.1252"
        } else {
            "C.UTF-8"
        }
        .into()
    });
    let mut vm = Vm::try_new().unwrap();
    let name = vm.try_intern(locale.as_bytes()).unwrap();
    vm.set_global(b"chosen", name).unwrap();
    let setup = compile(b"assert(os.setlocale(chosen));return tostring(1.5)").unwrap();
    let Value::String(point) = vm.execute(&setup).unwrap()[0] else {
        panic!("formatted number")
    };
    let rendered = vm.string_bytes(point).unwrap();
    let point = String::from_utf8(rendered[1..rendered.len() - 1].to_vec()).unwrap();
    let convert = compile(b"return tonumber(input)").unwrap();
    vm.try_load_program(&convert).unwrap();
    let mut count = 0;
    let mut check = |text: &str, expected: Option<u64>| {
        let localized = text.replace('.', &point);
        let value = vm.try_intern(localized.as_bytes()).unwrap();
        vm.set_global(b"input", value).unwrap();
        vm.collect_garbage().unwrap();
        vm.try_load_program(&convert).unwrap();
        let (result, allocations) = probe(1, || vm.execute(&convert).map(|v| v[0]));
        assert_eq!(
            allocations.attempts,
            0,
            "conversion allocated: {} bytes",
            text.len()
        );
        let actual = match result.unwrap() {
            Value::Number(n) => Some(n.to_bits()),
            Value::Nil => None,
            v => panic!("unexpected {v:?}"),
        };
        assert_eq!(
            actual,
            expected,
            "conversion mismatch for {} bytes: {:.100}",
            text.len(),
            text
        );
        count += 1;
    };
    for text in [
        "0.0",
        "-0.0",
        "+0.0",
        "1.5",
        ".25",
        "25.",
        "  -1.25e+3  ",
        "1.5e9999999999999999999999999999999999999999999999999999",
        "-1.5e-9999999999999999999999999999999999999999999999999999",
    ] {
        check(text, Some(text.trim().parse::<f64>().unwrap().to_bits()));
    }
    for lower in [
        0,
        1,
        2,
        0x000f_ffff_ffff_fffe,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x3caf_ffff_ffff_ffff,
        0x3fef_ffff_ffff_ffff,
        0x3ff0_0000_0000_0000,
        0x3ff0_0000_0000_0001,
        0x43df_ffff_ffff_ffff,
        0x7fef_ffff_ffff_fffe,
        0x7fef_ffff_ffff_ffff,
    ] {
        for (index, (text, bits)) in near_midpoint(lower).into_iter().enumerate() {
            assert_eq!(
                text.parse::<f64>().unwrap().to_bits(),
                bits,
                "independent midpoint fixture"
            );
            // The pinned UCRT loses the distant nonzero suffix above a tie;
            // Linux retains it. Keep the exact-arithmetic fixture check above
            // and assert each platform's separately observed Lua result here.
            let observed = if cfg!(all(windows, target_env = "msvc")) && index == 2 {
                lower + (lower & 1)
            } else {
                bits
            };
            check(&text, Some(observed));
            check(&format!("-{text}"), Some(observed | (1 << 63)));
        }
    }
    for (index, text) in [
        format!("{}1.25", "0".repeat(30000)),
        format!("1.{}5", "0".repeat(30000)),
        format!("1{}5.e-30001", "0".repeat(30000)),
        format!("0.{}15e30002", "0".repeat(30000)),
        format!(
            "-0.{}e99999999999999999999999999999999999999999999",
            "0".repeat(30000)
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // UCRT clamps the explicit exponent before its radix adjustment.
        let value = match (cfg!(all(windows, target_env = "msvc")), index) {
            (true, 2) => 0.0,
            (true, 3) => f64::INFINITY,
            _ => text.parse::<f64>().unwrap(),
        };
        check(&text, Some(value.to_bits()));
    }
    for (text, value) in [
        ("0x1.8p1", 3.0f64),
        ("-0x.8p+2", -2.0),
        ("+0Xab.cdP-3", 21.47509765625),
        ("0x0.0p-99999", 0.0),
        ("-0x0.0p99999", -0.0),
        ("0x1.0p99999", f64::INFINITY),
        ("0x1.0p-99999", 0.0),
        ("0x1.00000000000008p0", 1.0),
        (
            "0x1.00000000000008000001p0",
            f64::from_bits(1.0f64.to_bits() + 1),
        ),
    ] {
        check(text, Some(value.to_bits()));
    }
    for (index, text) in [
        format!("0x1.{}1p0", "0".repeat(30000)),
        format!("0x0.{}1p120004", "0".repeat(30000)),
    ]
    .into_iter()
    .enumerate()
    {
        let value = if cfg!(all(windows, target_env = "msvc")) && index == 1 {
            f64::INFINITY
        } else {
            1.0
        };
        check(&text, Some(value.to_bits()));
    }
    for text in [
        "",
        ".",
        "+.",
        "1.2.3",
        "1.2e",
        "1.2e+",
        "1.2e-",
        "1.2e3x",
        "1.2e999999999999999999999999999999999x",
        "0x.",
        "0x..1",
        "0x1.2p",
        "0x1.2p+",
        "0x1.2p-",
        "0x1.2pbad",
        "0x1.2p1x",
        "0x1pbad",
        "0x1p99999999999999999999999999999999x",
        "1.5\0ignored",
    ] {
        check(
            text,
            if text.contains('\0') {
                Some(1.5f64.to_bits())
            } else {
                None
            },
        );
    }
    let mut state = 0x8abc_de01_2345_6789u64;
    for i in 0..400 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let text = format!("{}.{}e{}", state, state.rotate_left(29), (i % 700) - 350);
        check(&text, Some(text.parse::<f64>().unwrap().to_bits()));
    }
    eprintln!("numeric conversion used zero allocations for {count} cases ({locale})");
    vm.execute(&compile(b"assert(os.setlocale('C'))").unwrap())
        .unwrap();
}
