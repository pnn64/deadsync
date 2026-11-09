"""Map frozen inventory sites to current Rust items without changing sources.

This produces review candidates, not a proof that matching names preserve behavior.
It deliberately retains unresolved sites for manual review.
"""
import collections
import hashlib
import json
from pathlib import Path

D = Path(__file__).resolve().parent
ROOT = D.parents[2]
AUDIT = D / "production-inventory-audit-v1"
FROZEN = ROOT / "target/rust-lua-production-before"
PACKAGES = ("deadsync-song-lua", "deadsync-noteskin", "deadsync-theme-simply-love")
CATEGORIES = (
    "source_reference_lines",
    "raw_c_api_and_identity_references",
    "handle_field_and_parameter_candidates",
    "state_limit_and_gc_references",
)


def guarded(item):
    return any('cfg (test)' in a or 'feature = "legacy-oracle"' in a
               for a in item["attributes"])


def signature(item):
    return item["kind"], item["name"] or json.dumps(item["imports"], sort_keys=True)


def main():
    inventory = json.loads((D / "inventory.json").read_text(encoding="utf-8"))
    inventory["source_reference_lines"] = []
    for file in inventory["source_references"]:
        lines = (FROZEN / file["file"]).read_text(encoding="utf-8").splitlines()
        for line in file["lines"]:
            inventory["source_reference_lines"].append({
                "file": file["file"], "line": line, "source": lines[line - 1].strip(),
            })
    hashes = json.loads((D / "baseline-sources.json").read_text(encoding="utf-8"))
    indexes = {}
    for version in ("frozen", "current"):
        indexes[version] = {}
        for package in PACKAGES:
            files = json.loads((AUDIT / f"{version}-{package}.json").read_text(encoding="utf-8"))
            indexes[version][package] = {f["file"]: f for f in files}
    verified = {}
    output = {}
    forced = {"actor_classes", "actor_input", "actor_methods", "compile", "input", "legacy", "raw_vm"}
    for category in CATEGORIES:
        rows = []
        for original in inventory[category]:
            path = original["file"].replace("\\", "/")
            _, package, _, relative = path.split("/", 3)
            if path not in verified:
                actual = hashlib.sha256((FROZEN / path).read_bytes()).hexdigest()
                assert actual == hashes[path], path
                verified[path] = actual
            old_file = indexes["frozen"][package].get(relative)
            row = {"original": original, "frozen_file_sha256": verified[path]}
            if old_file is None:
                row["classification"] = "excluded_subtree_requires_manual_review"
            else:
                containers = [i for i in old_file["items"]
                              if i["line"] <= original["line"] <= i["end_line"]]
                if not containers:
                    row["classification"] = "outside_item_requires_manual_review"
                else:
                    old = min(containers, key=lambda i: i["end"] - i["start"])
                    row["frozen_item"] = old
                    new_file = indexes["current"][package].get(relative)
                    new = [i for i in new_file["items"] if signature(i) == signature(old)] if new_file else []
                    row["current_candidates"] = new
                    if guarded(old):
                        kind = "preexisting_test_or_oracle_item"
                    elif package == "deadsync-song-lua" and relative.split("/")[0].removesuffix(".rs") in forced:
                        kind = "song_module_guarded_by_root_requires_confirmation"
                    elif new and all(guarded(i) for i in new):
                        kind = "item_guarded_test_or_oracle"
                    elif not new:
                        kind = "removed_or_replaced_item_requires_review"
                    else:
                        kind = "normal_or_changed_item_requires_review"
                    row["classification"] = kind
            rows.append(row)
        output[category] = {"counts": dict(collections.Counter(r["classification"] for r in rows)), "rows": rows}
    output["method"] = "Conservative top-level Rust AST matching. Module/member guards, moves and false positives require manual review. No behavior claim from name matching."
    output["verified_frozen_sources"] = verified
    path = AUDIT / "candidate-classification-initial.json"
    path.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    for category in CATEGORIES:
        print(category, output[category]["counts"])
    groups = collections.defaultdict(set)
    for category in CATEGORIES:
        for row in output[category]["rows"]:
            if row["classification"] in {"normal_or_changed_item_requires_review", "removed_or_replaced_item_requires_review", "outside_item_requires_manual_review", "excluded_subtree_requires_manual_review"}:
                i = row.get("frozen_item", {})
                groups[row["original"]["file"]].add((i.get("kind", "?"), i.get("name", "?"), row["classification"]))
    for file, entries in sorted(groups.items()):
        print(file, sorted(entries))


if __name__ == "__main__":
    main()
