"""Generate bounded song Lua programs and compare them with ITGmania observations.

Uses only Python's standard library. See docs/testing/song-lua-fuzzing.md.
"""
import argparse
from collections import Counter
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import random
import re
import signal
import struct
import subprocess
import sys
import time
import wave
import zlib
from song_lua_fuzz_catalog import TRANSFORMS, TWEENS, COLORS, EFFECTS, EXCLUSIONS, tween_call


ROOT = Path(__file__).resolve().parents[1]
VERSION = 2
FACTORS = {
    "actors": ("quad", "sprite", "mixed"),
    "hierarchy": ("flat", "nested", "deep"),
    "transform": tuple(TRANSFORMS),
    "tween": TWEENS,
    "queue": ("queued", "interrupted", "finished", "hurried"),
    "color": ("diffuse", "alpha", "glow", "inherited", *COLORS),
    "effect": tuple(EFFECTS),
    "clock": ("timer", "beat", "music", "beatnooffset", "musicnooffset"),
    "commands": ("direct", "queue", "broadcast"),
    "modifiers": ("none", "global", "column"),
    "notefield": ("none", "transform", "spline"),
    "timing": ("steady", "bpm", "stop", "delay"),
    "logic": ("closures", "loop", "branch"),
}
TEST_NAME = "generated_song_lua_compiled_matches_itgmania"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    temporary.replace(path)


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def case_seed(seed, index):
    value = hashlib.sha256(f"song-lua-v{VERSION}:{seed}:{index}".encode()).digest()
    return int.from_bytes(value[:8], "big")


def pairs(row):
    return {(a, row[a], b, row[b]) for a, b in itertools.combinations(FACTORS, 2) if a in row and b in row}


def all_pairs():
    return {(a, x, b, y) for a, b in itertools.combinations(FACTORS, 2)
            for x in FACTORS[a] for y in FACTORS[b]}


def factor_rows(seed, count, strategy):
    """Greedy covering-array prefix, followed by unrestricted random rows.

    The prefix is independent of requested campaign length. Coverage is measured,
    not assumed: a short campaign can leave pairs uncovered.
    """
    uncovered = all_pairs()
    for index in range(count):
        rng = random.Random(case_seed(seed, index))
        candidates = [{key: rng.choice(values) for key, values in FACTORS.items()}
                      for _ in range(64 if strategy == "pairwise" and uncovered else 1)]
        row = max(candidates, key=lambda item: len(pairs(item) & uncovered))
        uncovered.difference_update(pairs(row))
        yield row


def coverage(rows):
    observed = set().union(*(pairs(row) for row in rows)) if rows else set()
    total = all_pairs()
    return {"covered": len(observed & total), "total": len(total),
            "missing": [list(pair) for pair in sorted(total - observed)]}


def api_coverage(methods):
    inventory = read_json(ROOT / "tests/fixtures/itgmania-song-lua-micro/actor-class-inventory.json")
    return {"meaning": "authored method names, not runtime branch or argument coverage",
            "authored_methods": sorted(methods),
            "classes": {name: {"authored": sorted(set(info["methods"]) & methods),
                                "not_authored": sorted(set(info["methods"]) - methods)}
                        for name, info in inventory["classes"].items()},
            "exclusions": EXCLUSIONS}


def generate_case(seed, index, factors):
    rng = random.Random(case_seed(seed, index))
    actors = []
    for number in range(rng.randint(2, 5)):
        kind = factors["actors"]
        if kind == "mixed":
            kind = "quad" if number % 2 == 0 else "sprite"
        actors.append({
            "kind": kind, "x": rng.choice([-128, -1, 0, 1, 128, 256]),
            "y": rng.choice([-96, 0, 96, 240]), "z": rng.choice([-16, 0, 16]),
            "rotation": [rng.choice([-90, -15, 0, 15, 90]) for _ in range(3)],
            "zoom": rng.choice([0.25, 0.5, 1, 1.5]),
            "skew": rng.choice([-0.25, 0, 0.25]),
            "align": rng.choice([0, 0.5, 1]),
            "color": [rng.choice([0, 0.25, 0.5, 1]) for _ in range(3)] + [rng.choice([0.25, 0.5, 1])],
            "duration": rng.choice([0, 1 / 60, 0.125, 0.25, 0.5]),
            "target": rng.choice([-120, -1, 0, 1, 120]),
            "order": rng.choice([-1, 0, 1]),
        })
    rng.shuffle(actors)
    return {"version": VERSION, "campaign_seed": seed, "index": index,
            "seed": case_seed(seed, index), "lua_seed": rng.randint(1, 2147483647),
            "factors": factors, "actors": actors,
            "bpm": rng.choice([90, 120, 150, 180]),
            "next_bpm": rng.choice([75, 135, 240]),
            "pause": rng.choice([1 / 60, 0.125, 0.5]),
            "boundary": rng.choice([0.5, 1, 2]),
            "percentage": rng.choice([-100, -25, 25, 50, 100]),
            "column": rng.randint(1, 4)}


