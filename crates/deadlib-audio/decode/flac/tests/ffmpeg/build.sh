#!/bin/sh
set -eu
root=$1
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir "$root"
curl --fail --location --connect-timeout 20 --max-time 180 --output "$root/source.tar.xz" https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
sha256sum "$root/source.tar.xz" > "$root/source.sha256"
printf '%s  %s\n' 8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e "$root/source.tar.xz" | sha256sum --check --status
tar -xf "$root/source.tar.xz" -C "$root"
cd "$root/ffmpeg-9.0.2"
./configure --disable-everything --disable-programs --disable-doc --disable-network \
    --disable-autodetect --disable-x86asm --enable-decoder=flac --enable-parser=flac \
    --enable-demuxer=flac,ogg --enable-protocol=file > "$root/configure.log"
make -j8 > "$root/build.log" 2>&1
cc -std=c11 -O2 -Wall -Wextra -Werror -I. "$source_dir/oracle.c" libavformat/libavformat.a libavcodec/libavcodec.a \
    libavutil/libavutil.a -lm -pthread -o "$root/ffmpeg-flac-oracle"
"$root/ffmpeg-flac-oracle" --version
