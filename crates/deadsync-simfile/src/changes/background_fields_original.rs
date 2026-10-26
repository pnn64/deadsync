pub fn bgchange_field_rejects_non_media(field: &str) -> bool {
    field.as_bytes().windows(4).any(|window| {
        window[0] == b'.'
            && (window[1..].eq_ignore_ascii_case(b"ini")
                || window[1..].eq_ignore_ascii_case(b"xml"))
    })
}

pub fn parse_bgchange_color(field: &str) -> Option<[f32; 4]> {
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    if let Some(hex) = field.strip_prefix('#')
        && matches!(hex.len(), 6 | 8)
    {
        let r = f32::from(u8::from_str_radix(&hex[0..2], 16).ok()?) / 255.0;
        let g = f32::from(u8::from_str_radix(&hex[2..4], 16).ok()?) / 255.0;
        let b = f32::from(u8::from_str_radix(&hex[4..6], 16).ok()?) / 255.0;
        let a = if hex.len() == 8 {
            f32::from(u8::from_str_radix(&hex[6..8], 16).ok()?) / 255.0
        } else {
            1.0
        };
        return Some([r, g, b, a]);
    }
    let mut parts = field
        .split([',', '^'])
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let red = parts.next()?.parse::<f32>().ok()?;
    let green = parts.next()?.parse::<f32>().ok()?;
    let blue = parts.next()?.parse::<f32>().ok()?;
    let alpha = match parts.next() {
        Some(alpha) => alpha.parse::<f32>().ok()?,
        None => 1.0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some([red, green, blue, alpha])
}

pub fn normalized_background_lua_color(field: Option<&str>) -> String {
    let Some(color) = field.and_then(|field| parse_bgchange_color(&field.replace('^', ","))) else {
        return String::new();
    };
    let [r, g, b, a] =
        color.map(|component| (component * 255.0).round_ties_even().clamp(0.0, 255.0) as u8);
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}
