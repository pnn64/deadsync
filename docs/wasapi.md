# Windows audio

`Auto` and `Shared` use event-driven WASAPI shared output. On Windows 10/11,
deadsync queries `IAudioClient3`, sets the stream category to `GameMedia`, queries
the legal engine periods, and requests the smallest supported multiple of the
fundamental period with `InitializeSharedAudioStream`. Hardware offload, raw
processing, and engine format locking are disabled.

Windows 7 does not expose `IAudioClient3`, so it keeps the original
`IAudioClient::Initialize` shared path with zero duration and periodicity. The
new interface is discovered through COM; it adds no Windows 10 DLL imports.
If modern initialization fails (for example, an unsupported driver or a locked
engine), deadsync logs the failure, releases the client, and opens a fresh legacy
shared client. A requested shared sample rate is checked without initializing a
temporary stream; an unsupported rate falls back to the endpoint mix format.

`Exclusive` uses `IAudioClient::Initialize` on every Windows version.
`IAudioClient3::InitializeSharedAudioStream` is a shared-only API. Exclusive
negotiation first tries a renderer-supported mix format, then PCM16 at the same
sample rate and channel layout. Mono/stereo drivers are also probed with a plain
PCM descriptor. The renderer supports float32 and PCM16; a device that requires
another representation returns an explicit error. Exclusive mode never silently
switches to shared mode or another requested sample rate.

Exclusive event streams request equal buffer duration and periodicity. If a
driver reports `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`, deadsync obtains the aligned
frame count, releases the failed client, and retries once on a new client. Each
buffer-ready event submits a complete buffer. Health-check timeouts probe the
clock without submitting a packet. Shutdown stops the stream and releases COM
interfaces before closing their event handle.

## Playback clock

Each packet is timestamped from the correlated device position and QPC reading
returned by `IAudioClock::GetPosition`. Its first native stream frame is tracked
across successful buffer submissions, including startup silence. The timestamp
in seconds is `qpc + submitted_frames / sample_rate - position / clock_frequency`.
`GetFrequency` supplies the position's units; shared and exclusive clocks need
not report positions in sample frames.

Before the device position advances, the backend submits silence without
consuming music or publishing a guessed playback timestamp. Opening or recovering
a client creates a new native frame counter while preserving the mixer's frame
timeline. Buffer padding controls how much can be written; it is not used as the
playback clock. Reported stream latency remains diagnostic information.

This removes a launch-dependent bias in the former padding-based clock. Existing
global offsets may need one recalibration after upgrading. Measurements describe
software timing relative to reported device playback. Acoustic timing also
depends on the driver and downstream hardware.

## Microsoft guidance

Microsoft generally favors AudioGraph for new applications, and identifies
WASAPI as appropriate when applications need more control or lower latency.
Deadsync's custom mixer, device-clock timing, and optional exclusive access fit
that use case. Microsoft documents `IAudioClient3` as the shared-mode small-buffer
API. [Low Latency Audio](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio)

Microsoft also advises evaluating low-period shared streams before selecting
exclusive output. Its exclusive-mode contract requires whole packets for event
streams and a fresh client after a buffer-alignment failure.
[Exclusive-Mode Streams](https://learn.microsoft.com/en-us/windows/win32/coreaudio/exclusive-mode-streams),
[GetCurrentPadding](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getcurrentpadding),
[Initialize](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-initialize)

The low-latency guide additionally recommends Windows real-time work queues for
WASAPI scheduling. Deadsync currently retains its dedicated event-driven render
thread registered with MMCSS as `Pro Audio`; it does not yet use those work queues.
The exclusive-mode guide also recommends releasing exclusive ownership when an
application loses focus; automatic focus-based release is not implemented here.
These are separate scheduling/lifecycle considerations, not features supplied by
`IAudioClient3` initialization itself. The cited guides describe those recommendations.

## Diagnostics and verification

Startup/recovery logs identify the actual initialization API, sample rate, buffer
size, and period. Timing telemetry reports the negotiated shared period or the
actual exclusive buffer period, including driver alignment. A period is a buffer
servicing interval, not a measurement of total speaker latency.

Clock regression tests cover launch phase, delayed callbacks, device frequency
units, startup silence, stream reopening, and long-running counters. The opt-in
native clock tests compare the published mixer timeline against an independent
device reading on the following callback, including the normal clock
interpolation/extrapolation. They print each session's median, mean, and 5th/95th
percentile error in milliseconds for modern shared, legacy shared, and exclusive
output. Run endpoint tests serially so exclusive ownership does not interrupt
another probe.

All new negotiation, format allocation, and diagnostic formatting occurs while
opening/recovering the endpoint. Steady-state rendering uses event waits, the
existing reusable mixer storage, and direct writes to WASAPI buffers. There is no
new per-callback allocation or busy polling.

Normal regression tests include COM mocks for old-interface discovery, modern
minimum-period negotiation, fresh-client fallback, exclusive alignment retries,
access errors, and sample format selection:

```powershell
cargo test -p deadlib-audio-backend-wasapi
cargo clippy -p deadlib-audio-backend-wasapi --all-targets --no-deps -- -D warnings
```

Opt-in hardware tests render silence. Exclusive testing briefly takes ownership
of the default endpoint; run serially with exclusive access enabled:

```powershell
cargo test -p deadlib-audio-backend-wasapi --lib -- --ignored --nocapture --test-threads=1
cargo test -p deadlib-audio-backend-wasapi --test hardware -- --ignored --nocapture --test-threads=1
```

On Windows 11 build 26100 with the available 48 kHz endpoint, native tests observed
a 128-frame (2.667 ms) modern shared period and a 128-frame exclusive buffer. The
forced legacy shared test reported a 480-frame (10 ms) default period. Sustained
shared/exclusive/shared playback, advancing clocks, and prompt shutdown passed.
These results cover this endpoint, not every Windows driver or end-to-end latency.

The device-position clock was additionally checked across 16 fresh test processes
with two modern shared stream openings each (48 kHz, 280-frame buffer, 128-frame
period). Across those 32 sessions, median errors against subsequent device-clock
readings ranged from -0.004006 to +0.002229 ms, a 0.006235 ms spread. Per-session
5th/95th percentiles stayed within approximately -0.044 to +0.046 ms. Legacy shared
and exclusive clock checks also passed. These are device-clock comparisons, not
acoustic loopback measurements.

The Windows 7 x64 target can be checked with the same standard-library build used
by the release workflow:

```powershell
cargo +nightly check -p deadlib-audio-backend-wasapi -Z build-std=std,panic_abort --target x86_64-win7-windows-msvc --locked
```

This target check and the mocked Windows 7 interface path passed. Running the
binary on an actual Windows 7 installation remains a separate compatibility test.
