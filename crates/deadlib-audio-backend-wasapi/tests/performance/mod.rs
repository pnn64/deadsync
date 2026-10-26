//! Setup-path regression checks and opt-in benchmarks of production functions.
use super::*;
use std::hint::black_box;
use windows::core::{GUID, HRESULT, Interface, Result, implement};

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "../../../../tests/support/perf.rs"]
mod perf;

#[implement(Audio::IMMDevice)]
struct MixDevice(Audio::IAudioClient);

impl Audio::IMMDevice_Impl for MixDevice_Impl {
    fn Activate(
        &self,
        iid: *const GUID,
        _: Com::CLSCTX,
        _: *const StructuredStorage::PROPVARIANT,
        out: *mut *mut c_void,
    ) -> Result<()> {
        // SAFETY: Activate's caller provides the IID and writable interface output;
        // QueryInterface transfers an owned reference on success.
        unsafe { self.0.query(iid, out).ok() }
    }
    fn OpenPropertyStore(
        &self,
        _: Com::STGM,
    ) -> Result<windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetId(&self) -> Result<PWSTR> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetState(&self) -> Result<Audio::DEVICE_STATE> {
        Err(Foundation::E_NOTIMPL.into())
    }
}

#[implement(Audio::IAudioClient)]
struct MixClient {
    format: Vec<u8>,
    failure: Option<HRESULT>,
}

impl Audio::IAudioClient_Impl for MixClient_Impl {
    fn GetMixFormat(&self) -> Result<*mut Audio::WAVEFORMATEX> {
        if let Some(error) = self.failure {
            return Err(error.into());
        }
        // SAFETY: copy initialized bytes into a fresh COM-task allocation. Ownership
        // passes to the production caller, just as with the real GetMixFormat API.
        unsafe {
            let out = Com::CoTaskMemAlloc(self.format.len()).cast::<u8>();
            if out.is_null() {
                return Err(Foundation::E_OUTOFMEMORY.into());
            }
            out.copy_from_nonoverlapping(self.format.as_ptr(), self.format.len());
            Ok(out.cast())
        }
    }
    fn Initialize(
        &self,
        _: Audio::AUDCLNT_SHAREMODE,
        _: u32,
        _: i64,
        _: i64,
        _: *const Audio::WAVEFORMATEX,
        _: *const GUID,
    ) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetBufferSize(&self) -> Result<u32> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetDevicePeriod(&self, _: *mut i64, _: *mut i64) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetStreamLatency(&self) -> Result<i64> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetCurrentPadding(&self) -> Result<u32> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn IsFormatSupported(
        &self,
        _: Audio::AUDCLNT_SHAREMODE,
        _: *const Audio::WAVEFORMATEX,
        _: *mut *mut Audio::WAVEFORMATEX,
    ) -> HRESULT {
        Foundation::E_NOTIMPL
    }
    fn Start(&self) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn Stop(&self) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn Reset(&self) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn SetEventHandle(&self, _: HANDLE) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetService(&self, _: *const GUID, _: *mut *mut c_void) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
}

fn mix_device(
    rate: u32,
    channels: u16,
    extension: usize,
    failure: Option<HRESULT>,
) -> Audio::IMMDevice {
    // All bytes, including any format-specific extension, are initialized.
    let mut format = vec![0; size_of::<Audio::WAVEFORMATEX>() + extension];
    let wave = waveformat_mut(&mut format);
    wave.nSamplesPerSec = rate;
    wave.nChannels = channels;
    wave.cbSize = extension as u16;
    let client: Audio::IAudioClient = MixClient { format, failure }.into();
    MixDevice(client).into()
}

#[test]
fn mix_probe_preserves_rates_channels_extensions_and_errors() {
    for rate in [0, 1, 44_100, 48_000, 192_000, u32::MAX] {
        for channels in [0, 1, 2, 6, u16::MAX] {
            for extension in [0, 22, 256, u16::MAX as usize] {
                let device = mix_device(rate, channels, extension, None);
                assert_eq!(
                    device_mix_format(&device).unwrap(),
                    (rate.max(1), usize::from(channels.max(1)))
                );
            }
        }
    }
    let error = device_mix_format(&mix_device(48_000, 2, 22, Some(Foundation::E_ACCESSDENIED)))
        .unwrap_err();
    assert_eq!(
        error,
        format!(
            "failed to query WASAPI mix format: {}",
            windows::core::Error::from(Foundation::E_ACCESSDENIED)
        )
    );
}

