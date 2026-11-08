// Frozen format parser from fc59baf80 for exact differential checks.
fn original_parse_format_chunk(
    reader: &mut BufReader<File>,
    chunk_size: u32,
) -> Result<Spec, Box<dyn std::error::Error + Send + Sync>> {
    if chunk_size < 16 {
        return Err(format!("WAV fmt chunk is too small ({chunk_size} bytes)").into());
    }
    let mut fmt = vec![0u8; chunk_size as usize];
    reader.read_exact(&mut fmt)?;

    let format_tag = le_u16(&fmt, 0)?;
    let channels = le_u16(&fmt, 2)? as usize;
    let sample_rate_hz = le_u32(&fmt, 4)?;
    let block_align = le_u16(&fmt, 12)? as usize;
    let bits_per_sample = le_u16(&fmt, 14)?;
    let bytes_per_sample = block_align
        .checked_div(channels.max(1))
        .ok_or("WAV block alignment is invalid")?;
    if channels == 0 || sample_rate_hz == 0 || bytes_per_sample == 0 || block_align == 0 {
        return Err("WAV format fields are invalid".into());
    }

    let encoding = match format_tag {
        WAVE_FORMAT_PCM => parse_pcm_format(bytes_per_sample)?,
        WAVE_FORMAT_IEEE_FLOAT => parse_float_format(bytes_per_sample)?,
        WAVE_FORMAT_EXTENSIBLE => {
            parse_extensible_encoding(&fmt, bytes_per_sample, bits_per_sample)?
        }
        _ => return Err(format!("unsupported WAV format tag 0x{format_tag:04x}").into()),
    };

    Ok(Spec {
        channels,
        sample_rate_hz,
        block_align,
        encoding,
        data_offset: 0,
        frames_total: 0,
    })
}
