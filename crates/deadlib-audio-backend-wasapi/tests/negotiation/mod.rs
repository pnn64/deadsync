use super::*;
use std::{cell::RefCell, ffi::c_void, rc::Rc};
use windows::core::{BOOL, GUID, HRESULT, Result, implement};

mod legacy;

fn float_format(channels: u16, rate: u32) -> Vec<u8> {
    // Explicit bytes avoid reading padding in a Rust test fixture.
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&(KernelStreaming::WAVE_FORMAT_EXTENSIBLE as u16).to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 4).to_le_bytes());
    bytes.extend_from_slice(&(channels * 4).to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&22u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&(if channels == 2 { 3u32 } else { 0x3f }).to_le_bytes());
    bytes.extend_from_slice(&[3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    bytes
}

#[test]
fn legal_period_is_the_smallest_supported_multiple() {
    for fundamental in 1..=16 {
        for min in 1..=128 {
            for max in [min, min + 3, min + 64] {
                let expected = (min..=max).find(|p| p % fundamental == 0);
                assert_eq!(minimum_period(fundamental, min, max), expected);
            }
        }
    }
    for (fundamental, min, max) in [(0, 1, 10), (1, 0, 10), (1, 10, 1), (2, u32::MAX, u32::MAX)] {
        assert_eq!(minimum_period(fundamental, min, max), None);
    }
}

#[test]
fn aligned_duration_round_trips_to_driver_frame_count() {
    for rate in [44_100u32, 48_000, 96_000, 192_000] {
        for frames in [1u32, 128, 240, 448, 512, 1024, u32::MAX] {
            let hns = aligned_period_hns(frames, rate).unwrap() as u64;
            let restored = (hns * u64::from(rate) + 5_000_000) / 10_000_000;
            assert_eq!(restored, u64::from(frames));
        }
    }
    assert_eq!(aligned_period_hns(0, 48_000), None);
    assert_eq!(aligned_period_hns(128, 0), None);
}

#[test]
fn integer_only_stereo_driver_gets_pcm_without_changing_rate() {
    let mut queries = Vec::new();
    let chosen = select_exclusive_format(float_format(2, 44_100), |bytes| {
        queries.push(bytes.to_vec());
        Ok(u32::from(waveformat(bytes).wFormatTag) == Audio::WAVE_FORMAT_PCM)
    })
    .unwrap()
    .unwrap();
    assert_eq!(queries.len(), 3);
    let wave = waveformat(&chosen);
    assert_eq!(
        (wave.nSamplesPerSec, wave.nChannels, wave.wBitsPerSample),
        (44_100, 2, 16)
    );
    assert_eq!(
        (wave.nBlockAlign, wave.nAvgBytesPerSec, wave.cbSize),
        (4, 176_400, 0)
    );
    assert_eq!(chosen.len(), size_of::<Audio::WAVEFORMATEX>());
    assert_eq!(&queries[1][18..24], &[16, 0, 3, 0, 0, 0]);
    assert_eq!(&queries[1][24..28], &[1, 0, 0, 0]);
}

#[test]
fn format_probing_preserves_supported_float_and_multichannel_layouts() {
    let original = float_format(6, 48_000);
    assert_eq!(
        select_exclusive_format(original.clone(), |_| Ok(true)).unwrap(),
        Some(original.clone())
    );
    let mut queries = Vec::new();
    assert!(
        select_exclusive_format(original, |bytes| {
            queries.push(bytes.to_vec());
            Ok(false)
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(queries.len(), 2);
    assert_eq!(&queries[1][20..24], &0x3fu32.to_le_bytes());
    assert_eq!({ waveformat(&queries[1]).nChannels }, 6);
}

#[test]
fn device_errors_stop_format_probing() {
    let mut calls = 0;
    let error = select_exclusive_format(float_format(2, 48_000), |_| {
        calls += 1;
        Err("device invalidated".into())
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error, "device invalidated");
}

#[derive(Debug, PartialEq)]
enum Call {
    Properties(i32, bool, i32),
    Periods,
    Shared(u32, u32),
    Legacy(i32, u32, i64, i64),
    BufferSize,
    Drop,
    Activate,
    FormatQuery,
}
type Calls = Rc<RefCell<Vec<Call>>>;

#[implement(Audio::IAudioClient3)]
struct MockClient {
    calls: Calls,
    failure: Option<HRESULT>,
}

impl Drop for MockClient {
    fn drop(&mut self) {
        self.calls.borrow_mut().push(Call::Drop);
    }
}

fn mock(calls: &Calls, failure: Option<HRESULT>) -> Audio::IAudioClient {
    let client: Audio::IAudioClient3 = MockClient {
        calls: Rc::clone(calls),
        failure,
    }
    .into();
    client.cast().unwrap()
}

impl Audio::IAudioClient_Impl for MockClient_Impl {
    fn Initialize(
        &self,
        mode: Audio::AUDCLNT_SHAREMODE,
        flags: u32,
        duration: i64,
        period: i64,
        _: *const Audio::WAVEFORMATEX,
        _: *const GUID,
    ) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(Call::Legacy(mode.0, flags, duration, period));
        self.failure.unwrap_or(Foundation::S_OK).ok()
    }
    fn GetBufferSize(&self) -> Result<u32> {
        self.calls.borrow_mut().push(Call::BufferSize);
        Ok(448)
    }
    fn GetDevicePeriod(&self, default: *mut i64, min: *mut i64) -> Result<()> {
        // SAFETY: the production caller supplies two writable stack locals.
        unsafe {
            *default = 100_000;
            *min = 30_000;
        }
        Ok(())
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
        closest: *mut *mut Audio::WAVEFORMATEX,
    ) -> HRESULT {
        self.calls.borrow_mut().push(Call::FormatQuery);
        if !closest.is_null() {
            // SAFETY: shared-format queries supply a writable pointer local.
            unsafe { *closest = std::ptr::null_mut() };
        }
        self.failure.unwrap_or(Foundation::S_OK)
    }
    fn GetMixFormat(&self) -> Result<*mut Audio::WAVEFORMATEX> {
        Err(Foundation::E_NOTIMPL.into())
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
    fn SetEventHandle(&self, _: Foundation::HANDLE) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn GetService(&self, _: *const GUID, _: *mut *mut c_void) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
}

impl Audio::IAudioClient2_Impl for MockClient_Impl {
    fn IsOffloadCapable(&self, _: Audio::AUDIO_STREAM_CATEGORY) -> Result<BOOL> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn SetClientProperties(&self, properties: *const Audio::AudioClientProperties) -> Result<()> {
        // SAFETY: the production caller supplies a live AudioClientProperties.
        let properties = unsafe { &*properties };
        self.calls.borrow_mut().push(Call::Properties(
            properties.eCategory.0,
            properties.bIsOffload.as_bool(),
            properties.Options.0,
        ));
        Ok(())
    }
    fn GetBufferSizeLimits(
        &self,
        _: *const Audio::WAVEFORMATEX,
        _: BOOL,
        _: *mut i64,
        _: *mut i64,
    ) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
}

impl Audio::IAudioClient3_Impl for MockClient_Impl {
    fn GetSharedModeEnginePeriod(
        &self,
        _: *const Audio::WAVEFORMATEX,
        default: *mut u32,
        fundamental: *mut u32,
        min: *mut u32,
        max: *mut u32,
    ) -> Result<()> {
        self.calls.borrow_mut().push(Call::Periods);
        // SAFETY: the production caller supplies four writable stack locals.
        unsafe {
            *default = 448;
            *fundamental = 4;
            *min = 48;
            *max = 448;
        }
        Ok(())
    }
    fn GetCurrentSharedModeEnginePeriod(
        &self,
        _: *mut *mut Audio::WAVEFORMATEX,
        _: *mut u32,
    ) -> Result<()> {
        Err(Foundation::E_NOTIMPL.into())
    }
    fn InitializeSharedAudioStream(
        &self,
        flags: u32,
        period: u32,
        _: *const Audio::WAVEFORMATEX,
        _: *const GUID,
    ) -> Result<()> {
        self.calls.borrow_mut().push(Call::Shared(flags, period));
        self.failure.unwrap_or(Foundation::S_OK).ok()
    }
}

#[test]
fn modern_shared_uses_negotiated_minimum_without_legacy_initialization() {
    let calls = Calls::default();
    let opened = initialize_shared_client(mock(&calls, None), &float_format(2, 44_100), || {
        panic!("unexpected fallback")
    })
    .unwrap();
    assert_eq!(opened.period_frames, Some(48));
    assert_eq!(
        *calls.borrow(),
        [
            Call::Properties(Audio::AudioCategory_GameMedia.0, false, 0),
            Call::Periods,
            Call::Shared(Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 48),
        ]
    );
}

#[test]
fn locked_or_unsupported_modern_stream_releases_client_before_legacy_fallback() {
    for failure in [
        Audio::AUDCLNT_E_ENGINE_PERIODICITY_LOCKED,
        Audio::AUDCLNT_E_UNSUPPORTED_FORMAT,
        Foundation::E_NOTIMPL,
    ] {
        let calls = Calls::default();
        let opened = initialize_shared_client(
            mock(&calls, Some(failure)),
            &float_format(2, 44_100),
            || {
                calls.borrow_mut().push(Call::Activate);
                Ok(mock(&calls, None))
            },
        )
        .unwrap();
        assert_eq!(opened.period_frames, None);
        assert_eq!(
            &calls.borrow()[3..],
            [
                Call::Drop,
                Call::Activate,
                Call::Legacy(
                    Audio::AUDCLNT_SHAREMODE_SHARED.0,
                    Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    0,
                    0
                ),
            ]
        );
    }
}

#[test]
fn exclusive_alignment_releases_client_and_retries_exact_driver_frame_count() {
    let calls = Calls::default();
    let _opened = initialize_exclusive(
        mock(&calls, Some(Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED)),
        &float_format(2, 44_100),
        || {
            calls.borrow_mut().push(Call::Activate);
            Ok(mock(&calls, None))
        },
    )
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        [
            Call::Legacy(
                Audio::AUDCLNT_SHAREMODE_EXCLUSIVE.0,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                30_000,
                30_000
            ),
            Call::BufferSize,
            Call::Drop,
            Call::Activate,
            Call::Legacy(
                Audio::AUDCLNT_SHAREMODE_EXCLUSIVE.0,
                Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                101_587,
                101_587
            ),
        ]
    );
}

#[test]
fn exclusive_access_failure_is_reported_without_shared_fallback() {
    let calls = Calls::default();
    assert!(
        initialize_exclusive(
            mock(&calls, Some(Audio::AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED)),
            &float_format(2, 48_000),
            || panic!("unexpected retry")
        )
        .is_err()
    );
    assert_eq!(calls.borrow().len(), 2); // Failed exclusive Initialize, then release.
}

#[test]
fn repeated_alignment_failure_does_not_loop() {
    let calls = Calls::default();
    let mut activations = 0;
    assert!(
        initialize_exclusive(
            mock(&calls, Some(Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED)),
            &float_format(2, 44_100),
            || {
                activations += 1;
                Ok(mock(&calls, Some(Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED)))
            }
        )
        .is_err()
    );
    assert_eq!(activations, 1);
}

#[test]
fn shared_rate_probe_requires_exact_support_without_initializing_stream() {
    for status in [
        Foundation::S_OK,
        Foundation::S_FALSE,
        Audio::AUDCLNT_E_UNSUPPORTED_FORMAT,
    ] {
        let calls = Calls::default();
        let client = mock(&calls, Some(status));
        assert_eq!(
            validate_shared_format(&client, &float_format(2, 44_100)).is_ok(),
            status == Foundation::S_OK
        );
        assert_eq!(*calls.borrow(), [Call::FormatQuery]);
    }
}
