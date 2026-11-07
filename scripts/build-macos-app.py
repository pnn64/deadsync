#!/usr/bin/env python3
"""Build a relocatable DeadSync.app with its native Vulkan dependencies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run(*args, capture=False, cwd=None):
    result = subprocess.run([str(arg) for arg in args], check=True, cwd=cwd,
                            text=True, stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else None


def dependencies(image):
    return [line.strip().split(" (compatibility version", 1)[0]
            for line in run("otool", "-L", image, capture=True).splitlines() if line.startswith("\t")]


def rpaths(image):
    output = run("otool", "-l", image, capture=True)
    return re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset", output)


def system_library(name):
    return name.startswith(("/System/Library/", "/usr/lib/"))


def resolve_library(name, image, executable):
    def expand(value):
        return value.replace("@loader_path", str(image.parent)).replace(
            "@executable_path", str(executable.parent))
    if name.startswith("@rpath/"):
        for directory in rpaths(image) + rpaths(executable):
            candidate = Path(expand(directory)) / name[len("@rpath/"):]
            if candidate.is_file():
                return candidate.resolve()
    else:
        candidate = Path(expand(name))
        if candidate.is_file():
            return candidate.resolve()
    raise RuntimeError(f"Cannot resolve dependency {name} of {image}")


def molten_vk():
    candidates = []
    if os.environ.get("MOLTENVK_LIBRARY"):
        candidates.append(Path(os.environ["MOLTENVK_LIBRARY"]))
    if shutil.which("brew"):
        prefix = run("brew", "--prefix", "molten-vk", capture=True).strip()
        candidates.append(Path(prefix) / "lib/libMoltenVK.dylib")
    candidates.extend(Path(prefix) / "lib/libMoltenVK.dylib" for prefix in
                      ["/opt/homebrew/opt/molten-vk", "/usr/local/opt/molten-vk"])
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    raise RuntimeError("MoltenVK not found: brew install molten-vk, or set MOLTENVK_LIBRARY")


def embed_libraries(executable, binary, frameworks):
    copied = {}
    queue = [(executable, binary)]

    def copy_library(source):
        name = source.name
        if name in copied:
            if copied[name] != source:
                raise RuntimeError(f"Conflicting library names: {copied[name]} and {source}")
            return name
        if source.suffix != ".dylib":
            raise RuntimeError(f"Unsupported non-system framework dependency: {source}")
        copied[name] = source
        destination = frameworks / name
        shutil.copy2(source, destination)
        destination.chmod(0o755)
        run("install_name_tool", "-id", f"@rpath/{name}", destination)
        queue.append((source, destination))
        return name

    driver = copy_library(molten_vk())
    while queue:
        source, destination = queue.pop(0)
        source_dependencies = list(dict.fromkeys(dependencies(source)))
        if source != executable:
            source_dependencies = source_dependencies[1:]  # LC_ID_DYLIB, not a dependency
        for dependency in source_dependencies:
            if system_library(dependency):
                continue
            resolved = resolve_library(dependency, source, executable)
            # otool -L includes a dylib's own LC_ID_DYLIB as its first entry.
            if source != executable and resolved == source:
                continue
            name = copy_library(resolved)
            relative = (f"@executable_path/../Frameworks/{name}" if destination == binary
                        else f"@loader_path/{name}")
            run("install_name_tool", "-change", dependency, relative, destination)
    return driver, copied


def minimum_macos(images):
    versions = [(11, 0)]
    for image in images:
        output = run("otool", "-l", image, capture=True)
        values = re.findall(r"cmd LC_BUILD_VERSION\s+cmdsize \d+\s+platform \d+\s+minos (\d+\.\d+(?:\.\d+)?)", output)
        values += re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (\d+\.\d+(?:\.\d+)?)", output)
        versions.extend(tuple(int(part) for part in value.split(".")) for value in values)
    return ".".join(str(part) for part in max(versions))


def make_app(args):
    output = args.output.resolve()
    if output.suffix != ".app":
        raise RuntimeError("--output must end in .app")
    if output.exists():
        if not args.replace:
            raise RuntimeError(f"Output exists; pass --replace to rebuild it: {output}")
        with (output / "Contents/Info.plist").open("rb") as stream:
            if plistlib.load(stream).get("CFBundleIdentifier") != args.identifier:
                raise RuntimeError(f"Refusing to replace a different application: {output}")
    metadata = json.loads(run("cargo", "metadata", "--format-version", "1", "--no-deps",
                              capture=True, cwd=ROOT))
    package = next(package for package in metadata["packages"] if package["name"] == "deadsync")
    version = package["version"]
    if args.binary:
        executable = args.binary.resolve()
    else:
        run("cargo", "build", "--locked", "--package", "deadsync", "--profile", args.profile, cwd=ROOT)
        directory = "debug" if args.profile == "dev" else args.profile
        executable = Path(metadata["target_directory"]) / directory / "deadsync"
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise RuntimeError(f"Missing executable: {executable}")
    architectures = set(run("lipo", "-archs", executable, capture=True).split())
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".deadsync-app-", dir=output.parent) as temporary:
        bundle = Path(temporary) / output.name
        contents = bundle / "Contents"
        macos, resources, frameworks = (contents / name for name in ["MacOS", "Resources", "Frameworks"])
        for directory in [macos, resources, frameworks]:
            directory.mkdir(parents=True)
        binary = macos / "deadsync"
        shutil.copy2(executable, binary)
        binary.chmod(0o755)
        # Ship repository content only, never local song packs or user data.
        tracked = run("git", "ls-files", "-z", "assets", "songs", "courses", "README.md", "LICENSE",
                      capture=True, cwd=ROOT).split("\0")
        for relative in filter(None, tracked):
            source = ROOT / relative
            destination = resources / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        shutil.copy2(ROOT / "assets/graphics/icon/icon.icns", resources / "DeadSync.icns")
        driver, library_sources = embed_libraries(executable, binary, frameworks)
        licenses = resources / "licenses"
        licenses.mkdir()
        for name, source in library_sources.items():
            for filename in ["LICENSE", "LICENSE.txt", "COPYING"]:
                notice = source.parent.parent / filename
                if notice.is_file():
                    shutil.copy2(notice, licenses / f"{name}-{filename}")
        images = [binary, *frameworks.glob("*.dylib")]
        for image in images:
            if not architectures <= set(run("lipo", "-archs", image, capture=True).split()):
                raise RuntimeError(f"Architecture mismatch in {image}")
            for dependency in dependencies(image):
                if not system_library(dependency) and not dependency.startswith(("@loader_path/", "@executable_path/", "@rpath/")):
                    raise RuntimeError(f"Unbundled dependency: {dependency}")
        manifest_dir = resources / "vulkan/icd.d"
        manifest_dir.mkdir(parents=True)
        (manifest_dir / "MoltenVK_icd.json").write_text(json.dumps({
            "file_format_version": "1.0.0", "ICD": {
                "library_path": f"../../../Frameworks/{driver}",
                "api_version": "1.2.0", "is_portability_driver": True,
            }}, indent=2) + "\n")
        plist = {
            "CFBundleName": "DeadSync", "CFBundleDisplayName": "DeadSync",
            "CFBundleIdentifier": args.identifier, "CFBundleExecutable": "deadsync",
            "CFBundlePackageType": "APPL", "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleShortVersionString": version, "CFBundleVersion": version,
            "CFBundleIconFile": "DeadSync.icns", "NSHighResolutionCapable": True,
            "LSApplicationCategoryType": "public.app-category.music-games",
            "LSMinimumSystemVersion": minimum_macos(images),
            "NSBluetoothAlwaysUsageDescription": "DeadSync connects to your Bluetooth heart-rate monitor during play.",
            "NSInputMonitoringUsageDescription": "DeadSync reads dance-pad and keyboard inputs with precise timing.",
            "NSHumanReadableCopyright": "DeadSync contributors. GNU GPL v3. See Resources/LICENSE.",
        }
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump(plist, stream)
        for image in [*frameworks.glob("*.dylib"), binary, bundle]:
            command = ["codesign", "--force", "--sign", args.sign, "--identifier", args.identifier]
            if args.sign != "-":
                command.extend(["--options", "runtime", "--timestamp"])
            run(*command, image)
        run("codesign", "--verify", "--deep", "--strict", bundle)
        previous = Path(temporary) / "previous.app"
        if output.exists():
            output.rename(previous)
        try:
            bundle.rename(output)
        except OSError:
            if previous.exists():
                previous.rename(output)
            raise
    print(f"Application: {output}")
    if args.zip:
        arch = "universal" if len(architectures) > 1 else next(iter(architectures))
        archive = output.parent / f"deadsync-v{version}-{arch}-macos-app.zip"
        if archive.exists() and not args.replace:
            raise RuntimeError(f"Archive exists; pass --replace to rebuild it: {archive}")
        with tempfile.TemporaryDirectory(prefix=".deadsync-zip-", dir=output.parent) as temporary:
            staged_archive = Path(temporary) / archive.name
            run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", output, staged_archive)
            staged_archive.replace(archive)
        checksum = archive.with_suffix(archive.suffix + ".sha256")
        hasher = hashlib.sha256()
        with archive.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                hasher.update(block)
        digest = hasher.hexdigest()
        checksum.write_text(f"{digest}  {archive.name}\n")
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                stream.write(f"archive={archive}\nchecksum={checksum}\n")
        print(f"Archive: {archive}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=["dev", "local", "release"], default="release")
    parser.add_argument("--binary", type=Path, help="Package an already built native executable")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/DeadSync.app")
    parser.add_argument("--identifier", default="org.deadsync.game")
    parser.add_argument("--sign", default="-", help="codesign identity; default is local ad-hoc signing")
    parser.add_argument("--replace", action="store_true", help="Replace an existing app with the same bundle identifier")
    parser.add_argument("--zip", action="store_true", help="Also create a ZIP and SHA-256 checksum")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("Application bundling requires macOS and Xcode Command Line Tools")
    try:
        make_app(args)
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"error: {error}\n")


if __name__ == "__main__":
    main()
