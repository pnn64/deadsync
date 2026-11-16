#[must_use]
pub fn scale_value(value: f32, from_low: f32, from_high: f32, to_low: f32, to_high: f32) -> f32 {
    let span = from_high - from_low;
    if span.abs() <= f32::EPSILON {
        to_low
    } else {
        (value - from_low) * (to_high - to_low) / span + to_low
    }
}

fn seconds_to_time_parts(seconds: f64) -> (i64, i64, i64) {
    let minutes = (seconds / 60.0).trunc() as i64;
    let secs = (seconds % 60.0).trunc() as i64;
    let centis = ((seconds - (minutes * 60 + secs) as f64) * 100.0).trunc() as i64;
    let centis = centis.clamp(0, 99);
    (minutes, secs, centis)
}

// Callers supply two or three parts, a first width of one or two, and
// centiseconds already clamped by seconds_to_time_parts.
fn format_clock<const N: usize>(
    parts: [i64; N],
    first_width: usize,
    centis: Option<i64>,
) -> String {
    let mut buffers = [itoa::Buffer::new(); N];
    let mut digits = [""; N];
    for ((buffer, output), value) in buffers.iter_mut().zip(digits.iter_mut()).zip(parts) {
        *output = buffer.format(value);
    }
    let capacity = digits
        .iter()
        .enumerate()
        .map(|(index, text)| text.len().max(if index == 0 { first_width } else { 2 }))
        .sum::<usize>()
        + (N - 1)
        + if centis.is_some() { 3 } else { 0 };
    let mut out = String::with_capacity(capacity);
    for (index, text) in digits.into_iter().enumerate() {
        if index != 0 {
            out.push(':');
        }
        let width = if index == 0 { first_width } else { 2 };
        if text.len() < width {
            out.push('0');
        }
        out.push_str(text);
    }
    if let Some(centis) = centis {
        out.push('.');
        out.push(char::from(b'0' + (centis / 10) as u8));
        out.push(char::from(b'0' + (centis % 10) as u8));
    }
    out
}

#[must_use]
pub fn seconds_to_hhmmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format_clock([minutes / 60, minutes % 60, seconds], 2, None)
}

#[must_use]
pub fn seconds_to_mss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format_clock([minutes, seconds], 1, None)
}

#[must_use]
pub fn seconds_to_mmss(seconds: f64) -> String {
    let (minutes, seconds, _) = seconds_to_time_parts(seconds);
    format_clock([minutes, seconds], 2, None)
}

#[must_use]
pub fn seconds_to_mss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format_clock([minutes, seconds], 1, Some(centis))
}

#[must_use]
pub fn seconds_to_mmss_ms_ms(seconds: f64) -> String {
    let (minutes, seconds, centis) = seconds_to_time_parts(seconds);
    format_clock([minutes, seconds], 2, Some(centis))
}

#[must_use]
pub fn format_number_and_suffix(value: i64) -> String {
    let suffix = if (value % 100) / 10 == 1 {
        "th"
    } else {
        match value % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    let mut buffer = itoa::Buffer::new();
    let digits = buffer.format(value);
    let mut out = String::with_capacity(digits.len() + suffix.len());
    out.push_str(digits);
    out.push_str(suffix);
    out
}
