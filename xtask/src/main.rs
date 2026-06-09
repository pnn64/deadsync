//! deadsync developer task runner.
//!
//! Currently provides a single task, `hot-watch`, a cross-platform Rust port of
//! the old `scripts/hot-watch.ps1`. It gives subsecond hot-reload iteration for
//! the `deadsync-screens` cdylib by bypassing cargo's per-edit workspace
//! fingerprinting (~0.9s): it captures cargo's exact `rustc` invocation for the
//! cdylib ONCE, then re-runs that command directly on every edit and republishes
//! the freshly linked dynamic library where the running host's reloader watches.
//!
//! Measured on Windows (warm target, edit src/screens/menu/render.rs):
//!     cargo build -p deadsync-screens --profile hot   ~1.99s / edit
//!     rustc-direct + rust-lld (this tool)             ~0.74s / edit   (~2.7x)
//!
//! Why this is sound: BUILD_HASH / LAYOUT_HASH are baked into the ENGINE RLIB
//! (src/hot reads them via env! at rlib-compile time from build.rs). The cdylib
//! only references those consts, so re-linking it against the SAME rlib
//! reproduces the exact handshake the host expects. The handshake folds in
//! `-C prefer-dynamic` (DEADSYNC_SHARED_ALLOC), so the HOST and this loop MUST
//! use identical RUSTFLAGS or the host rejects the cdylib at load. The linker
//! choice is NOT part of the hash, so swapping in rust-lld is safe.
//!
//! Only the two cdylib sources are watched (crates/deadsync-screens/src/lib.rs
//! and the #[path]-included src/screens/menu/render.rs). Editing ENGINE code
//! changes the rlib (and the statically-linked host); this fast loop does NOT
//! rebuild that -- restart the host with a normal `cargo run` for engine edits.
//!
//! Usage:
//!     # Terminal A (host) -- SAME RUSTFLAGS as the watcher:
//!     RUSTFLAGS="-C prefer-dynamic" cargo run --profile hot --bin deadsync --features hot
//!     # Terminal B (this watcher):
//!     cargo xtask hot-watch

use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "\
usage: cargo xtask hot-watch [options]

options:
  --profile <name>     cargo profile to build (default: hot)
  --rustflags <flags>  RUSTFLAGS for the build; MUST match the host
                       (default: \"-C prefer-dynamic\")
  --crate <name>       cdylib crate to watch (default: deadsync-screens)
  --no-lld             use the default linker instead of bundled rust-lld
  --poll-ms <n>        file poll interval in ms (default: 120)
  --once               build/publish once and exit (no watch loop)
  --root <dir>         workspace root (default: auto-detected via cargo)
  -h, --help           show this help";

struct Args {
    profile: String,
    rustflags: String,
    krate: String,
    no_lld: bool,
    poll_ms: u64,
    once: bool,
    root: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            profile: "hot".into(),
            rustflags: "-C prefer-dynamic".into(),
            krate: "deadsync-screens".into(),
            no_lld: false,
            poll_ms: 120,
            once: false,
            root: None,
        }
    }
}

fn main() {
    init_term();
    let mut it = std::env::args().skip(1);
    let sub = it.next().unwrap_or_default();
    match sub.as_str() {
        "hot-watch" => {}
        "-h" | "--help" | "" => {
            println!("{USAGE}");
            return;
        }
        other => {
            eprintln!("unknown task: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }

    let mut a = Args::default();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--profile" => a.profile = require(&mut it, "--profile"),
            "--rustflags" => a.rustflags = require(&mut it, "--rustflags"),
            "--crate" => a.krate = require(&mut it, "--crate"),
            "--no-lld" => a.no_lld = true,
            "--once" => a.once = true,
            "--poll-ms" => {
                a.poll_ms = require(&mut it, "--poll-ms")
                    .parse()
                    .unwrap_or_else(|_| fail("--poll-ms expects an integer"))
            }
            "--root" => a.root = Some(PathBuf::from(require(&mut it, "--root"))),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            other => fail(&format!("unknown argument: {other}\n\n{USAGE}")),
        }
    }

    if let Err(e) = run(a) {
        eprintln!("\nerror: {e}");
        std::process::exit(1);
    }
}

fn require(it: &mut impl Iterator<Item = String>, flag: &str) -> String {
    it.next()
        .unwrap_or_else(|| fail(&format!("{flag} requires a value")))
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

// --- terminal styling (pure std, no deps) ----------------------------------

/// Prepare the terminal: on Windows, enable ANSI escape processing so the colors
/// and the in-place spinner render in classic consoles (Windows Terminal already
/// supports them, but conhost needs the mode flag set explicitly).
fn init_term() {
    #[cfg(windows)]
    enable_vt();
}

/// Whether to emit ANSI color/control codes. Disabled when output is redirected,
/// when `NO_COLOR` is set, or for `TERM=dumb`. Computed once.
fn color() -> bool {
    static COLOR: OnceLock<bool> = OnceLock::new();
    *COLOR.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
            return false;
        }
        std::io::stdout().is_terminal()
    })
}

