use rubato::{
    FastFixedOut, PolynomialDegree, Resampler, SincFixedOut, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};
use smallvec::SmallVec;
use std::hint::black_box;
use std::time::{Duration, Instant};

const OUT_FRAMES_PER_CALL: usize = 256;
const PLANAR_INPUT_CAP_FRAMES: usize = 4096;
const PLANAR_COMPACT_THRESHOLD_FRAMES: usize = 2048;
const PACKET_FRAMES: usize = 1024;

struct BenchResult {
    name: &'static str,
    iters: usize,
    elapsed: Duration,
    checksum: i64,
}

struct PlanarAccum {
    channels: Vec<Vec<f32>>,
    start_frame: usize,
}

impl PlanarAccum {
    fn new(channels: usize, capacity_frames: usize) -> Self {
        let mut planar = Vec::with_capacity(channels);
        for _ in 0..channels {
            planar.push(Vec::with_capacity(capacity_frames));
        }
        Self {
            channels: planar,
            start_frame: 0,
        }
    }

    #[inline(always)]
    fn available_frames(&self) -> usize {
        self.channels
            .first()
            .map_or(0, |channel| channel.len().saturating_sub(self.start_frame))
    }

    fn push_i16_interleaved(&mut self, interleaved: &[i16], channels: usize) {
        if interleaved.is_empty() || channels == 0 {
            return;
        }
        let frames = interleaved.len() / channels;
        for channel in &mut self.channels {
            channel.reserve(frames);
        }
        for frame in interleaved.chunks_exact(channels) {
            for (channel, sample) in self.channels.iter_mut().zip(frame.iter()) {
                channel.push(f32::from(*sample) / 32768.0);
            }
        }
    }

    fn consume_frames(&mut self, frames: usize) {
        let total_frames = self.channels.first().map_or(0, Vec::len);
        self.start_frame = (self.start_frame + frames).min(total_frames);
        self.compact_if_needed();
    }

    fn clear(&mut self) {
        self.start_frame = 0;
        for channel in &mut self.channels {
            channel.clear();
        }
    }

    fn compact_if_needed(&mut self) {
        if self.start_frame == 0 {
            return;
        }
        let total_frames = self.channels.first().map_or(0, Vec::len);
        let remaining_frames = total_frames.saturating_sub(self.start_frame);
        if remaining_frames == 0 {
            self.clear();
            return;
        }
        if self.start_frame < PLANAR_COMPACT_THRESHOLD_FRAMES && self.start_frame * 2 < total_frames
        {
            return;
        }
        for channel in &mut self.channels {
            channel.copy_within(self.start_frame.., 0);
            channel.truncate(remaining_frames);
        }
        self.start_frame = 0;
    }
}

fn main() {
    println!("audio resample microbench");
    println!("input is synthetic i16 audio; timings are best used as ratios on this machine\n");

    bench_channel_only_resample();
    bench_staging();
    bench_resampler_construction();
    bench_output_write();
    bench_fast_preview();
}

#[inline(always)]
fn resampler_params() -> SincInterpolationParameters {
    SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    }
}

fn bench_channel_only_resample() {
    let input = make_interleaved_i16(48_000, 1);

    let sinc = bench("channel only: current sinc mono->stereo", 4, || {
        resample_planar_sinc_checksum(black_box(&input), 1, 2, 1.0)
    });
    let direct = bench("channel only: direct i16 mono->stereo", 4_000, || {
        direct_mono_to_stereo_checksum(black_box(&input))
    });

    print_result(&sinc);
    print_result(&direct);
    print_ratio("direct vs current", &sinc, &direct);
    println!();
}

