//! Native lossless frame encoding. Floating point is used only to propose LPC
//! coefficients; prediction, residual validation, and emitted PCM are integer exact.
use crate::{
    Error, StreamInfo,
    bitstream::BitWriter,
    checksum::{crc8, crc16},
    reserve,
};

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    /// Search effort from 0 (fast fixed prediction) through 8 (full LPC search).
    pub compression_level: u8,
    /// Nominal block size for streaming. Final blocks may be smaller than 16.
    pub block_size: u16,
    /// Write sample numbers, allowing callers of FrameEncoder to vary block sizes.
    pub variable_block_size: bool,
    /// Override the level's LPC order limit, in 0..=32.
    pub max_lpc_order: Option<u8>,
    /// Override the level's partition-order limit, in 0..=15.
    pub max_partition_order: Option<u8>,
}
impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
            compression_level: 5,
            block_size: 4096,
            variable_block_size: false,
            max_lpc_order: None,
            max_partition_order: None,
        }
    }
}
impl EncodeOptions {
    pub fn validate(&self) -> Result<(), Error> {
        if self.sample_rate >= 1 << 20
            || !(1..=8).contains(&self.channels)
            || !(4..=32).contains(&self.bits_per_sample)
            || self.compression_level > 8
            || self.block_size < 16
            || self.max_lpc_order.is_some_and(|n| n > 32)
            || self.max_partition_order.is_some_and(|n| n > 15)
        {
            return Err(Error::Invalid("encoder options"));
        }
        Ok(())
    }
    pub fn stream_info(&self) -> Result<StreamInfo, Error> {
        self.validate()?;
        Ok(StreamInfo {
            min_block_size: if self.variable_block_size {
                16
            } else {
                self.block_size
            },
            max_block_size: self.block_size,
            min_frame_size: 0,
            max_frame_size: 0,
            sample_rate: self.sample_rate,
            channels: self.channels,
            bits_per_sample: self.bits_per_sample,
            total_samples: 0,
            md5: [0; 16],
        })
    }
    fn lpc_order(&self) -> usize {
        usize::from(
            self.max_lpc_order
                .unwrap_or([0, 0, 0, 6, 8, 8, 10, 12, 32][self.compression_level as usize]),
        )
    }
    fn partition_order(&self) -> u8 {
        self.max_partition_order
            .unwrap_or([0, 2, 3, 3, 4, 5, 6, 6, 8][self.compression_level as usize])
    }
}

#[derive(Clone, Copy, Default)]
struct Partition {
    k: u8,
    width: u8,
}
#[derive(Default)]
struct RicePlan {
    method: u8,
    order: u8,
    bits: u64,
    partitions: Vec<Partition>,
}
#[derive(Clone, Copy, Default)]
struct Model {
    kind: u8,
    order: u8,
    shift: u8,
    precision: u8,
    coeff: [i16; 32],
}
#[derive(Default)]
struct SubframePlan {
    model: Model,
    width: u32,
    wasted: u32,
    bits: u64,
    rice: RicePlan,
}
#[derive(Default)]
struct Analysis {
    residual: Vec<i32>,
    rice: RicePlan,
    partitions: Vec<Partition>,
}