fn paint(code: &str, s: &str) -> String {
    if color() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn bold(s: &str) -> String {
    paint("1", s)
}
fn dim(s: &str) -> String {
    paint("2", s)
}
fn red(s: &str) -> String {
    paint("31", s)
}
fn green(s: &str) -> String {
    paint("32", s)
}
fn yellow(s: &str) -> String {
    paint("33", s)
}
fn cyan(s: &str) -> String {
    paint("36", s)
}

/// Print an aligned `  label    value` line (label dimmed). An empty label emits
/// the leading padding only, to hang a continuation under a previous label.
fn kv(label: &str, value: &str) {
    println!("  {} {value}", dim(&format!("{label:<9}")));
}

/// The `#N` reload counter badge.
fn tag_label(tag: u32) -> String {
    bold(&format!("#{tag}"))
}

#[cfg(windows)]
fn enable_vt() {
    use std::os::raw::c_void;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn GetConsoleMode(h: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut c_void, mode: u32) -> i32;
    }
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h.is_null() || h as isize == -1 {
            return;
        }
        let mut mode = 0u32;
        if GetConsoleMode(h, &mut mode) == 0 {
            return;
        }
        SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }
}

fn run(a: Args) -> Result<(), String> {
    let root = match &a.root {
        Some(r) => r.clone(),
        None => workspace_root()?,
    };
    let crate_us = a.krate.replace('-', "_");
    let render = root.join("src").join("screens").join("menu").join("render.rs");
    let cdylib_lib = root.join("crates").join(&a.krate).join("src").join("lib.rs");
    let watch: Vec<PathBuf> = [render.clone(), cdylib_lib.clone()]
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    if watch.is_empty() {
        return Err(format!(
            "no watch targets exist under {} -- is --root correct?",
            root.display()
        ));
    }

    println!();
    println!("  {}", bold(&cyan("deadsync hot-watch")));
    kv("workspace", &dim(&root.display().to_string()));
    kv("profile", &cyan(&a.profile));
    kv(
        "rustflags",
        &format!("{}   {}", yellow(&a.rustflags), dim("(the host must use the SAME)")),
    );

    // 1. Capture cargo's exact rustc command for the cdylib (one cargo build).
    let mut cmd = capture_rustc_cmd(&root, &a, &crate_us, &render)?;

    // 2. Fail closed if the scraped command doesn't look like the cdylib build.
    for needle in [
        "--crate-type cdylib",
        &format!("--crate-name {crate_us}"),
        "--out-dir",
        "--extern deadsync=",
    ] {
        if !cmd.contains(needle) {
            return Err(format!(
                "captured rustc command is missing '{needle}' -- refusing to run a wrong/partial build"
            ));
        }
    }
    if cmd.len() > 7000 {
        return Err(format!(
            "captured rustc command is {} chars; too close to the cmd.exe limit -- aborting",
            cmd.len()
        ));
    }

    // 3. Resolve output locations. The host watches the TOP-LEVEL artifact at
    //    target/<profile>/<lib>, while rustc writes only into deps/.
    let out_dir = PathBuf::from(
        extract_value(&cmd, "--out-dir").ok_or("no --out-dir value in captured command")?,
    );
    let profile_dir = out_dir
        .parent()
        .ok_or("--out-dir has no parent")?
        .to_path_buf();
    let lib_name = format!("{DLL_PREFIX}{crate_us}{DLL_SUFFIX}");
    let deps_dll = out_dir.join(&lib_name);
    let top_dll = profile_dir.join(&lib_name);

    // 4. Snapshot the engine rlib. If a concurrent host `cargo build` rebuilds
    //    it, our direct relink would link a NEWER engine than the running host
    //    statically contains -> a silently incompatible cdylib. We abort instead.
    let rlib = extract_value(&cmd, "--extern deadsync=").map(PathBuf::from);
    let rlib_snap = rlib.as_ref().and_then(|p| snapshot(p));

    // 5. Redirect incremental to a private dir so we never race Cargo's own
    //    incremental cache (the source of "Access is denied (os error 5)").
    let priv_inc = profile_dir.join("xtask-incremental");
    fs::create_dir_all(&priv_inc).ok();
    cmd = replace_value(&cmd, "-C incremental=", &priv_inc.to_string_lossy());

    // 6. Make rustc emit human-readable errors (cargo asked for JSON).
    cmd = cmd.replace("--error-format=json", "--error-format=human");
    cmd = remove_kv(&cmd, "--json=");

    // 7. Optionally swap in rust-lld (Windows: via an lld-link.exe shim). On
    //    other platforms we keep whatever linker the captured command/config
    //    already specifies (devs there usually configure lld/mold themselves).
    if cfg!(windows) && !a.no_lld {
        match resolve_lld_link(&root) {
            Ok(lld) => {
                cmd = format!("{cmd} -C linker={}", shell_quote(&lld.display().to_string()));
                kv("linker", &format!("rust-lld {}", dim(&format!("({})", lld.display()))));
            }
            Err(e) => kv("linker", &dim(&format!("default (rust-lld unavailable: {e})"))),
        }
    } else {
        kv("linker", &dim("default (from captured command / cargo config)"));
    }

    if let Some(p) = &rlib {
        kv("rlib", &dim(&p.display().to_string()));
    }
    let mut first = true;
    for p in &watch {
        kv(if first { "watching" } else { "" }, &dim(&p.display().to_string()));
        first = false;
    }
    println!();
    println!(
        "  {}",
        dim("Start the host in another terminal with the SAME RUSTFLAGS:")
    );
    if cfg!(windows) {
        println!("      {}", green(&format!("$env:RUSTFLAGS = \"{}\"", a.rustflags)));
        println!(
            "      {}",
            green(&format!(
                "cargo run --profile {} --bin deadsync --features hot",
                a.profile
            ))
        );
    } else {
        println!(
            "      {}",
            green(&format!(
                "RUSTFLAGS=\"{}\" cargo run --profile {} --bin deadsync --features hot",
                a.rustflags, a.profile
            ))
        );
    }
    println!();

    let mut ctx = Ctx {
        root,
        krate: a.krate,
        crate_us,
        rustflags: a.rustflags,
        cmd,
        rlib,
        rlib_snap,
        deps_dll,
        top_dll,
        count: 0,
    };

    // Prime once so the host's first load matches this loop's output.
    ctx.relink();

    if a.once {
        return Ok(());
    }

    // Robust polling watcher (no FileSystemWatcher event plumbing).
    let mut last: Vec<(PathBuf, Option<SystemTime>)> =
        watch.iter().map(|p| (p.clone(), mtime(p))).collect();
    println!("  {}", dim("watching for edits — Ctrl-C to stop"));
    loop {
        sleep(Duration::from_millis(a.poll_ms));
        let mut changed = false;
        for (p, t) in last.iter_mut() {
            let now = mtime(p);
            if now != *t {
                *t = now;
                changed = true;
            }
        }
        if changed {
            sleep(Duration::from_millis(40)); // let the editor finish writing
            for (p, t) in last.iter_mut() {
                *t = mtime(p);
            }
            ctx.relink();
        }
    }
}