fn bench_staging() {
    let input = make_interleaved_i16(44_100, 2);
    let ratio = 48_000.0 / 44_100.0;

    let planar = bench("staging: current PlanarAccum + sinc", 4, || {
        resample_planar_sinc_checksum(black_box(&input), 2, 2, ratio)
    });
    let direct = bench("staging: direct fill + sinc", 4, || {
        let resampler =
            SincFixedOut::<f32>::new(ratio, 1.0, resampler_params(), OUT_FRAMES_PER_CALL, 2)
                .unwrap();
        resample_direct_fill_checksum(resampler, black_box(&input), 2, 2)
    });

    print_result(&planar);
    print_result(&direct);
    print_ratio("direct fill vs PlanarAccum", &planar, &direct);
    println!();
}

fn bench_resampler_construction() {
    let construct = bench("construct: SincFixedOut::new", 80, || {
        let resampler =
            SincFixedOut::<f32>::new(48_000.0 / 44_100.0, 1.0, resampler_params(), 256, 2).unwrap();
        black_box(resampler.input_frames_next() as i64)
    });

    let mut reusable =
        SincFixedOut::<f32>::new(48_000.0 / 44_100.0, 1.0, resampler_params(), 256, 2).unwrap();
    let reset = bench("construct: reset existing", 80_000, || {
        reusable.reset();
        black_box(reusable.input_frames_next() as i64)
    });

    print_result(&construct);
    print_result(&reset);
    print_ratio("reset existing vs construct", &construct, &reset);
    println!();
}

fn bench_output_write() {
    let frames = 4096;
    let mut out = vec![vec![0.0f32; frames], vec![0.0f32; frames]];
    for frame in 0..frames {
        out[0][frame] = ((frame as f32 * 0.017).sin() * 0.8).clamp(-1.0, 1.0);
        out[1][frame] = ((frame as f32 * 0.011).cos() * 0.8).clamp(-1.0, 1.0);
    }

    let generic = bench("write output: generic modulo stereo", 20_000, || {
        let mut tmp = Vec::with_capacity(frames * 2);
        write_output_generic(black_box(&out), frames, 2, &mut tmp);
        checksum_i16(&tmp)
    });
    let stereo = bench("write output: specialized stereo", 20_000, || {
        let mut tmp = Vec::with_capacity(frames * 2);
        write_output_stereo(black_box(&out), frames, &mut tmp);
        checksum_i16(&tmp)
    });

    print_result(&generic);
    print_result(&stereo);
    print_ratio("specialized vs generic", &generic, &stereo);
    println!();
}

fn bench_fast_preview() {
    let input = make_interleaved_i16(44_100, 2);
    let ratio = 48_000.0 / 44_100.0;

    let sinc = bench("preview: SincFixedOut current params", 4, || {
        let resampler =
            SincFixedOut::<f32>::new(ratio, 1.0, resampler_params(), OUT_FRAMES_PER_CALL, 2)
                .unwrap();
        resample_direct_fill_checksum(resampler, black_box(&input), 2, 2)
    });
    let fast_cubic = bench("preview: FastFixedOut cubic", 20, || {
        let resampler =
            FastFixedOut::<f32>::new(ratio, 1.0, PolynomialDegree::Cubic, OUT_FRAMES_PER_CALL, 2)
                .unwrap();
        resample_direct_fill_checksum(resampler, black_box(&input), 2, 2)
    });
    let fast_linear = bench("preview: FastFixedOut linear", 20, || {
        let resampler =
            FastFixedOut::<f32>::new(ratio, 1.0, PolynomialDegree::Linear, OUT_FRAMES_PER_CALL, 2)
                .unwrap();
        resample_direct_fill_checksum(resampler, black_box(&input), 2, 2)
    });

    print_result(&sinc);
    print_result(&fast_cubic);
    print_result(&fast_linear);
    print_ratio("fast cubic vs sinc", &sinc, &fast_cubic);
    print_ratio("fast linear vs sinc", &sinc, &fast_linear);
    println!();
}

