use deadsync_config::ini::SimpleIni;
use deadsync_config::options::{RuntimeIoLoadOptions, load_runtime_io_options};
use deadsync_lights::{
    DriverKind, GameplayPadLightMode, PacDriveLightOrdering, SerialPortName,
    parse_driver_or_default, parse_gameplay_pad_lights_or_default,
};
use std::fmt::Debug;
use std::hint::black_box;
use std::str::FromStr;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/light_text_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

const DRIVERS: &[&str] = &[
    "",
    "0",
    "false",
    "off",
    "none",
    "disabled",
    "snek",
    "snekboard",
    "lit",
    "litboard",
    "sextetstream",
    "win32serial",
    "lightsdriverwin32serial",
    "sextetserial",
    "fusion",
    "icedragonfusion",
    "lightsdriverfusion",
    "gpb",
    "icedragongpb",
    "lightsdrivergpb",
    "pac",
    "pacdrive",
    "ultimarcpacdrive",
    "piuio",
    "piuioleds",
    "linuxpiuioleds",
    "lightsdriverpiuioleds",
    "itgio",
    "linuxitgio",
    "lightsdriveritgio",
    "hidbluedot",
    "bluedot",
    "stac2",
    "stacv2",
    "stac2hid",
    "icedragonstac2",
    "minimaid",
    "minimaidhid",
    "linuxminimaid",
    "win32minimaid",
];
const MODES: &[&str] = &[
    "", "input", "buttons", "press", "pressed", "chart", "notes", "notedata", "arrows",
];

fn check_names<T: Debug + PartialEq>(
    names: &[&str],
    parse: impl Fn(&str) -> Result<T, ()>,
    reference: impl Fn(&str) -> Result<T, ()>,
) {
    let check = |raw: &str| assert_eq!(parse(raw), reference(raw), "{raw:?}");
    for name in names {
        check(name);
        check(&name.to_ascii_uppercase());
        for index in 0..name.len() {
            let mut case = name.as_bytes().to_vec();
            case[index] = case[index].to_ascii_uppercase();
            check(std::str::from_utf8(&case).unwrap());
        }
        for mask in 0..32 {
            let case: String = name
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if (mask >> (i % 5)) & 1 != 0 {
                        byte.to_ascii_uppercase()
                    } else {
                        byte
                    })
                })
                .collect();
            check(&case);
            check(&format!("\u{2003}{case}\u{a0}"));
        }
        for index in 0..=name.len() {
            for byte in 0..=127 {
                check(&format!(
                    "{}{}{}",
                    &name[..index],
                    char::from(byte),
                    &name[index..]
                ));
            }
            for extra in ["\u{e9}", "\u{96ea}", "\u{212a}", "\u{2003}", "\0", "-_ ."] {
                check(&format!("{}{extra}{}", &name[..index], &name[index..]));
            }
        }
        check(&format!("{name}{}", "a".repeat(4096)));
        check(&format!(
            "{}{name}{}",
            "\u{96ea}-_".repeat(1024),
            " .\u{e9}".repeat(1024)
        ));
    }
    for raw in [
        "",
        "unknown",
        "\u{96ea}",
        "\u{212a}",
        "-_.\0",
        " \r\n",
        "\u{2003}\u{a0}",
    ] {
        check(raw);
    }
}

#[test]
fn light_preferences_preserve_aliases_ascii_folding_ignored_unicode_and_bounds() {
    check_names(DRIVERS, DriverKind::from_str, original::driver);
    check_names(MODES, GameplayPadLightMode::from_str, original::mode);
}

fn check_port(raw: &str) {
    for fallback in [
        "COM54".to_owned(),
        "Fallback Port".to_owned(),
        "z".repeat(64),
    ] {
        let a = SerialPortName::parse(&fallback, SerialPortName::default());
        let b = original::SerialPortName::parse(&fallback, original::SerialPortName::default());
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        let a = SerialPortName::parse(raw, a);
        let b = original::SerialPortName::parse(raw, b);
        assert_eq!(a.as_str(), b.as_str(), "{raw:?}");
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "{raw:?}");
    }
}

#[test]
fn serial_names_preserve_all_bytes_length_limits_trimming_and_fallback_records() {
    for len in [0, 1, 4, 15, 16, 17, 63, 64, 65] {
        for byte in 0..=127 {
            for index in 0..len {
                let mut raw = vec![b'x'; len];
                raw[index] = byte;
                check_port(std::str::from_utf8(&raw).unwrap());
            }
        }
        check_port(&"x".repeat(len));
        check_port(&format!("\u{2003}{}\u{a0}", "x".repeat(len)));
    }
    let base = "x".repeat(64);
    for index in 0..64 {
        for extra in ["\u{e9}", "\u{96ea}", "\u{2003}", "\u{1f600}"] {
            let end = (index + extra.len()).min(base.len());
            check_port(&format!("{}{extra}{}", &base[..index], &base[end..]));
        }
    }
    for raw in [
        "",
        "COM7",
        "\\\\.\\COM54",
        "/dev/ttyUSB0",
        "/dev/serial/by-id/usb-controller-0123456789",
        "a b",
        " a b ",
        "\0",
        "\r\n",
    ] {
        check_port(raw);
    }
}

fn defaults() -> RuntimeIoLoadOptions<DriverKind, GameplayPadLightMode, SerialPortName> {
    RuntimeIoLoadOptions {
        input_debounce_seconds: 0.017,
        lights_driver: DriverKind::Fusion,
        gameplay_pad_lights: GameplayPadLightMode::Chart,
        lights_com_port: SerialPortName::default(),
    }
}

