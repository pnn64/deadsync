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
require_cmd sed

ssh_cmd=(ssh "${ssh_opts[@]}" -p "${port}" "${user}@${host}")
scp_cmd=(scp "${ssh_opts[@]}" -P "${port}")
remote_dir="${remote_root%/}/${tag}-${arch}-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-0}"
remote_bin="target/${target}/release/deadsync"
local_bin_dir="target/${target}/release"
local_bin="${local_bin_dir}/deadsync"

shell_sq() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

ssh_run() {
  local script cmd
  script="set -eu
export PATH=\"\$HOME/.cargo/bin:\$PATH\"
if [ -f \"\$HOME/.cargo/env\" ]; then
  . \"\$HOME/.cargo/env\"
fi
${1}"
  cmd="$(shell_sq "${script}")"
  "${ssh_cmd[@]}" "/bin/sh -lc ${cmd}"
}

remote_dir_q="$(shell_sq "${remote_dir}")"
target_q="$(shell_sq "${target}")"
term_color_q="$(shell_sq "${CARGO_TERM_COLOR:-always}")"

cleanup() {
  ssh_run "rm -rf ${remote_dir_q}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "Verifying FreeBSD builder ${user}@${host}:${port}"
ssh_run "uname -srm
command -v cargo >/dev/null
command -v tar >/dev/null
command -v git >/dev/null"

echo "Preparing remote workspace ${remote_dir}"
ssh_run "rm -rf ${remote_dir_q}
mkdir -p ${remote_dir_q}"

echo "Uploading source snapshot"
tar \
  --exclude='./target' \
  --exclude='./dist' \
  --exclude='./cache' \
  --exclude='./save' \
  --exclude='./songs' \
  --exclude='./courses' \
  --exclude='./.codex' \
  -cf - . | ssh_run "tar -xf - -C ${remote_dir_q}"

echo "Building ${arch} (${target}) on FreeBSD"
ssh_run "cd ${remote_dir_q}
CARGO_TERM_COLOR=${term_color_q} cargo build --release --locked --target ${target_q}"

mkdir -p "${local_bin_dir}"
echo "Downloading ${remote_bin}"
"${scp_cmd[@]}" "${user}@${host}:${remote_dir}/${remote_bin}" "${local_bin}"
chmod +x "${local_bin}"

if [ ! -x "${local_bin}" ]; then
  echo "missing downloaded executable: ${local_bin}" >&2
  exit 1
fi

echo "FreeBSD build ready at ${local_bin}"
