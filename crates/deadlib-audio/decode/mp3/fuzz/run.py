"""Run already ASan-built MP3 fuzz targets; retain logs, corpora and provenance."""
import argparse, concurrent.futures, hashlib, json, shutil, subprocess, time
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--binaries', type=Path, required=True)
    p.add_argument('--seeds', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--seconds', type=int, default=180)
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    crate = Path(__file__).resolve().parent.parent
    source = {str(path.relative_to(crate)): sha(path) for path in crate.rglob('*')
              if path.is_file() and path.suffix in ['.rs', '.toml', '.lock', '.py']
              and not any(part in path.parts for part in ['target', 'validation', '__pycache__'])}

    def work(target):
        binary = (args.binaries / target).resolve()
        corpus = args.output / 'corpus' / target
        corpus.mkdir(parents=True)
        for path in (args.seeds / target).iterdir():
            if path.is_file():
                shutil.copyfile(path, corpus / path.name)
        seeds = {path.name: sha(path) for path in corpus.iterdir() if path.is_file()}
        artifacts = args.output / 'artifacts' / target
        artifacts.mkdir(parents=True)
        command = [str(binary), str(corpus.resolve()),
                   '-artifact_prefix=' + str(artifacts.resolve()) + '/',
                   '-max_total_time=' + str(args.seconds), '-timeout=10',
                   '-rss_limit_mb=1024', '-max_len=32769', '-seed=438179',
                   '-print_final_stats=1']
        started = time.time()
        with (args.output / (target + '.log')).open('wb') as log:
            run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
        record = {'target': target, 'command': command, 'exit_code': run.returncode,
                  'elapsed_seconds': time.time() - started, 'binary_sha256': sha(binary),
                  'initial_seeds': seeds, 'final_corpus': {
                      path.name: sha(path) for path in corpus.iterdir() if path.is_file()},
                  'artifacts': [path.name for path in artifacts.iterdir()]}
        (args.output / (target + '.json')).write_text(json.dumps(record, indent=2) + '\n')
        print(target, run.returncode, flush=True)
        return record

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        records = list(pool.map(work, ['frames', 'streams']))
    report = {'passed': all(r['exit_code'] == 0 and not r['artifacts'] for r in records),
              'timestamp_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'source_sha256': source, 'records': records}
    (args.output / 'validation.json').write_text(json.dumps(report, indent=2) + '\n')
    assert report['passed']


if __name__ == '__main__':
    main()
