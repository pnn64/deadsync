//! Exact native JPEG regression snapshots, independent of reference decoders.
//!
//! Run from a stable working directory:
//!   native_snapshot capture SNAPSHOT_DIR INPUT...
//!   native_snapshot verify SNAPSHOT_DIR INPUT...
//!   native_snapshot size INPUT...  (decode and count bytes without writing)
//! Options: --files-from LIST, --relative-to DIR, --max-bytes N.
//! Lists contain UTF-8 paths relative to the working directory, one per line.
//! Directories recursively select .jpg/.jpeg/.jpe; explicit files accept any suffix.
//! Both strict policies are exercised for pixel output with both upsamplers and
//! for metadata-retaining coefficient, lossless, and hierarchical decode results.
//! Public arrays and metadata are explicit binary records; capacities/private
//! allocation accounting are deliberately excluded. Reconstruction convenience
//! methods are not separately invoked: pixel decode and native coefficient/plane
//! results are recorded. This binary is a correctness oracle, never a benchmark.
#![forbid(unsafe_code)]

use deadlib_image_jpeg as jpeg;
use native_samples::{Image, Samples};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
};

const SNAPSHOT_MAGIC: &[u8] = b"JPEG_NATIVE_SNAPSHOT\0\x01";
const MANIFEST_MAGIC: &[u8] = b"JPEG_NATIVE_MANIFEST\0\x01";
const DEFAULT_MAX_BYTES: u64 = 1_000_000_000;
const IO_BUFFER: usize = 64 * 1024;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Capture,
    Verify,
    Size,
}

struct Config {
    mode: Mode,
    directory: PathBuf,
    relative_to: PathBuf,
    inputs: Vec<PathBuf>,
    max_bytes: u64,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn different(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn usage() -> &'static str {
    "usage: native_snapshot capture|verify SNAPSHOT_DIR [--relative-to DIR] \
     [--max-bytes N] [--files-from LIST] INPUT...\n\
     Or: native_snapshot size [OPTIONS] INPUT... (no files written).\n\
     Directories select JPEG suffixes recursively; explicit files accept every suffix.\n\
     LIST paths are relative to the working directory. Blank lines and # comments are ignored.\n\
     Capture requires a new directory and never overwrites snapshots. Default cap: 1,000,000,000 bytes."
}

fn argument(args: &[OsString], index: &mut usize, option: &str) -> io::Result<OsString> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| invalid(format!("missing value for {option}")))
}

fn config() -> io::Result<Option<Config>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{}", usage());
        return Ok(None);
    }
    let mode = match args.first().and_then(|arg| arg.to_str()) {
        Some("capture") => Mode::Capture,
        Some("verify") => Mode::Verify,
        Some("size") => Mode::Size,
        _ => return Err(invalid(usage())),
    };
    let directory = if mode == Mode::Size {
        PathBuf::new()
    } else {
        args.get(1)
            .map(PathBuf::from)
            .ok_or_else(|| invalid(usage()))?
    };
    let mut relative_to = std::env::current_dir()?;
    let mut max_bytes = DEFAULT_MAX_BYTES;
    let mut inputs = Vec::new();
    let mut index = if mode == Mode::Size { 1 } else { 2 };
    let mut positional = false;
    while index < args.len() {
        let arg = &args[index];
        if !positional && arg == "--" {
            positional = true;
        } else if !positional && arg == "--relative-to" {
            relative_to = PathBuf::from(argument(&args, &mut index, "--relative-to")?);
        } else if !positional && arg == "--max-bytes" {
            let value = argument(&args, &mut index, "--max-bytes")?;
            max_bytes = value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|&value| value > 0)
                .ok_or_else(|| invalid("--max-bytes requires a positive integer"))?;
        } else if !positional && arg == "--files-from" {
            let path = PathBuf::from(argument(&args, &mut index, "--files-from")?);
            let list = fs::read_to_string(&path)?;
            for line in list.trim_start_matches('\u{feff}').lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    inputs.push(PathBuf::from(line));
                }
            }
        } else if !positional && arg.to_string_lossy().starts_with("--") {
            return Err(invalid(format!(
                "unknown option: {}",
                arg.to_string_lossy()
            )));
        } else {
            inputs.push(PathBuf::from(arg));
        }
        index += 1;
    }
    if inputs.is_empty() {
        return Err(invalid(
            "supply input files/directories or --files-from LIST",
        ));
    }
    let relative_to = fs::canonicalize(relative_to)?;
    if !relative_to.is_dir() {
        return Err(invalid("--relative-to must name a directory"));
    }
    Ok(Some(Config {
        mode,
        directory,
        relative_to,
        inputs,
        max_bytes,
    }))
}

