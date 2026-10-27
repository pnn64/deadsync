//! Emit independent decoder test fixtures into a fresh directory.
#[allow(dead_code)]
#[path = "../tests/support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "../tests/support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "../tests/support/compositor.rs"]
mod compositor;
fn main() -> std::io::Result<()> {
    let root =
        std::path::PathBuf::from(std::env::args_os().nth(1).expect("fresh output directory"));
    std::fs::create_dir(&root)?;
    let cases = compositor::fixtures();
    for (name, case) in &cases {
        std::fs::write(root.join(format!("{name}.png")), case.wire())?;
    }
    println!(
        "Created {} independent native-precision APNG fixtures at {}",
        cases.len(),
        root.display()
    );
    Ok(())
}
