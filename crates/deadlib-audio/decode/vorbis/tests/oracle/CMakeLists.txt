cmake_minimum_required(VERSION 3.16)
project(deadlib_vorbis_oracle C)
if(NOT DEFINED REFERENCE_ROOT)
    message(FATAL_ERROR "Set REFERENCE_ROOT to the directory containing pinned ogg/vorbis sources")
endif()
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(INSTALL_DOCS OFF CACHE BOOL "" FORCE)
set(INSTALL_PKG_CONFIG_MODULE OFF CACHE BOOL "" FORCE)
set(INSTALL_CMAKE_PACKAGE_MODULE OFF CACHE BOOL "" FORCE)
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
add_subdirectory("${REFERENCE_ROOT}/ogg" ogg)
# Satisfy Vorbis's FindOgg using the existing build target; no installed system
# library or binary path is substituted for the pinned source build.
set(OGG_INCLUDE_DIR "${REFERENCE_ROOT}/ogg/include" CACHE PATH "" FORCE)
set(OGG_LIBRARY ogg CACHE STRING "" FORCE)
add_subdirectory("${REFERENCE_ROOT}/vorbis" vorbis)
add_executable(deadlib_vorbis_oracle oracle.c)
target_include_directories(deadlib_vorbis_oracle PRIVATE "${REFERENCE_ROOT}/vorbis/lib")
target_link_libraries(deadlib_vorbis_oracle PRIVATE vorbis vorbisfile)

# Run the pinned upstream coded test grid verbatim, retaining its successful
# outputs for native differential validation instead of deleting each fixture.
add_executable(deadlib_vorbis_upstream_vectors
    "${REFERENCE_ROOT}/vorbis/test/test.c"
    "${REFERENCE_ROOT}/vorbis/test/util.c"
    "${REFERENCE_ROOT}/vorbis/test/write_read.c"
    retain_vectors.c)
target_compile_definitions(deadlib_vorbis_upstream_vectors PRIVATE _USE_MATH_DEFINES)
if(MSVC)
    target_compile_options(deadlib_vorbis_upstream_vectors PRIVATE /FI"${CMAKE_CURRENT_SOURCE_DIR}/retain_vectors.h")
else()
    target_compile_options(deadlib_vorbis_upstream_vectors PRIVATE -include "${CMAKE_CURRENT_SOURCE_DIR}/retain_vectors.h")
endif()
target_link_libraries(deadlib_vorbis_upstream_vectors PRIVATE vorbisenc vorbis vorbisfile)
