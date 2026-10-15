Parity VM storage maintenance runs from `scripts/parity_vm_storage.py`.

On the Linux VM, run `python3 /home/parity/jobs/parity_vm_storage.py` after jobs,
or add `--watch` to check every five minutes. A lock prevents duplicate workers.
The current worker is started with `nohup`; it is not a system service.

The script compresses inactive native staging traces and terminal replay reports
larger than 10 MB, after ten minutes without changes. It checks process arguments,
working directories, open files and memory mappings before and after compression.
Protected process inspection uses read-only `sudo -n`; file operations run as the
normal user. If inspection or verification fails, the raw file remains.

Before removing a raw file, the script verifies the SHA-256 of the decompressed
copy and writes `/home/parity/jobs/vm-storage-maintenance.jsonl`. Compressed traces,
reports, captures, logs, source snapshots and replay helpers remain available.
Restore a raw file when needed with `zstd -d --keep PATH.zst`.

Reclaim completed Cargo artifacts only after the helper has been copied outside
the target directory and no build uses that target. Keep live replay inputs and
all deferred Mawaru9 material. Avoid retaining additional raw archive copies;
verify them against a preserved compressed capture before removal.
