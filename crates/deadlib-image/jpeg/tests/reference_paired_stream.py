"""Paired file output: DNL, real abbreviated inputs, many components, all processes.

The C inputs explicitly apply a raw external table bank and fill known heights.
This supplements, rather than replaces, genuine B.5 checks in reference_abbreviated.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, samples, check_frame_data
from stream_support import prefix_tables, update_tables, clear_stale_exp
from reference_stream_encode import known_heights


def sha(data):
    return hashlib.sha256(data).hexdigest()


def suffix(prefix, tail):
    return Path(str(prefix) + tail)


def run(command, env=None):
    result = subprocess.run([str(x) for x in command], capture_output=True, env=env, timeout=180)
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.returncode, result.stderr.decode(errors="replace"), result.stdout.decode(errors="replace")[:1000])
    return result.stdout.decode().strip()


def main():
    parser = argparse.ArgumentParser()
    for name in ("native", "ijg", "reference", "out"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    tools = args.out / "tools"
    tools.mkdir(exist_ok=True)
    hashes = {}
    for name in ("native", "ijg", "reference"):
        source = getattr(args, name)
        target = tools / (name + source.suffix)
        if not args.resume:
            shutil.copy2(source, target)
        hashes[name] = sha(target.read_bytes())
        assert hashes[name] == sha(source.read_bytes())
        setattr(args, name, target.resolve())
    test_root = Path(__file__).resolve().parent
    prior = test_root / "reports/stream-encode.json"
    sessions = json.loads(prior.read_text())["sources"]
    for source in sessions:
        for file, expected in zip(source["files"], source["sha256"], strict=True):
            assert sha((test_root / "fixtures" / file).read_bytes()) == expected
    identity = {"tools": hashes, "drivers": {name:sha((test_root/name).read_bytes()) for name in (Path(__file__).name,"reference_stream_encode.py","stream_support.py","hierarchy_support.py")}, "source_report_sha256":sha(prior.read_bytes()), "sources": sessions}
    checkpoint = args.out / "checkpoint.jsonl"
    if args.resume:
        old = [json.loads(line) for line in checkpoint.read_text().splitlines()]
        assert old[0] == identity
        done = {row["case"]:row for row in old[1:]}
    else:
        checkpoint.write_text(json.dumps(identity) + "\n")
        done = {}
    work = args.out / "case"
    work.mkdir(exist_ok=True)
    records, processes = [], set()
    for index, source in enumerate(sessions):
        paths = [test_root / "fixtures" / name for name in source["files"]]
        for buffer, fixed, restart in ((31,0,1),(8192,1,7)):
            key = f"s{index}-b{buffer}-f{fixed}-r{restart}"
            if key in done:
                records.append(done[key])
                processes.update(done[key]["processes"])
                continue
            (args.out / "current-case.json").write_text(json.dumps({"case":key,"source":source}))
            prefix = work / "encoded"
            assert run([args.native,"encode-stream",buffer,1,prefix,fixed,restart,2,*paths]) == "images 2"
            output_paths = [suffix(prefix,f".image-{i}{tail}") for i in range(2) for tail in (".tables.jpg",".jpg")]
            # Original DNL/abbreviated bytes are decoded with a real native
            # persistent cache, independently of the equivalent C inputs below.
            assert run([args.native,"decode",work/"native","--max-scans",8192,*output_paths]) == "tables 2 images 2"
            bank, results, markers = {}, [], set()
            for i in range(2):
                packet, image = (path.read_bytes() for path in output_paths[i*2:i*2+2])
                update_tables(bank, packet, True)
                header, frames, _, _, _ = structure(image, False)
                markers.update(frame["marker"] for frame in frames)
                ordinary_dct = header["marker"] != 0xde and frames[0]["marker"] in (0xc0,0xc1,0xc2,0xc9,0xca)
                ordinary_lp = header["marker"] != 0xde and not ordinary_dct
                tails = (["coef"] if ordinary_dct else ["samples"] if ordinary_lp else
                         [f"frame-{j}.{'samples' if f['marker'] in (0xc3,0xc7,0xcb,0xcf) else 'coef'}" for j,f in enumerate(frames)] + ["samples"])
                expected = []
                for tail in tails:
                    data = suffix(prefix,f".image-{i}.expected.{tail}").read_bytes()
                    assert suffix(work/"native",f".image-{i}.{tail}").read_bytes() == data, (key,i,tail)
                    expected.append(data)
                if not ordinary_dct and not ordinary_lp:
                    expected.pop()  # final composition is checked natively above
                equivalent = known_heights(prefix_tables(bank,image),header,frames)
                c_input = work / "equivalent-known-height.jpg"
                c_input.write_bytes(equivalent)
                if ordinary_dct:
                    run([args.ijg,c_input,work/"ijg.coef"])
                    assert (work/"ijg.coef").read_bytes() == expected[0], (key,i,"IJG")
                fractional = any(max(c[1] for c in f["components"])%c[1] or max(c[2] for c in f["components"])%c[2] for f in frames for c in f["components"])
                observation_keys = [(f["marker"],f["width"],f["height"]) for f in frames]
                duplicate = len(set(observation_keys)) != len(observation_keys)
                blocks, count, exp_copy = 0, 0, False
                if not fractional and not duplicate:
                    adjusted = clear_stale_exp(equivalent,frames)
                    exp_copy = adjusted != equivalent
                    c_input.write_bytes(adjusted)
                    observation = work / "reference.frames"
                    observation.write_text("")
                    pgx = work / "pgx"
                    run([args.reference,"-c","-U",c_input,pgx], os.environ|{"DEADLIB_JPEG_FRAME_DUMP":str(observation.resolve())})
                    if ordinary_lp and len(frames[0]["components"])<=4:
                        precision,_,_,planes = samples(expected[0])
                        for ci,(shape,values) in enumerate(planes):
                            assert suffix(pgx,f"_{ci}.h").read_text().split() == ["PG","ML",f"+{precision}",str(shape[3]),str(shape[4])]
                            raw = suffix(pgx,f"_{ci}.raw").read_bytes()
                            wanted = bytes(values) if precision<=8 else struct.pack(">"+"H"*len(values),*values)
                            assert raw == wanted, (key,i,ci,"PGX")
                            count += len(values)
                    else:
                        blocks,count = check_frame_data(frames,expected,observation.read_text())
                results.append({"image":i,"table_bytes":len(packet),"image_bytes":len(image),"table_sha256":sha(packet),"image_sha256":sha(image),"native_original_headers_exact":True,"IJG_exact":ordinary_dct,"JPEG_XT_exact":not fractional and not duplicate,"JPEG_XT_exclusion":"fractional parser limitation" if fractional else "observer cannot distinguish repeated frame keys" if duplicate else None,"C_external_table_copy":True,"C_known_height_copy":True,"C_EXP00_copy":exp_copy,"JPEG_XT_DCT_blocks":blocks,"JPEG_XT_sample_values":count})
                update_tables(bank,image)
            processes.update(markers)
            row = {"case":key,"source":source,"buffer_bytes":buffer,"fixed_Huffman":bool(fixed),"restart":restart,"DNL":True,"images":2,"file_vector_identical":True,"processes":sorted(markers),"results":results}
            records.append(row)
            with checkpoint.open("a") as file:
                file.write(json.dumps(row)+"\n")
            if len(records)%25==0:
                print(json.dumps({"sessions":len(records),"last":key}),flush=True)
    assert len(records)==148 and processes == {0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf}
    images = [image for row in records for image in row["results"]]
    report = {**identity,"scope":"Paired incremental file output with exact vector identity and native original-header cache checks. C inputs explicitly reconstruct external table state and fill known DNL heights; these are entropy checks, not genuine C abbreviated/DNL parsing.","sessions":len(records),"images":len(images),"processes":sorted(processes),"IJG_exact_images":sum(i["IJG_exact"] for i in images),"JPEG_XT_exact_images":sum(i["JPEG_XT_exact"] for i in images),"JPEG_XT_DCT_blocks":sum(i["JPEG_XT_DCT_blocks"] for i in images),"JPEG_XT_sample_values":sum(i["JPEG_XT_sample_values"] for i in images),"C_external_table_and_known_height_copies":len(images),"C_EXP00_copies":sum(i["C_EXP00_copy"] for i in images),"results":records}
    (args.out/"report.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({k:v for k,v in report.items() if k not in ("results","sources","tools","drivers")}),flush=True)


if __name__ == "__main__":
    main()
