# deadsync's Windows 7 patch

This directory vendors the complete `rfd` 0.18.0 release from crates.io
(upstream commit `2d35d7d07ab4d021ca92e5378ae0e6ed5fbf8d01`). The published
archive has SHA-256
`c4b9d4857b10297ffb0f64cae711e983a7aeaee561848d56d429d3858bd8ed93`.

The only source change from upstream is in
`src/backend/win_cid/file_dialog/com.rs`: free shell-allocated paths using
`CoTaskMemFree` resolved from `ole32.dll`, instead of the `windows-sys` 0.61
binding that imports it from `combase.dll`. Windows 7 does not have
`combase.dll`; an eager import prevents the executable from starting, even
if no file dialog is opened. The other COM functions used by this backend
already import from `ole32.dll`.

Upstream CI whitespace has also been normalized.

The workspace's `[patch.crates-io]` entry redirects consumers of this version
to the vendor copy. The Windows 7 release workflow runs
`scripts/check-vendored-rfd.ps1` before building to verify that Cargo actually
selected this copy. A dependency bump must not silently bypass the patch.

When updating, replace the full upstream package (including both manifests
and the upstream lockfile), reapply the COM allocator patch, and update the
shell's version requirement and workspace lockfile together. Preserve new
upstream changes in `com.rs`, such as 0.18.0's `GetOptions` visibility.

Build both `x86_64-win7-windows-msvc` and `i686-win7-windows-msvc` using
nightly and `-Z build-std=std,panic_abort`. The Windows release workflow runs
`scripts/check-win7-imports.ps1` against each executable to reject eager
`combase.dll` and WinRT imports. Actual dialog behavior still needs testing
on Windows 7; an import check alone cannot establish full OS compatibility.