/// Encodes one independent frame into reusable storage, borrowing its encoded
/// bytes until the next call. Inputs are planar signed, right-aligned integer PCM.
pub struct FrameEncoder {
    options: EncodeOptions,
    work: Vec<i64>,
    plans: [SubframePlan; 8],
    analysis: Analysis,
    writer: BitWriter,
    output: Vec<u8>,
    assignment: u8,
}
impl FrameEncoder {
    pub fn new(options: EncodeOptions) -> Result<Self, Error> {
        options.validate()?;
        Ok(Self {
            options,
            work: Vec::new(),
            plans: std::array::from_fn(|_| SubframePlan::default()),
            analysis: Analysis::default(),
            writer: BitWriter::new(),
            output: Vec::new(),
            assignment: 0,
        })
    }
    pub fn options(&self) -> EncodeOptions {
        self.options
    }
    /// Number is the frame number for fixed blocks, sample number for variable blocks.
    /// This raw API deliberately leaves stream sequencing to the caller.
    pub fn encode(&mut self, planes: &[&[i32]], number: u64) -> Result<&[u8], Error> {
        let channels = usize::from(self.options.channels);
        if planes.len() != channels {
            return Err(Error::Invalid("encoder channel count"));
        }
        let n = planes[0].len();
        if n == 0 || n > 65535 || planes.iter().any(|p| p.len() != n) {
            return Err(Error::Invalid("encoder block size"));
        }
        if number
            >= 1u64
                << if self.options.variable_block_size {
                    36
                } else {
                    31
                }
        {
            return Err(Error::Invalid("encoder frame number"));
        }
        let bound = 1i64 << (self.options.bits_per_sample - 1);
        if planes
            .iter()
            .flat_map(|p| p.iter())
            .any(|&x| i64::from(x) < -bound || i64::from(x) >= bound)
        {
            return Err(Error::Invalid("PCM outside encoder bit depth"));
        }
        let count = if channels == 2 { 4 } else { channels };
        resize(&mut self.work, n * count, 0)?;
        for (dst, src) in self.work.chunks_exact_mut(n).zip(planes) {
            for (dst, &x) in dst.iter_mut().zip(*src) {
                *dst = x.into();
            }
        }
        if channels == 2 {
            for (i, (&l, &r)) in planes[0].iter().zip(planes[1]).enumerate() {
                let l = i64::from(l);
                let r = i64::from(r);
                self.work[2 * n + i] = (l + r) >> 1;
                self.work[3 * n + i] = l - r;
            }
        }
        for (ch, plane) in self.work.chunks_exact_mut(n).enumerate() {
            self.analysis.analyze(
                plane,
                u32::from(self.options.bits_per_sample) + u32::from(ch == 3 && channels == 2),
                self.options,
                &mut self.plans[ch],
            )?;
        }
        let mut selected = [0usize, 1, 2, 3, 4, 5, 6, 7];
        self.assignment = self.options.channels - 1;
        if channels == 2 {
            let mut cost = self.plans[0].bits + self.plans[1].bits;
            for (assignment, a, b) in [(8, 0, 3), (9, 3, 1), (10, 2, 3)] {
                let bits = self.plans[a].bits + self.plans[b].bits;
                if bits < cost {
                    cost = bits;
                    self.assignment = assignment;
                    selected[0] = a;
                    selected[1] = b;
                }
            }
        }
        self.output.clear();
        write_header(&mut self.output, self.options, n, self.assignment, number)?;
        self.writer.clear();
        for &ch in &selected[..channels] {
            let start = self.writer.position();
            write_subframe(
                &mut self.writer,
                &self.work[ch * n..(ch + 1) * n],
                &self.plans[ch],
                &mut self.analysis.residual,
            )?;
            debug_assert_eq!((self.writer.position() - start) as u64, self.plans[ch].bits);
        }
        self.writer.align_zero();
        reserve(&mut self.output, self.writer.as_bytes().len() + 2)?;
        self.output.extend_from_slice(self.writer.as_bytes());
        let crc = crc16(&self.output);
        self.output.extend_from_slice(&crc.to_be_bytes());
        Ok(&self.output)
    }
    pub fn channel_assignment(&self) -> u8 {
        self.assignment
    }
}

