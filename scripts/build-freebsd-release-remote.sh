#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
arch="${2:-x86_64}"
target="${3:-x86_64-unknown-freebsd}"

if [ -z "${tag}" ]; then
  echo "usage: $0 <tag> [arch] [target]" >&2
  exit 1
fi

host="${FREEBSD_RELEASE_HOST:-}"
user="${FREEBSD_RELEASE_USER:-root}"
port="${FREEBSD_RELEASE_PORT:-22}"
remote_root="${FREEBSD_RELEASE_REMOTE_DIR:-/tmp/deadsync-release}"
ssh_opts=()
if [ -n "${FREEBSD_RELEASE_SSH_OPTS:-}" ]; then
  # shellcheck disable=SC2206
  ssh_opts=(${FREEBSD_RELEASE_SSH_OPTS})
fi

if [ -z "${host}" ]; then
  echo "FREEBSD_RELEASE_HOST is required." >&2
  exit 1
fi

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

require_cmd git
require_cmd ssh
require_cmd scp
require_cmd tar

ssh_cmd=(ssh "${ssh_opts[@]}" -p "${port}" "${user}@${host}")
scp_cmd=(scp "${ssh_opts[@]}" -P "${port}")
remote_dir="${remote_root%/}/${tag}-${arch}-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-0}"
remote_bin="target/${target}/release/deadsync"
local_bin_dir="target/${target}/release"
local_bin="${local_bin_dir}/deadsync"

cleanup() {
  "${ssh_cmd[@]}" "rm -rf '${remote_dir}'" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "Verifying FreeBSD builder ${user}@${host}:${port}"
"${ssh_cmd[@]}" "uname -srm && command -v cargo >/dev/null && command -v tar >/dev/null && command -v git >/dev/null"

echo "Preparing remote workspace ${remote_dir}"
"${ssh_cmd[@]}" "rm -rf '${remote_dir}' && mkdir -p '${remote_dir}'"

echo "Uploading source snapshot"
tar \
  --exclude='./target' \
  --exclude='./dist' \
  --exclude='./cache' \
  --exclude='./save' \
  --exclude='./songs' \
  --exclude='./courses' \
  --exclude='./.codex' \
  -cf - . | "${ssh_cmd[@]}" "tar -xf - -C '${remote_dir}'"

echo "Building ${arch} (${target}) on FreeBSD"
"${ssh_cmd[@]}" "cd '${remote_dir}' && CARGO_TERM_COLOR='${CARGO_TERM_COLOR:-always}' cargo build --release --locked --target '${target}'"

mkdir -p "${local_bin_dir}"
echo "Downloading ${remote_bin}"
"${scp_cmd[@]}" "${user}@${host}:${remote_dir}/${remote_bin}" "${local_bin}"
chmod +x "${local_bin}"

if [ ! -x "${local_bin}" ]; then
  echo "missing downloaded executable: ${local_bin}" >&2
  exit 1
fi

echo "FreeBSD build ready at ${local_bin}"
