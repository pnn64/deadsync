cmake_minimum_required(VERSION 3.16)
project(deadlib_flac_decode_benchmark C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../tests/oracle" references)
add_executable(flac_decode_benchmark benchmark.cpp)
target_include_directories(flac_decode_benchmark PRIVATE "${FFMPEG_ROOT}")
target_link_libraries(flac_decode_benchmark PRIVATE FLAC
    "${FFMPEG_ROOT}/libavcodec/libavcodec.a" "${FFMPEG_ROOT}/libavutil/libavutil.a" m pthread dl)