fn bench<F>(name: &'static str, iters: usize, mut f: F) -> BenchResult
where
    F: FnMut() -> i64,
{
    let mut checksum = 0i64;
    for _ in 0..2 {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    let start = Instant::now();
    for _ in 0..iters {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    BenchResult {
        name,
        iters,
        elapsed: start.elapsed(),
        checksum,
    }
}

fn print_result(result: &BenchResult) {
    let total_ms = result.elapsed.as_secs_f64() * 1000.0;
    let per_iter_us = result.elapsed.as_secs_f64() * 1_000_000.0 / result.iters as f64;
    println!(
        "{:<42} {:>9.3} ms total {:>10.3} us/iter checksum {}",
        result.name, total_ms, per_iter_us, result.checksum
    );
}

fn print_ratio(label: &str, base: &BenchResult, candidate: &BenchResult) {
    let base_us = base.elapsed.as_secs_f64() * 1_000_000.0 / base.iters as f64;
    let candidate_us = candidate.elapsed.as_secs_f64() * 1_000_000.0 / candidate.iters as f64;
    println!("{label}: {:.2}x", base_us / candidate_us);
}

fn make_interleaved_i16(frames: usize, channels: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        for channel in 0..channels {
            let x = (frame as u32)
                .wrapping_mul(1_664_525)
                .wrapping_add((channel as u32).wrapping_mul(1_013_904_223));
            out.push(((x >> 16) as i16).wrapping_sub(16_384));
        }
    }
    out
}

fn resample_planar_sinc_checksum(input: &[i16], in_ch: usize, out_ch: usize, ratio: f64) -> i64 {
    let mut resampler =
        SincFixedOut::<f32>::new(ratio, 1.0, resampler_params(), OUT_FRAMES_PER_CALL, in_ch)
            .unwrap();
    let mut in_planar = PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES);
    let mut resample_in = resampler.input_buffer_allocate(true);
    let mut resample_out = resampler.output_buffer_allocate(true);
    let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
    let mut checksum = 0i64;

    for packet in input.chunks(PACKET_FRAMES * in_ch) {
        in_planar.push_i16_interleaved(packet, in_ch);
        loop {
            let need = resampler.input_frames_next();
            if in_planar.available_frames() < need {
                break;
            }
            let produced_frames = {
                let mut input_slices = SmallVec::<[&[f32]; 2]>::with_capacity(in_ch);
                let start = in_planar.start_frame;
                let end = start + need;
                for channel in &in_planar.channels {
                    input_slices.push(&channel[start..end]);
                }
                resampler
                    .process_into_buffer(input_slices.as_slice(), &mut resample_out, None)
                    .unwrap()
                    .1
            };
            in_planar.consume_frames(need);
            write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
            checksum = checksum.wrapping_add(checksum_i16(&out_tmp));
        }
    }

    if in_planar.available_frames() > 0 {
        let remain = in_planar.available_frames();
        let need = resampler.input_frames_next();
        let copy_frames = remain.min(need);
        let start = in_planar.start_frame;
        let end = start + copy_frames;
        for (dst, channel) in resample_in.iter_mut().zip(&in_planar.channels) {
            dst[..need].fill(0.0);
            dst[..copy_frames].copy_from_slice(&channel[start..end]);
        }
        let produced_frames = resampler
            .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)
            .unwrap()
            .1;
        write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
        checksum = checksum.wrapping_add(checksum_i16(&out_tmp));
    }

    let need = resampler.input_frames_next();
    for dst in &mut resample_in {
        dst[..need].fill(0.0);
    }
    let produced_frames = resampler
        .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)
        .unwrap()
        .1;
    write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
    checksum.wrapping_add(checksum_i16(&out_tmp))
}