struct Ctx {
    root: PathBuf,
    krate: String,
    crate_us: String,
    rustflags: String,
    cmd: String,
    rlib: Option<PathBuf>,
    rlib_snap: Option<(u64, SystemTime)>,
    deps_dll: PathBuf,
    top_dll: PathBuf,
    count: u32,
}

impl Ctx {
    fn relink(&mut self) {
        self.count += 1;
        let tag = self.count;

        // Guard: the engine rlib must be byte-identical to the one the running
        // host statically links. If it changed, abort -- linking a newer engine
        // into the cdylib than the host contains is UB.
        if let (Some(p), Some(snap)) = (&self.rlib, &self.rlib_snap) {
            if snapshot(p).as_ref() != Some(snap) {
                println!(
                    "  {} {}",
                    tag_label(tag),
                    yellow("⚠ engine rlib changed — restart host + watcher (stale-host hazard)")
                );
                return;
            }
        }

        let start = Instant::now();
        let result = self.run_with_progress(tag);
        let dt = start.elapsed().as_secs_f64();

        match result {
            Ok((true, _)) => {
                if let Err(e) = self.publish() {
                    println!(
                        "  {} {} {}",
                        tag_label(tag),
                        red("✗ relinked but publish failed"),
                        dim(&format!("({dt:.2}s): {e}"))
                    );
                    return;
                }
                println!(
                    "  {} {} {}",
                    tag_label(tag),
                    green("✓ reloaded"),
                    dim(&format!("{dt:.2}s"))
                );
            }
            Ok((false, output)) => {
                println!(
                    "  {} {} {}",
                    tag_label(tag),
                    red("✗ build failed"),
                    dim(&format!("({dt:.2}s)"))
                );
                for line in tail(&output, 30).lines() {
                    println!("     {}", dim(line));
                }
            }
            Err(e) => println!(
                "  {} {} {}",
                tag_label(tag),
                red("✗ build error"),
                dim(&format!("({dt:.2}s): {e}"))
            ),
        }
    }