fn jpeg_suffix(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|suffix| {
            ["jpg", "jpeg", "jpe"]
                .iter()
                .any(|wanted| suffix.eq_ignore_ascii_case(wanted))
        })
}

fn collect(
    path: &Path,
    explicit: bool,
    directories: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let path = fs::canonicalize(path)?;
    let metadata = fs::metadata(&path)?;
    if metadata.is_dir() {
        if directories.insert(path.clone()) {
            for entry in fs::read_dir(&path)? {
                collect(&entry?.path(), false, directories, files)?;
            }
        }
    } else if metadata.is_file() {
        if explicit || jpeg_suffix(&path) {
            files.push(path);
        }
    } else if explicit {
        return Err(invalid(format!("not a regular file: {}", path.display())));
    }
    Ok(())
}

fn relative_path(path: &Path, base: &Path) -> io::Result<PathBuf> {
    let path_parts: Vec<_> = path.components().collect();
    let base_parts: Vec<_> = base.components().collect();
    let common = path_parts
        .iter()
        .zip(&base_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0
        || path_parts[common..]
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
        || base_parts[common..]
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(format!(
            "cannot express {} relative to {}; use a base on the same volume",
            path.display(),
            base.display()
        )));
    }
    let mut relative = PathBuf::new();
    for _ in &base_parts[common..] {
        relative.push("..");
    }
    for part in &path_parts[common..] {
        relative.push(part.as_os_str());
    }
    Ok(relative)
}

struct Capture {
    output: BufWriter<File>,
    remaining: u64,
    position: u64,
}

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other(
                "snapshot byte cap exceeded; capture is incomplete (no completion manifest)",
            ));
        }
        let written = self.output.write(bytes)?;
        self.remaining -= written as u64;
        self.position += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

struct Compare {
    expected: BufReader<File>,
    position: u64,
}

impl Write for Compare {
    fn write(&mut self, actual: &[u8]) -> io::Result<usize> {
        let mut expected = [0u8; 8192];
        for chunk in actual.chunks(expected.len()) {
            let buffer = &mut expected[..chunk.len()];
            self.expected.read_exact(buffer).map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    different(format!(
                        "baseline ended before generated snapshot byte {}",
                        self.position + chunk.len() as u64
                    ))
                } else {
                    error
                }
            })?;
            if let Some(at) = buffer.iter().zip(chunk).position(|(a, b)| a != b) {
                return Err(different(format!(
                    "snapshot mismatch at byte {}: expected {:02x}, actual {:02x}",
                    self.position + at as u64,
                    buffer[at],
                    chunk[at]
                )));
            }
            self.position += chunk.len() as u64;
        }
        Ok(actual.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum Destination {
    Capture(Capture),
    Compare(Compare),
    Count(u64),
}

impl Destination {
    fn open(mode: Mode, path: &Path, remaining: u64) -> io::Result<Self> {
        Ok(match mode {
            Mode::Capture => Self::Capture(Capture {
                output: BufWriter::with_capacity(
                    IO_BUFFER,
                    OpenOptions::new().write(true).create_new(true).open(path)?,
                ),
                remaining,
                position: 0,
            }),
            Mode::Verify => Self::Compare(Compare {
                expected: BufReader::with_capacity(IO_BUFFER, File::open(path)?),
                position: 0,
            }),
            Mode::Size => Self::Count(0),
        })
    }
    fn finish(mut self) -> io::Result<u64> {
        self.flush()?;
        match self {
            Self::Capture(writer) => Ok(writer.position),
            Self::Count(position) => Ok(position),
            Self::Compare(mut writer) => {
                if writer.expected.read(&mut [0u8; 1])? != 0 {
                    return Err(different(format!(
                        "baseline has trailing bytes after generated snapshot byte {}",
                        writer.position
                    )));
                }
                Ok(writer.position)
            }
        }
    }
}

impl Write for Destination {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Capture(writer) => writer.write(bytes),
            Self::Compare(writer) => writer.write(bytes),
            Self::Count(position) => {
                *position = position
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| invalid("snapshot size overflow"))?;
                Ok(bytes.len())
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Capture(writer) => writer.flush(),
            Self::Compare(writer) => writer.flush(),
            Self::Count(_) => Ok(()),
        }
    }
}

