# DeadSync test fixtures

Shared fixture data for [DeadSync](https://github.com/pnn64/deadsync), mounted at
`tests/fixtures` in that repository. Rust test runners, Cargo manifests, and
fixture capture and maintenance scripts remain in DeadSync.

The initial fixture import preserves all 3,879 logical file contents from
DeadSync commit `7ef537b8b509898a190fbda2886e175f8fe0a959` (`tests/fixtures`). Only the current fixture snapshot
is imported; its historical revisions remain in DeadSync's Git history.

## Running tests

From a DeadSync checkout:

```sh
git submodule update --init tests/fixtures
python scripts/prepare_test_fixtures.py
cargo test --test full_song_lua -- --list
cargo test --test full_song_lua Accendio
```

See `full_song_lua/README.md` for fixture selection, corpus runs, and reference
maintenance. Commands and repository-relative paths in fixture guides refer to
the DeadSync checkout. A standalone fixture checkout does not contain the test
runners or maintenance scripts.

Seventeen archives of 50 MB or more are stored as binary parts of at most
48,000,000 bytes. `prepare_test_fixtures.py` verifies each part's SHA-256 and the
complete archive before restoring its original filename locally. Restored
archives are ignored by Git. Every file in this repository's branch history is
strictly below 50,000,000 bytes.

## Updating fixtures

1. In DeadSync, create a branch in the submodule with
   `git -C tests/fixtures switch -c update-fixtures`.
2. Run capture or maintenance scripts from the DeadSync root. Use
   `python scripts/prepare_test_fixtures.py --split` before adding large archives;
   this creates parts and stages removal of their original files from Git while
   keeping the local originals. Then stage the parts and manifest in this repo.
   For example,
   `python -X utf8 scripts/catalog_committed_songs.py` regenerates the catalogs
   from files tracked by this fixture repository (requires `zstandard`).
3. Commit and push the fixture changes in this repository.
4. In DeadSync, commit the updated `tests/fixtures` submodule pointer after its
   referenced commit is available on the remote.

Keep local-only archives excluded by `.gitignore` out of commits. Existing
capture provenance, content-addressed archive names, and checksums stay intact.

Enable the size guard in each standalone or submodule checkout:

```sh
git config core.hooksPath .githooks
```

The pre-push hook runs `python scripts/check_file_sizes.py` and rejects any Git
blob of 50,000,000 bytes or more in reachable history. It also works as a manual
check before publishing fixture changes.