#[test]
fn mix_probe_needs_no_rust_heap_allocation() {
    let device = mix_device(48_000, 2, 22, None);
    perf::assert_no_churn(|| {
        assert_eq!(device_mix_format(&device).unwrap(), (48_000, 2));
    });
}

#[test]
fn device_id_encoding_preserves_every_unicode_scalar_and_embedded_nulls() {
    for text in ["", "ASCII", "åäö 日本語", "🎵😀", "a\0b"] {
        let expected: Vec<u16> = text.encode_utf16().chain([0]).collect();
        assert_eq!(wide_null(text), expected);
    }
    let all_scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    let expected: Vec<u16> = all_scalars.encode_utf16().chain([0]).collect();
    assert_eq!(wide_null(&all_scalars), expected);
}

#[test]
fn device_id_encoding_does_not_reallocate() {
    for text in ["", "ASCII", "åäö 日本語 🎵😀", "a\0b"] {
        let (encoded, counts) = perf::measure(|| wide_null(text));
        assert_eq!(counts.allocs, 1);
        assert_eq!(counts.reallocs, 0);
        assert_eq!(encoded.last(), Some(&0));
    }
}

#[test]
fn device_id_encoding_preserves_ascii_boundaries_and_unicode_tails() {
    for len in 0..=513 {
        let ascii: String = (0..len).map(|i| char::from((i % 128) as u8)).collect();
        let expected: Vec<u16> = ascii.encode_utf16().chain([0]).collect();
        assert_eq!(wide_null(&ascii), expected);
        for index in [0, len / 2, len] {
            let mut text = ascii.clone();
            text.insert(index, '🎵');
            let expected: Vec<u16> = text.encode_utf16().chain([0]).collect();
            assert_eq!(wide_null(&text), expected);
        }
    }
}

#[test]
fn com_task_free_accepts_null_and_owned_memory_without_rust_allocations() {
    // SAFETY: null is accepted by CoTaskMemFree, and each non-null pointer comes
    // from CoTaskMemAlloc and is freed exactly once.
    unsafe {
        free_com_task_mem(std::ptr::null());
        for size in [1, 18, 40, 4096, 65535] {
            let ptr = Com::CoTaskMemAlloc(size);
            assert!(!ptr.is_null());
            perf::assert_no_churn(|| free_com_task_mem(ptr));
        }
    }
}

