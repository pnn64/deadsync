# IANA language registries

Pinned on 2026-10-26 from the official IANA protocol registries:

- [Language Subtag Registry](https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry), File-Date 2026-09-17.
- [Language Tag Extensions Registry](https://www.iana.org/assignments/language-tag-extensions-registry/language-tag-extensions-registry), File-Date 2014-04-02.

IANA and IETF dedicate protocol-registry data under CC0 1.0; see the
[IANA licensing terms](https://www.iana.org/help/licensing-terms) and
[CC0 dedication](https://creativecommons.org/publicdomain/zero/1.0/legalcode).
These files preserve the original bytes. SHA-256 hashes appear in the generated
Rust table header.

`python scripts/update_language_registry.py` regenerates encoder tables offline.
`--check` checks reproducibility; `--refresh` downloads current IANA snapshots.
Tables contain only registered subtag keys, reserved private ranges and extlang
prefixes. Tests independently read every record/range from these snapshots.