impl Analysis {
    fn analyze(
        &mut self,
        samples: &mut [i64],
        bits: u32,
        options: EncodeOptions,
        best: &mut SubframePlan,
    ) -> Result<(), Error> {
        let combined = samples.iter().fold(0u64, |a, &x| a | x as u64);
        let wasted = combined.trailing_zeros().min(bits - 1);
        if wasted != 0 {
            for x in samples.iter_mut() {
                *x >>= wasted;
            }
        }
        let width = bits - wasted;
        let overhead = 8 + u64::from(wasted);
        best.model = Model {
            kind: 1,
            ..Model::default()
        };
        best.width = width;
        best.wasted = wasted;
        best.bits = overhead + samples.len() as u64 * u64::from(width);
        best.rice.partitions.clear();
        if samples.iter().all(|&x| x == samples[0]) {
            best.model.kind = 0;
            best.bits = overhead + u64::from(width);
            return Ok(());
        }
        resize(&mut self.residual, samples.len(), 0)?;
        let fixed_limit = if options.compression_level == 0 { 2 } else { 4 };
        for order in 0..=fixed_limit.min(samples.len() - 1) {
            let mut model = Model {
                kind: 8 + order as u8,
                order: order as u8,
                ..Model::default()
            };
            let coefficients: &[i16] = match order {
                0 => &[],
                1 => &[1],
                2 => &[2, -1],
                3 => &[3, -3, 1],
                4 => &[4, -6, 4, -1],
                _ => unreachable!(),
            };
            model.coeff[..order].copy_from_slice(coefficients);
            self.consider(samples, model, options, best)?;
        }
        let max_order = options.lpc_order().min(samples.len() - 1);
        if max_order == 0 {
            return Ok(());
        }
        // Autocorrelation of a Hann-windowed signal; this affects compression
        // quality only. The exact integer residual is tested for every proposal.
        let mut corr = [0f64; 33];
        for (lag, value) in corr.iter_mut().enumerate().take(max_order + 1) {
            for i in lag..samples.len() {
                let win = |j: usize| {
                    0.5 - 0.5
                        * (std::f64::consts::TAU * j as f64 / (samples.len() - 1) as f64).cos()
                };
                *value += (samples[i] as f64 * win(i)) * (samples[i - lag] as f64 * win(i - lag));
            }
        }
        let mut coeff = [0f64; 32];
        let mut error = corr[0];
        for order in 1..=max_order {
            if !error.is_finite() || error <= corr[0] * 1e-12 {
                break;
            }
            let mut numerator = corr[order];
            for j in 0..order - 1 {
                numerator -= coeff[j] * corr[order - j - 1];
            }
            let reflection = numerator / error;
            if !reflection.is_finite() || reflection.abs() >= 1.0 {
                break;
            }
            let previous = coeff;
            for j in 0..order - 1 {
                coeff[j] = previous[j] - reflection * previous[order - j - 2];
            }
            coeff[order - 1] = reflection;
            error *= 1.0 - reflection * reflection;
            if let Some(model) = quantize(&coeff[..order]) {
                self.consider(samples, model, options, best)?;
            }
        }
        Ok(())
    }
    fn consider(
        &mut self,
        samples: &[i64],
        model: Model,
        options: EncodeOptions,
        best: &mut SubframePlan,
    ) -> Result<(), Error> {
        if !make_residual(samples, model, &mut self.residual) {
            return Ok(());
        }
        let mut overhead =
            8 + u64::from(best.wasted) + u64::from(model.order) * u64::from(best.width) + 6;
        if model.kind >= 32 {
            overhead += 9 + u64::from(model.order) * u64::from(model.precision);
        }
        if overhead >= best.bits {
            return Ok(());
        }
        choose_rice(
            &self.residual,
            model.order as usize,
            options.partition_order(),
            &mut self.rice,
            &mut self.partitions,
        )?;
        let cost = overhead + self.rice.bits;
        if cost < best.bits {
            best.bits = cost;
            best.model = model;
            best.rice.method = self.rice.method;
            best.rice.order = self.rice.order;
            best.rice.bits = self.rice.bits;
            best.rice.partitions.clear();
            reserve(&mut best.rice.partitions, self.rice.partitions.len())?;
            best.rice
                .partitions
                .extend_from_slice(&self.rice.partitions);
        }
        Ok(())
    }
}

