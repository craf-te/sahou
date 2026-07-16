cmake_minimum_required(VERSION 3.20)
project(SahouTD LANGUAGES CXX)

# Path to the Rust artifacts, built with --target x86_64-pc-windows-msvc.
# Passed by the justfile: -DSAHOU_TARGET_DIR=<repo>/target/x86_64-pc-windows-msvc/release
if(NOT DEFINED SAHOU_TARGET_DIR)
    message(FATAL_ERROR "SAHOU_TARGET_DIR not set (path to target/x86_64-pc-windows-msvc/release)")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# This file lives in runtimes/touchdesigner/windows/.
set(TD "${CMAKE_CURRENT_SOURCE_DIR}/..")
set(SRC "${TD}/src")

# System libs the sahou_core staticlib pulls in (mac's CoreFoundation/Security analogue).
# Captured from `cargo rustc -p sahou-core --features capi --target x86_64-pc-windows-msvc
# -- --print native-static-libs` (the /defaultlib:msvcrt it also reports is satisfied by /MD below).
set(RUST_NATIVE_LIBS kernel32 ntdll userenv ws2_32 dbghelp)

add_library(SahouOut SHARED
    "${SRC}/SahouOutCHOP.cpp" "${SRC}/payload.cpp" "${SRC}/envelope.cpp")
add_library(SahouIn SHARED
    "${SRC}/SahouInCHOP.cpp" "${SRC}/envelope.cpp" "${SRC}/outcome.cpp")

foreach(tgt SahouOut SahouIn)
    target_include_directories(${tgt} PRIVATE
        "${TD}/vendor" "${TD}/../../core" "${TD}/transport")
    target_compile_definitions(${tgt} PRIVATE SAHOU_CAPI)
    # Treat sources as UTF-8 regardless of the machine's locale code page (silences C4819
    # and keeps non-ASCII string literals from being mangled on e.g. a CP932 host).
    target_compile_options(${tgt} PRIVATE /utf-8)
    set_target_properties(${tgt} PROPERTIES
        PREFIX "" SUFFIX ".dll"
        MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")   # /MD
    target_link_libraries(${tgt} PRIVATE
        "${SAHOU_TARGET_DIR}/sahou_core.lib"
        "${SAHOU_TARGET_DIR}/sahou_transport.dll.lib"
        ${RUST_NATIVE_LIBS})
endforeach()
