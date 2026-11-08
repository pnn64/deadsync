"""Build current harness modules against a newer ITGmania STB source tree.

Only a private copy's build glue changes. The harness and engine checkouts stay
untouched. A patch, source identities, build result and binary hash are retained
so this adapter cannot be mistaken for the unmodified pinned harness binary.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import tomllib


def git(source, *args):
    return subprocess.check_output(["git", "-C", str(source), *args],
                                   text=True, encoding="utf-8").strip()


def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new, 1)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def junction(target, link, helper):
    link.parent.mkdir(parents=True, exist_ok=True)
    if os.name == "nt":
        helper.write_text(
            "param([string]$Link, [string]$Target)\n"
            "New-Item -ItemType Junction -Path $Link -Value $Target | Out-Null\n",
            encoding="utf-8")
        subprocess.run(["powershell", "-NoProfile", "-File", str(helper),
                        "-Link", str(link), "-Target", str(target)], check=True)
    else:
        link.symlink_to(target, target_is_directory=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--harness-source", required=True, type=Path)
    parser.add_argument("--itgmania-source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--codecs-only", action="store_true", help=
                        "Use unchanged v1.2 support classes with the primary image loaders only")
    args = parser.parse_args()
    harness = args.harness_source.resolve()
    engine = args.itgmania_source.resolve()
    native = harness / "vendor/itgmania" if args.codecs_only else engine
    output = args.output.resolve()
    for source in (harness, engine):
        assert not git(source, "status", "--porcelain", "--untracked-files=no"), source
    assert (engine / "src/RageSurface_Load_STB.cpp").is_file()
    assert (engine / "extern/stb_image.c").is_file()
    assert (engine / "extern/stb/stb_image.h").is_file()
    support_proofs = []
    if args.codecs_only:
        for relative in [
            "src/RageFile.h", "src/RageFile.cpp", "src/RageFileBasic.h", "src/RageFileBasic.cpp",
            "src/RageSurface.h", "src/RageSurface.cpp", "src/RageSurfaceUtils.h",
            "src/RageSurfaceUtils.cpp", "src/RageSurfaceUtils_Zoom.cpp", "src/RageSurface_Load.h",
            "src/RageSurface_Load.cpp", "src/RageUtil.cpp", "src/RageUtil/Endian.h", "src/StdString.h",
            "src/RageBitmapTexture.cpp", "src/RageTexture.cpp", "src/RageTextureID.cpp",
        ]:
            primary = (engine / relative).read_bytes().replace(b"\r\n", b"\n")
            baseline = (native / relative).read_bytes().replace(b"\r\n", b"\n")
            assert primary == baseline, f"codec support implementation differs: {relative}"
            support_proofs.append({"path": relative, "sha256_lf": hashlib.sha256(primary).hexdigest()})
    # Existing output is never deleted or overwritten; poll a live build rather
    # than restarting it just because a tool observation timed out.
    output.mkdir(parents=True, exist_ok=False)
    private = output / "source"
    private.mkdir()
    files = subprocess.check_output(["git", "-C", str(harness), "ls-files", "-z"])
    for relative in files.decode("utf-8").split("\0"):
        if not relative:
            continue
        source = harness / relative
        if not source.is_file():
            continue  # Vendor gitlinks are linked explicitly below.
        destination = private / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
    junction(harness / "vendor/simply-love", private / "vendor/simply-love",
             output / "prepare-junction.ps1")
    manifest_path = private / "Cargo.toml"
    manifest = manifest_path.read_text(encoding="utf-8")
    if args.codecs_only:
        version = tomllib.loads(manifest)["package"]["version"]
        manifest = replace_once(manifest, f'version = "{version}"',
                                f'version = "{version}+primary-codecs"')
    assert "[workspace]" not in manifest
    manifest += "\n# Isolated generation adapter; not a deadsync workspace member.\n[workspace]\n"
    manifest_path.write_bytes(manifest.encode("utf-8"))
    path = private / "build.rs"
    before = path.read_text(encoding="utf-8")
    after = replace_once(before, '    let root = manifest.join(root);', '''    let root = manifest.join(root);
    println!("cargo:rerun-if-env-changed=ITGMANIA_CODEC_ROOT");
    let codec_root = env::var_os("ITGMANIA_CODEC_ROOT").map(PathBuf::from)
        .unwrap_or_else(|| root.clone());''')
    after = replace_once(after, "    compile_image_deps(&root, &out, &target_os);",
                         "    compile_image_deps(&codec_root, &out, &target_os);")
    after = replace_once(after, "        build.file(root.join(relative));", '''        let image_loader = matches!(*relative, "src/RageSurface_Load_PNG.cpp" |
            "src/RageSurface_Load_GIF.cpp" | "src/RageSurface_Load_BMP.cpp" | "src/RageSurface_Load_JPEG.cpp");
        build.file(if image_loader { codec_root.join(relative) } else { root.join(relative) });''')
    after = replace_once(after, "    build.compile(\"itgmania_oracle\");", """    if codec_root.join("src/RageSurface_Load_STB.cpp").is_file() {
        build.include(codec_root.join("extern"))
            .include(codec_root.join("extern/stb"))
            .file(codec_root.join("src/RageSurface_Load_STB.cpp"));
    }
    build.compile("itgmania_oracle");""")
    start = after.index('    println!("cargo:rustc-link-lib=static=itgmania_png");')
    end = after.index("\n\n    compile_audio_deps", start)
    legacy_links = after[start:end]
    after = after[:start] + """    if codec_root.join("src/RageSurface_Load_STB.cpp").is_file() {
        println!("cargo:rustc-link-lib=static=itgmania_stb");
    } else {
""" + "\n".join("    " + line for line in legacy_links.splitlines()) + "\n    }" + after[end:]
    after = replace_once(after, "fn compile_image_deps(root: &Path, out: &Path, target_os: &str) {", """fn compile_image_deps(root: &Path, out: &Path, target_os: &str) {
    if root.join("src/RageSurface_Load_STB.cpp").is_file() {
        cc::Build::new().cargo_metadata(false).warnings(false)
            .include(root.join("extern"))
            .include(root.join("extern/stb"))
            .file(root.join("extern/stb_image.c"))
            .compile("itgmania_stb");
        return;
    }""")
    path.write_bytes(after.encode("utf-8"))
    patch = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                        fromfile="harness/build.rs", tofile="adapter/build.rs"))
    (output / "build-adapter.patch").write_bytes(patch.encode("utf-8"))
    metadata = {"schema_version": 1, "kind": "private_native_build_adapter",
                "harness_revision": git(harness, "rev-parse", "HEAD"),
                "itgmania_revision": git(engine, "rev-parse", "HEAD"),
                "native_baseline_revision": git(native, "rev-parse", "HEAD"),
                "simply_love_revision": git(harness / "vendor/simply-love", "rev-parse", "HEAD"),
                "original_build_rs_sha256": sha(harness / "build.rs"),
                "adapter_build_rs_sha256": sha(path),
                "adapter_patch_sha256": sha(output / "build-adapter.patch"),
                "builder_sha256": sha(Path(__file__)),
                "scope": ("Primary image loader and STB sources; unchanged pinned support implementations verified by hash. Other engine behavior is not certified by this codec adapter."
                          if args.codecs_only else "Unchanged harness Rust/native adapters and current engine sources; STB build selection only."),
                "codec_support_proofs": support_proofs}
    shutil.copyfile(__file__, output / "builder.py")
    (output / "adapter.json").write_bytes((json.dumps(metadata, indent=2) + "\n").encode())
    print(json.dumps(metadata), flush=True)
    env = os.environ.copy()
    env["ITGMANIA_ROOT"] = str(native)
    env["ITGMANIA_CODEC_ROOT"] = str(engine)
    command = ["cargo", "+1.98.1", "build", "--manifest-path", str(manifest_path),
               "--target-dir", str(output / "target")]
    started = time.monotonic()
    with (output / "build.log").open("wb") as log:
        result = subprocess.run(command, cwd=private, env=env, stdout=log, stderr=subprocess.STDOUT)
    metadata["build_exit_code"] = result.returncode
    metadata["build_seconds"] = time.monotonic() - started
    if result.returncode == 0:
        executable = output / "target/debug" / ("itgmania-harness-rs.exe" if os.name == "nt" else "itgmania-harness-rs")
        metadata["binary"] = str(executable)
        metadata["binary_sha256"] = sha(executable)
    (output / "adapter.json").write_bytes((json.dumps(metadata, indent=2) + "\n").encode())
    print("Build exit code:", result.returncode, flush=True)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
