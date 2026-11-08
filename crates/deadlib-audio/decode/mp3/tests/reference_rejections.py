"""Historical strict rejection, prefix parity and explicit recovery IO checks."""
import argparse, hashlib, json, struct, subprocess
from pathlib import Path
from reference_historical import compare, pcm, describe


def physical_count(path):
    """Independent header-only CBR/VBR count; no entropy error is concealed here."""
    data = path.read_bytes(); offset = 0; count = samples = 0
    while data[offset:offset+3] == b'ID3':
        h = data[offset:offset+10]; size = 0
        for byte in h[6:10]:
            assert byte < 128
            size = size*128+byte
        offset += 10+size+(10 if h[3] == 4 and h[5]&16 else 0)
    end = len(data)-128 if data[-128:-125] == b'TAG' else len(data)
    while offset+4 <= end:
        h = int.from_bytes(data[offset:offset+4], 'big')
        assert h>>21 == 2047 and h>>17&3 == 1
        version = h>>19&3; assert version in [3, 2, 0]
        rate = [44100, 48000, 32000][h>>10&3]//{3: 1, 2: 2, 0: 4}[version]
        rates = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320] if version == 3 else [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
        bitrate = rates[h>>12&15]; assert bitrate
        size = (144000 if version == 3 else 72000)*bitrate//rate+(h>>9&1)
        if offset+size > end:
            break
        offset += size; count += 1; samples += 1152 if version == 3 else 576
    return {'frames': count, 'samples': samples, 'incomplete_tail_bytes': end-offset}


def main():
    p = argparse.ArgumentParser()
    for name in ['manifest', 'corpus', 'output', 'indexed-recovery']:
        p.add_argument('--'+name, type=Path, required=True)
    for name in ['native', 'mad', 'stream']:
        p.add_argument('--'+name, required=True)
    p.add_argument('--wsl-distro')
    args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    prefix = ['wsl', '-d', args.wsl_distro, '--'] if args.wsl_distro else []
    def toolpath(path):
        path = path.resolve()
        return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:] if prefix else str(path)
    records = []
    for entry in json.loads(args.manifest.read_text())['files']:
        if not entry['role'].startswith('invalid_'):
            continue
        source = args.corpus/entry['file']; assert sha(source) == entry['sha256']
        commands = []; outputs = []
        for name, binary, mode in [('native', args.native, 'native'), ('mad', args.mad, 'mad')]:
            output = args.output/(source.stem+'-'+name+'.bin'); outputs.append(output)
            command = prefix+[binary, mode, toolpath(source), toolpath(output)]
            run = subprocess.run(command, capture_output=True, text=True)
            record = {'command': command, 'exit_code': run.returncode, 'stdout': run.stdout, 'stderr': run.stderr}
            assert run.returncode == 1 and entry[name+'_error'] in run.stderr, record
            assert ('at byte '+str(entry['first_rejected_byte']) if name == 'native' else 'at '+str(entry['first_rejected_byte'])+':') in run.stderr, record
            commands.append(record)
        delta = compare(pcm(outputs[0]), pcm(outputs[1]))
        assert delta['rms'] <= 2e-6 and delta['max'] <= 2e-5
        record = {'file': source.name, 'input_sha256': sha(source), 'commands': commands, 'prefix': delta,
                  'first_rejected_byte': entry['first_rejected_byte']}
        if entry['role'] == 'invalid_scfsi':
            physical = physical_count(source)
            indexed = describe(args.indexed_recovery)
            assert indexed['samples'] == physical['samples'] and indexed['blocks'] == physical['frames']
            recovery = []
            for chunk in [1, 17, 4096]:
                output = args.output/(source.stem+'-recovered-'+str(chunk)+'.bin')
                command = prefix+[args.stream, toolpath(source), toolpath(output), str(chunk), 'recover', 'discard']
                run = subprocess.run(command, capture_output=True, text=True)
                assert not run.returncode, run.stderr
                (args.output/(source.stem+'-'+str(chunk)+'.log')).write_text(run.stdout+run.stderr)
                stats = list(map(int, next(line.split('\t')[1:] for line in run.stdout.splitlines() if line.startswith('summary\t'))))
                assert stats[0]+stats[1] == physical['frames'] and stats[1] > 0
                assert stats[2] == physical['samples'] and stats[4] == physical['incomplete_tail_bytes']
                delta = compare(pcm(output), pcm(args.indexed_recovery))
                assert delta['bit_identical'] and delta['sample_values'] == physical['samples']*indexed['channels']
                recovery.append({'chunk': chunk, 'command': command, 'exit_code': run.returncode,
                                 'statistics': stats, 'indexed_pcm': delta, 'pcm_sha256': sha(output)})
            record.update(physical=physical, recovery=recovery, indexed_recovery_sha256=sha(args.indexed_recovery))
        records.append(record)
        (args.output/'results.json').write_text(json.dumps({'method': __doc__, 'records': records}, indent=2)+'\n')
        print(source.name, 'strict prefix and explicit recovery pass', flush=True)
    binaries = {binary: (subprocess.check_output(prefix+['sha256sum', binary], text=True).split()[0] if prefix else sha(Path(binary))) for binary in [args.native, args.mad, args.stream]}
    proof = {'passed': len(records) == 2, 'files': len(records), 'binaries': binaries,
             'script_sha256': sha(Path(__file__)), 'manifest_sha256': sha(args.manifest)}
    (args.output/'provenance.json').write_text(json.dumps(proof, indent=2)+'\n'); assert proof['passed']


if __name__ == '__main__':
    main()