def number(value):
    return format(value, ".12g")


def lua_source(case):
    f = case["factors"]
    lines = [f"-- Generated song Lua v{VERSION}; seed {case['seed']}; case {case['index']}",
             "local actors = {}", "local fired = {}", "local root = {Name='FuzzRoot', FOV=0}"]
    # The host supplies the seed. Lua randomness here tests the engine RNG too.
    if f["logic"] == "closures":
        lines += ["local function make_adjust(offset)", "  return function(value) return value + offset end",
                  "end", f"local adjust = make_adjust({case['column']})"]
    else:
        lines += ["local function adjust(value)",
                  ("  if math.random(1, 2) == 1 then return -value end" if f["logic"] == "branch"
                   else "  for i=1,3 do value = value + i end"), "  return value", "end"]
    for i, actor in enumerate(case["actors"], 1):
        constructor = "Sprite" if actor["kind"] == "sprite" else "Quad"
        lines += [f"local a{i} = Def.{constructor}{{", f"  Name='FuzzActor{i}',"]
        if constructor == "Sprite":
            lines.append("  Texture='texture.png',")
        lines += ["  InitCommand=function(self)", f"    actors[{i}] = self",
                  f"    self:xy({actor['x']},{actor['y']}):z({actor['z']}):zoom({actor['zoom']})",
                  f"      :rotationx({actor['rotation'][0]}):rotationy({actor['rotation'][1]}):rotationz({actor['rotation'][2]})",
                  f"      :skewx({actor['skew']}):halign({actor['align']}):valign({actor['align']}):draworder({actor['order']})"]
        if constructor == "Quad":
            lines.append("    self:setsize(24,32)")
        if f["color"] == "diffuse":
            lines.append("    self:diffuse(" + ",".join(map(number, actor["color"])) + ")")
        elif f["color"] == "alpha":
            lines.append(f"    self:diffusealpha({actor['color'][3]})")
        elif f["color"] == "glow":
            lines.append("    self:glow(" + ",".join(map(number, actor["color"])) + ")")
        elif f["color"] in COLORS:
            lines.append("    self" + COLORS[f["color"]])
        lines.append("    self" + TRANSFORMS[f["transform"]])
        if EFFECTS[f["effect"]]:
            lines.append("    self" + EFFECTS[f["effect"]])
        if f["effect"] != "none":
            lines.append(f"    self:effectclock('{f['clock']}'):effectperiod(0.5):effectoffset(0.125)")
        lines += ["  end,", "  MoveCommand=function(self)",
                  f"    self:{tween_call(f['tween'], number(actor['duration']))}:x(adjust({actor['target']}))"
                  + re.sub(r"-?\d+(?:\.\d+)?", lambda m: number(float(m[0]) * 1.25), TRANSFORMS[f["transform"]]).replace("'left'", "'right'").replace("'bottom'", "'top'"),
                  "      :linear(0.125):y(16):sleep(0.016666666667):decelerate(0.25):zoom(0.75)"]
        if f["queue"] == "interrupted":
            lines.append("    self:stoptweening():accelerate(0.125):x(8)")
        elif f["queue"] == "finished":
            lines.append("    self:finishtweening():linear(0.125):x(-8)")
        elif f["queue"] == "hurried":
            lines.append("    self:hurrytweening(0.5)")
        lines += ["  end,", "  OnCommand=function(self)"]
        if f["commands"] == "direct":
            lines.append("    self:playcommand('Move')")
        elif f["commands"] == "queue":
            lines.append("    self:sleep(0.016666666667):queuecommand('Move')")
        lines += ["  end,", "  FuzzMoveMessageCommand=function(self) self:playcommand('Move') end,",
                  "}"]
        lines.append(f"root[#root+1] = a{i}")
    # One controller deliberately crosses the same boundary three ways. Closures
    # retain fired state and actor references; every callback remains bounded.
    # Native Foreground retires an animation when its startup tween queue ends.
    # Keep it alive for the entire eight-beat capture, including later callbacks.
    lines += ["root.OnCommand=function(self)", "  self:sleep(12)"]
    if f["color"] == "inherited":
        lines.append("  self:diffuse(0.5,0.75,1,0.5)")
    if f["commands"] == "broadcast":
        lines.append("  MESSAGEMAN:Broadcast('FuzzMove')")
    lines += ["  self:SetUpdateFunction(function(self)",
              "    local beat = GAMESTATE:GetSongBeat()",
              f"    for i,threshold in ipairs{{{number(case['boundary'] - 1 / 192)}, {number(case['boundary'])}, {number(case['boundary'] + 1 / 192)}}} do",
              "      if beat >= threshold and not fired[i] then", "        fired[i] = true",
              "        for _,actor in ipairs(actors) do actor:rotationz(i*15) end"]
    if f["commands"] == "broadcast":
        lines.append("        MESSAGEMAN:Broadcast('FuzzMove')")
    if f["modifiers"] != "none":
        mod = "Drunk" if f["modifiers"] == "global" else f"MoveX{case['column']}"
        lines += ["        local po = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')",
                  f"        po:FromString('*9999 {case['percentage']}% {mod}, *9999 25% Reverse')"]
    if f["notefield"] != "none":
        lines += ["        local field = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')",
                  f"        local column = field:GetColumnActors()[{case['column']}]"]
        if f["notefield"] == "transform":
            lines.append("        column:x(i*4):rotationz(i*5):zoomy(0.75)")
        else:
            lines += ["        local handler = column:GetPosHandler()",
                      "        handler:SetSplineMode('NoteColumnSplineMode_Offset'):SetBeatsPerT(1)",
                      "        handler:GetSpline():SetSize(2):SetPoint(1,{i*4,0,0}):SetPoint(2,{0,i*8,0}):Solve()"]
    lines += ["      end", "    end", "  end)", "end"]
    # Preserve insertion order and tie-breaking draworder while adding ancestry.
    if f["hierarchy"] != "flat":
        lines += ["local group = {Name='FuzzGroup', InitCommand=function(self) self:xy(8,-4):rotationz(10) end}",
                  "for i=1,#root do group[#group+1] = root[i]; root[i] = nil end",
                  "root[1] = Def.ActorFrame(group)"]
        if f["hierarchy"] == "deep":
            lines.append("root[1] = Def.ActorFrame{Name='FuzzOuter', InitCommand=function(self) self:zoom(0.75) end, root[1]}")
    lines.append("return Def.ActorFrame(root)")
    return "\n".join(lines) + "\n"


