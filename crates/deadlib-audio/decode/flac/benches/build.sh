#!/bin/sh
set -eu
root=$1
repo=$2
archive=$3
mkdir "$root"
tar -xf "$archive" -C "$root"
cd "$root/ffmpeg-9.0.2"
./configure --disable-everything --disable-programs --disable-doc --disable-network \
    --disable-autodetect --disable-avformat --disable-avfilter --disable-avdevice \
    --disable-swresample --disable-swscale --enable-decoder=flac --enable-lto \
    > "$root/ffmpeg-configure.log"
make -j8 > "$root/ffmpeg-build.log" 2>&1
cmake -S "$repo/crates/deadlib-audio/decode/flac/benches/oracle" -B "$root/oracle" \
    -DCMAKE_BUILD_TYPE=Release \
    -DREFERENCE_ROOT="$repo/target/deadlib-flac-reference/flac" \
    -DOGG_REFERENCE_ROOT="$repo/target/deadlib-audio-reference/ogg" \
    -DFFMPEG_ROOT="$root/ffmpeg-9.0.2" > "$root/oracle-configure.log"
cmake --build "$root/oracle" --target flac_decode_benchmark -j8 > "$root/oracle-build.log" 2>&1
printf '%s\n' "$root/oracle/flac_decode_benchmark"