    /// Run the captured build while animating an in-place spinner with elapsed
    /// time. When output is not a terminal, fall back to a single static line so
    /// piped logs stay clean (no carriage-return churn).
    fn run_with_progress(&self, tag: u32) -> io::Result<(bool, String)> {
        if !color() {
            println!("  {} building…", tag_label(tag));
            return self.run_captured();
        }
        let done = AtomicBool::new(false);
        let out = std::thread::scope(|s| {
            s.spawn(|| {
                const FRAMES: [&str; 10] =
                    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let start = Instant::now();
                let mut i = 0usize;
                while !done.load(Ordering::Relaxed) {
                    print!(
                        "\r  {} {} building {}",
                        tag_label(tag),
                        cyan(FRAMES[i % FRAMES.len()]),
                        dim(&format!("{:.1}s", start.elapsed().as_secs_f64()))
                    );
                    let _ = io::stdout().flush();
                    i += 1;
                    sleep(Duration::from_millis(80));
                }
            });
            let r = self.run_captured();
            done.store(true, Ordering::Relaxed);
            r
        });
        // Erase the spinner line so the result line starts clean.
        print!("\r\x1b[2K");
        let _ = io::stdout().flush();
        out
    }

    /// Re-run the captured rustc command via the platform shell. Using a shell
    /// (verbatim, the same way `cargo -v` would invoke it) avoids re-tokenizing
    /// cargo's shell-quoted command line ourselves. Returns (success, output).
    fn run_captured(&self) -> io::Result<(bool, String)> {
        let mut c = shell_command(&self.cmd);
        c.current_dir(&self.root)
            // RUSTFLAGS is already baked into the captured args; set it anyway so
            // any wrapper (sccache) hashes identically. The CARGO_* vars mirror
            // what cargo would set, guarding any future env!/option_env! use.
            .env("RUSTFLAGS", &self.rustflags)
            .env("CARGO_MANIFEST_DIR", self.root.join("crates").join(&self.krate))
            .env("CARGO_PKG_NAME", &self.krate)
            .env("CARGO_CRATE_NAME", &self.crate_us)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let out = c.output()?;
        let mut s = String::new();
        s.push_str(&String::from_utf8_lossy(&out.stdout));
        s.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok((out.status.success(), s))
    }

