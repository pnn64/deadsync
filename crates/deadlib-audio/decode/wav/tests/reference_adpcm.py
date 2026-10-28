"""ADPCM differential evidence; includes reference disagreements without masking them."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import wave
from reference_support import build_dr_wav


def run(command):
    return subprocess.run([str(v) for v in command], check=True, capture_output=True)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def encoded_wave(ima, channels, blocks, block_bytes, frames):
    extra = struct.pack('<H', frames)
    if not ima:
        extra += struct.pack('<H14h', 7, 256, 0, 512, -256, 0, 0, 192, 64, 240, 0, 460, -208, 392, -232)
    fmt = struct.pack('<HHIIHHH', 17 if ima else 2, channels, 8000,
                      8000 * block_bytes // frames, block_bytes, 4, len(extra)) + extra
    body = b'WAVE'
    for tag, payload in [(b'fmt ', fmt), (b'fact', struct.pack('<I', len(blocks) * frames)), (b'data', b''.join(blocks))]:
        body += tag + struct.pack('<I', len(payload)) + payload + b'\0' * (len(payload) & 1)
    return b'RIFF' + struct.pack('<I', len(body)) + body


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    root = Path(__file__).resolve().parents[1]
    workspace = root.parents[3]
    sys.path.insert(0, str(workspace / 'target/wav/python'))
    import soundfile as sf
    dr_wav, dr_provenance = build_dr_wav(workspace, root)
    ffmpeg = shutil.which('ffmpeg')
    if not ffmpeg:
        raise RuntimeError('ffmpeg is required')
    run(['cargo', 'build', '-p', 'deadlib-audio-decode-wav', '--example', 'adpcm', '--target-dir', 'target/wav'])
    binary = workspace / 'target/wav/debug/examples/adpcm'
    if binary.with_suffix('.exe').exists():
        binary = binary.with_suffix('.exe')
    corpus = workspace / 'target/wav/reference-adpcm'
    corpus.mkdir(parents=True, exist_ok=True)
    results = []

    def compare(name, payload):
        path = corpus / f'{name}.wav'
        path.write_bytes(payload)
        actual = run([binary, path]).stdout
        references = {
            'ffmpeg': run([ffmpeg, '-v', 'error', '-i', path, '-f', 's16le', '-acodec', 'pcm_s16le', '-']).stdout,
            'dr_wav': run([dr_wav, path]).stdout,
            'libsndfile': sf.read(str(path), dtype='int16', always_2d=True)[0].astype('<i2').tobytes(),
        }
        compared = {}
        for ref, expected in references.items():
            trimmed = expected[:len(actual)]
            mismatch = None if actual == trimmed else next((i // 2 for i, (a, b) in enumerate(zip(actual, trimmed)) if a != b), min(len(actual), len(trimmed)) // 2)
            compared[ref] = dict(full_sha256=sha(expected), full_samples=len(expected)//2,
                                 fact_trimmed_sha256=sha(trimmed), fact_trimmed_exact=actual == trimmed,
                                 first_mismatch=mismatch, exact_including_length=actual == expected)
        # Fixed per-format expectations follow RIFFNEW, not whichever library
        # happens to match each individual fixture. Keep every disagreement visible.
        ima = name.startswith(('ima-', 'adpcm_ima_wav-'))
        gate = ('dr_wav', 'libsndfile') if ima else ('ffmpeg',)
        passed = all(compared[ref]['fact_trimmed_exact'] for ref in gate)
        results.append(dict(name=name, fixture_sha256=sha(payload), spec_arithmetic_gate=gate,
                            actual_sha256=sha(actual), actual_samples=len(actual)//2,
                            references=compared, passed=passed))

    # Every IMA initial step index and nibble, with zero and both i16 extrema.
    for channels in (1, 2):
        blocks = []
        for initial in (0, -32768, 32767):
            for index in range(89):
                for code in range(16):
                    header = b''.join(struct.pack('<hBB', initial, index, 0) for _ in range(channels))
                    data = b''.join(bytes([code | code << 4]) * 4 for _ in range(channels))
                    blocks.append(header + data)
        compare(f'ima-synthetic-{channels}', encoded_wave(True, channels, blocks, 8 * channels, 9))
        blocks = []
        for predictor in range(7):
            for one in (-32768, -33, 0, 32767):
                for two in (-32768, -257, 0, 32767):
                    for pattern in range(16):
                        header = bytes([predictor] * channels) + struct.pack('<'+'h'*channels, *([32]*channels))
                        header += struct.pack('<'+'h'*channels, *([one]*channels)) + struct.pack('<'+'h'*channels, *([two]*channels))
                        blocks.append(header + bytes(((i+pattern)*17) & 255 for i in range(16*channels)))
        compare(f'ms-synthetic-{channels}', encoded_wave(False, channels, blocks, 23 * channels, 34))

    for codec in ('adpcm_ms', 'adpcm_ima_wav'):
        for channels in (1, 2):
            for length in (17, 1025, 4097):
                for signal in ('sine', 'noise', 'impulses'):
                    rng = random.Random(0x574156)
                    samples = []
                    for i in range(length):
                        for c in range(channels):
                            v = int(30000 * math.sin(i * (c+1) * .073)) if signal == 'sine' else rng.randint(-32768,32767) if signal == 'noise' else (32767 if i % 257 == c else 0)
                            samples.append(v)
                    source = corpus / 'input.wav'
                    with wave.open(str(source), 'wb') as sink:
                        sink.setparams((channels, 2, 44100, 0, 'NONE', 'not compressed'))
                        sink.writeframes(struct.pack('<'+'h'*len(samples), *samples))
                    output = corpus / 'encoded.wav'
                    run([ffmpeg, '-v', 'error', '-y', '-i', source, '-c:a', codec, output])
                    compare(f'{codec}-{channels}-{length}-{signal}', output.read_bytes())
    report = dict(ffmpeg=run([ffmpeg, '-version']).stdout.decode().splitlines()[0],
                  dr_wav=dr_provenance, libsndfile=sf.__libsndfile_version__, soundfile=sf.__version__,
                  rustc=run(['rustc', '-V']).stdout.decode().strip(), binary_sha256=sha(binary.read_bytes()),
                  sources={p.relative_to(root).as_posix(): sha(p.read_bytes()) for p in sorted((root/'src').rglob('*.rs'))},
                  validation_sources={p.relative_to(root).as_posix(): sha(p.read_bytes()) for p in [root/'Cargo.toml', root/'examples/adpcm.rs', Path(__file__), root/'tests/reference_support.py', root/'tests/oracle/dr_wav.c']},
                  cases=results, passed=sum(r['passed'] for r in results), total=len(results))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(f"{report['passed']}/{report['total']} specified-arithmetic differential cases; report: {args.output}")
    for ref in ('ffmpeg', 'dr_wav', 'libsndfile'):
        print(ref, sum(c['references'][ref]['fact_trimmed_exact'] for c in results), '/', len(results), 'fact-trimmed exact')
    if report['passed'] != report['total']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