fn quantize(coeff: &[f64]) -> Option<Model> {
    for shift in (0..=15).rev() {
        let mut model = Model {
            kind: 31 + coeff.len() as u8,
            order: coeff.len() as u8,
            shift,
            ..Model::default()
        };
        let mut error = 0f64;
        let mut precision = 1;
        let mut valid = true;
        for (dst, &src) in model.coeff.iter_mut().zip(coeff) {
            error += src * (1u32 << shift) as f64;
            let value = error.round();
            if !(-16384.0..=16383.0).contains(&value) {
                valid = false;
                break;
            }
            *dst = value as i16;
            error -= value;
            precision = precision.max(signed_width(*dst as i32));
        }
        if valid {
            model.precision = precision;
            return Some(model);
        }
    }
    None
}
fn make_residual(samples: &[i64], model: Model, output: &mut [i32]) -> bool {
    let order = model.order as usize;
    for i in order..samples.len() {
        let mut prediction = 0i64;
        for j in 0..order {
            prediction += i64::from(model.coeff[j]) * samples[i - j - 1];
        }
        let value = samples[i] - (prediction >> model.shift);
        if value < -i64::from(i32::MAX) || value > i64::from(i32::MAX) {
            return false;
        }
        output[i] = value as i32;
    }
    true
}
#[inline]
fn folded(x: i32) -> u32 {
    ((x as u32) << 1) ^ ((x >> 31) as u32)
}
fn signed_width(x: i32) -> u8 {
    if x == 0 {
        0
    } else {
        (33 - (if x < 0 { !x } else { x } as u32).leading_zeros()) as u8
    }
}

