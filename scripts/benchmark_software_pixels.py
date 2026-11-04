"""Snapshot software pixel kernels and measure thread cycles and allocations."""
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
ARCHIVE = Path(os.environ.get("DEADSYNC_SOFTWARE_PIXELS_ARCHIVE", ROOT / "target/software-pixels"))
PACKAGE = Path("crates/deadlib-render-backend-software")


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
    if debug:
        workspace_manifest += '\n[profile.release.package.deadlib-render-backend-software]\nopt-level = 0\ndebug-assertions = true\noverflow-checks = true\n'
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
            # The new integration harness also benchmarks the original row loop.
            # Backfill that test-only name without changing baseline call sites.
            contents += b'\n#[cfg(test)]\nfn copy_target_rows<T: Copy>(source: &[T], dest: &mut [T], source_width: usize, dest_width: usize) {\n    for (source, dest) in source.chunks_exact(source_width).zip(dest.chunks_exact_mut(dest_width)) {\n        dest[..source.len()].copy_from_slice(source);\n    }\n}\n'
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
    build_command = ["cargo", "test", "--offline", "--jobs", "2", "--manifest-path", str(work / "Cargo.toml"), "-p", "deadlib-render-backend-software", "--lib", "--test", "pixel_paths", "--test", "half_kernels", "--test", "texture_pipeline", "--release", "--no-run", "--message-format=json"]
    with (out / "build.log").open("w", encoding="utf-8") as stream:
        build = subprocess.run(build_command, cwd=ROOT, env=build_env, stdout=stream, stderr=subprocess.STDOUT)
    if build.returncode: raise RuntimeError((out / "build.log").read_text(encoding="utf-8")[-12000:])
    binaries = {}
    wanted = ['deadlib_render_backend_software','pixel_paths','half_kernels','texture_pipeline']
    for line in (out / 'build.log').read_text(encoding='utf-8').splitlines():
        try: event = json.loads(line)
        except json.JSONDecodeError: continue
        name = event.get('target',{}).get('name')
        if event.get('reason') == 'compiler-artifact' and name in wanted and event.get('executable'):
            assert name not in binaries,name
            binary = Path(event['executable'])
            destination = out / binary.name
            shutil.copyfile(binary,destination)
            binaries[name] = dict(path=str(destination),sha256=sha(destination),profile=event['profile'])
    assert set(binaries)==set(wanted),list(binaries)
    for name, artifact in binaries.items():
        command = [artifact['path'],'--test-threads=1']
        with (out / (name+'-correctness.log')).open('w',encoding='utf-8') as stream:
            result = subprocess.run(command,cwd=work,stdout=stream,stderr=subprocess.STDOUT)
        log = (out / (name+'-correctness.log')).read_text(encoding='utf-8')
        artifact.update(command=command,exit_code=result.returncode,correctness_sha256=sha(out / (name+'-correctness.log')),
            passed_tests=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',log))),
            ignored_tests=sum(map(int,re.findall(r'test result: ok\. \d+ passed; \d+ failed; (\d+) ignored',log))))
        if result.returncode: raise RuntimeError(name+'\n'+log[-12000:])
    metadata = dict(build_command=build_command,base_revision=base['git_revision'],baseline_revision=baseline,debug=debug,
        source_archive=str(archive),source_archive_sha256=sha(archive),workspace_manifest=workspace_manifest,overlays=hashes,
        dependency_archive=str(supplement),dependency_archive_sha256=sha(supplement),dependency_packages=sorted(packages),
        external_fixture_root=str(fixture_root),external_fixtures=external_fixtures,fixtures_sha256=sha(fixtures),shipped_png_count=len(pngs),
        checkout_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        build_sha256=sha(out / 'build.log'),exit_code=0,rustflags=build_env['RUSTFLAGS'],compile_temp=str(temporary),test_temp=os.environ.get('TEMP'),
        rustc=subprocess.check_output(['rustc','-Vv'],text=True),lockfile=(work / 'Cargo.lock').read_text(encoding='utf-8'),binaries=binaries,
        crate_profile=binaries['pixel_paths']['profile'],passed_tests=sum(x['passed_tests'] for x in binaries.values()))
    for name in binaries: assert binaries[name]['profile']['opt_level']==('0' if debug else '3')
    report = ROOT / f'docs/performance/2026-11-04-software-pixels-{stage}.manifest.json'
    report.write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    print(stage+': '+str({name:(x['passed_tests'],x['ignored_tests']) for name,x in binaries.items()}),flush=True)
    return metadata,Path(binaries['pixel_paths']['path']),work

def run(stage,baseline,debug,runs,reference_stage):
    metadata,binary,work=full_tests(stage,baseline,debug)
    out=ARCHIVE/stage
    metadata['driver_sources']={}
    for relative in [Path(__file__).relative_to(ROOT),Path('tests/support/kernel_bench.rs'),Path('tests/support/perf.rs')]:
        destination=out/'source'/relative
        destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes((ROOT/relative).read_bytes())
        metadata['driver_sources'][relative.as_posix()]=sha(destination)
    def execute(test,filename,extra=None):
        command=[str(binary),'backend::pixel_paths_performance::'+test,'--exact','--ignored','--nocapture','--test-threads=1']
        result=subprocess.run(command,cwd=work,env=dict(os.environ,**(extra or {})),capture_output=True,text=True)
        (out/filename).write_text(result.stdout+result.stderr,encoding='utf-8')
        if result.returncode:raise RuntimeError(result.stdout+result.stderr)
        return result.stdout+result.stderr
    execute('dump_pixel_traces','traces.log',{'DEADSYNC_PIXEL_TRACE_DIR':str(out/'traces')})
    metadata['traces']={}
    for name in ['half','writer','copy','byte']:
        path=out/'traces'/(name+'.bin');reference=ARCHIVE/(reference_stage or 'baseline')/'traces'/path.name
        digest=sha(path);reference_digest=sha(reference) if reference.exists() else None
        metadata['traces'][name]=dict(bytes=path.stat().st_size,sha256=digest,reference=str(reference),reference_sha256=reference_digest)
        if reference_digest is not None: assert digest==reference_digest,(stage,name,'trace differs')
    metadata['runs']=0 if debug else runs
    report=ROOT/f'docs/performance/2026-11-04-software-pixels-{stage}'
    report.with_suffix('.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    if debug:return
    rows=[]
    labels=None
    for index in range(runs):
        output=''.join(execute('benchmark_'+family+'_candidates',f'{family}-{index}.log') for family in ['pixel','copy','byte'])
        pairs=re.findall(r'([\w/-]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)',output)
        current_labels={x[0] for x in pairs}
        assert len(pairs)==len(current_labels) and len(pairs)>=60,len(pairs)
        if labels is None:labels=current_labels
        else:assert labels==current_labels
        churn={label:dict(re.findall(r'(\w+): (\d+)',fields)) for label,fields in re.findall(r'([\w/-]+): allocation Churn \{ ([^}]+) \}',output)}
        for label,a,b,c,d in pairs:
            assert churn[label+'/original']==churn[label+'/current'] and all(int(v)==0 for v in churn[label+'/current'].values()),label
            for variant,cpu,ns in [('original',a,c),('current',b,d)]:rows.append(dict(run=index,workload=label,variant=variant,cycles_per_fixture=cpu,ns_per_fixture=ns,**{k:int(v) for k,v in churn[label+'/'+variant].items()}))
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
