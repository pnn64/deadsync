use crate::{Format, ImageError, ImageResult};
use crate::{decode_error, output_buffer, output_len, rgba};
use deadlib_image::RgbaImage;
use deadlib_image_gif::{
    Background, Block, CompositeOptions, Compositor, DecodeOptions, Decoder, Event, Frame, Header,
    RenderedFrame,
};
use std::io::{BufRead, Cursor};

/// An animated GIF decoder with a reusable canvas and frame buffer.
pub struct GifFrames<'a> {
    input: Cursor<&'a [u8]>,
    decoder: Decoder,
    compositor: Option<Compositor>,
    dimensions: (u32, u32),
    ended: bool,
}

impl<'a> GifFrames<'a> {
    pub fn new(bytes: &'a [u8]) -> ImageResult<Self> {
        let mut input = Cursor::new(bytes);
        let mut decoder = Decoder::new(DecodeOptions {
            strict: false,
            check_bounds: false,
            ..DecodeOptions::default()
        });
        let dimensions = read_header(&mut input, &mut decoder)?;
        Ok(Self {
            input,
            decoder,
            compositor: None,
            dimensions,
            ended: false,
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    /// The frame borrows the composited canvas until the next call.
    pub fn next_frame(&mut self) -> ImageResult<Option<RenderedFrame<'_>>> {
        if self.ended {
            return Ok(None);
        }
        while let Some(event) = next_event(&mut self.input, &mut self.decoder)? {
            if let Event::Block(Block::Frame(frame)) = event {
                if self.compositor.is_none() {
                    let header = self.decoder.header().expect("GIF header");
                    self.compositor = Some(
                        Compositor::with_options(
                            header,
                            CompositeOptions {
                                background: Background::Transparent,
                                ..CompositeOptions::default()
                            },
                        )
                        .map_err(gif_error)?,
                    );
                }
                let compositor = self.compositor.as_mut().expect("GIF canvas");
                let output = compositor.render(&frame).map_err(gif_error)?;
                self.decoder.recycle_frame(frame);
                return Ok(Some(output));
            }
        }
        self.ended = true;
        Ok(None)
    }
}

pub(crate) fn decode_first(input: &mut impl BufRead) -> ImageResult<RgbaImage> {
    let mut decoder = Decoder::new(DecodeOptions {
        strict: false,
        ..DecodeOptions::default()
    });
    while let Some(event) = next_event(input, &mut decoder)? {
        match event {
            Event::Header => {
                let header = decoder.header().expect("GIF header event");
                output_len(header.width.into(), header.height.into())?;
            }
            Event::Block(Block::Frame(frame)) => {
                return render_first(decoder.header().expect("GIF header"), frame);
            }
            _ => {}
        }
    }
    Err(decode_error(Format::Gif, "GIF has no frame"))
}

pub(crate) fn dimensions(input: &mut impl BufRead) -> ImageResult<(u32, u32)> {
    let mut decoder = Decoder::new(DecodeOptions {
        strict: false,
        ..DecodeOptions::default()
    });
    read_header(input, &mut decoder)
}

fn read_header(input: &mut impl BufRead, decoder: &mut Decoder) -> ImageResult<(u32, u32)> {
    if next_event(input, decoder)? == Some(Event::Header) {
        let header = decoder.header().expect("GIF header event");
        Ok((header.width.into(), header.height.into()))
    } else {
        Err(decode_error(Format::Gif, "missing GIF header"))
    }
}

fn next_event(input: &mut impl BufRead, decoder: &mut Decoder) -> ImageResult<Option<Event>> {
    loop {
        let bytes = input.fill_buf()?;
        if bytes.is_empty() {
            decoder.finish().map_err(gif_error)?;
            return Ok(None);
        }
        let progress = decoder.update(bytes).map_err(gif_error)?;
        input.consume(progress.consumed);
        match progress.event {
            Some(Event::End) => return Ok(None),
            Some(event) => return Ok(Some(event)),
            None => {}
        }
    }
}

fn gif_error(error: deadlib_image_gif::Error) -> ImageError {
    decode_error(Format::Gif, error)
}

fn render_first(header: &Header, frame: Frame) -> ImageResult<RgbaImage> {
    let width = u32::from(header.width);
    let height = u32::from(header.height);
    let mut colors = [[0; 4]; 256];
    let palette = frame.palette(header).unwrap_or_default();
    for (color, &[red, green, blue]) in colors.iter_mut().zip(palette) {
        *color = [red, green, blue, 255];
    }
    if let Some(index) = frame.control.and_then(|control| control.transparent_index) {
        colors[index as usize] = [0; 4];
    }
    let mut output = output_buffer(width, height)?;
    if frame.width == header.width && frame.height == header.height {
        output.extend(
            frame
                .indices
                .into_iter()
                .flat_map(|index| colors[index as usize]),
        );
    } else {
        // A texture's first frame starts on a transparent logical canvas.
        output.resize(output_len(width, height)?, 0);
        let rows = output
            .chunks_exact_mut(width as usize * 4)
            .skip(usize::from(frame.top));
        for (row, source) in rows.zip(frame.indices.chunks_exact(usize::from(frame.width))) {
            let start = usize::from(frame.left) * 4;
            let pixels = row[start..].as_chunks_mut::<4>().0;
            for (pixel, &index) in pixels.iter_mut().zip(source) {
                *pixel = colors[index as usize];
            }
        }
    }
    rgba(width, height, output)
}
