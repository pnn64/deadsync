"""Measure notefield feedback guards and atlas paths with complete sprite and panic parity checks."""
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
ARCHIVE = Path(os.environ.get("DEADSYNC_NOTEFIELD_FEEDBACK_ARCHIVE", ROOT / "target/notefield-feedback"))
PACKAGE = Path("crates/deadsync-notefield")


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
    root_manifest = tomllib.loads(subprocess.check_output(["git", "show", "HEAD:Cargo.toml"], cwd=ROOT, text=True, encoding="utf-8"))
    packages = set()

    def visit(package):
        if package in packages:
            return
        packages.add(package)
        manifest = tomllib.loads(subprocess.check_output(["git", "show", f"HEAD:{package}/Cargo.toml"], cwd=ROOT, text=True, encoding="utf-8"))

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
    changed = subprocess.check_output(["git", "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], cwd=ROOT, text=True, encoding="utf-8").splitlines()
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
    workspace_manifest = base["workspace_manifest"].replace('members = ["crates/deadsync-simfile"]', 'members = ["crates/deadsync-simfile", "crates/deadsync-notefield", "crates/deadsync-profile", "crates/deadsync-input-fsr", "crates/deadsync-config", "crates/deadsync-input"]')
    workspace_manifest = workspace_manifest.replace('[workspace.dependencies]', '[workspace.package]\nversion = ' + json.dumps(root_manifest["workspace"]["package"]["version"]) + '\n[workspace.dependencies]\nmlua = { package = "deadlib-lua-legacy", version = "=0.12.2", path = "crates/deadlib-lua-legacy" }')
    if debug:
        workspace_manifest += '\n[profile.release.package.deadsync-notefield]\nopt-level = 0\ndebug-assertions = true\noverflow-checks = true\n'
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
    fixture_root = Path(os.environ.get("DEADSYNC_FEEDBACK_FIXTURE_ROOT", ARCHIVE / "frozen-fixtures"))
    external_fixtures = {}
    for path in (ROOT / PACKAGE / "src").rglob("*.rs"):
        for name in re.findall(r'tests/fixtures/[\w/.-]+', path.read_text(encoding="utf-8")):
            relative = Path(name)
            source = ROOT / relative
            if not source.is_file():
                source = fixture_root / relative
            contents = source.read_bytes()
            destination = work / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
            snapshot = out / "fixtures" / relative
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(contents)
            external_fixtures[relative.as_posix()] = hashlib.sha256(contents).hexdigest()
    hashes = {}
    for relative in sorted(paths):
        contents = (ROOT / relative).read_bytes()
        if baseline and relative in [PACKAGE/'src/error_bar.rs',PACKAGE/'src/judgment_feedback.rs',PACKAGE/'src/placement.rs']:
            contents=subprocess.check_output(['git','show',f'{baseline}:{relative.as_posix()}'],cwd=ROOT)
            if relative.name!='placement.rs':
                module='feedback_kernel_perf' if relative.name=='error_bar.rs' else 'atlas_kernel_perf'
                attribute = '#[allow(clippy::approx_constant, reason="Fixture colors are authored RGB values.")]\n' if module=='feedback_kernel_perf' else ''
                contents+=f'\n#[cfg(test)]\n{attribute}mod {module} {{\n    include!("{module}.rs");\n}}\n'.encode()
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
    build_command = ["cargo", "test", "--offline", "--locked", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadsync-notefield", "--lib", "--tests", "--release", "--no-run", "--message-format=json"]
    with (out / "build.log").open("w", encoding="utf-8") as stream:
        build = subprocess.run(build_command, cwd=ROOT, env=build_env, stdout=stream, stderr=subprocess.STDOUT)
    if build.returncode: raise RuntimeError((out / "build.log").read_text(encoding="utf-8")[-12000:])
    binaries = []
    profiles = []
    for line in (out / "build.log").read_text(encoding="utf-8").splitlines():
        try: event = json.loads(line)
        except json.JSONDecodeError: continue
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "deadsync_notefield" and event.get("executable"):
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
                    checkout_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, encoding="utf-8").strip(),
                    external_fixture_root=str(fixture_root), external_fixtures=external_fixtures,
                    fixtures_sha256=sha(fixtures), shipped_png_count=len(pngs),
                    binary_sha256=sha(out / binary.name), build_sha256=sha(out / "build.log"),
                    correctness_sha256=sha(out / "correctness.log"), exit_code=result.returncode,
                    rustflags=build_env["RUSTFLAGS"], compile_temp=str(temporary), test_temp=os.environ.get("TEMP"),
                    rustc=subprocess.check_output(["rustc", "-Vv"], text=True),
                    lockfile=(work / "Cargo.lock").read_text(encoding="utf-8"),
                    passed_tests=sum(map(int, re.findall(r"test result: ok\. (\d+) passed", log))))
    (ROOT / f"docs/performance/2026-11-05-notefield-feedback-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if result.returncode: raise RuntimeError(log[-12000:])
    integration=[]
    for line in (out / 'build.log').read_text(encoding='utf-8').splitlines():
        try: event=json.loads(line)
        except json.JSONDecodeError: continue
        if event.get('reason')=='compiler-artifact' and event.get('target',{}).get('kind')==['test'] and event.get('executable'):
            integration.append(event)
    assert not integration, 'notefield has no integration-test targets'
    metadata['integration']={}
    for event in integration:
        name=event['target']['name'];test_binary=Path(event['executable']);archived=out/test_binary.name;shutil.copyfile(test_binary,archived)
        test_command=[str(archived),'--test-threads=1']
        with (out/f'{name}.log').open('w',encoding='utf-8') as stream:
            test_result=subprocess.run(test_command,cwd=work,stdout=stream,stderr=subprocess.STDOUT)
        test_log=(out/f'{name}.log').read_text(encoding='utf-8')
        metadata['integration'][name]=dict(command=test_command,binary_sha256=sha(archived),profile=event['profile'],log_sha256=sha(out/f'{name}.log'),exit_code=test_result.returncode,passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',test_log))))
        (ROOT / f'docs/performance/2026-11-05-notefield-feedback-{stage}.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
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
    def execute(namespace,test,filename,extra=None):
        command=[str(binary),namespace+'::'+test,'--exact','--ignored','--nocapture','--test-threads=1']
        result=subprocess.run(command,cwd=work,env=dict(os.environ,**(extra or {})),capture_output=True,text=True,encoding="utf-8")
        (out/filename).write_text(result.stdout+result.stderr,encoding='utf-8')
        if result.returncode:raise RuntimeError(result.stdout+result.stderr)
        return result.stdout+result.stderr
    namespaces=[('feedback','error_bar::feedback_kernel_perf','DEADSYNC_FEEDBACK_TRACE_OUT',28),('atlas','judgment_feedback::atlas_kernel_perf','DEADSYNC_ATLAS_TRACE_OUT',12)]
    parts={}
    for label,namespace,variable,count in namespaces:
        trace=out/(label+'-trace.bin');execute(namespace,'dump_trace',label+'-trace.log',{variable:str(trace)})
        parts[label]=dict(bytes=trace.stat().st_size,sha256=sha(trace))
    trace=out/'trace.bin'
    with trace.open('wb') as output:
        for label,_,_,_ in namespaces:
            source=out/(label+'-trace.bin');output.write(source.stat().st_size.to_bytes(8,'little'))
            with source.open('rb')as stream:shutil.copyfileobj(stream,output)
    reference=ARCHIVE/(reference_stage or 'baseline')/'trace.bin'
    reference_digest=sha(reference) if reference.exists() else None
    metadata['trace']=dict(bytes=trace.stat().st_size,sha256=sha(trace),parts=parts,reference=str(reference),reference_sha256=reference_digest)
    if reference_digest is not None:assert reference_digest==sha(trace),'trace differs'
    metadata['runs']=0 if debug else runs
    report=ROOT/f'docs/performance/2026-11-05-notefield-feedback-{stage}'
    report.with_suffix('.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    if debug:return
    rows=[];labels=set()
    for index in range(runs):
        for block,namespace,_,count in namespaces:
            output=execute(namespace,'benchmark',f'{block}-bench-{index}.log')
            pairs=re.findall(r'([\w/-]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)',output)
            assert len(pairs)==count and len({x[0]for x in pairs})==count
            labels.update(x[0]for x in pairs)
            churn={label:dict(re.findall(r'(\w+): (\d+)',fields)) for label,fields in re.findall(r'([\w/-]+): allocation Churn \{ ([^}]+) \}',output)}
            for label,a,b,c,d in pairs:
                for variant,cpu,ns in [('original',a,c),('current',b,d)]:
                    counters=churn[label+'/'+variant];assert len(counters)==6,label
                    rows.append(dict(run=index,workload=label,variant=variant,cycles_per_fixture=cpu,ns_per_fixture=ns,**{k:int(v)for k,v in counters.items()}))
            print(f'{stage}: {block} {index+1}/{runs}, {len(pairs)} workloads',flush=True)
    assert len(labels)==40 and len(rows)==runs*80
    with report.with_suffix('.tsv').open('w',newline='')as stream:
        writer=csv.DictWriter(stream,fieldnames=list(rows[0]),delimiter='\t');writer.writeheader();writer.writerows(rows)
    for label in sorted(labels):
        old=[float(r['cycles_per_fixture'])for r in rows if r['workload']==label and r['variant']=='original'];new=[float(r['cycles_per_fixture'])for r in rows if r['workload']==label and r['variant']=='current']
        print(f'{label}: {statistics.median(old):.2f} -> {statistics.median(new):.2f}; paired throughput {statistics.median(a/b for a,b in zip(old,new)):.3f}x',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('stage');parser.add_argument('--baseline');parser.add_argument('--debug',action='store_true');parser.add_argument('--runs',type=int,default=5);parser.add_argument('--reference')
    args=parser.parse_args()
    if args.runs<1:parser.error('positive runs required')
    run(args.stage,args.baseline,args.debug,args.runs,args.reference)
