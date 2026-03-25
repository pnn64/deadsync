#!/usr/bin/env bash
set -euo pipefail

arch="${1:-}"
target="${2:-}"

if [ -z "${arch}" ]; then
  echo "usage: $0 <arch> [target]" >&2
  exit 1
fi

if [ -z "${target}" ]; then
  case "${arch}" in
    x86_64) target="native" ;;
    arm64) target="aarch64-unknown-linux-gnu" ;;
    *)
      echo "unknown arch: ${arch}" >&2
      exit 1
      ;;
  esac
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required to build Linux releases." >&2
  exit 1
fi

image="${LINUX_BUILD_IMAGE:-debian:12}"
host_uid="$(id -u)"
host_gid="$(id -g)"

run_x86_64() {
  docker run --rm -i \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}" \
    -e DEBIAN_FRONTEND=noninteractive \
    -e HOST_UID="${host_uid}" \
    -e HOST_GID="${host_gid}" \
    -v "${PWD}:/work" \
    -w /work \
    "${image}" \
    bash -seu <<'EOF'
trap 'if [ -e /work/target ]; then chown -R "$HOST_UID:$HOST_GID" /work/target; fi' EXIT
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
  libasound2-dev \
  libx11-dev \
  libx11-xcb-dev \
  libxi-dev \
  libxcursor-dev \
  libxrandr-dev \
  libxinerama-dev \
  libwayland-dev \
  libxkbcommon-dev \
  libvulkan-dev \
  libudev-dev \
  libgl1-mesa-dev \
  libxcb1-dev \
  libxcb-randr0-dev
curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
. "$HOME/.cargo/env"
cargo build --release --locked
EOF
}

run_arm64() {
  docker run --rm -i \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}" \
    -e DEBIAN_FRONTEND=noninteractive \
    -e HOST_UID="${host_uid}" \
    -e HOST_GID="${host_gid}" \
    -v "${PWD}:/work" \
    -w /work \
    "${image}" \
    bash -seu <<'EOF'
trap 'if [ -e /work/target ]; then chown -R "$HOST_UID:$HOST_GID" /work/target; fi' EXIT
dpkg --add-architecture arm64
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
  gcc-aarch64-linux-gnu \
  libc6-dev-arm64-cross \
  libasound2-dev:arm64 \
  libx11-dev:arm64 \
  libx11-xcb-dev:arm64 \
  libxi-dev:arm64 \
  libxcursor-dev:arm64 \
  libxrandr-dev:arm64 \
  libxinerama-dev:arm64 \
  libwayland-dev:arm64 \
  libxkbcommon-dev:arm64 \
  libvulkan-dev:arm64 \
  libudev-dev:arm64 \
  libgl1-mesa-dev:arm64 \
  libxcb1-dev:arm64 \
  libxcb-randr0-dev:arm64
curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain stable
. "$HOME/.cargo/env"
rustup target add aarch64-unknown-linux-gnu
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
cargo build --release --locked --target aarch64-unknown-linux-gnu
EOF
}

case "${arch}" in
  x86_64)
    if [ "${target}" != "native" ]; then
      echo "x86_64 builds only support target=native, got: ${target}" >&2
      exit 1
    fi
    run_x86_64
    ;;
  arm64)
    if [ "${target}" != "aarch64-unknown-linux-gnu" ]; then
      echo "arm64 builds only support target=aarch64-unknown-linux-gnu, got: ${target}" >&2
      exit 1
    fi
    run_arm64
    ;;
  *)
    echo "unknown arch: ${arch}" >&2
    exit 1
    ;;
esac
