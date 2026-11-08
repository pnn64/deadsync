use deadlib_image_jpeg::{
    CoefficientComponent, CoefficientImage, ComponentSpec, FrameHeader, Limits, Process,
};

pub fn model(
    count: usize,
    precision: u8,
    layout: usize,
    quantizers: usize,
) -> CoefficientImage<'static> {
    let specs: Vec<_> = (0..count)
        .map(|i| {
            let (h, v) = match layout {
                1 => ((i % 4 + 1) as u8, ((i / 4) % 4 + 1) as u8),
                2 if i.is_multiple_of(3) => (2, 2),
                3 => (4, 4),
                _ => (1, 1),
            };
            ComponentSpec {
                id: (i * 73 + 251) as u8,
                horizontal_sampling: h,
                vertical_sampling: v,
                quantization_table: 0,
            }
        })
        .collect();
    let max_h = specs
        .iter()
        .map(|c| c.horizontal_sampling as usize)
        .max()
        .unwrap();
    let max_v = specs
        .iter()
        .map(|c| c.vertical_sampling as usize)
        .max()
        .unwrap();
    let (w, h) = (17usize, 19usize);
    let mut components = Vec::with_capacity(count);
    for (i, &spec) in specs.iter().enumerate() {
        let width = (w * spec.horizontal_sampling as usize).div_ceil(max_h);
        let height = (h * spec.vertical_sampling as usize).div_ceil(max_v);
        let bw = w.div_ceil(8 * max_h) * spec.horizontal_sampling as usize;
        let bh = h.div_ceil(8 * max_v) * spec.vertical_sampling as usize;
        let q = std::array::from_fn(|k| match quantizers {
            1 => (1 + (i % 4 * 31 + k * 7) % 255) as u16,
            2 => (1 + (i + k * 17) % 255) as u16,
            3 => (256 + (i * 257 + k * 37) % 32000) as u16,
            _ => (1 + k * 3 % 32) as u16,
        });
        let mut blocks = vec![[0i16; 64]; bw * bh];
        for (j, block) in blocks.iter_mut().enumerate() {
            block[0] = ((i * 131 + j * 37) % 401) as i16 - 200;
            if precision == 12 {
                block[0] *= 9;
            }
            for k in [1, 7, 8, 19, 56, 63] {
                block[k] = ((i * 3 + j * 5 + k) % 7) as i16 - 3;
            }
        }
        components.push(CoefficientComponent {
            spec,
            width,
            height,
            blocks_wide: bw,
            blocks_high: bh,
            quantization: q,
            successive_approximation: [0; 64],
            blocks,
        });
    }
    CoefficientImage::from_components(
        FrameHeader {
            process: Process::ExtendedSequential,
            precision,
            width: w as u16,
            height: h as u16,
            components: specs,
        },
        components,
        vec![],
        Limits::default(),
    )
    .unwrap()
}

pub fn same(a: &CoefficientImage<'_>, b: &CoefficientImage<'_>) {
    assert_eq!(
        (a.frame.precision, a.frame.width, a.frame.height),
        (b.frame.precision, b.frame.width, b.frame.height)
    );
    assert_eq!(a.components.len(), b.components.len());
    for (a, b) in a.components.iter().zip(&b.components) {
        assert_eq!(
            (
                a.spec.id,
                a.spec.horizontal_sampling,
                a.spec.vertical_sampling,
                a.width,
                a.height
            ),
            (
                b.spec.id,
                b.spec.horizontal_sampling,
                b.spec.vertical_sampling,
                b.width,
                b.height
            )
        );
        assert_eq!(a.quantization, b.quantization);
        for y in 0..a.height.div_ceil(8) {
            for x in 0..a.width.div_ceil(8) {
                assert_eq!(
                    a.blocks[y * a.blocks_wide + x],
                    b.blocks[y * b.blocks_wide + x]
                );
            }
        }
    }
}

pub fn coefficient_dump(data: &[u8], frame: &FrameHeader, components: &[CoefficientComponent]) {
    assert_eq!(&data[..4], b"JCO1");
    let mut position = 4;
    fn word(data: &[u8], p: &mut usize) -> usize {
        let n = u32::from_le_bytes(data[*p..*p + 4].try_into().unwrap()) as usize;
        *p += 4;
        n
    }
    for n in [
        frame.precision as usize,
        frame.width as usize,
        frame.height as usize,
        components.len(),
    ] {
        assert_eq!(word(data, &mut position), n);
    }
    for c in components {
        for n in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
            c.width.div_ceil(8),
            c.height.div_ceil(8),
        ] {
            assert_eq!(word(data, &mut position), n);
        }
        for q in c.quantization {
            assert_eq!(
                u16::from_le_bytes(data[position..position + 2].try_into().unwrap()),
                q
            );
            position += 2;
        }
        for y in 0..c.height.div_ceil(8) {
            for x in 0..c.width.div_ceil(8) {
                for value in c.blocks[y * c.blocks_wide + x] {
                    assert_eq!(
                        i16::from_le_bytes(data[position..position + 2].try_into().unwrap()),
                        value
                    );
                    position += 2;
                }
            }
        }
    }
    assert_eq!(position, data.len());
}

pub fn sample_dump(
    data: &[u8],
    precision: u8,
    width: u16,
    height: u16,
    planes: &[deadlib_image_jpeg::ComponentPlane],
) {
    assert_eq!(&data[..4], b"JLP1");
    let mut position = 4;
    fn word(data: &[u8], p: &mut usize) -> usize {
        let n = u32::from_le_bytes(data[*p..*p + 4].try_into().unwrap()) as usize;
        *p += 4;
        n
    }
    for n in [
        precision as usize,
        width as usize,
        height as usize,
        planes.len(),
    ] {
        assert_eq!(word(data, &mut position), n);
    }
    for c in planes {
        for n in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
        ] {
            assert_eq!(word(data, &mut position), n);
        }
        for &v in &c.samples {
            assert_eq!(
                u16::from_le_bytes(data[position..position + 2].try_into().unwrap()),
                v
            );
            position += 2;
        }
    }
    assert_eq!(position, data.len());
}