pub(crate) fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    work();
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
fn frame_time_matches_integer_division_across_rates_and_frame_counts() {
    for rate in [
        0,
        1,
        2,
        3,
        44_100,
        48_000,
        96_000,
        192_000,
        1_000_000_000,
        u32::MAX,
    ] {
        for frames in [0, 1, 2, 127, 128, 441, 480, 1024, u32::MAX - 1, u32::MAX] {
            let expected = (u64::from(frames) * 1_000_000_000)
                .checked_div(u64::from(rate))
                .unwrap_or(0);
            assert_eq!(FramesToNanos::new(rate).convert(frames), expected);
        }
    }
    for rate in 1..=65_536u32 {
        for frames in [rate - 1, rate, rate + 1, u32::MAX] {
            assert_eq!(
                FramesToNanos::new(rate).convert(frames),
                u64::from(frames) * 1_000_000_000 / u64::from(rate)
            );
        }
    }
    for power in 1..32 {
        let rate = 1u32 << power;
        for rate in [rate - 1, rate, rate + 1] {
            for frames in [1, rate, u32::MAX] {
                assert_eq!(
                    FramesToNanos::new(rate).convert(frames),
                    u64::from(frames) * 1_000_000_000 / u64::from(rate)
                );
            }
        }
    }
    let mut random = 0x7d51_24d3u32;
    for _ in 0..100_000 {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let rate = random;
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let frames = random;
        let expected = (u64::from(frames) * 1_000_000_000)
            .checked_div(u64::from(rate))
            .unwrap_or(0);
        assert_eq!(FramesToNanos::new(rate).convert(frames), expected);
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; no audio endpoint required"]
fn benchmark_frame_setup() {
    for rate in [44_100, 48_000, 96_000, 192_000, 65_536, u32::MAX] {
        measure(&format!("wasapi/frame_setup_{rate}"), 1_000_000, || {
            black_box(FramesToNanos::new(black_box(rate)));
        });
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; no audio endpoint required"]
fn benchmark_frame_conversion() {
    for (rate, frames) in [
        (48_000, 128),
        (44_100, 441),
        (192_000, 512),
        (96_000, u32::MAX),
        (1, u32::MAX),
        (0, u32::MAX),
    ] {
        let converter = black_box(FramesToNanos::new(rate));
        measure(
            &format!("wasapi/frame_convert_{rate}_{frames}"),
            1_000_000,
            || {
                black_box(converter.convert(black_box(frames)));
            },
        );
    }
}

#[implement(Audio::IAudioClock)]
struct FixedClock(u64);

impl Audio::IAudioClock_Impl for FixedClock_Impl {
    fn GetFrequency(&self) -> Result<u64> {
        Ok(10_000_000)
    }
    fn GetPosition(&self, position: *mut u64, qpc: *mut u64) -> Result<()> {
        // SAFETY: the production caller supplies writable out parameters. The
        // optional QPC pointer is checked, as required by the COM signature.
        unsafe {
            *position = 0;
            if !qpc.is_null() {
                *qpc = self.0;
            }
        }
        Ok(())
    }
    fn GetCharacteristics(&self) -> Result<u32> {
        Ok(0)
    }
}

#[test]
fn playback_anchor_matches_wide_arithmetic_at_saturation_boundaries() {
    for qpc in [
        0,
        1,
        u64::MAX / 100 - 1,
        u64::MAX / 100,
        u64::MAX / 100 + 1,
        u64::MAX,
    ] {
        let clock: Audio::IAudioClock = FixedClock(qpc).into();
        for delay in [0, 1, 99, 100, u64::MAX - 1, u64::MAX] {
            let expected =
                (u128::from(qpc) * 100 + u128::from(delay)).min(u128::from(u64::MAX - 1)) as u64;
            assert_eq!(
                playback_anchor_nanos_after_delay(&clock, delay).unwrap(),
                expected
            );
        }
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; no audio endpoint required"]
fn benchmark_playback_anchor() {
    for (qpc, delay, label) in [
        (123_456_789_012, 2_666_666, "typical"),
        (0, 0, "zero"),
        (u64::MAX / 100, 100, "add_saturates"),
        (u64::MAX, 1, "multiply_saturates"),
    ] {
        let clock: Audio::IAudioClock = FixedClock(qpc).into();
        measure(&format!("wasapi/anchor_{label}"), 1_000_000, || {
            black_box(
                playback_anchor_nanos_after_delay(black_box(&clock), black_box(delay)).unwrap(),
            );
        });
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; no audio endpoint required"]
fn benchmark_setup() {
    let _com = ComGuard::new().unwrap();
    measure("wasapi/com_alloc_free_40", 100_000, || {
        // SAFETY: a fresh, non-null COM-task allocation is freed once per iteration.
        unsafe {
            let ptr = Com::CoTaskMemAlloc(black_box(40));
            assert!(!ptr.is_null());
            free_com_task_mem(black_box(ptr));
        }
    });
    for extension in [0, 22, 256] {
        let device = mix_device(48_000, 2, extension, None);
        measure(&format!("wasapi/mix_probe_{extension}"), 100_000, || {
            black_box(device_mix_format(black_box(&device)).unwrap());
        });
    }
    let unicode = "åäö 日本語 🎵😀".repeat(6);
    for text in [
        "{0.0.0.00000000}.{12345678-1234-1234-1234-123456789abc}",
        unicode.as_str(),
    ] {
        let label = if text.is_ascii() { "ascii" } else { "unicode" };
        measure(&format!("wasapi/device_id_{label}"), 100_000, || {
            black_box(wide_null(black_box(text)));
        });
    }
}

fn format_bytes(tag: u16, bits: u16, subtype: u16) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    let wave = waveformat_mut(&mut bytes);
    wave.wFormatTag = tag;
    wave.nChannels = 2;
    wave.nSamplesPerSec = 48_000;
    wave.nBlockAlign = bits / 4;
    wave.nAvgBytesPerSec = u32::from(wave.nBlockAlign) * 48_000;
    wave.wBitsPerSample = bits;
    wave.cbSize = 22;
    bytes[24..40].copy_from_slice(&[
        subtype as u8,
        (subtype >> 8) as u8,
        0,
        0,
        0,
        0,
        16,
        0,
        128,
        0,
        0,
        170,
        0,
        56,
        155,
        113,
    ]);
    bytes
}

fn format_cases() -> [(u16, u16, u16, &'static str); 6] {
    [
        (1, 16, 1, "pcm16"),
        (3, 32, 3, "float32"),
        (0xfffe, 16, 1, "ext_pcm16"),
        (0xfffe, 32, 3, "ext_float32"),
        (0xfffe, 24, 1, "ext_unsupported_bits"),
        (0xfffe, 32, 5, "ext_unsupported_guid"),
    ]
}

#[test]
fn format_recognition_accepts_only_pcm16_and_float32() {
    for bits in 0..=u16::MAX {
        for (tag, subtype, expected) in [
            (1, 1, (bits == 16).then_some(16)),
            (3, 3, (bits == 32).then_some(32)),
            (0xfffe, 1, (bits == 16).then_some(16)),
            (0xfffe, 3, (bits == 32).then_some(32)),
            (0xffff, 1, None),
        ] {
            let bytes = format_bytes(tag, bits, subtype);
            let actual = sample_format_from_waveformat(&bytes).map(|format| match format {
                WasapiSampleFormat::I16 => 16,
                WasapiSampleFormat::F32 => 32,
            });
            assert_eq!(
                actual, expected,
                "tag {tag}, bits {bits}, subtype {subtype}"
            );
        }
    }
}

#[test]
fn format_recognition_checks_the_complete_guid_and_handles_short_buffers() {
    for (bits, subtype) in [(16, 1), (32, 3)] {
        let valid = format_bytes(0xfffe, bits, subtype);
        let mut padded = vec![0xff];
        padded.extend_from_slice(&valid);
        padded.extend_from_slice(&[0xaa; 17]);
        assert!(sample_format_from_waveformat(&padded[1..]).is_some());
        for index in 24..40 {
            for value in 0..=u8::MAX {
                let mut bytes = valid.clone();
                bytes[index] = value;
                assert_eq!(
                    sample_format_from_waveformat(&bytes).is_some(),
                    value == valid[index],
                    "GUID byte {index}, value {value}"
                );
            }
        }
        for len in 0..=40 {
            assert_eq!(
                sample_format_from_waveformat(&valid[..len]).is_some(),
                len == 40
            );
        }
        let plain = format_bytes(subtype, bits, subtype);
        for len in 0..=40 {
            assert_eq!(
                sample_format_from_waveformat(&plain[..len]).is_some(),
                len >= size_of::<Audio::WAVEFORMATEX>()
            );
        }
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; no audio endpoint required"]
fn benchmark_formats() {
    for (tag, bits, subtype, label) in format_cases() {
        let bytes = format_bytes(tag, bits, subtype);
        measure(&format!("wasapi/format_{label}"), 1_000_000, || {
            black_box(sample_format_from_waveformat(black_box(&bytes)));
        });
    }
}

#[test]
#[ignore = "manual CPU-cycle/allocation benchmark; opens the default audio endpoint"]
fn benchmark_prepare() {
    let rate = prepare(None, String::new(), None, WasapiAccessMode::Shared)
        .unwrap()
        .sample_rate_hz;
    for (mode, requested, label) in [
        (WasapiAccessMode::Shared, None, "shared_mix"),
        (WasapiAccessMode::Shared, Some(rate), "shared_requested"),
        (WasapiAccessMode::Shared, Some(1), "shared_rejected"),
        (WasapiAccessMode::Exclusive, None, "exclusive_mix"),
    ] {
        measure(&format!("wasapi/prepare_{label}"), 50, || {
            black_box(prepare(None, String::new(), black_box(requested), black_box(mode)).unwrap());
        });
    }
}

#[test]
#[ignore = "requires an available Windows audio endpoint"]
fn preparation_keeps_mix_bytes_on_rejected_rate_and_changes_only_rate_fields() {
    let _com = ComGuard::new().unwrap();
    let device = open_output_device(None).unwrap();
    let client = build_audio_client(&device).unwrap();
    let mix = get_mix_format_bytes(&client).unwrap();
    for requested in [None, Some(0), Some(1)] {
        if requested == Some(1) {
            let mut invalid = mix.clone();
            set_waveformat_sample_rate(&mut invalid, 1);
            assert!(init::validate_shared_format(&client, &invalid).is_err());
        }
        let (prep, counts) = perf::measure(|| {
            prepare(None, String::new(), requested, WasapiAccessMode::Shared).unwrap()
        });
        assert_eq!(prep.format, mix);
        assert_eq!(counts.allocs, if requested == Some(1) { 2 } else { 1 });
        assert_eq!(counts.reallocs, 0);
    }
    let rate = waveformat(&mix).nSamplesPerSec;
    let mut expected = mix.clone();
    set_waveformat_sample_rate(&mut expected, rate);
    let prep = prepare(None, String::new(), Some(rate), WasapiAccessMode::Shared).unwrap();
    assert_eq!(prep.format, expected);
}
