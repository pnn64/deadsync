//! An endpoint exposing only IAudioClient models Windows 7 capability discovery.
use super::*;

#[implement(Audio::IAudioClient)]
struct LegacyClient {
    inner: Audio::IAudioClient,
}

impl Audio::IAudioClient_Impl for LegacyClient_Impl {
    fn Initialize(
        &self,
        mode: Audio::AUDCLNT_SHAREMODE,
        flags: u32,
        duration: i64,
        period: i64,
        format: *const Audio::WAVEFORMATEX,
        session: *const GUID,
    ) -> Result<()> {
        // SAFETY: this forwards the production caller's live format/session
        // pointers to the mock without retaining them.
        unsafe {
            self.inner
                .Initialize(mode, flags, duration, period, format, Some(session))
        }
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

#[test]
fn missing_modern_interface_uses_original_win7_initialization() {
    let calls = Calls::default();
    let client: Audio::IAudioClient = LegacyClient {
        inner: mock(&calls, None),
    }
    .into();
    let opened = initialize_shared_client(client, &float_format(2, 48_000), || {
        panic!("Windows 7 must reuse the uninitialized legacy client")
    })
    .unwrap();
    assert_eq!(opened.period_frames, None);
    assert_eq!(
        *calls.borrow(),
        [Call::Legacy(
            Audio::AUDCLNT_SHAREMODE_SHARED.0,
            Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0
        )]
    );
}
