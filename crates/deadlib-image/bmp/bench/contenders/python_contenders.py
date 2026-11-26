"""Measure the public in-memory OpenCV and Pillow APIs with the same BMP corpus."""

import argparse
import csv
import ctypes
import io
import os
import struct
import sys
from pathlib import Path
from statistics import median
from time import perf_counter_ns

import cv2
import numpy as np
from PIL import Image, UnidentifiedImageError

HEADER = "origin group case w h codec input_bytes output_bytes ns cycles verifier wire status".split()


def clocks():
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    process = kernel.GetCurrentProcess()
    kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    kernel.QueryProcessCycleTime.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint64)]
    cpu = int(os.environ.get("BMP_BENCH_CPU", 2))
    if not kernel.SetProcessAffinityMask(process, 1 << cpu):
        raise ctypes.WinError(ctypes.get_last_error())
    cycles = ctypes.c_uint64()

    def read():
        if not kernel.QueryProcessCycleTime(process, ctypes.byref(cycles)):
            raise ctypes.WinError(ctypes.get_last_error())
        return cycles.value

    return read


def measure(task, pixels, span, cycles):
    if not span:
        return 0, 0
    batch = 16 if pixels < 100_000 else 1
    times, counts = [], []
    for _ in range(5):
        started, initial = perf_counter_ns(), cycles()
        iterations = 0
        while True:
            for _ in range(batch):
                task()
            iterations += batch
            elapsed = perf_counter_ns() - started
            if elapsed >= span:
                break
        counts.append((cycles() - initial) // iterations)
        times.append(elapsed // iterations)
    return median(times), median(counts)


def pil_decode(data):
    try:
        with Image.open(io.BytesIO(data), formats=["BMP"]) as source:
            return source.size, source.convert("RGBA").tobytes()
    except UnidentifiedImageError as error:
        raise ValueError("cannot identify BMP input") from error


def cv_decode(data):
    source = cv2.imdecode(np.frombuffer(data, dtype=np.uint8), cv2.IMREAD_UNCHANGED)
    if source is None or source.dtype != np.uint8:
        raise ValueError("unsupported BMP pixels")
    if source.ndim == 2:
        code = cv2.COLOR_GRAY2RGBA
    else:
        code = cv2.COLOR_BGR2RGBA if source.shape[2] == 3 else cv2.COLOR_BGRA2RGBA
    pixels = cv2.cvtColor(source, code)
    return (pixels.shape[1], pixels.shape[0]), memoryview(pixels).cast("B")


def pil_encode(data, shape, channels):
    with Image.frombytes("RGB" if channels == 3 else "RGBA", shape, data) as source:
        sink = io.BytesIO()
        source.save(sink, format="BMP")
        return sink.getvalue()


def cv_encode(data, channels):
    code = cv2.COLOR_RGB2BGR if channels == 3 else cv2.COLOR_RGBA2BGRA
    ok, bmp = cv2.imencode(".bmp", cv2.cvtColor(data, code))
    if not ok:
        raise ValueError("OpenCV could not encode BMP")
    return memoryview(bmp).cast("B")


def compare(actual, shape, expected):
    dims, pixels = actual
    if dims != shape:
        raise ValueError(f"shape differs {dims[0]}x{dims[1]}")
    if expected is None:
        raise ValueError("accepted without an independent oracle")
    if pixels != expected:
        raise ValueError(f"{sum(a != b for a, b in zip(pixels, expected))} sample bytes differ")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["decode", "encode"])
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    span = 0 if args.verify_only else int(os.environ.get("BMP_BENCH_MS", 6)) * 1_000_000
    cycles = clocks() if span else None
    cv2.setNumThreads(1)
    output = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
    output.writerow(HEADER)
    rows = args.manifest.read_text(encoding="utf-8").splitlines()
    reverse = "BMP_BENCH_REVERSE" in os.environ
    if reverse:
        rows.reverse()
    for i, row in enumerate(rows):
        origin, group, case, w, h, path, expected_file, *oracle = row.split("\t")
        w, h = int(w), int(h)
        data = Path(path).read_bytes()
        expected = None
        encoding = args.kind == "encode"
        if encoding:
            expected = data
            channels = 4 if group.startswith("rgba") else 3
            source = np.frombuffer(data, dtype=np.uint8).reshape(h, w, 4)
            source = np.ascontiguousarray(source[:, :, :channels])
            packed = source.tobytes()
        elif expected_file != "-":
            expected = Path(expected_file).read_bytes()
        codecs = ["pillow", "opencv"]
        if bool(i % 2) != reverse:
            codecs.reverse()
        for codec in codecs:
            verifier, wire = (oracle or ["-"])[0], "-"
            input_size = len(packed) if encoding else len(data)
            output_size = ns = count = 0
            try:
                if encoding:
                    if expected_file == "top":
                        raise ValueError("top-down output unavailable")
                    if codec == "pillow":
                        task = lambda: pil_encode(packed, (w, h), channels)
                    else:
                        task = lambda: cv_encode(source, channels)
                    bmp = task()
                    dib, width, height, planes, depth, compression = struct.unpack_from("<IiiHHI", bmp, 14)
                    wire = f"{dib}/{depth}/{compression}/{'top' if height < 0 else 'bottom'}"
                    if depth != channels * 8 or width != w or height != h or planes != 1:
                        raise ValueError("emitted different depth or orientation")
                    check, name = (cv_decode, "opencv") if codec == "pillow" else (pil_decode, "pillow")
                    try:
                        compare(check(bmp), (w, h), expected)
                    except (ValueError, OSError, cv2.error) as error:
                        raise ValueError("no independent exact round trip") from error
                    verifier, output_size = name, len(bmp)
                    del bmp
                else:
                    decode = pil_decode if codec == "pillow" else cv_decode
                    task = lambda: decode(data)
                    compare(task(), (w, h), expected)
                    output_size = w * h * 4
                ns, count = measure(task, w * h, span, cycles)
                status = "ok"
            except (ValueError, OSError, cv2.error) as error:
                status = " ".join(str(error).split())
            output.writerow((origin, group, case, w, h, codec, input_size, output_size,
                             ns, count, verifier, wire, status))
        sys.stdout.flush()


if __name__ == "__main__":
    main()
