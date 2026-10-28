"""Build unmodified pinned mozjpeg 4.1.1 and public 8/12-bit API clients.

Windows/MSVC reproduction helper. SIMD is disabled for numerical reference
checks; this configuration must not be used to claim competitive throughput.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import urllib.request

SOURCE = "https://codeload.github.com/mozilla/mozjpeg/tar.gz/refs/tags/v4.1.1"
SHA256 = "66b1b8d6b55d263f35f27f55acaaa3234df2a401232de99b6d099e2bb0a9d196"
COMMIT = "a2d2907ff023227e80c1e4efa809812410275a12"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--work", required=True, type=Path)
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    archive = work / "v4.1.1.tar.gz"
    if not archive.exists():
        archive.write_bytes(urllib.request.urlopen(SOURCE, timeout=45).read())
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == SHA256
    source = work / "mozjpeg-4.1.1"
    if not source.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(work, filter="data")
    # Detect local source modifications before trusting a reference build.
    with tarfile.open(archive) as tar:
        for member in tar:
            if member.isfile():
                assert (work / member.name).read_bytes() == tar.extractfile(member).read(), member.name
    reports = []
    for precision in (8, 12):
        build = work / f"build-{precision}"
        options = ["-DCMAKE_POLICY_VERSION_MINIMUM=3.5", "-DENABLE_SHARED=OFF", "-DENABLE_STATIC=ON", "-DWITH_SIMD=OFF",
                   "-DWITH_TURBOJPEG=OFF", "-DPNG_SUPPORTED=OFF", "-DWITH_CRT_DLL=OFF",
                   f"-DWITH_12BIT={'ON' if precision == 12 else 'OFF'}",
                   "-DWITH_ARITH_ENC=ON", "-DWITH_ARITH_DEC=ON"]
        subprocess.run(["cmake", "-S", str(source), "-B", str(build), *options], check=True)
        subprocess.run(["cmake", "--build", str(build), "--config", "Release", "--target",
                        "cjpeg-static", "jpegtran-static", "--", "/m:1", "/verbosity:minimal"], check=True)
        client = work / f"client-{precision}"
        subprocess.run(["cmake", "-S", str(Path(__file__).parent / "oracle/classic"), "-B", str(client),
                        "-DJPEG_INCLUDE_DIR=" + str(source), "-DJPEG_CONFIG_DIR=" + str(build),
                        "-DJPEG_LIBRARY=" + str(build / "Release/jpeg-static.lib")], check=True)
        subprocess.run(["cmake", "--build", str(client), "--config", "Release", "--",
                        "/m:1", "/verbosity:minimal"], check=True)
        tools = {"cjpeg": build / "Release/cjpeg-static.exe",
                 "jpegtran": build / "Release/jpegtran-static.exe",
                 "oracle": client / "Release/classic-oracle.exe"}
        reports.append({"precision": precision, "cmake_options": options,
                        "arithmetic": precision == 8,
                        "tools": {k: {"path": str(v), "sha256": hashlib.sha256(v.read_bytes()).hexdigest()}
                                  for k, v in tools.items()}})
    report = {"reference": "mozjpeg 4.1.1", "source": SOURCE, "commit": COMMIT,
              "archive_sha256": SHA256, "codec_source_changes": False,
              "configuration_note": "Upstream WITH_12BIT disables arithmetic. SIMD off; correctness only.",
              "builds": reports}
    (work / "build.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
