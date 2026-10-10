// Frozen from 0530013f08d5b481d29533127de2dc41033f63be; only visibility differs.
use super::*;
pub(super) fn load_and_resample_sfx(
    path: &Path,
    output: OutputFormat,
    options: decode::DecodeOptions,
) -> Result<Arc<[i16]>, Box<dyn std::error::Error + Send + Sync>> {
    let opened = decode::open_file(path, options)?;
    let mut reader = opened.reader;
    let in_ch = opened.channels;
    let in_hz = opened.sample_rate_hz;
    let frames_total_hint = opened.frames_total_hint;
    let out_ch = output.channels;
    let out_hz = output.sample_rate_hz;
    let output_capacity = sfx_output_capacity(frames_total_hint, in_hz, output);

    if in_hz == out_hz {
        let mut pkt_buf = Vec::new();
        let mut decoded_data = Vec::with_capacity(output_capacity);
        while reader.read_dec_packet_into(&mut pkt_buf)? {
            if !pkt_buf.is_empty() {
                if in_ch == out_ch {
                    decoded_data.extend_from_slice(&pkt_buf);
                } else {
                    append_channel_mapped_i16(&pkt_buf, in_ch, out_ch, &mut decoded_data);
                }
            }
        }
        return Ok(Arc::from(decoded_data.into_boxed_slice()));
    }

    let ratio = f64::from(out_hz) / f64::from(in_hz);
    let mut resampler = new_resampler(ratio, 1.0, in_ch)?;

    let mut in_planar = PlanarAccum::new(in_ch, PLANAR_INPUT_CAP_FRAMES);
    let mut resample_in = vec![vec![0.0; resampler.input_frames_max()]; in_ch];
    let mut resample_out = vec![vec![0.0; resampler.output_frames_max()]; in_ch];
    let mut out_tmp = Vec::with_capacity(OUT_FRAMES_PER_CALL * out_ch);
    let mut pkt_buf = Vec::new();
    let mut resampled_data = Vec::with_capacity(output_capacity);
    let mut resampler_lead_frames = compat_lead_frames(ratio);

    while reader.read_dec_packet_into(&mut pkt_buf)? {
        if pkt_buf.is_empty() {
            continue;
        }
        in_planar.push_i16_interleaved(&pkt_buf, in_ch);
        loop {
            let need = resampler.input_frames_next();
            if in_planar.available_frames() < need {
                break;
            }
            let produced_frames = process_resampler(
                &mut resampler,
                &in_planar.channels,
                in_planar.start_frame,
                &mut resample_out,
            )?
            .1;
            in_planar.consume_frames(need);
            if produced_frames == 0 {
                break;
            }
            write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
            trim_resampler_lead(&mut out_tmp, out_ch, &mut resampler_lead_frames);
            resampled_data.extend_from_slice(&out_tmp);
        }
    }

    if !in_planar.is_empty() {
        let remain = in_planar.available_frames();
        let need = resampler.input_frames_next();
        let copy_frames = remain.min(need);
        let start = in_planar.start_frame;
        let end = start + copy_frames;
        for (dst, channel) in resample_in.iter_mut().zip(&in_planar.channels) {
            dst[copy_frames..need].fill(0.0);
            dst[..copy_frames].copy_from_slice(&channel[start..end]);
        }
        let produced_frames =
            process_resampler(&mut resampler, &resample_in, 0, &mut resample_out)?.1;
        if produced_frames > 0 {
            write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
            trim_resampler_lead(&mut out_tmp, out_ch, &mut resampler_lead_frames);
            resampled_data.extend_from_slice(&out_tmp);
        }
        in_planar.clear();
    }

    let need = resampler.input_frames_next();
    for dst in &mut resample_in {
        dst[..need].fill(0.0);
    }
    let produced_frames = process_resampler(&mut resampler, &resample_in, 0, &mut resample_out)?.1;
    if produced_frames > 0 {
        write_resampler_output(&resample_out, produced_frames, out_ch, &mut out_tmp);
        trim_resampler_lead(&mut out_tmp, out_ch, &mut resampler_lead_frames);
        resampled_data.extend_from_slice(&out_tmp);
    }
    Ok(Arc::from(resampled_data.into_boxed_slice()))
}