fn resample_direct_fill_checksum<R>(
    mut resampler: R,
    input: &[i16],
    in_ch: usize,
    out_ch: usize,
) -> i64
where
    R: Resampler<f32>,
{
    let mut resample_in = resampler.input_buffer_allocate(true);
    let mut resample_out = resampler.output_buffer_allocate(true);
    let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
    let mut checksum = 0i64;
    let mut filled = 0usize;
    let mut src_frame = 0usize;
    let total_frames = input.len() / in_ch;

    while src_frame < total_frames {
        let need = resampler.input_frames_next();
        let copy_frames = (need - filled).min(total_frames - src_frame);
        copy_interleaved_to_planar(
            &mut resample_in,
            filled,
            input,
            src_frame,
            copy_frames,
            in_ch,
        );
        filled += copy_frames;
        src_frame += copy_frames;
        if filled < need {
            break;
        }
        let produced_frames = resampler
            .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)
            .unwrap()
            .1;
        write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
        checksum = checksum.wrapping_add(checksum_i16(&out_tmp));
        filled = 0;
    }

    if filled > 0 {
        let need = resampler.input_frames_next();
        for channel in &mut resample_in {
            channel[filled..need].fill(0.0);
        }
        let produced_frames = resampler
            .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)
            .unwrap()
            .1;
        write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
        checksum = checksum.wrapping_add(checksum_i16(&out_tmp));
    }

    let need = resampler.input_frames_next();
    for channel in &mut resample_in {
        channel[..need].fill(0.0);
    }
    let produced_frames = resampler
        .process_into_buffer(resample_in.as_slice(), &mut resample_out, None)
        .unwrap()
        .1;
    write_output_generic(&resample_out, produced_frames, out_ch, &mut out_tmp);
    checksum.wrapping_add(checksum_i16(&out_tmp))
}

fn copy_interleaved_to_planar(
    dst: &mut [Vec<f32>],
    dst_frame: usize,
    src: &[i16],
    src_frame: usize,
    frames: usize,
    channels: usize,
) {
    for frame in 0..frames {
        let src_base = (src_frame + frame) * channels;
        for channel in 0..channels {
            dst[channel][dst_frame + frame] = f32::from(src[src_base + channel]) / 32768.0;
        }
    }
}

fn direct_mono_to_stereo_checksum(input: &[i16]) -> i64 {
    let mut tmp = Vec::with_capacity(PACKET_FRAMES * 2);
    let mut checksum = 0i64;
    for packet in input.chunks(PACKET_FRAMES) {
        tmp.clear();
        for &sample in packet {
            tmp.push(sample);
            tmp.push(sample);
        }
        checksum = checksum.wrapping_add(checksum_i16(&tmp));
    }
    checksum
}

fn write_output_generic(
    out: &[Vec<f32>],
    produced_frames: usize,
    out_ch: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    if out.is_empty() || produced_frames == 0 || out_ch == 0 {
        out_tmp.clear();
        return 0;
    }
    let produced_frames = produced_frames.min(out[0].len());
    let produced_samples = produced_frames.saturating_mul(out_ch);
    if out_tmp.len() < produced_samples {
        out_tmp.resize(produced_samples, 0);
    } else {
        out_tmp.truncate(produced_samples);
    }
    for frame in 0..produced_frames {
        let base = frame * out_ch;
        for channel in 0..out_ch {
            let sample = out[channel % out.len()][frame];
            out_tmp[base + channel] = f32_to_i16(sample);
        }
    }
    produced_frames
}

fn write_output_stereo(out: &[Vec<f32>], produced_frames: usize, out_tmp: &mut Vec<i16>) -> usize {
    if out.len() < 2 || produced_frames == 0 {
        out_tmp.clear();
        return 0;
    }
    let produced_frames = produced_frames.min(out[0].len()).min(out[1].len());
    let produced_samples = produced_frames * 2;
    if out_tmp.len() < produced_samples {
        out_tmp.resize(produced_samples, 0);
    } else {
        out_tmp.truncate(produced_samples);
    }
    for frame in 0..produced_frames {
        let base = frame * 2;
        out_tmp[base] = f32_to_i16(out[0][frame]);
        out_tmp[base + 1] = f32_to_i16(out[1][frame]);
    }
    produced_frames
}

#[inline(always)]
fn f32_to_i16(sample: f32) -> i16 {
    (sample * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

fn checksum_i16(samples: &[i16]) -> i64 {
    let mut checksum = 0i64;
    for &sample in samples.iter().step_by(17) {
        checksum = checksum
            .wrapping_mul(1_315_423_911)
            .wrapping_add(i64::from(sample));
    }
    checksum
}