    /// Atomic publish: copy deps/<lib> to a temp sibling of the watched path,
    /// then rename over it so the host's poll never sees a half-written file.
    fn publish(&self) -> io::Result<()> {
        let tmp = self.top_dll.with_extension("tmp");
        fs::copy(&self.deps_dll, &tmp)?;

        // Copy the PDB too (Windows debug symbols); absent elsewhere.
        let deps_pdb = self.deps_dll.with_extension("pdb");
        if deps_pdb.exists() {
            let _ = fs::copy(&deps_pdb, self.top_dll.with_extension("pdb"));
        }

        // Retry the rename: the host may hold a transient handle mid shadow-copy.
        for attempt in 0..5 {
            match fs::rename(&tmp, &self.top_dll) {
                Ok(()) => return Ok(()),
                Err(e) if attempt == 4 => return Err(e),
                Err(_) => sleep(Duration::from_millis(30)),
            }
        }
        Ok(())
    }
}

/// Locate the workspace root by asking cargo, then taking the manifest's dir.
fn workspace_root() -> Result<PathBuf, String> {
    let out = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .map_err(|e| format!("running `cargo locate-project`: {e}"))?;
    if !out.status.success() {
        return Err("`cargo locate-project` failed".into());
    }
    let manifest = String::from_utf8_lossy(&out.stdout).trim().to_string();
    PathBuf::from(&manifest)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("could not derive workspace root from {manifest}"))
}

/// Run `cargo build -p <crate> --profile <p> -v` and scrape the verbose
/// `Running \`...\`` line for the cdylib's rustc invocation. If the cdylib is
/// already fresh (no line printed), touch render.rs and retry once.
fn capture_rustc_cmd(
    root: &Path,
    a: &Args,
    crate_us: &str,
    render: &Path,
) -> Result<String, String> {
    let name_needle = format!("--crate-name {crate_us}");

    let attempt = |force_touch: bool| -> Result<Option<String>, String> {
        if force_touch {
            touch(render)?;
        }
        let out = Command::new("cargo")
            .args(["build", "-p", &a.krate, "--profile", &a.profile, "-v"])
            .current_dir(root)
            .env("RUSTFLAGS", &a.rustflags)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("running `cargo build`: {e}"))?;

        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&out.stdout));
        combined.push_str(&String::from_utf8_lossy(&out.stderr));

        if !out.status.success() {
            return Err(format!(
                "`cargo build -p {}` failed:\n{}",
                a.krate,
                tail(&combined, 25)
            ));
        }

        for line in combined.lines() {
            if line.contains(&name_needle) && line.contains("--crate-type cdylib") {
                if let Some(cmd) = between_backticks(line) {
                    return Ok(Some(cmd));
                }
            }
        }
        Ok(None)
    };

    if let Some(cmd) = attempt(false)? {
        return Ok(cmd);
    }
    if let Some(cmd) = attempt(true)? {
        return Ok(cmd);
    }
    Err(format!(
        "could not capture the rustc command for {} (no verbose 'Running ... {name_needle}' cdylib line)",
        a.krate
    ))
}

