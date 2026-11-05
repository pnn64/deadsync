"""Snapshot profile identity helpers and measure thread cycles and allocations."""
import argparse
import csv
import filecmp
import hashlib
import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import statistics
import subprocess
import tarfile
import tomllib

filecmp.BUFSIZE = 1024 * 1024

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path(os.environ.get("DEADSYNC_PROFILE_IDENTITY_ARCHIVE", ROOT / "target/profile-identity"))
PACKAGE = Path("crates/deadsync-profile")


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def item(source, marker):
    start = source.index(marker)
    brace = source.index("{", start)
    depth, end = 1, brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def full_tests(stage, baseline=None, debug=False):
    base = json.loads((ROOT / "docs/performance/2026-11-04-partner-charts-full-release.manifest.json").read_text(encoding="utf-8"))
    archive = Path(base["source_archive"])
    assert sha(archive) == base["source_archive_sha256"]
    work = archive.parent.parent / "full-workspace"
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    root_manifest = tomllib.loads(subprocess.check_output(["git", "show", "HEAD:Cargo.toml"], cwd=ROOT, text=True))
    packages = set()

    def visit(package):
        if package in packages:
            return
        packages.add(package)
        manifest = tomllib.loads(subprocess.check_output(["git", "show", f"HEAD:{package}/Cargo.toml"], cwd=ROOT, text=True))

        def tables(table):
            for key, value in table.items():
                if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                    for name, dependency in value.items():
                        if isinstance(dependency, dict):
                            if dependency.get("workspace"):
                                dependency = root_manifest["workspace"]["dependencies"][name]
                                parent = "."
                            else:
                                parent = package
                            if isinstance(dependency, dict) and "path" in dependency:
                                visit(posixpath.normpath(parent + "/" + dependency["path"]))
                elif isinstance(value, dict):
                    tables(value)
        tables(manifest)

    visit(PACKAGE.as_posix())
    supplement = out / "dependency-source.tar"
    with supplement.open("wb") as stream:
        subprocess.run(["git", "archive", "HEAD", *sorted(packages)], cwd=ROOT, stdout=stream, check=True)
    changed = subprocess.check_output(["git", "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], cwd=ROOT, text=True).splitlines()
    paths = {Path(name) for name in changed}
    paths.update(path.relative_to(ROOT) for path in (ROOT / PACKAGE).rglob("*.rs"))
    paths.add(PACKAGE / "Cargo.toml")
    # Preserve cache mtimes when extending the earlier simfile workspace.
    with tarfile.open(supplement) as source:
        for member in source:
            if member.isdir():
                continue
            assert member.isfile(), member.name
            destination = (work / member.name).resolve()
            assert destination.is_relative_to(work.resolve())
            if Path(member.name) in paths:
                continue
            contents = source.extractfile(member).read()
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
    workspace_manifest = base["workspace_manifest"].replace('members = ["crates/deadsync-simfile"]', 'members = ["crates/deadsync-simfile", "crates/deadsync-notefield", "crates/deadsync-profile"]')
    workspace_manifest = workspace_manifest.replace('[workspace.dependencies]', '[workspace.package]\nversion = ' + json.dumps(root_manifest["workspace"]["package"]["version"]) + '\n[workspace.dependencies]\nmlua = { package = "deadlib-lua-legacy", version = "=0.12.2", path = "crates/deadlib-lua-legacy" }')
    if debug:
        workspace_manifest += '\n[profile.release.package.deadsync-profile]\nopt-level = 0\ndebug-assertions = true\noverflow-checks = true\n'
    assert '"crates/deadsync-notefield"' in workspace_manifest
    if (work / "Cargo.toml").read_text(encoding="utf-8") != workspace_manifest:
        (work / "Cargo.toml").write_text(workspace_manifest, encoding="utf-8")
    fixtures = out / "notefield-assets.tar"
    with fixtures.open("wb") as stream:
        subprocess.run(["git", "archive", "HEAD", "assets/noteskins/dance/cel"], cwd=ROOT, stdout=stream, check=True)
    with tarfile.open(fixtures) as source:
        source.extractall(work, filter="data")
    pngs = list((work / "assets/noteskins/dance/cel").glob("*.png"))
    assert pngs and all(path.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n" for path in pngs)
    common = Path(subprocess.check_output(["git", "rev-parse", "--git-common-dir"], cwd=ROOT, text=True).strip())
    if not common.is_absolute():
        common = ROOT / common
    fixture_root = Path(os.environ.get("DEADSYNC_NOTEFIELD_FIXTURE_ROOT", common.resolve().parent))
    external_fixtures = {}
    fixture_names = ['itgmania-primary-bitmaps', 'itgmania-song-lua-micro']
    for name in fixture_names:
        directory = fixture_root / 'tests/fixtures' / name
        for source in directory.rglob('*'):
            if not source.is_file(): continue
            relative = source.relative_to(fixture_root)
            destination = work / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            contents = source.read_bytes()
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
            snapshot = out / 'fixtures' / relative
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(contents)
            external_fixtures[relative.as_posix()] = sha(source)
    hashes = {}
    for relative in sorted(paths):
        contents = (ROOT / relative).read_bytes()
        if baseline and relative == PACKAGE / 'src/lib.rs':
            contents = subprocess.check_output(['git','show',f'{baseline}:{relative.as_posix()}'],cwd=ROOT)
            contents += b'\n#[cfg(test)]\n#[path="identity_performance.rs"]\nmod identity_performance;\n'
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists() or destination.read_bytes() != contents: destination.write_bytes(contents)
        snapshot = out / "source" / relative
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_bytes(contents)
        hashes[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    temporary = ARCHIVE / "compile-temp"
    temporary.mkdir(exist_ok=True)
    build_env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static", TEMP=str(temporary), TMP=str(temporary))
    build_command = ["cargo", "test", "--offline", "--locked", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-profile", "--lib", "--tests", "--release", "--no-run", "--message-format=json"]
    with (out / "build.log").open("w", encoding="utf-8") as stream:
        build = subprocess.run(build_command, cwd=ROOT, env=build_env, stdout=stream, stderr=subprocess.STDOUT)
    if build.returncode: raise RuntimeError((out / "build.log").read_text(encoding="utf-8")[-12000:])
    binaries = []
    profiles = []
    for line in (out / "build.log").read_text(encoding="utf-8").splitlines():
        try: event = json.loads(line)
        except json.JSONDecodeError: continue
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "deadsync_profile" and event.get("executable"):
            binaries.append(Path(event["executable"]))
            profiles.append(event["profile"])
    assert len(binaries) == 1, binaries
    binary = binaries[0]
    command = [str(binary), "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=work, stdout=stream, stderr=subprocess.STDOUT)
    shutil.copyfile(binary, out / binary.name)
    log = (out / "correctness.log").read_text(encoding="utf-8")
    metadata = dict(build_command=build_command, command=command, base_revision=base["git_revision"], baseline_revision=baseline, debug=debug, crate_profile=profiles[0],
                    source_archive=str(archive), source_archive_sha256=sha(archive), workspace_manifest=workspace_manifest, overlays=hashes,
                    dependency_archive=str(supplement), dependency_archive_sha256=sha(supplement), dependency_packages=sorted(packages),
                    external_fixture_root=str(fixture_root), external_fixtures=external_fixtures,
                    fixtures_sha256=sha(fixtures), shipped_png_count=len(pngs),
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    binary_sha256=sha(out / binary.name), build_sha256=sha(out / "build.log"),
                    correctness_sha256=sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=build_env["RUSTFLAGS"], compile_temp=str(temporary), test_temp=os.environ.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(encoding="utf-8"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-05-profile-identity-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if result.returncode: raise RuntimeError(log[-12000:])
    integration=[]
    for line in (out / 'build.log').read_text(encoding='utf-8').splitlines():
        try: event=json.loads(line)
        except json.JSONDecodeError: continue
        if event.get('reason')=='compiler-artifact' and event.get('target',{}).get('kind')==['test'] and event.get('executable'):
            integration.append(event)
    assert {event['target']['name'] for event in integration}=={'name_comparison','noteskin_parts','pad_lookup_growth','pad_config_migration'}
    metadata['integration']={}
    for event in integration:
        name=event['target']['name'];test_binary=Path(event['executable']);archived=out/test_binary.name;shutil.copyfile(test_binary,archived)
        test_command=[str(archived),'--test-threads=1']
        with (out/f'{name}.log').open('w',encoding='utf-8') as stream:
            test_result=subprocess.run(test_command,cwd=work,stdout=stream,stderr=subprocess.STDOUT)
        test_log=(out/f'{name}.log').read_text(encoding='utf-8')
        metadata['integration'][name]=dict(command=test_command,binary_sha256=sha(archived),profile=event['profile'],log_sha256=sha(out/f'{name}.log'),exit_code=test_result.returncode,passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',test_log))))
        (ROOT / f'docs/performance/2026-11-05-profile-identity-{stage}.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
        if test_result.returncode:raise RuntimeError(test_log[-12000:])
    passed=sum(m['passed_tests'] for m in metadata['integration'].values())
    print(f"{stage}: {metadata['passed_tests']} unit + {passed} integration tests passed", flush=True)
    return metadata, out / binary.name, work



def run(stage,baseline,debug,runs,reference_stage):
    metadata,binary,work=full_tests(stage,baseline,debug)
    out=ARCHIVE/stage
    metadata['driver_sources']={}
    for relative in [Path(__file__).relative_to(ROOT),Path('tests/support/kernel_bench.rs'),Path('tests/support/perf.rs')]:
        destination=out/'source'/relative;destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes((ROOT/relative).read_bytes());metadata['driver_sources'][relative.as_posix()]=sha(destination)
    def execute(test,filename,extra=None):
        command=[str(binary),'identity_performance::'+test,'--exact','--ignored','--nocapture','--test-threads=1']
        result=subprocess.run(command,cwd=work,env=dict(os.environ,**(extra or {})),capture_output=True,text=True)
        (out/filename).write_text(result.stdout+result.stderr,encoding='utf-8')
        if result.returncode:raise RuntimeError(result.stdout+result.stderr)
        return result.stdout+result.stderr
    trace=out/'trace.bin';execute('dump_identity_trace','trace.log',{'DEADSYNC_IDENTITY_TRACE_OUT':str(trace)})
    reference=ARCHIVE/(reference_stage or 'baseline')/'trace.bin'
    reference_digest=sha(reference) if reference.exists() else None
    metadata['trace']=dict(bytes=trace.stat().st_size,sha256=sha(trace),reference=str(reference),reference_sha256=reference_digest)
    if reference_digest is not None:assert reference_digest==sha(trace),'trace differs'
    metadata['runs']=0 if debug else runs
    report=ROOT/f'docs/performance/2026-11-05-profile-identity-{stage}'
    report.with_suffix('.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    if debug:return
    rows=[];labels=None
    for index in range(runs):
        output=execute('benchmark_identity',f'bench-{index}.log')
        pairs=re.findall(r'([\w/-]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)',output)
        current_labels={x[0] for x in pairs};assert len(pairs)==26 and len(current_labels)==26,len(pairs)
        if labels is None:labels=current_labels
        else:assert labels==current_labels
        churn={label:dict(re.findall(r'(\w+): (\d+)',fields)) for label,fields in re.findall(r'([\w/-]+): allocation Churn \{ ([^}]+) \}',output)}
        for label,a,b,c,d in pairs:
            for variant,cpu,ns in [('original',a,c),('current',b,d)]:
                counters=churn[label+'/'+variant];assert len(counters)==6,label
                rows.append(dict(run=index,workload=label,variant=variant,cycles_per_fixture=cpu,ns_per_fixture=ns,**{k:int(v) for k,v in counters.items()}))
        print(f'{stage}: {index+1}/{runs}, {len(pairs)} workloads',flush=True)
    with report.with_suffix('.tsv').open('w',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=list(rows[0]),delimiter='\t');writer.writeheader();writer.writerows(rows)
    for label in sorted(labels):
        old=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='original'];new=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='current']
        print(f'{label}: {statistics.median(old):.2f} -> {statistics.median(new):.2f}; paired throughput {statistics.median(a/b for a,b in zip(old,new)):.3f}x',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('stage');parser.add_argument('--baseline');parser.add_argument('--debug',action='store_true');parser.add_argument('--runs',type=int,default=5);parser.add_argument('--reference')
    args=parser.parse_args()
    if args.runs<1:parser.error('positive runs required')
    run(args.stage,args.baseline,args.debug,args.runs,args.reference)
