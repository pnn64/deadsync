//! External scan-script oracle client. All codec work is native Rust.
use deadlib_image_jpeg::{
    CodingOptions, CoefficientImage, EntropyCoding, Limits, ScanSpec, decode_coefficients,
    encode_coefficients,
};

fn random_script(image: &CoefficientImage<'_>, mut seed: u32) -> Vec<ScanSpec> {
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed
    };
    let mut max_ac = 0;
    let mut max_dc = 0;
    for block in image.components.iter().flat_map(|c| &c.blocks) {
        max_dc = max_dc.max(32 - i32::from(block[0]).unsigned_abs().leading_zeros());
        for &value in &block[1..] {
            max_ac = max_ac.max(32 - i32::from(value).unsigned_abs().leading_zeros());
        }
    }
    let minimum = max_ac
        .saturating_sub(u32::from(image.frame.precision) + 2)
        .max((max_dc + 1).saturating_sub(u32::from(image.frame.precision) + 3))
        as u8;
    // Each node owns a band's initial precision. Legal actions can interleave
    // bands and components, including AC before other components' initial DCs.
    let mut nodes = Vec::new();
    let mut dc_nodes = Vec::new();
    for component in 0..image.components.len() {
        dc_nodes.push(nodes.len());
        nodes.push((
            component,
            0u8,
            0u8,
            minimum + (random() % u32::from(14 - minimum)) as u8,
        ));
        let mut ss = 1;
        while ss <= 63 {
            let se = (ss + (random() % 13) as u8).min(63);
            nodes.push((
                component,
                ss,
                se,
                minimum + (random() % u32::from(14 - minimum)) as u8,
            ));
            ss = se + 1;
        }
    }
    let mut precision = vec![-1i8; nodes.len()];
    let mut script = Vec::new();
    loop {
        let candidates: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, &(component, ss, _, _))| {
                (precision[i] != 0 && (ss == 0 || precision[dc_nodes[component]] >= 0)).then_some(i)
            })
            .collect();
        if candidates.is_empty() {
            break;
        }
        let i = candidates[random() as usize % candidates.len()];
        let (component, ss, se, initial) = nodes[i];
        let (ah, al) = if precision[i] < 0 {
            (0, initial)
        } else {
            (precision[i] as u8, precision[i] as u8 - 1)
        };
        script.push(ScanSpec::new(&[image.components[component].spec.id], ss, se, ah, al).unwrap());
        precision[i] = al as i8;
    }
    script
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("usage: rewrite_scans input.jpg output.jpg arithmetic[0/1] optimize[0/1] restart autoN|splitN|partialN|separate|mixed|randomSEED".into());
    }
    let bytes = std::fs::read(&args[0])?;
    let image = decode_coefficients(&bytes, Limits::default())?;
    let ids: Vec<_> = image.components.iter().map(|c| c.spec.id).collect();
    let mut script = Vec::new();
    let mode = args[5].as_str();
    let mut initial = 0;
    let progressive = !matches!(mode, "separate" | "mixed");
    if let Some(value) = mode.strip_prefix("auto") {
        initial = value.parse()?;
    } else if let Some(value) = mode.strip_prefix("random") {
        script = random_script(&image, value.parse()?);
    } else if let Some(value) = mode.strip_prefix("split") {
        let maximum: u8 = value.parse()?;
        if maximum > 13 {
            return Err("initial Al exceeds 13".into());
        }
        for al in (0..=maximum).rev() {
            let ah = if al == maximum { 0 } else { al + 1 };
            for &id in ids.iter().rev() {
                script.push(ScanSpec::new(&[id], 0, 0, ah, al)?);
            }
            for &id in ids.iter().rev() {
                for (ss, se) in [(6, 63), (1, 5)] {
                    script.push(ScanSpec::new(&[id], ss, se, ah, al)?);
                }
            }
        }
    } else if let Some(value) = mode.strip_prefix("partial") {
        let al = value.parse()?;
        for &id in ids.iter().rev() {
            script.push(ScanSpec::new(&[id], 0, 0, 0, al)?);
        }
    } else if mode == "separate" {
        for &id in ids.iter().rev() {
            script.push(ScanSpec::new(&[id], 0, 63, 0, 0)?);
        }
    } else if mode == "mixed" {
        script.push(ScanSpec::new(&ids[..1], 0, 63, 0, 0)?);
        if ids.len() > 1 {
            script.push(ScanSpec::new(&ids[1..], 0, 63, 0, 0)?);
        }
    } else {
        return Err("unknown scan mode".into());
    }
    let encoded = encode_coefficients(
        &image,
        CodingOptions {
            progressive,
            successive_approximation: initial,
            scan_script: &script,
            entropy_coding: if args[2] == "1" {
                EntropyCoding::Arithmetic
            } else {
                EntropyCoding::Huffman
            },
            optimize_huffman: args[3] == "1",
            restart_interval: args[4].parse()?,
            ..CodingOptions::default()
        },
    )?;
    std::fs::write(&args[1], encoded)?;
    if mode.starts_with("random") {
        println!("{}", script.len());
    }
    Ok(())
}
