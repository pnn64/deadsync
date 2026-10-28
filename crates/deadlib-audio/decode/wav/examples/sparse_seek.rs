//! Physical sparse-file seek/size validation and isolated process memory probe.
//! This is validation infrastructure, not a production platform dependency.
use deadlib_audio_decode_wav::{
    AdpcmKind, AdpcmReader, AdpcmSpec, AdpcmWriter, Container, Reader, SampleFormat, Spec,
    WriteOptions, Writer,
};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

#[cfg(windows)]
mod platform {
    use std::{
        ffi::c_void,
        fs::File,
        io,
        os::windows::{fs::MetadataExt, io::AsRawHandle},
    };
    #[repr(C)]
    #[derive(Default)]
    struct Standard {
        allocated: i64,
        end: i64,
        links: u32,
        delete_pending: u8,
        directory: u8,
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct Memory {
        cb: u32,
        faults: u32,
        pub peak: usize,
        pub working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        commit: usize,
        pub peak_commit: usize,
        pub private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            handle: *mut c_void,
            code: u32,
            input: *mut c_void,
            input_len: u32,
            output: *mut c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetFileInformationByHandleEx(
            handle: *mut c_void,
            class: i32,
            info: *mut c_void,
            size: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(process: *mut c_void, counters: *mut Memory, size: u32) -> i32;
    }
    pub fn sparse(file: &File) -> io::Result<()> {
        let mut returned = 0;
        // SAFETY: live synchronous file handle. FSCTL_SET_SPARSE accepts null
        // input to set the sparse flag, no output, and a writable returned count.
        if unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                0x000900c4,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn allocated(file: &File) -> io::Result<u64> {
        let mut info = Standard::default();
        // SAFETY: FileStandardInfo=1 writes FILE_STANDARD_INFO into this C-layout
        // buffer with the exact passed size while the file handle remains live.
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                1,
                (&mut info as *mut Standard).cast(),
                std::mem::size_of::<Standard>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        assert!(info.allocated >= 0);
        assert_eq!(info.end as u64, file.metadata()?.len());
        assert_ne!(
            file.metadata()?.file_attributes() & 0x200,
            0,
            "sparse attribute is required"
        );
        Ok(info.allocated as u64)
    }
    pub fn memory() -> io::Result<Memory> {
        let mut info = Memory {
            cb: std::mem::size_of::<Memory>() as u32,
            ..Memory::default()
        };
        // SAFETY: current-process pseudo-handle and a fully sized writable
        // PROCESS_MEMORY_COUNTERS_EX buffer, used only during this call.
        if unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut info,
                std::mem::size_of::<Memory>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }
}
#[cfg(not(windows))]
mod platform {
    use std::{fs::File, io};
    pub struct Memory {
        pub peak: usize,
        pub working: usize,
        pub private: usize,
        pub peak_commit: usize,
    }
    pub fn sparse(_: &File) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this probe uses Windows sparse and process-memory APIs",
        ))
    }
    pub fn allocated(_: &File) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub fn memory() -> io::Result<Memory> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

fn positions(frames: u64, offset: u64) -> Vec<u64> {
    let mut p = vec![
        0,
        1,
        (1 << 30) - 1,
        1 << 30,
        (1 << 32) - 1,
        1 << 32,
        frames - 1,
    ];
    let boundary = ((1u64 << 32) - offset) / 4;
    p.extend([boundary - 1, boundary]);
    p.retain(|&v| v < frames);
    p.sort_unstable();
    p.dedup();
    p
}
fn value(frame: u64) -> i32 {
    ((frame ^ (frame >> 32) ^ frame.rotate_right(29)) as u32)
        .wrapping_mul(1664525)
        .wrapping_add(1013904223) as i32
}
fn options(container: Container) -> WriteOptions {
    WriteOptions {
        container,
        auto_rf64: false,
        force_extensible: true,
    }
}
fn create(
    path: &Path,
    container: Container,
    ima: bool,
    frames: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(path)?;
    platform::sparse(&file)?;
    // Known headers are final up front. The fixture builder fills sparse zero
    // extents directly; it deliberately does not claim Writer accepted holes.
    let mut file = if ima {
        AdpcmWriter::with_frames(
            file,
            AdpcmSpec {
                kind: AdpcmKind::Ima,
                channels: 1,
                sample_rate: 8000,
                block_bytes: 4,
            },
            options(container),
            &[],
            frames,
        )?
        .into_inner()
    } else {
        Writer::with_frames(
            file,
            Spec::new(SampleFormat::I32, 1, 48000),
            options(container),
            &[],
            frames,
        )?
        .into_inner()
    };
    let offset = file.stream_position()?;
    let end = offset
        .checked_add(frames.checked_mul(4).ok_or("data overflow")?)
        .ok_or("file overflow")?;
    file.set_len(end)?;
    for frame in positions(frames, offset) {
        file.seek(SeekFrom::Start(offset + frame * 4))?;
        if ima {
            file.write_all(&(value(frame) as i16).to_le_bytes())?;
            file.write_all(&[0, 0])?;
        } else {
            file.write_all(&value(frame).to_le_bytes())?;
        }
    }
    file.sync_all()?;
    assert!(
        platform::allocated(&file)? < 1 << 20,
        "sparse fixture unexpectedly allocated over 1 MiB"
    );
    Ok(offset)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 6 {
        return Err("sparse_seek create|verify FILE rf64|bw64 pcm|ima FRAMES".into());
    }
    let initial = platform::memory()?;
    let container = match args[3].as_str() {
        "rf64" => Container::Rf64,
        "bw64" => Container::Bw64,
        _ => return Err("container".into()),
    };
    let ima = match args[4].as_str() {
        "ima" => true,
        "pcm" => false,
        _ => return Err("codec".into()),
    };
    let frames = args[5].parse::<u64>()?;
    if frames < 64 {
        return Err("at least 64 frames required".into());
    }
    let path = Path::new(&args[2]);
    match args[1].as_str() {
        "create" => {
            create(path, container, ima, frames)?;
        }
        "verify" => {}
        _ => return Err("mode".into()),
    }
    let raw = File::open(path)?;
    let file_bytes = raw.metadata()?.len();
    let allocated = platform::allocated(&raw)?;
    drop(raw);
    let before = platform::memory()?;
    let start = Instant::now();
    let mut samples = 0;
    let offset;
    if ima {
        let mut reader = AdpcmReader::new(File::open(path)?)?;
        assert_eq!(reader.container(), container);
        assert_eq!(reader.frames(), frames);
        offset = reader
            .chunks()
            .iter()
            .find(|c| c.id == *b"data")
            .unwrap()
            .offset;
        for frame in positions(frames, offset) {
            reader.seek_frame(frame)?;
            let mut out = [0i16; 1];
            assert_eq!(reader.read_i16(&mut out)?, 1);
            assert_eq!(out[0], value(frame) as i16);
            assert_eq!(reader.position(), frame + 1);
            samples += 1;
        }
        reader.seek_frame(42)?;
        let mut zero = [1];
        assert_eq!(reader.read_i16(&mut zero)?, 1);
        assert_eq!(zero, [0]);
        reader.seek_frame(frames)?;
        assert_eq!(reader.read_i16(&mut zero)?, 0);
        assert!(reader.seek_frame(frames + 1).is_err());
        assert_eq!(reader.position(), frames);
    } else {
        let mut reader = Reader::new(File::open(path)?)?;
        assert_eq!(reader.container(), container);
        assert_eq!(reader.frames()?, frames);
        offset = reader.data_chunk().offset;
        for frame in positions(frames, offset) {
            reader.seek_frame(frame)?;
            let mut out = [0i32; 1];
            assert_eq!(reader.read_i32(&mut out)?, 1);
            assert_eq!(out[0], value(frame));
            assert_eq!(reader.position(), frame + 1);
            samples += 1;
        }
        reader.seek_frame(42)?;
        let mut zero = [1];
        assert_eq!(reader.read_i32(&mut zero)?, 1);
        assert_eq!(zero, [0]);
        reader.seek_frame(frames)?;
        assert_eq!(reader.read_i32(&mut zero)?, 0);
        assert!(reader.seek_frame(frames + 1).is_err());
        assert_eq!(reader.position(), frames);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let after = platform::memory()?;
    assert_eq!(file_bytes, offset + frames * 4);
    println!(
        "{{\"pointer_bits\":{},\"frames\":{frames},\"file_bytes\":{file_bytes},\"allocated_bytes\":{allocated},\"data_offset\":{offset},\"marker_samples\":{samples},\"decode_probe_seconds\":{elapsed},\"initial_working_set\":{},\"initial_peak_working_set\":{},\"before_decode_working_set\":{},\"before_decode_peak_working_set\":{},\"after_decode_working_set\":{},\"peak_working_set\":{},\"private_commit\":{},\"peak_commit\":{}}}",
        usize::BITS,
        initial.working,
        initial.peak,
        before.working,
        before.peak,
        after.working,
        after.peak,
        after.private,
        after.peak_commit
    );
    Ok(())
}