def simfile_source(case):
    f = case["factors"]
    timing = f"#BPMS:0={case['bpm']}"
    if f["timing"] == "bpm":
        timing += f",2={case['next_bpm']}"
    timing += ";\n"
    if f["timing"] in ("stop", "delay"):
        timing += f"#{'STOPS' if f['timing'] == 'stop' else 'DELAYS'}:2={number(case['pause'])};\n"
    return (f"#TITLE:Lua Fuzz {case['index']};\n#MUSIC:silence.wav;\n#OFFSET:0;\n"
            + timing + "#FGCHANGES:0=default.lua=1=0=0=1;\n"
            "#NOTES:dance-single::Challenge:1:0,0,0,0,0:\n"
            "1000\n0100\n0010\n0001\n,\n1000\n0100\n0010\n0001\n;\n")


def write_case(folder, case):
    folder.mkdir(parents=True, exist_ok=False)
    write_json(folder / "case.json", case)
    (folder / "default.lua").write_text(lua_source(case), encoding="utf-8")
    (folder / "song.sm").write_text(simfile_source(case), encoding="utf-8")
    # Tiny deterministic assets keep both engines independent of external packs.
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    pixels = b"".join(b"\0" + bytes([255, 128, 64, 255]) * 16 for _ in range(16))
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 16, 16, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))
    (folder / "texture.png").write_bytes(png)
    with wave.open(str(folder / "silence.wav"), "wb") as audio:
        audio.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
        audio.writeframes(b"\0\0" * 8000 * 12)