struct Encoder<'a> {
    output: &'a mut dyn Write,
}

impl Encoder<'_> {
    fn raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.output.write_all(bytes)
    }
    fn byte(&mut self, value: u8) -> io::Result<()> {
        self.raw(&[value])
    }
    fn u16(&mut self, value: u16) -> io::Result<()> {
        self.raw(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> io::Result<()> {
        self.raw(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> io::Result<()> {
        self.raw(&value.to_le_bytes())
    }
    fn len(&mut self, value: usize) -> io::Result<()> {
        self.u64(value as u64)
    }
    fn blob(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.len(bytes.len())?;
        self.raw(bytes)
    }
    fn text(&mut self, text: &str) -> io::Result<()> {
        self.blob(text.as_bytes())
    }
    fn variable(&mut self, mut value: u64) -> io::Result<()> {
        while value >= 128 {
            self.byte((value as u8 & 127) | 128)?;
            value >>= 7;
        }
        self.byte(value as u8)
    }
    // Numeric arrays use zero/literal runs. The low run-length bit is one for
    // literals, whose values are fixed little-endian u16s. This is lossless;
    // signed i16 coefficients are supplied in their two's-complement encoding.
    fn words(&mut self, length: usize, values: impl IntoIterator<Item = u16>) -> io::Result<()> {
        self.len(length)?;
        let mut values = values.into_iter().peekable();
        let mut literal = [0u8; 8192];
        while let Some(value) = values.next() {
            if value == 0 {
                let mut count = 1u64;
                while values.peek() == Some(&0) {
                    values.next();
                    count += 1;
                }
                self.variable(count << 1)?;
            } else {
                literal[..2].copy_from_slice(&value.to_le_bytes());
                let mut count = 1;
                while count < literal.len() / 2 && values.peek().is_some_and(|&value| value != 0) {
                    let value = values.next().expect("peeked numeric sample");
                    literal[count * 2..count * 2 + 2].copy_from_slice(&value.to_le_bytes());
                    count += 1;
                }
                self.variable((count as u64) << 1 | 1)?;
                self.raw(&literal[..count * 2])?;
            }
        }
        Ok(())
    }
    fn path(&mut self, path: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            self.byte(1)?;
            let units: Vec<_> = path.as_os_str().encode_wide().collect();
            self.len(units.len())?;
            for unit in units {
                self.u16(unit)?;
            }
            Ok(())
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            self.byte(2)?;
            self.blob(path.as_os_str().as_bytes())
        }
        #[cfg(not(any(windows, unix)))]
        {
            self.byte(3)?;
            self.blob(path.as_os_str().as_encoded_bytes())
        }
    }
}

fn error(out: &mut Encoder<'_>, error: jpeg::Error) -> io::Result<()> {
    match error {
        jpeg::Error::Truncated => out.byte(0),
        jpeg::Error::Invalid(message) => {
            out.byte(1)?;
            out.text(message)
        }
        jpeg::Error::Unsupported(message) => {
            out.byte(2)?;
            out.text(message)
        }
        jpeg::Error::LimitExceeded(message) => {
            out.byte(3)?;
            out.text(message)
        }
        jpeg::Error::Allocation => out.byte(4),
        jpeg::Error::Io(kind) => {
            out.byte(5)?;
            out.text(&format!("{kind:?}"))
        }
    }
}

fn result<T>(
    out: &mut Encoder<'_>,
    value: &jpeg::Result<T>,
    success: impl FnOnce(&mut Encoder<'_>, &T) -> io::Result<()>,
) -> io::Result<()> {
    match value {
        Ok(value) => {
            out.byte(1)?;
            success(out, value)
        }
        Err(value) => {
            out.byte(0)?;
            error(out, *value)
        }
    }
}

fn color(out: &mut Encoder<'_>, color: jpeg::ColorSpace) -> io::Result<()> {
    out.byte(match color {
        jpeg::ColorSpace::Grayscale => 0,
        jpeg::ColorSpace::YCbCr => 1,
        jpeg::ColorSpace::Rgb => 2,
        jpeg::ColorSpace::Cmyk => 3,
        jpeg::ColorSpace::Ycck => 4,
        jpeg::ColorSpace::Unknown => 5,
    })
}

fn polarity(out: &mut Encoder<'_>, value: jpeg::CmykPolarity) -> io::Result<()> {
    out.byte(match value {
        jpeg::CmykPolarity::Conventional => 0,
        jpeg::CmykPolarity::AdobeInverted => 1,
    })
}

fn spec(out: &mut Encoder<'_>, value: &jpeg::ComponentSpec) -> io::Result<()> {
    out.raw(&[
        value.id,
        value.horizontal_sampling,
        value.vertical_sampling,
        value.quantization_table,
    ])
}

fn specs(out: &mut Encoder<'_>, values: &[jpeg::ComponentSpec]) -> io::Result<()> {
    out.len(values.len())?;
    for value in values {
        spec(out, value)?;
    }
    Ok(())
}

fn frame(out: &mut Encoder<'_>, value: &jpeg::FrameHeader) -> io::Result<()> {
    out.byte(value.process as u8)?;
    out.byte(value.precision)?;
    out.u16(value.width)?;
    out.u16(value.height)?;
    specs(out, &value.components)
}

fn metadata(out: &mut Encoder<'_>, values: &[jpeg::MetadataSegment<'_>]) -> io::Result<()> {
    out.len(values.len())?;
    for value in values {
        out.byte(value.marker)?;
        out.len(value.offset)?;
        out.len(value.preceding_scans)?;
        out.blob(value.data)?;
    }
    Ok(())
}

fn coefficient_components(
    out: &mut Encoder<'_>,
    values: &[jpeg::CoefficientComponent],
) -> io::Result<()> {
    out.len(values.len())?;
    for value in values {
        spec(out, &value.spec)?;
        out.len(value.width)?;
        out.len(value.height)?;
        out.len(value.blocks_wide)?;
        out.len(value.blocks_high)?;
        out.words(value.quantization.len(), value.quantization.iter().copied())?;
        out.raw(&value.successive_approximation.map(|value| value as u8))?;
        out.len(value.blocks.len())?;
        out.words(
            value.blocks.len() * 64,
            value
                .blocks
                .iter()
                .flat_map(|block| block.iter().map(|&value| value as u16)),
        )?;
    }
    Ok(())
}

fn planes(out: &mut Encoder<'_>, values: &[jpeg::ComponentPlane]) -> io::Result<()> {
    out.len(values.len())?;
    for value in values {
        spec(out, &value.spec)?;
        out.len(value.width)?;
        out.len(value.height)?;
        out.words(value.samples.len(), value.samples.iter().copied())?;
    }
    Ok(())
}

fn coding(out: &mut Encoder<'_>, values: &[jpeg::LosslessCoding]) -> io::Result<()> {
    out.len(values.len())?;
    for value in values {
        out.raw(&[value.predictor, value.point_transform])?;
    }
    Ok(())
}

fn pixels(out: &mut Encoder<'_>, value: &Image) -> io::Result<()> {
    out.u32(value.width)?;
    out.u32(value.height)?;
    out.byte(value.color_type as u8)?;
    out.byte(value.bit_depth)?;
    match &value.samples {
        Samples::U8(values) => {
            out.byte(0)?;
            out.blob(values)
        }
        Samples::U16(values) => {
            out.byte(1)?;
            out.words(values.len(), values.iter().copied())
        }
    }
}

fn coefficients(out: &mut Encoder<'_>, value: &jpeg::CoefficientImage<'_>) -> io::Result<()> {
    frame(out, &value.frame)?;
    color(out, value.color_space())?;
    polarity(out, value.cmyk_polarity())?;
    out.len(value.scans)?;
    out.len(value.consumed)?;
    metadata(out, &value.metadata)?;
    coefficient_components(out, &value.components)
}

fn lossless(out: &mut Encoder<'_>, value: &jpeg::LosslessImage<'_>) -> io::Result<()> {
    frame(out, &value.frame)?;
    color(out, value.color_space())?;
    out.len(value.scans)?;
    out.len(value.consumed)?;
    metadata(out, &value.metadata)?;
    coding(out, &value.coding)?;
    planes(out, &value.components)
}

fn hierarchical(out: &mut Encoder<'_>, value: &jpeg::HierarchicalImage<'_>) -> io::Result<()> {
    out.byte(value.header.precision)?;
    out.u16(value.header.width)?;
    out.u16(value.header.height)?;
    specs(out, &value.header.components)?;
    color(out, value.color_space())?;
    out.len(value.scans)?;
    out.len(value.consumed)?;
    metadata(out, &value.metadata)?;
    out.len(value.frames.len())?;
    for value in &value.frames {
        frame(out, &value.frame)?;
        out.byte(u8::from(value.expansion.horizontal))?;
        out.byte(u8::from(value.expansion.vertical))?;
        out.len(value.offset)?;
        out.len(value.first_scan)?;
        out.len(value.scans)?;
        match &value.data {
            jpeg::HierarchicalData::Dct(values) => {
                out.byte(0)?;
                coefficient_components(out, values)?;
            }
            jpeg::HierarchicalData::Lossless {
                components,
                coding: values,
            } => {
                out.byte(1)?;
                coding(out, values)?;
                planes(out, components)?;
            }
        }
    }
    Ok(())
}

fn pixel_calls(out: &mut Encoder<'_>, input: &[u8]) -> io::Result<()> {
    // Keep at most the two distinct strict results. Later equal results use
    // references only after exact Image/Error equality, never hashes.
    let mut previous: Vec<(u8, jpeg::Result<Image>)> = Vec::new();
    let mut ordinal = 0;
    for strict in [true, false] {
        for (method_id, method) in [
            (0, jpeg::Upsampling::Nearest),
            (1, jpeg::Upsampling::Linear),
        ] {
            out.text("pixels")?;
            out.byte(u8::from(strict))?;
            out.byte(method_id)?;
            let actual = jpeg::decode_with_options(
                input,
                jpeg::DecodeOptions {
                    strict,
                    ..Default::default()
                },
                method,
            );
            if let Some((earlier, _)) = previous.iter().find(|(_, value)| value == &actual) {
                out.byte(1)?;
                out.byte(*earlier)?;
            } else {
                out.byte(0)?;
                result(out, &actual, pixels)?;
                if strict {
                    previous.push((ordinal, actual));
                }
            }
            ordinal += 1;
        }
    }
    Ok(())
}

fn same_metadata(a: &[jpeg::MetadataSegment<'_>], b: &[jpeg::MetadataSegment<'_>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.marker == b.marker
                && a.offset == b.offset
                && a.preceding_scans == b.preceding_scans
                && a.data == b.data
        })
}

fn same_coefficients(a: &[jpeg::CoefficientComponent], b: &[jpeg::CoefficientComponent]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.spec == b.spec
                && a.width == b.width
                && a.height == b.height
                && a.blocks_wide == b.blocks_wide
                && a.blocks_high == b.blocks_high
                && a.quantization == b.quantization
                && a.successive_approximation == b.successive_approximation
                && a.blocks == b.blocks
        })
}