fn partition_cost(samples: &[i32], max_k: u8) -> (u64, Partition) {
    let sum = samples.iter().map(|&x| u64::from(folded(x))).sum::<u64>();
    let mean = sum / samples.len() as u64;
    let mut k = (63 - mean.max(1).leading_zeros()).min(u32::from(max_k)) as u8;
    let cost = |k: u8| {
        samples
            .iter()
            .map(|&x| u64::from(folded(x) >> k))
            .sum::<u64>()
            + samples.len() as u64 * (u64::from(k) + 1)
    };
    let mut best = cost(k);
    // Rice length is convex in k. Walk down/up to the exact minimum, rather
    // than assuming that a mean-derived parameter is always optimal.
    while k > 0 {
        let next = cost(k - 1);
        if next > best {
            break;
        }
        k -= 1;
        best = next;
    }
    while k < max_k {
        let next = cost(k + 1);
        if next >= best {
            break;
        }
        k += 1;
        best = next;
    }
    let width = samples.iter().map(|&x| signed_width(x)).max().unwrap_or(0);
    let escape = 5 + samples.len() as u64 * u64::from(width);
    if width <= 31 && escape < best {
        (escape, Partition { k: 255, width })
    } else {
        (best, Partition { k, width: 0 })
    }
}
fn choose_rice(
    residual: &[i32],
    predictor: usize,
    max_order: u8,
    best: &mut RicePlan,
    trial: &mut Vec<Partition>,
) -> Result<(), Error> {
    best.bits = u64::MAX;
    let limit = max_order.min(residual.len().trailing_zeros() as u8);
    for order in 0..=limit {
        let size = residual.len() >> order;
        if size <= predictor {
            break;
        }
        for method in 0..=1 {
            let parameter_bits = 4 + method;
            let mut bits = 0u64;
            trial.clear();
            reserve(trial, 1usize << order)?;
            for part in 0..1usize << order {
                let start = if part == 0 { predictor } else { part * size };
                let (cost, param) = partition_cost(
                    &residual[start..(part + 1) * size],
                    if method == 0 { 14 } else { 30 },
                );
                bits += cost + u64::from(parameter_bits);
                trial.push(param);
            }
            if bits < best.bits {
                best.method = method;
                best.order = order;
                best.bits = bits;
                best.partitions.clear();
                reserve(&mut best.partitions, trial.len())?;
                best.partitions.extend_from_slice(trial);
            }
        }
    }
    Ok(())
}
fn write_subframe(
    w: &mut BitWriter,
    samples: &[i64],
    plan: &SubframePlan,
    residual: &mut Vec<i32>,
) -> Result<(), Error> {
    w.write(
        u64::from(plan.model.kind) * 2 + u64::from(plan.wasted != 0),
        8,
    )?;
    if plan.wasted != 0 {
        w.unary(u64::from(plan.wasted - 1))?;
    }
    match plan.model.kind {
        0 => w.signed(samples[0], plan.width)?,
        1 => {
            for &x in samples {
                w.signed(x, plan.width)?;
            }
        }
        _ => {
            let model = plan.model;
            let order = model.order as usize;
            for &x in &samples[..order] {
                w.signed(x, plan.width)?;
            }
            if model.kind >= 32 {
                w.write(u64::from(model.precision - 1), 4)?;
                w.write(u64::from(model.shift), 5)?;
                for &c in &model.coeff[..order] {
                    w.signed(i64::from(c), model.precision.into())?;
                }
            }
            resize(residual, samples.len(), 0)?;
            if !make_residual(samples, model, residual) {
                return Err(Error::Invalid("encoder residual invariant"));
            }
            let rice = &plan.rice;
            w.write(u64::from(rice.method), 2)?;
            w.write(u64::from(rice.order), 4)?;
            let size = samples.len() >> rice.order;
            let param_bits = u32::from(rice.method) + 4;
            for (i, param) in rice.partitions.iter().enumerate() {
                let start = if i == 0 { order } else { i * size };
                let part = &residual[start..(i + 1) * size];
                if param.k == 255 {
                    w.write((1 << param_bits) - 1, param_bits)?;
                    w.write(u64::from(param.width), 5)?;
                    for &x in part {
                        w.signed(i64::from(x), param.width.into())?;
                    }
                } else {
                    w.write(u64::from(param.k), param_bits)?;
                    for &x in part {
                        let x = u64::from(folded(x));
                        w.unary(x >> param.k)?;
                        w.write(x, param.k.into())?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn write_header(
    out: &mut Vec<u8>,
    options: EncodeOptions,
    n: usize,
    assignment: u8,
    number: u64,
) -> Result<(), Error> {
    reserve(out, 16)?;
    let block = match n {
        192 => 1,
        576 => 2,
        1152 => 3,
        2304 => 4,
        4608 => 5,
        _ if n.is_power_of_two() && (256..=32768).contains(&n) => n.trailing_zeros() as u8,
        _ if n <= 256 => 6,
        _ => 7,
    };
    let rate = match options.sample_rate {
        88200 => 1,
        176400 => 2,
        192000 => 3,
        8000 => 4,
        16000 => 5,
        22050 => 6,
        24000 => 7,
        32000 => 8,
        44100 => 9,
        48000 => 10,
        96000 => 11,
        x if x != 0 && x.is_multiple_of(1000) && x / 1000 <= 255 => 12,
        x if x <= 65535 => 13,
        x if x.is_multiple_of(10) && x / 10 <= 65535 => 14,
        _ => 0,
    };
    let depth = match options.bits_per_sample {
        8 => 1,
        12 => 2,
        16 => 4,
        20 => 5,
        24 => 6,
        32 => 7,
        _ => 0,
    };
    out.extend_from_slice(&[
        255,
        248 | u8::from(options.variable_block_size),
        block << 4 | rate,
        assignment << 4 | depth << 1,
    ]);
    if number < 128 {
        out.push(number as u8);
    } else {
        let len = [7, 11, 16, 21, 26, 31, 36]
            .iter()
            .position(|&bits| number < 1 << bits)
            .unwrap()
            + 1;
        out.push((255u8 << (8 - len)) | (number >> (6 * (len - 1))) as u8);
        for i in (0..len - 1).rev() {
            out.push(128 | ((number >> (6 * i)) as u8 & 63));
        }
    }
    if block == 6 {
        out.push((n - 1) as u8);
    } else if block == 7 {
        out.extend_from_slice(&((n - 1) as u16).to_be_bytes());
    }
    match rate {
        12 => out.push((options.sample_rate / 1000) as u8),
        13 => out.extend_from_slice(&(options.sample_rate as u16).to_be_bytes()),
        14 => out.extend_from_slice(&((options.sample_rate / 10) as u16).to_be_bytes()),
        _ => (),
    }
    out.push(crc8(out));
    Ok(())
}
pub(crate) fn resize<T: Clone>(data: &mut Vec<T>, len: usize, value: T) -> Result<(), Error> {
    if len > data.len() {
        let extra = len - data.len();
        reserve(data, extra)?;
    }
    data.resize(len, value);
    Ok(())
}
