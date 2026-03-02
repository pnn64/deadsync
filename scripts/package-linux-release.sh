#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
if [ -z "${tag}" ]; then
  echo "usage: $0 <tag>" >&2
  exit 1
fi

arch_raw="${RUNNER_ARCH:-X64}"
arch="$(printf '%s' "${arch_raw}" | tr '[:upper:]' '[:lower:]')"
bin_path="target/release/deadsync"
assets_path="target/release/assets"

if [ ! -x "${bin_path}" ]; then
  echo "missing executable: ${bin_path}" >&2
  exit 1
fi
if [ ! -d "${assets_path}" ]; then
  echo "missing assets directory: ${assets_path}" >&2
  exit 1
fi

dist_dir="dist"
pkg_name="deadsync-${tag}-linux-${arch}"
stage_dir="${dist_dir}/${pkg_name}"
archive_path="${dist_dir}/${pkg_name}.tar.gz"
checksum_path="${archive_path}.sha256"

rm -rf "${stage_dir}"
mkdir -p "${stage_dir}"

cp "${bin_path}" "${stage_dir}/deadsync"
cp -r "${assets_path}" "${stage_dir}/assets"
cp README.md LICENSE "${stage_dir}/"
if [ -f "deadsync.ini" ]; then
  cp deadsync.ini "${stage_dir}/deadsync.ini"
fi

tar -C "${dist_dir}" -czf "${archive_path}" "${pkg_name}"
sha256sum "${archive_path}" > "${checksum_path}"

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "archive=${archive_path}"
    echo "checksum=${checksum_path}"
    echo "stage=${stage_dir}"
  } >> "${GITHUB_OUTPUT}"
fi