fn load(ini: &SimpleIni) -> RuntimeIoLoadOptions<DriverKind, GameplayPadLightMode, SerialPortName> {
    load_runtime_io_options(
        ini,
        defaults(),
        deadsync_input::parse_input_debounce_seconds,
        parse_driver_or_default,
        parse_gameplay_pad_lights_or_default,
        SerialPortName::parse,
    )
}

#[test]
fn runtime_io_loading_preserves_complete_values_missing_keys_and_defaults() {
    for (driver, mode, port) in [
        (None, None, None),
        (Some(""), Some(""), Some("")),
        (Some("-\u{96ea}_"), Some("-\u{96ea}_"), Some("\u{96ea}")),
        (
            Some("LightsDriver_Win32Serial"),
            Some("NoTe-DaTa"),
            Some(" COM9 "),
        ),
        (Some("bad-driver"), Some("bad-mode"), Some("bad\u{1b}port")),
        (
            Some("FUSION"),
            Some("input"),
            Some("/dev/serial/by-id/usb-controller-0123456789"),
        ),
    ] {
        let mut text = String::from("[Options]\nInputDebounceTime=20ms\n");
        for (key, value) in [
            ("LightsDriver", driver),
            ("GameplayPadLights", mode),
            ("LightsComPort", port),
        ] {
            if let Some(value) = value {
                text.push_str(&format!("{key}={value}\n"));
            }
        }
        let mut ini = SimpleIni::new();
        ini.load_str(&text);
        let expected = load_runtime_io_options(
            &ini,
            RuntimeIoLoadOptions {
                input_debounce_seconds: 0.017,
                lights_driver: DriverKind::Fusion,
                gameplay_pad_lights: GameplayPadLightMode::Chart,
                lights_com_port: original::SerialPortName::default(),
            },
            deadsync_input::parse_input_debounce_seconds,
            |raw, default| original::driver(raw).unwrap_or(default),
            |raw, default| original::mode(raw).unwrap_or(default),
            original::SerialPortName::parse,
        );
        assert_eq!(format!("{:?}", load(&ini)), format!("{expected:?}"));
    }
}

fn inputs(names: &[&str], decorated: bool) -> Vec<String> {
    (0..512)
        .map(|i| {
            let name = names[i / 4 % names.len()];
            match i % 4 {
                0 => name.to_owned(),
                1 => name.to_ascii_uppercase(),
                2 if decorated => name.chars().map(|c| format!("{c}-\u{96ea}_")).collect(),
                2 => format!("\u{2003}{name}\t"),
                _ => ["unknown", "", "-_.\u{e9}", "unexpected123"][i / 4 % 4].to_owned(),
            }
        })
        .collect()
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 128, work);
}

fn parse_fixture<T>(label: &str, values: &[String], parse: impl Fn(&str) -> T) {
    measure(label, || {
        for raw in black_box(values) {
            black_box(parse(black_box(raw)));
        }
    });
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_light_text() {
    let drivers = inputs(DRIVERS, false);
    let modes = inputs(MODES, false);
    parse_fixture("driver/plain", &drivers, DriverKind::from_str);
    parse_fixture(
        "driver/decorated",
        &inputs(DRIVERS, true),
        DriverKind::from_str,
    );
    let long_drivers: Vec<_> = (0..64)
        .map(|i| format!("{}{}", DRIVERS[i % DRIVERS.len()], "x".repeat(256)))
        .collect();
    parse_fixture("driver/long_rejected", &long_drivers, DriverKind::from_str);
    parse_fixture("mode/plain", &modes, GameplayPadLightMode::from_str);
    parse_fixture(
        "mode/decorated",
        &inputs(MODES, true),
        GameplayPadLightMode::from_str,
    );
    let long_modes: Vec<_> = (0..64)
        .map(|i| format!("{}{}", MODES[i % MODES.len()], "x".repeat(256)))
        .collect();
    parse_fixture(
        "mode/long_rejected",
        &long_modes,
        GameplayPadLightMode::from_str,
    );
    let ports: Vec<_> = (0..512).map(|i| format!("COM{}", i % 64)).collect();
    let long_ports: Vec<_> = (0..512).map(|i| format!("/dev/{i:055}")).collect();
    let invalid_ports: Vec<_> = (0..512)
        .map(|i| format!("{}\u{1b}", "x".repeat(16 + i % 48)))
        .collect();
    let mixed_ports: Vec<_> = (0..512)
        .map(|i| match i % 5 {
            0 => ports[i].clone(),
            1 => long_ports[i].clone(),
            2 => invalid_ports[i].clone(),
            3 => "\u{96ea}".to_owned(),
            _ => " ".repeat(65),
        })
        .collect();
    let default = SerialPortName::default();
    for (label, values) in [
        ("short", &ports),
        ("long", &long_ports),
        ("invalid", &invalid_ports),
        ("mixed", &mixed_ports),
    ] {
        parse_fixture(&format!("port/{label}"), values, |raw| {
            SerialPortName::parse(raw, black_box(default))
        });
    }
    let inis: Vec<_> = (0..32).map(|i| {
        let mut ini = SimpleIni::new();
        ini.load_str(&format!("[Options]\nInputDebounceTime=20ms\nLightsDriver={}\nGameplayPadLights={}\nLightsComPort={}\n", drivers[i * 7 % 512], modes[i * 11 % 512], mixed_ports[i * 13 % 512]));
        ini
    }).collect();
    measure("load/runtime_io", || {
        for ini in black_box(&inis) {
            black_box(load(black_box(ini)));
        }
    });
    let ordering: Vec<_> = (0..512)
        .map(|i| ["openitg", "lumenar", "sm5", "unknown", " openitg "][i % 5].to_owned())
        .collect();
    parse_fixture(
        "control/pac_order",
        &ordering,
        PacDriveLightOrdering::from_preference,
    );
}
