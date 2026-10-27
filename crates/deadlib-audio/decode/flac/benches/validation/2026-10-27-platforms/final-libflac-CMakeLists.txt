cmake_minimum_required(VERSION 3.16)
project(deadlib_flac_latency_benchmark C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../tests/oracle" references)
add_executable(flac_latency_benchmark benchmark.cpp opaque.c)
target_link_libraries(flac_latency_benchmark PRIVATE FLAC)
# Every decoded PCM address escapes whole-program optimization. Keep the helper
# separate from LTO on both compilers; no samples are copied or written to disk.
if(MSVC)
    set_source_files_properties(opaque.c PROPERTIES COMPILE_OPTIONS "/GL-")
else()
    set_source_files_properties(opaque.c PROPERTIES COMPILE_OPTIONS "-fno-lto")
endif()