def run_process(command, cwd, log, timeout, environment=None):
    started = time.monotonic()
    command = [str(arg) for arg in command]
    options = ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.CREATE_NO_WINDOW}
               if os.name == "nt" else {"start_new_session": True})
    with log.open("wb") as output:
        process = subprocess.Popen(command, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                                   stdout=output, stderr=subprocess.STDOUT, **options)
        def stop_tree():
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               creationflags=subprocess.CREATE_NO_WINDOW)
            else:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            process.wait()
        try:
            code = process.wait(timeout=timeout)
            return {"returncode": code, "seconds": time.monotonic() - started, "command": command}
        except subprocess.TimeoutExpired:
            stop_tree()
            return {"timeout": True, "seconds": time.monotonic() - started,
                    "command": command}
        except KeyboardInterrupt:
            stop_tree()
            raise


def build_runner(oracle, folder):
    target = "song_lua_itgmania_semantic_parity"
    command = ["cargo", "test", "--test", target, "--no-run", "--message-format=json"]
    with (folder / "build.log").open("wb") as errors:
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=errors, check=True)
    executables = []
    for line in result.stdout.splitlines():
        item = json.loads(line)
        if item.get("reason") == "compiler-artifact" and item["target"]["name"] == target and item.get("executable"):
            executables.append(Path(item["executable"]))
    if len(executables) != 1:
        raise ValueError(f"expected one built {target} executable, got {executables}")
    return executables[0]


