#!/usr/bin/env bash
set -euo pipefail

image="${DEADSYNC_GLIBC_IMAGE:-debian:12}"
workdir="$(pwd)"
uid="$(id -u)"
gid="$(id -g)"
home_dir="/tmp/deadsync-home"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required" >&2
  exit 1
fi

if [ "$#" -eq 0 ]; then
  set -- --release --locked
fi

docker run --rm \
  -e HOST_UID="${uid}" \
  -e HOST_GID="${gid}" \
  -e HOME="${home_dir}" \
  -v "${workdir}:/work" \
  -w /work \
  "${image}" \
  bash -lc '
    set -euo pipefail
    fix_owner() {
      [ -e /work/target ] && chown -R "$HOST_UID:$HOST_GID" /work/target
    }
    trap fix_owner EXIT
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install --no-install-recommends -y \
      ca-certificates \
      curl \
      git \
      python3 \
      build-essential \
      pkg-config \
      cmake \
      ninja-build \
      libdbus-1-dev \
      libasound2-dev \
      libudev-dev \
      libgl1-mesa-dev \
      libx11-dev \
      libxi-dev \
      libxcursor-dev \
      libxrandr-dev \
      libxinerama-dev \
      libwayland-dev \
      libxkbcommon-dev \
      libvulkan-dev
    mkdir -p "$HOME"
    curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
    cargo build "$@"
  ' bash "$@"
