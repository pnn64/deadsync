"""Record the reviewed disposition of the original, fixed migration inventory.

This source audit is intentionally separate from behavioral, graph and link gates.
"""
import collections
import hashlib
import json
import re
import tomllib
from pathlib import Path

from audit_fixed_inventory import AUDIT, CATEGORIES, D, FROZEN, ROOT, guarded


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    initial = json.loads((AUDIT / "candidate-classification-initial.json").read_text())
    inventory = json.loads((D / "inventory.json").read_text())
    evidence = {}

    def contains(file, *texts):
        path = ROOT / file
        source = path.read_text(encoding="utf-8")
        for text in texts:
            assert text in source, (file, text)
        evidence[file] = {"sha256": sha(path), "reviewed_markers": list(texts)}
        return source

    song_root = contains("crates/deadsync-song-lua/src/lib.rs")
    for module in ("actor_classes", "actor_input", "actor_methods", "compile", "input", "raw_vm"):
        assert f'#[cfg(any(test, feature = "legacy-oracle"))]\nmod {module};' in song_root
    assert '#[cfg(any(test, feature = "legacy-oracle"))]\n#[cfg(any(test, feature = "test-support"))]\npub mod legacy;' in song_root
    contains("crates/deadsync-song-lua/src/rust_engine.rs",
             'use deadlib_lua::{Error, HostCall, Root, TableId, Value, Vm};',
             '#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]\npub mod test_support;')
    contains("crates/deadsync-song-lua/src/base_transform.rs",
             '#[cfg(any(test, feature = "legacy-oracle"))]\n    pub fn read(actor: &Table)')
    contains("crates/deadsync-song-lua/src/render_flags.rs",
             '#[cfg(any(test, feature = "legacy-oracle"))]\n    pub fn snapshot(self)')
    contains("crates/deadsync-song-lua/src/lua_util.rs",
             '#[cfg(any(test, feature = "legacy-oracle"))]\nimpl<const N: usize> mlua::FromLua for LuaFieldText<N>',
             '#[cfg(any(test, feature = "legacy-oracle"))]\nimpl mlua::IntoLua for ActorCommandName')
    contains("crates/deadsync-song-lua/src/playback/prepare.rs",
             '#[cfg(test)]\n#[path = "prepare_tests.rs"]\nmod submission_tests;')
    contains("crates/deadsync-song-lua/src/perframe.rs",
             '#[cfg(any(test, feature = "legacy-oracle"))]\nfn apply_scheduled_overlay_states<')
    pure_frames = contains("crates/deadsync-song-lua/src/frame_capture.rs",
                           'pub(crate) fn apply_scheduled_overlay_states_uncached(')
    scanned = json.loads((AUDIT / "current-deadsync-song-lua.json").read_text())
    frame_file = next(f for f in scanned if f["file"] == "frame_capture.rs")
    frame_item = next(i for i in frame_file["items"] if i["name"] == "apply_scheduled_overlay_states_uncached")
    frame_body = "\n".join(pure_frames.splitlines()[frame_item["line"] - 1:frame_item["end_line"]])
    assert not re.search(r'\b(?:mlua|ffi|Lua|Table|Function)\b', frame_body)
    contains("crates/deadsync-noteskin/src/compiler.rs",
             'pub use rust_engine::compile_data;',
             '#[cfg(test)]\n#[allow(dead_code)]\n#[path = "compiler/legacy_tests.rs"]\nmod legacy;')
    contains("crates/deadsync-noteskin/src/compiler/legacy_tests.rs",
             'include!("../../tests/support/legacy.rs");',
             '#[path = "../compiler_callbacks_perf.rs"]',
             '#[path = "../compiler_aliases_perf.rs"]')
    contains("crates/deadsync-noteskin/src/compiler_callbacks_perf.rs",
             '#[path = "compiler_callbacks_before.rs"]\nmod before;')
    oracle = contains("crates/deadsync-noteskin/tests/support/legacy.rs")
    oracle_audit = json.loads((D / "noteskin-oracle-source-audit.json").read_text())
    assert sha(ROOT / "crates/deadsync-noteskin/tests/support/legacy.rs") == oracle_audit["oracle_file_sha256"]
    assert oracle_audit["all_engine_function_bodies_match"]
    contains("crates/deadsync-noteskin/src/compiler/rust_engine.rs",
             'use deadlib_lua::{Error, HostCall, HostFunction, Root, Value as LuaValue, Vm};',
             'roots: Vec<Root>', 'self.vm.pin(value)', 'self.vm.unpin(root)')
    contains("crates/deadsync-theme-simply-love/src/step_stats_gifs.rs",
             'deadlib_lua::Vm::try_new_with_memory_limit(MAX_LUA_MEMORY)',
             'rust_engine::capture(&mut vm, content, source, side, wide)')
    contains("crates/deadsync-theme-simply-love/src/step_stats_gifs/rust_engine.rs",
             'use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm};',
             'vm.set_instruction_limit(', 'vm.unpin(root)', 'call.with_roots(')

    def classify(row):
        status = row["classification"]
        file = row["original"]["file"]
        text = row["original"]["source"]
        name = row.get("frozen_item", {}).get("name", "")
        if status == "preexisting_test_or_oracle_item":
            return "preexisting_test_scope", "Frozen item already excluded from production; original guard recorded."
        if status == "item_guarded_test_or_oracle":
            assert all(guarded(i) for i in row["current_candidates"])
            return "legacy_item_isolated", "Current matching items have explicit test/oracle guards."
        if status == "song_module_guarded_by_root_requires_confirmation":
            return "legacy_module_isolated", "Current lib.rs root module guard verified above."
        if text.startswith(("//", "/*", "*")):
            return "comment_or_documentation", "Original inventory match is prose, with no executable handle/access."
        if "deadsync-noteskin/src/compiler_callbacks_" in file or file.endswith("compiler_aliases_perf.rs"):
            return "preexisting_test_module", "Verified compiler::legacy cfg(test) -> performance control -> before module chain."
        if file == "crates/deadsync-noteskin/src/compiler.rs":
            if row.get("frozen_item", {}).get("kind") == "use":
                return "legacy_import_removed", "Normal compiler imports Rust adapter; mlua exists only in dev dependencies."
            assert name in oracle_audit["engine_functions"] and f"fn {name}(" in oracle
            return "legacy_helper_moved_to_test_oracle", "Exact frozen function body audit remains valid for current oracle file SHA."
        if file.endswith("deadsync-noteskin/src/compiler/rust_engine.rs"):
            return "rust_engine_host_value", "LuaValue aliases deadlib_lua::Value; Session owns VM and explicit roots, emits native bundle."
        if file.endswith("/base_transform.rs") and name == "BaseTransform":
            return "legacy_member_isolated", "read is individually test/oracle guarded; native get/set storage remains shared."
        if file.endswith("/render_flags.rs") and name == "RenderFlags":
            return "legacy_member_isolated", "snapshot is individually test/oracle guarded; native flag storage remains shared."
        if file.endswith("/lua_util.rs") and name in ("LuaFieldText < N >", "ActorCommandName"):
            return "legacy_trait_impl_isolated", "Native FromLua/IntoLua impl explicitly guarded; same-name inherent impl contains pure storage methods."
        if file.endswith("/perframe.rs") and name == "apply_scheduled_overlay_states_uncached":
            return "pure_helper_extracted", "frame_capture has callback-based native state loop; legacy Lua wrapper is guarded."
        if file.endswith("/rust_engine/test_support.rs"):
            return "legacy_test_support_module_isolated", "rust_engine root requires test-support AND test/oracle for this module."
        if "/rust_engine/" in file or file.endswith("/rust_engine.rs"):
            assert "deadsync-song-lua" in file
            return "rust_engine_host_value", "Existing shared Rust engine bindings use deadlib_lua values, Root, traced captures and scoped host calls."
        if file.endswith("/step_stats_gifs.rs"):
            return "gif_rust_engine_replacement", "Bounded Rust VM dispatch verified; native capture removed from production, frozen oracle maintained by gif_probe.py."
        if file.endswith("/playback/prepare_tests.rs"):
            return "preexisting_test_module", "prepare::submission_tests cfg(test) verified; match is fixture text."
        if "serde_json::Value" in text:
            return "native_json_snapshot", "Owned JSON snapshot contains no Lua runtime handle."
        if file.endswith("/gameplay.rs") and "::Function" in text:
            return "native_enum_discriminant", "Compiled ease target enum variant, with no retained function object."
        if file.endswith("/tween_pose.rs"):
            return "native_pose_value", "Value aliases SongLuaOverlayUpdateValue, an owned pose enum."
        if file.endswith("deadsync-noteskin/src/actor.rs") and "Lua identifiers contain only ASCII bytes" in text:
            return "native_identifier_error_text", "UTF-8 validation of a Rust byte slice, without VM access."
        if file.endswith("deadsync-song-lua/src/lib.rs") and name == "SongLuaEaseTarget":
            return "native_enum_discriminant", "Function is a unit variant of compiled native output."
        # Remaining source-reference matches are known diagnostics/string literals.
        if '"' in text and not re.search(r'\b(?:mlua|ffi|Lua|Table|Value|Function)\s*(?:::|\(|>|,|\))', text):
            return "native_diagnostic_string", "Source match is a Rust diagnostic; no legacy runtime API or handle."
        raise AssertionError((file, row["original"]["line"], text, status, name))

    report = {"scope": "Disposition audit of every original inventory site, not a behavioral-completion or final link claim.",
              "inventory_sha256": sha(D / "inventory.json"), "evidence_sources": evidence, "categories": {}}
    for category in CATEGORIES:
        rows = []
        for row in initial[category]["rows"]:
            disposition, note = classify(row)
            rows.append({"original": row["original"], "disposition": disposition, "review": note,
                         "initial_classification": row["classification"],
                         "frozen_item": {k: row.get("frozen_item", {}).get(k) for k in ("kind", "name", "line", "end_line", "attributes")},
                         "current_items": [{k: i[k] for k in ("kind", "name", "line", "end_line", "attributes")} for i in row.get("current_candidates", [])]})
        report["categories"][category] = {"count": len(rows), "counts": dict(collections.Counter(r["disposition"] for r in rows)), "rows": rows}
        print(category, len(rows), report["categories"][category]["counts"])
    report["source_files"] = []
    for entry in inventory["source_references"]:
        file = entry["file"]
        assert sha(FROZEN / file) == entry["sha256"]
        report["source_files"].append({"file": file, "frozen_sha256": entry["sha256"],
                                       "current_sha256": sha(ROOT / file) if (ROOT / file).exists() else None,
                                       "original_line_count": len(entry["lines"])})
    report["direct_consumers"] = []
    for entry in inventory["direct_legacy_dependencies"]:
        file = entry["manifest"].replace("\\", "/")
        manifest = tomllib.loads((ROOT / file).read_text())
        deps = manifest["dependencies"]
        assert "deadlib-lua" in deps and not deps["deadlib-lua"].get("optional", False)
        if "mlua" in deps:
            assert deps["mlua"]["optional"]
            assert manifest["features"]["legacy-oracle"] == ["dep:mlua", "test-support"]
            assert manifest["features"]["test-support"] == []
        assert "rust-engine" not in manifest.get("features", {})
        report["direct_consumers"].append({"package": entry["package"], "manifest": file,
                                            "sha256": sha(ROOT / file), "rust_engine_unconditional": True,
                                            "legacy_scope": "explicit legacy-oracle feature and dev dependency" if "mlua" in deps else "dev dependency only" if "mlua" in manifest.get("dev-dependencies", {}) else "none"})
    report["normal_downstream_packages"] = inventory["normal_downstream_packages"]
    report["remaining_separate_gates"] = ["Final regression results", "Root/reentry/thread/session ownership behavior", "Final normal/build graph and link evidence", "Rendered application smoke", "Matched performance/memory results", "Final documentation/completion audit"]
    (AUDIT / "reviewed-site-dispositions.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("source files", len(report["source_files"]), "direct consumers", len(report["direct_consumers"]))


if __name__ == "__main__":
    main()