def evaluate_case(case_dir, case, output, args):
    output.mkdir(parents=True, exist_ok=False)
    simfile = case_dir / "song.sm"
    report = output / "deadsync.json"
    result = {"index": case["index"], "seed": case["seed"], "factors": case["factors"],
              "case": str(case_dir), "oracle": args.oracle, "deadsync_execution": "compiled",
              "sources": {name: digest(case_dir / name) for name in
                          ("case.json", "default.lua", "song.sm", "texture.png", "silence.wav")}}
    if args.oracle == "semantic":
        trace = output / "oracle" / "song.sm.semantic.json"
        command = [args.harness, "song-lua-semantic-baseline", case_dir, "--out", output / "oracle",
                   "--random-seed", case["lua_seed"], "--beat-step", args.beat_step,
                   "--until-beat", 8, "--max-events", args.max_events]
    else:
        trace = output / "native.json"
        command = [args.harness, "song-lua-trace", simfile, "--itgmania-bin", args.itgmania_bin,
                   "--out", trace, "--headless", "--drawable-samples", "--until-beat", 8,
                   "--capture-fps", 60, "--fixed-update-fps", 60, "--random-seed", case["lua_seed"],
                   "--fallback-theme", args.fallback_theme,
                   "--max-events", args.max_events, "--timeout", max(1, int(args.timeout) - 10)]
    capture_environment = os.environ.copy()
    if args.oracle == "semantic":
        # The semantic comparator verifies every referenced noteskin asset hash.
        # Both sides must read the workspace assets, not two installations' skins.
        shared_assets = {"ITGMANIA_SONG_LUA_NOTESKIN_ROOT": str(ROOT / "assets/noteskins"),
                         "ITGMANIA_SONG_LUA_NOTESKIN": "cyber"}
        capture_environment.update(shared_assets)
        capture_environment.pop("ITGMANIA_SONG_LUA_JUDGMENT", None)
        result["capture_environment"] = shared_assets
    result["capture"] = run_process(command, ROOT, output / "oracle.log", args.timeout, capture_environment)
    if result["capture"].get("timeout"):
        result["status"] = "oracle_timeout"
    elif result["capture"]["returncode"] != 0 or not trace.is_file():
        result["status"] = "oracle_error"
    else:
        try:
            data = read_json(trace)
            result["capabilities"] = data.get("capabilities", {})
            if args.oracle == "semantic":
                manifest = read_json(output / "oracle" / "_semantic_manifest.json")
                complete = (len(manifest["simfiles"]) == 1
                            and manifest["simfiles"][0]["status"] == "ok"
                            and manifest["simfiles"][0]["lua_entries"] > 0
                            and data.get("runtime_errors") == [] and data.get("dropped_events") == 0)
            else:
                # The compiled comparison performs full native capture validation.
                complete = data.get("dropped_events") == 0 and not data.get("runtime_errors")
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            result.update(status="oracle_error", error=f"invalid capture report: {error}")
            write_json(output / "result.json", result)
            return result
        if not complete:
            result["status"] = "oracle_incomplete"
        else:
            environment = os.environ.copy()
            environment["RUST_MIN_STACK"] = "33554432"
            result["replay_environment"] = dict(DEADSYNC_FUZZ_TRACE=str(trace), DEADSYNC_FUZZ_SIMFILE=str(simfile),
                                                DEADSYNC_FUZZ_REPORT=str(report), DEADSYNC_FUZZ_ORACLE=args.oracle,
                                                RUST_MIN_STACK="33554432")
            environment.update(result["replay_environment"])
            command = [args.runner, TEST_NAME, "--exact", "--ignored", "--nocapture", "--test-threads=1"]
            success_key = "compiled_checks_passed"
            result["replay"] = run_process(command, ROOT, output / "deadsync.log", args.timeout, environment)
            if result["replay"].get("timeout"):
                result["status"] = "deadsync_timeout"
            elif not report.is_file():
                result["status"] = "deadsync_error"
                tail = (output / "deadsync.log").read_text(encoding="utf-8", errors="replace")[-4000:]
                result["failure_summary"] = next((line.strip() for line in tail.splitlines()
                    if "could not compile" in line or "runtime error:" in line), tail[-600:])
                result["report"] = str(output / "deadsync.log")
            else:
                try:
                    replay = read_json(report)
                    result["pending_coverage"] = replay.get("pending_coverage", [])
                    result["report"] = str(report)
                    result["failure_summary"] = replay.get("error") or dict(Counter(
                        row.get("field", "unknown") for row in replay.get("mismatches", []))) or [
                        {"section": row["name"], "failed": row["failed"]}
                        for row in replay.get("sections", []) if row["failed"]]
                    if replay.get("error"):
                        result["status"] = "deadsync_error"
                        result["error"] = replay["error"]
                        write_json(output / "result.json", result)
                        return result
                    result["status"] = ("pass" if result["replay"]["returncode"] == 0
                                        and replay.get(success_key) is True
                                        and replay.get("execution_path") == "compiled-song-lua"
                                        and replay.get("retained_lua_vm") is False else "mismatch")
                except (OSError, ValueError, TypeError, AttributeError) as error:
                    result.update(status="deadsync_error", error=f"invalid replay report: {error}")
    write_json(output / "result.json", result)
    return result


