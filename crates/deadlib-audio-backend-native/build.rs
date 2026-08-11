fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_PIPEWIRE_AUDIO");
    println!("cargo:rustc-check-cfg=cfg(has_jack_audio)");
    println!("cargo:rustc-check-cfg=cfg(has_pipewire_audio)");
    println!("cargo:rustc-check-cfg=cfg(has_pulse_audio)");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        return;
    }

    println!("cargo:rustc-cfg=has_jack_audio");
    println!("cargo:rustc-cfg=has_pulse_audio");

    if std::env::var_os("CARGO_FEATURE_PIPEWIRE_AUDIO").is_some() {
        println!("cargo:rustc-cfg=has_pipewire_audio");
    }
}
