#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
if [ -z "${tag}" ]; then
  echo "usage: $0 <tag> [arch]" >&2
  exit 1
fi

map_arch() {
  local value
  value="$(printf '%s' "${1}" | tr '[:upper:]' '[:lower:]')"
  case "${value}" in
    x64 | amd64 | x86_64) echo "x86_64" ;;
    arm64 | aarch64) echo "arm64" ;;
    *)
      echo "unknown arch: ${1}" >&2
      return 1
      ;;
  esac
}

arch_raw="${RUNNER_ARCH:-$(uname -m)}"
if [ -n "${2:-}" ]; then
  arch_raw="${2}"
fi
arch="$(map_arch "${arch_raw}")"

bin_path="target/release/deadsync"

if [ ! -x "${bin_path}" ]; then
  echo "missing executable: ${bin_path}" >&2
  exit 1
fi
if [ ! -d "assets" ]; then
  echo "missing assets directory: assets" >&2
  exit 1
fi
if [ ! -d "songs" ]; then
  echo "missing songs directory: songs" >&2
  exit 1
fi
if [ ! -d "courses" ]; then
  echo "missing courses directory: courses" >&2
  exit 1
fi

dist_dir="dist"
pkg_name="deadsync-${tag}-${arch}-macos"
stage_dir="${dist_dir}/deadsync"
archive_path="${dist_dir}/${pkg_name}.tar.gz"

rm -rf "${stage_dir}"
mkdir -p "${stage_dir}"

cp "${bin_path}" "${stage_dir}/deadsync"
cp -r assets songs courses "${stage_dir}/"
cp README.md LICENSE "${stage_dir}/"

tar -C "${dist_dir}" -czf "${archive_path}" deadsync

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "archive=${archive_path}"
    echo "stage=${stage_dir}"
  } >> "${GITHUB_OUTPUT}"
fi
