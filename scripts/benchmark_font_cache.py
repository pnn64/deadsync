"""Snapshot font-cache kernels and measure thread cycles and allocations."""
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
ARCHIVE = Path(os.environ.get("DEADSYNC_FONT_CACHE_ARCHIVE", ROOT / "target/font-cache"))
PACKAGE = Path("crates/deadlib-present")


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


def full_tests(stage):
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
    # Preserve cache mtimes when extending the earlier simfile workspace.
    with tarfile.open(supplement) as source:
        for member in source:
            if member.isdir():
                continue
            assert member.isfile(), member.name
            destination = (work / member.name).resolve()
            assert destination.is_relative_to(work.resolve())
            contents = source.extractfile(member).read()
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != contents:
                destination.write_bytes(contents)
    workspace_manifest = base["workspace_manifest"].replace('members = ["crates/deadsync-simfile"]', 'members = ["crates/deadsync-simfile", "crates/deadsync-notefield"]')
    workspace_manifest = workspace_manifest.replace('[workspace.dependencies]', '[workspace.package]\nversion = ' + json.dumps(root_manifest["workspace"]["package"]["version"]) + '\n[workspace.dependencies]\nmlua = { package = "deadlib-lua-legacy", version = "=0.12.2", path = "crates/deadlib-lua-legacy" }')
    assert '"crates/deadsync-notefield"' in workspace_manifest
    if (work / "Cargo.toml").read_text(encoding="utf-8") != workspace_manifest:
        (work / "Cargo.toml").write_text(workspace_manifest, encoding="utf-8")
    changed = subprocess.check_output(["git", "diff", "--name-only", base["git_revision"], "HEAD", "--", "crates", "tests", "vendor", "Cargo.lock"], cwd=ROOT, text=True).splitlines()
    paths = {Path(name) for name in changed}
    paths.update(path.relative_to(ROOT) for path in (ROOT / PACKAGE).rglob("*.rs"))
    paths.add(PACKAGE / "Cargo.toml")
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
    hashes = {}
    for relative in sorted(paths):
        contents = (ROOT / relative).read_bytes()
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
    build_command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadlib-present", "--lib", "--release", "--no-run", "--message-format=json"]
    with (out / "build.log").open("w", encoding="utf-8") as stream:
        build = subprocess.run(build_command, cwd=ROOT, env=build_env, stdout=stream, stderr=subprocess.STDOUT)
    if build.returncode: raise RuntimeError((out / "build.log").read_text(encoding="utf-8")[-12000:])
    binaries = []
    for line in (out / "build.log").read_text(encoding="utf-8").splitlines():
        try: event = json.loads(line)
        except json.JSONDecodeError: continue
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == "deadlib_present" and event.get("executable"):
            binaries.append(Path(event["executable"]))
    assert len(binaries) == 1, binaries
    binary = binaries[0]
    command = [str(binary), "--test-threads=1"]
    with (out / "correctness.log").open("w", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=work, stdout=stream, stderr=subprocess.STDOUT)
    shutil.copyfile(binary, out / binary.name)
    log = (out / "correctness.log").read_text(encoding="utf-8")
    metadata = dict(build_command=build_command, command=command, base_revision=base["git_revision"],
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
    (ROOT / f"docs/performance/2026-11-04-font-cache-{stage}.manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if result.returncode: raise RuntimeError(log[-12000:])
    print(f"{stage}: {metadata['passed_tests']} tests passed", flush=True)


def run(stage, runs, debug, baseline, reference_stage, exhaustive):
    out = ARCHIVE / stage
    out.mkdir(parents=True, exist_ok=True)
    paths = [PACKAGE / ('src/' + name) for name in ['font.rs','font_cache_original.rs','font_cache_checks.rs','font_cache_performance.rs','font_cache_ablation.rs']] + [Path('tests/support/perf.rs'),Path('tests/support/kernel_bench.rs'),Path(__file__).relative_to(ROOT)]
    hashes = {}
    for relative in paths:
        contents = (ROOT / relative).read_bytes()
        if baseline and relative == PACKAGE / 'src/font.rs':
            contents = subprocess.check_output(['git','show',f'{baseline}:{relative.as_posix()}'],cwd=ROOT)
        destination = out / 'source' / relative
        destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes(contents)
        hashes[relative.as_posix()] = sha(destination)
    source_root = out / 'source'
    font = (source_root / PACKAGE / 'src/font.rs').read_text(encoding='utf-8')
    types = '\n'.join('#[derive(Debug,Clone)]\n'+item(font,'pub struct '+name+' {') for name in ['Glyph','Font'])
    types += '\n'+ '\n'.join(re.findall(r'pub type (?:GlyphMap|FontMap) = [^;]+;',font))
    functions = '\n'.join(('#[inline(always)]\n#[must_use]\n' if name=='round_half_to_even_i32' else '#[inline(always)]\n' if name=='compute_ascii_glyphs' and '#[inline(always)]\nfn compute_ascii_glyphs(' in font else '')+item(font,('pub fn ' if name=='refresh_chain_keys' else 'fn ')+name+'(') for name in ['compute_chain_key','refresh_chain_keys','compute_ascii_glyphs','round_half_to_even_i32'])
    source = '#![allow(dead_code)]\nuse std::{collections::HashMap,sync::Arc,hash::{DefaultHasher,Hash,Hasher}};\nuse rustc_hash::FxBuildHasher;\nuse smallvec::SmallVec;\n'+types+'\n'+functions
    for name,path in [('perf_alloc',Path('tests/support/perf.rs')),('perf_bench',Path('tests/support/kernel_bench.rs')),('font_cache_checks',PACKAGE/'src/font_cache_checks.rs'),('font_cache_performance',PACKAGE/'src/font_cache_performance.rs')]:
        source += f'\n#[path="{(source_root/path).as_posix()}"] mod {name};\n'
    compiled = out / 'compiled.rs'
    compiled.write_text(source,encoding='utf-8')
    deps = Path(os.environ.get('DEADSYNC_FONT_DEPS','D:/deadsync-partner-charts-20261104/full-workspace/target/release/deps'))
    libraries = {name: sorted(deps.glob('lib'+name+'-*.rlib'))[0] for name in ['rustc_hash','smallvec']}
    binary = out / 'font_cache.exe'
    command=['rustc','--test','--edition=2024','-C','opt-level='+('0' if debug else '3'),'-C','target-feature=+crt-static','-C','debuginfo=0','-C','lto=fat']
    for name,path in libraries.items():command+=['--extern',f'{name}={path}']
    command+=['-L',f'dependency={deps}',str(compiled),'-o',str(binary)]
    temporary=ARCHIVE/'tmp';temporary.mkdir(exist_ok=True)
    env=dict(os.environ,TEMP=str(temporary),TMP=str(temporary))
    def execute(args,filename,extra=None):
        result=subprocess.run(args,cwd=ROOT,env=env if extra is None else dict(env,**extra),capture_output=True,text=True)
        (out/filename).write_text(result.stdout+result.stderr,encoding='utf-8')
        if result.returncode:raise RuntimeError(result.stdout+result.stderr)
        return result.stdout
    execute(command,'build.log')
    output=execute([str(binary),'--test-threads=1'],'correctness.log')
    trace=out/'font-cache-trace.bin'
    execute([str(binary),'font_cache_checks::dump_font_cache_trace','--exact','--ignored','--nocapture','--test-threads=1'],'trace.log',{'DEADSYNC_FONT_CACHE_TRACE_OUT':str(trace)})
    reference=ARCHIVE/(reference_stage or 'baseline')/trace.name
    if reference.exists() and trace!=reference:assert filecmp.cmp(trace,reference,shallow=False),'font trace differs'
    if exhaustive:execute([str(binary),'font_cache_checks::exhaustive_rounding_domain','--exact','--ignored','--test-threads=1'],'exhaustive.log')
    metadata=dict(stage=stage,command=command,baseline_revision=baseline,sources=hashes,fragments_sha256=hashlib.sha256(functions.encode()).hexdigest(),compiled_sha256=sha(compiled),binary_sha256=sha(binary),correctness_sha256=sha(out/'correctness.log'),libraries={name:dict(path=str(path),sha256=sha(path)) for name,path in libraries.items()},rustc=subprocess.check_output(['rustc','-Vv'],text=True),cpu=os.environ.get('PROCESSOR_IDENTIFIER'),passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',output))),runs=0 if debug else runs,ablation_runs=0 if debug or baseline else runs,trace_sha256=sha(trace),trace_bytes=trace.stat().st_size,reference_trace=str(reference),reference_trace_sha256=sha(reference) if reference.exists() else None,exhaustive=exhaustive,exhaustive_sha256=sha(out/'exhaustive.log') if exhaustive else None)
    report=ROOT/f'docs/performance/2026-11-04-font-cache-{stage}'
    report.with_suffix('.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    if debug:
        print(f'{stage}: {metadata["passed_tests"]} tests passed',flush=True);return
    rows=[]
    for index in range(runs):
        output=execute([str(binary),'font_cache_performance::benchmark_font_cache','--exact','--ignored','--nocapture','--test-threads=1'],f'run-{index}.log')
        if not baseline:output+=execute([str(binary),'font_cache_performance::benchmark_ablations','--exact','--ignored','--nocapture','--test-threads=1'],f'ablation-{index}.log')
        pairs=re.findall(r'([\w/]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)',output)
        assert len(pairs)==(27 if baseline else 33),output
        churn={label:dict(re.findall(r'(\w+): (\d+)',fields)) for label,fields in re.findall(r'([\w/]+): allocation Churn \{ ([^}]+) \}',output)}
        for label,a,b,c,d in pairs:
            for variant,cpu,ns in [('original',a,c),('current',b,d)]:
                counts={k:int(v) for k,v in churn[label+'/'+variant].items()}
                if label.startswith('refresh/33/'):
                    assert churn[label+'/original']==churn[label+'/current']
                else:assert not any(counts.values()),(label,counts)
                rows.append(dict(run=index,workload=label,variant=variant,cycles_per_fixture=cpu,ns_per_fixture=ns,**counts))
        print(f'{stage}: run {index+1}/{runs}',flush=True)
    with report.with_suffix('.tsv').open('w',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=list(rows[0]),delimiter='\t');writer.writeheader();writer.writerows(rows)
    for label in sorted({r['workload'] for r in rows}):
        old=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='original']
        new=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='current']
        print(f'{label}: {statistics.median(old):.2f} -> {statistics.median(new):.2f}; paired throughput {statistics.median(a/b for a,b in zip(old,new)):.3f}x',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage');parser.add_argument('--runs',type=int,default=3);parser.add_argument('--debug',action='store_true');parser.add_argument('--baseline');parser.add_argument('--reference');parser.add_argument('--full',action='store_true');parser.add_argument('--exhaustive',action='store_true')
    args=parser.parse_args()
    if args.runs<1 or (args.full and (args.debug or args.exhaustive)) or (args.debug and args.exhaustive):parser.error('positive runs; exhaustive only for release kernels')
    if args.full:full_tests(args.stage)
    else:run(args.stage,args.runs,args.debug,args.baseline,args.reference,args.exhaustive)