fn same_planes(a: &[jpeg::ComponentPlane], b: &[jpeg::ComponentPlane]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.spec == b.spec && a.width == b.width && a.height == b.height && a.samples == b.samples
        })
}

fn same_coefficient_image(a: &jpeg::CoefficientImage<'_>, b: &jpeg::CoefficientImage<'_>) -> bool {
    a.frame == b.frame
        && a.scans == b.scans
        && a.consumed == b.consumed
        && a.color_space() == b.color_space()
        && a.cmyk_polarity() == b.cmyk_polarity()
        && same_metadata(&a.metadata, &b.metadata)
        && same_coefficients(&a.components, &b.components)
}

fn same_lossless_image(a: &jpeg::LosslessImage<'_>, b: &jpeg::LosslessImage<'_>) -> bool {
    a.frame == b.frame
        && a.scans == b.scans
        && a.consumed == b.consumed
        && a.color_space() == b.color_space()
        && a.coding == b.coding
        && same_metadata(&a.metadata, &b.metadata)
        && same_planes(&a.components, &b.components)
}

fn same_hierarchy(a: &jpeg::HierarchicalImage<'_>, b: &jpeg::HierarchicalImage<'_>) -> bool {
    a.header == b.header
        && a.scans == b.scans
        && a.consumed == b.consumed
        && a.color_space() == b.color_space()
        && same_metadata(&a.metadata, &b.metadata)
        && a.frames.len() == b.frames.len()
        && a.frames.iter().zip(&b.frames).all(|(a, b)| {
            a.frame == b.frame
                && a.expansion == b.expansion
                && a.offset == b.offset
                && a.first_scan == b.first_scan
                && a.scans == b.scans
                && match (&a.data, &b.data) {
                    (jpeg::HierarchicalData::Dct(a), jpeg::HierarchicalData::Dct(b)) => {
                        same_coefficients(a, b)
                    }
                    (
                        jpeg::HierarchicalData::Lossless {
                            components: a,
                            coding: ac,
                        },
                        jpeg::HierarchicalData::Lossless {
                            components: b,
                            coding: bc,
                        },
                    ) => ac == bc && same_planes(a, b),
                    _ => false,
                }
        })
}