/// Copy the toolchain's bundled rust-lld to an `lld-link.exe` shim (the name
/// LLD uses to select its MSVC-compatible driver), under target/ (gitignored).
fn resolve_lld_link(root: &Path) -> Result<PathBuf, String> {
    let sysroot = run_trim(Command::new("rustc").arg("--print").arg("sysroot"))?;
    let host = rustc_host()?;
    let rust_lld = PathBuf::from(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(&host)
        .join("bin")
        .join(format!("rust-lld{}", std::env::consts::EXE_SUFFIX));
    if !rust_lld.exists() {
        return Err(format!("rust-lld not found at {}", rust_lld.display()));
    }

    let shim_dir = root.join("target").join(".hot-watch");
    fs::create_dir_all(&shim_dir).map_err(|e| format!("creating {}: {e}", shim_dir.display()))?;
    let shim = shim_dir.join(format!("lld-link{}", std::env::consts::EXE_SUFFIX));

    let need_copy = match (fs::metadata(&shim), fs::metadata(&rust_lld)) {
        (Ok(s), Ok(r)) => s.len() != r.len(),
        _ => true,
    };
    if need_copy {
        fs::copy(&rust_lld, &shim).map_err(|e| format!("copying rust-lld shim: {e}"))?;
    }
    Ok(shim)
}

fn rustc_host() -> Result<String, String> {
    let v = run_trim(Command::new("rustc").arg("-vV"))?;
    v.lines()
        .find_map(|l| l.strip_prefix("host:"))
        .map(|h| h.trim().to_string())
        .ok_or_else(|| "could not parse host triple from `rustc -vV`".into())
}

fn run_trim(c: &mut Command) -> Result<String, String> {
    let out = c
        .output()
        .map_err(|e| format!("running {:?}: {e}", c.get_program()))?;
    if !out.status.success() {
        return Err(format!("{:?} failed", c.get_program()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(windows)]
fn shell_command(cmd: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("cmd");
    // raw_arg: pass the captured command line to cmd.exe verbatim so its own
    // (already shell-quoted) arguments are not re-escaped by Rust.
    c.raw_arg("/C");
    c.raw_arg(cmd);
    c
}

#[cfg(not(windows))]
fn shell_command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

// --- small string/path helpers (pure std, no regex) ------------------------

/// Read the value token immediately following `key` (handles a "quoted" value or
/// a bare value ending at whitespace). For `--out-dir` the leading space is
/// skipped; for `--extern deadsync=` the value starts right after `=`.
fn extract_value(cmd: &str, key: &str) -> Option<String> {
    let i = cmd.find(key)? + key.len();
    let rest = cmd[i..].trim_start();
    // Cargo's verbose output quotes args needing escaping: double-quoted on
    // Windows, single-quoted on Unix; bare otherwise. Strip whichever applies.
    for q in ['"', '\''] {
        if let Some(stripped) = rest.strip_prefix(q) {
            let end = stripped.find(q)?;
            return Some(stripped[..end].to_string());
        }
    }
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Replace the value token following `key` with `new_val`, quoted for the
/// target shell so paths with spaces/special characters survive `sh -c`/`cmd`.
fn replace_value(cmd: &str, key: &str, new_val: &str) -> String {
    let Some(start) = cmd.find(key) else {
        return cmd.to_string();
    };
    let after = start + key.len();
    let span = value_span(&cmd[after..]);
    format!("{}{}{}", &cmd[..after], shell_quote(new_val), &cmd[after + span..])
}

/// Remove a `key=value` token (and one trailing space) entirely.
fn remove_kv(cmd: &str, key: &str) -> String {
    let Some(start) = cmd.find(key) else {
        return cmd.to_string();
    };
    let after = start + key.len();
    let mut end = after + value_span(&cmd[after..]);
    if cmd[end..].starts_with(' ') {
        end += 1;
    }
    format!("{}{}", &cmd[..start], &cmd[end..])
}

/// Byte length of the value token at the start of `s` (bare, "double"- or
/// 'single'-quoted, matching Cargo's per-platform escaping).
fn value_span(s: &str) -> usize {
    for q in ['"', '\''] {
        if let Some(stripped) = s.strip_prefix(q) {
            return match stripped.find(q) {
                Some(end) => end + 2, // include both quotes
                None => s.len(),
            };
        }
    }
    s.find(char::is_whitespace).unwrap_or(s.len())
}

/// Quote a value for the shell used by `shell_command`: POSIX single-quotes on
/// Unix (no expansion of $, backticks, etc.), double-quotes on Windows cmd.exe.
fn shell_quote(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{value}\"")
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn between_backticks(line: &str) -> Option<String> {
    let a = line.find('`')?;
    let b = line.rfind('`')?;
    (b > a).then(|| line[a + 1..b].to_string())
}

fn touch(p: &Path) -> Result<(), String> {
    // Portable "touch": rewrite identical bytes to bump mtime (std has no API to
    // set mtime directly without a dependency). Open without truncation so an
    // interrupted write can't shrink the source file.
    use std::io::Write;
    let data = fs::read(p).map_err(|e| format!("reading {}: {e}", p.display()))?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(p)
        .map_err(|e| format!("opening {}: {e}", p.display()))?;
    f.write_all(&data)
        .map_err(|e| format!("touching {}: {e}", p.display()))
}

fn mtime(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).ok()?.modified().ok()
}

fn snapshot(p: &Path) -> Option<(u64, SystemTime)> {
    let m = fs::metadata(p).ok()?;
    Some((m.len(), m.modified().ok()?))
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}