def executable(path):
    path = path.resolve()
    if not path.is_file():
        raise ValueError(f"executable not found: {path}")
    return path


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--start", type=int, default=0, help="first case index; allows deterministic shards")
    parser.add_argument("--strategy", choices=("random", "pairwise"), default="pairwise")
    parser.add_argument("--out", type=Path, required=True, help="new directory; existing runs are never overwritten")
    parser.add_argument("--generate-only", action="store_true")
    parser.add_argument("--replay", type=Path, help="saved case.json; executes its saved Lua and assets unchanged")
    parser.add_argument("--oracle", choices=("native", "semantic"), default="native")
    suffix = ".exe" if os.name == "nt" else ""
    harness_root = ROOT.parent / "itgmania-harness"
    parser.add_argument("--harness", type=Path, default=harness_root / "target" / "release" / ("itgmania-harness-rs" + suffix))
    parser.add_argument("--itgmania-bin", type=Path,
                        default=harness_root / "target/native-engine-source/Program" / ("ITGmania-harness-trace" + suffix))
    parser.add_argument("--fallback-theme", default="Harness Simply Love", help="theme folder in the native install")
    parser.add_argument("--runner", type=Path, help="prebuilt deadsync test executable; otherwise builds once")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--max-events", type=int, default=100000)
    parser.add_argument("--beat-step", type=float, default=1 / 48, help="semantic sample interval (updates remain 60 Hz)")
    parser.add_argument("--fail-fast", action="store_true")
    args = parser.parse_args(argv)
    if args.count < 1 or args.start < 0 or args.max_events < 1:
        parser.error("count and max-events must be positive; start must be nonnegative")
    if not math.isfinite(args.timeout) or args.timeout <= 10 or not math.isfinite(args.beat_step) or args.beat_step <= 0:
        parser.error("timeout must be finite and >10 seconds; beat-step must be finite and positive")
    if args.replay and args.generate_only:
        parser.error("--replay cannot be combined with --generate-only")
    try:
        args.out = args.out.resolve()
        if args.out.exists():
            raise ValueError(f"output already exists: {args.out}; choose a new directory")
        if not args.generate_only:
            args.harness = executable(args.harness)
            if args.oracle == "native":
                args.itgmania_bin = executable(args.itgmania_bin)
            if args.runner:
                args.runner = executable(args.runner)
        saved = read_json(args.replay) if args.replay else None
        if saved is not None and saved.get("version") not in (1, VERSION):
            raise ValueError("unsupported saved case version")
        args.out.mkdir(parents=True)
        if not args.generate_only and not args.runner:
            print("Building the deadsync compiled comparison executable once...", flush=True)
            args.runner = build_runner(args.oracle, args.out)
        summary = {"version": VERSION, "seed": saved["campaign_seed"] if saved else args.seed,
                   "strategy": "replay" if saved else args.strategy,
                   "requested_count": 1 if saved else args.count, "start": saved["index"] if saved else args.start,
                   "oracle": args.oracle, "generate_only": args.generate_only,
                   "deadsync_execution": "compiled",
                   "complete": False, "results": [], "binaries": {}}
        if not args.generate_only:
            summary["binaries"] = {key: {"path": str(getattr(args, key)), "sha256": digest(getattr(args, key))}
                                   for key in ("harness", "runner") + (("itgmania_bin",) if args.oracle == "native" else ())}
        summary["configuration"] = {"timeout": args.timeout, "max_events": args.max_events,
                                    "beat_step": args.beat_step, "until_beat": 8,
                                    "fallback_theme": args.fallback_theme,
                                    "native_capture_fps": 60, "native_fixed_update_fps": 60}
        rows = [saved["factors"]] if saved else itertools.islice(
            factor_rows(args.seed, args.start + args.count, args.strategy), args.start, None)
        generated_rows, compared_rows, passing_rows = [], [], []
        authored_methods = set()
        for index, factors in enumerate(rows, args.start):
            case = saved or generate_case(args.seed, index, factors)
            if saved:
                case_dir = args.replay.resolve().parent
            else:
                case_dir = args.out / f"case-{index:06d}"
                write_case(case_dir, case)
            generated_rows.append(factors)
            authored_methods.update(re.findall(r":([A-Za-z_][A-Za-z_0-9]*)\s*\(",
                                               (case_dir / "default.lua").read_text(encoding="utf-8")))
            if args.generate_only:
                result = {"index": index, "seed": case["seed"], "status": "generated", "case": str(case_dir), "factors": factors}
            else:
                result = evaluate_case(case_dir, case, args.out / f"result-{case['index']:06d}", args)
                if result["status"] in ("pass", "mismatch"):
                    compared_rows.append(factors)
                if result["status"] == "pass":
                    passing_rows.append(factors)
            summary["results"].append(result)
            summary["counts"] = dict(Counter(r["status"] for r in summary["results"]))
            if not args.generate_only or len(generated_rows) % 100 == 0:
                print(f"case {case['index']}: {result['status']} ({dict(summary['counts'])})", flush=True)
            if result.get("failure_summary"):
                print(f"  {result['failure_summary']}\n  Details: {result['report']}", flush=True)
            write_json(args.out / "summary.json", summary)
            if args.fail_fast and result["status"] not in ("pass", "generated"):
                break
        summary["complete"] = len(generated_rows) == (1 if saved else args.count)
        summary["pair_coverage"] = {"generated": coverage(generated_rows), "compared": coverage(compared_rows),
                                    "passing": coverage(passing_rows)}
        summary["api_coverage"] = api_coverage(authored_methods)
        write_json(args.out / "summary.json", summary)
        print(f"Report: {args.out / 'summary.json'}", flush=True)
        return 1 if any(result["status"] not in ("pass", "generated") for result in summary["results"]) else 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"fuzz_song_lua: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