fn policy_pair<'a, T>(
    out: &mut Encoder<'_>,
    input: &'a [u8],
    label: &str,
    decode: impl Fn(&'a [u8], jpeg::DecodeOptions) -> jpeg::Result<T>,
    success: impl Fn(&mut Encoder<'_>, &T) -> io::Result<()>,
    equal: impl Fn(&T, &T) -> bool,
) -> io::Result<()> {
    let strict = decode(input, jpeg::DecodeOptions::default());
    out.text(label)?;
    out.byte(1)?;
    out.byte(0)?; // Full strict result.
    result(out, &strict, |out, value| success(out, value))?;
    let compatible = decode(
        input,
        jpeg::DecodeOptions {
            strict: false,
            ..Default::default()
        },
    );
    out.text(label)?;
    out.byte(0)?;
    let identical = match (&strict, &compatible) {
        (Ok(a), Ok(b)) => equal(a, b),
        (Err(a), Err(b)) => a == b,
        _ => false,
    };
    if identical {
        out.byte(1)?; // Exact reference to this API's strict result.
        out.byte(0)
    } else {
        out.byte(0)?;
        result(out, &compatible, |out, value| success(out, value))
    }
}

fn snapshot(
    out: &mut Encoder<'_>,
    ordinal: usize,
    count: usize,
    relative: &Path,
    input: &[u8],
) -> io::Result<()> {
    out.raw(SNAPSHOT_MAGIC)?;
    out.len(ordinal)?;
    out.len(count)?;
    out.path(relative)?;
    // Exact original bytes make input mutation detectable even if every decoder
    // returns the same error or all pixels happen to remain unchanged.
    out.blob(input)?;
    let limits = jpeg::Limits::default();
    out.len(limits.max_pixels)?;
    out.len(limits.max_memory_bytes)?;
    out.len(limits.max_metadata_bytes)?;
    out.len(limits.max_scans)?;
    pixel_calls(out, input)?;
    policy_pair(
        out,
        input,
        "coefficients",
        jpeg::decode_coefficients_with_options,
        coefficients,
        same_coefficient_image,
    )?;
    policy_pair(
        out,
        input,
        "lossless",
        jpeg::decode_lossless_with_options,
        lossless,
        same_lossless_image,
    )?;
    policy_pair(
        out,
        input,
        "hierarchical",
        jpeg::decode_hierarchical_with_options,
        hierarchical,
        same_hierarchy,
    )?;
    out.text("complete")
}

fn contextual(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn run() -> io::Result<()> {
    let Some(config) = config()? else {
        return Ok(());
    };
    let mut files = Vec::new();
    let mut directories = HashSet::new();
    for input in &config.inputs {
        collect(input, true, &mut directories, &mut files)?;
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err(invalid("no JPEG input files found"));
    }
    let relative: Vec<_> = files
        .iter()
        .map(|path| relative_path(path, &config.relative_to))
        .collect::<io::Result<_>>()?;
    let manifest = config.directory.join("manifest.bin");
    if config.mode == Mode::Capture {
        if let Some(parent) = config
            .directory
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        // Exclusive directory creation also rejects an empty existing directory.
        fs::create_dir(&config.directory).map_err(|error| contextual(&config.directory, error))?;
    } else if config.mode == Mode::Verify {
        // The manifest is written last. Its absence always rejects an incomplete
        // capture, even when a partial ordinal snapshot happens to compare.
        File::open(&manifest).map_err(|error| contextual(&manifest, error))?;
    }
    let mut total = 0u64;
    let mut lengths = Vec::with_capacity(files.len());
    for (ordinal, (path, relative)) in files.iter().zip(&relative).enumerate() {
        let name = config.directory.join(format!("{ordinal:06}.snap"));
        let input = fs::read(path)?;
        let mut destination =
            Destination::open(config.mode, &name, config.max_bytes.saturating_sub(total))
                .map_err(|error| contextual(&name, error))?;
        snapshot(
            &mut Encoder {
                output: &mut destination,
            },
            ordinal,
            files.len(),
            relative,
            &input,
        )
        .map_err(|error| contextual(relative, error))?;
        let length = destination
            .finish()
            .map_err(|error| contextual(relative, error))?;
        lengths.push(length);
        total = total
            .checked_add(length)
            .ok_or_else(|| invalid("snapshot size overflow"))?;
        println!(
            "{}\t{ordinal:06}\t{}\t{}\t{length}",
            match config.mode {
                Mode::Capture => "CAPTURED",
                Mode::Verify => "VERIFIED",
                Mode::Size => "SIZED",
            },
            relative.display(),
            input.len()
        );
    }
    let mut destination = Destination::open(
        config.mode,
        &manifest,
        config.max_bytes.saturating_sub(total),
    )
    .map_err(|error| contextual(&manifest, error))?;
    let mut out = Encoder {
        output: &mut destination,
    };
    out.raw(MANIFEST_MAGIC)?;
    out.len(files.len())?;
    for (path, length) in relative.iter().zip(lengths) {
        out.path(path)?;
        out.u64(length)?;
    }
    out.text("complete")?;
    let length = destination
        .finish()
        .map_err(|error| contextual(&manifest, error))?;
    total = total
        .checked_add(length)
        .ok_or_else(|| invalid("snapshot size overflow"))?;
    println!(
        "COMPLETE\t{}\t{}\t{total}",
        match config.mode {
            Mode::Capture => "capture",
            Mode::Verify => "verify",
            Mode::Size => "size",
        },
        files.len()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native_snapshot: {error}");
        std::process::exit(1);
    }
}
