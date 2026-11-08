"""Encode identical pinned WAV inputs through independent FFmpeg encoders.

Run where FFmpeg provides libshine and/or Windows Media Foundation. A failed
configuration is retained with its diagnostic, never counted as tested media.
"""
import argparse, hashlib, json, subprocess, wave
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--ffmpeg', default='ffmpeg')
    p.add_argument('--ffprobe', default='ffprobe')
    p.add_argument('--sources', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'version': subprocess.check_output([args.ffmpeg, '-version'], text=True),
              'records': [], 'unsupported': []}
    for encoder in ['libshine', 'mp3_mf', 'libmp3lame']:
        rates = [32000, 44100, 48000] if encoder == 'libshine' else [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000]
        for rate in rates:
            for channels in [1, 2]:
                source = args.sources / f'{rate}-{channels}.wav'
                target = args.output / f'{encoder}-{rate}-{channels}.mp3'
                bitrate = 128 if rate >= 32000 else 64 if rate >= 16000 else 32
                command = [args.ffmpeg, '-hide_banner', '-nostdin', '-loglevel', 'error',
                           '-i', str(source.resolve()), '-ar', str(rate), '-ac', str(channels),
                           '-c:a', encoder, '-b:a', f'{bitrate}k']
                # The MF transform emits packets containing multiple MPEG
                # frames. FFmpeg's muxer counts packets in Xing, yielding a
                # false frame count. Test its valid elementary output instead.
                if encoder == 'mp3_mf':
                    command += ['-write_xing', '0']
                command += [str(target.resolve())]
                run = subprocess.run(command, capture_output=True, text=True)
                record = {'file': target.name, 'encoder': encoder, 'rate': rate,
                          'channels': channels, 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                          'command': command, 'exit_code': run.returncode, 'stderr': run.stderr}
                with wave.open(str(source), 'rb') as wav:
                    record['source_samples'] = wav.getnframes()
                if not run.returncode:
                    probe_command = [args.ffprobe, '-v', 'error', '-count_frames',
                                     '-show_entries', 'stream=sample_rate,channels,nb_read_frames',
                                     '-of', 'json', str(target.resolve())]
                    probe = subprocess.run(probe_command, capture_output=True, text=True)
                    record['probe'] = {'command': probe_command, 'exit_code': probe.returncode,
                                       'stdout': probe.stdout, 'stderr': probe.stderr}
                    streams = json.loads(probe.stdout).get('streams', []) if not probe.returncode else []
                    usable = len(streams) == 1 and int(streams[0].get('nb_read_frames', 0)) > 0
                    if usable:
                        assert (int(streams[0]['sample_rate']), streams[0]['channels']) == (rate, channels)
                else:
                    usable = False
                if not usable:
                    record['reason'] = 'encoder failed or produced no independently decodable audio'
                    report['unsupported'].append(record)
                    target.unlink(missing_ok=True)
                else:
                    record['input_sha256'] = hashlib.sha256(target.read_bytes()).hexdigest()
                    report['records'].append(record)
                (args.output / 'generation.json').write_text(json.dumps(report, indent=2) + '\n')
                print(target.name, run.returncode, flush=True)


if __name__ == '__main__':
    main()
