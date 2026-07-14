#!/usr/bin/env sh
# Build & run the TD op's TD-independent tests. Run from anywhere (resolves the repo root).
#
#   pure unit tests : payload_test, envelope_test        (no SDK, no FFI)
#   ffi smoke       : payload_json -> sahou_prepare_publish -> OK/NO   (needs the C ABI)
#
# The FFI smoke test needs `just build-ffi` (target/release/libsahou_core.a + core/sahou.h)
# and a generated descriptor (`sahou gen td/examples/schema.sahou.yaml --out-dir td/examples/gen`).
# Uses /usr/bin/c++ explicitly (a bare `cc`/`c++` may be shell-aliased).
set -e

CXX="${CXX:-/usr/bin/c++}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
OUT="$(mktemp -d)"
CXXFLAGS="-std=c++17 -Wall -Wextra"

echo "== payload_test =="
$CXX $CXXFLAGS td/src/payload.cpp td/test/payload_test.cpp -o "$OUT/payload_test"
"$OUT/payload_test"

echo
echo "== envelope_test =="
$CXX $CXXFLAGS td/src/envelope.cpp td/test/envelope_test.cpp -o "$OUT/envelope_test"
"$OUT/envelope_test"

echo
echo "== ffi_smoke =="
if [ -f target/release/libsahou_core.a ] && [ -f td/examples/gen/descriptor.json ]; then
    $CXX $CXXFLAGS -DSAHOU_CAPI -Icore \
        td/src/payload.cpp td/src/envelope.cpp td/test/ffi_smoke.cpp target/release/libsahou_core.a \
        -framework CoreFoundation -framework Security -o "$OUT/ffi_smoke"
    "$OUT/ffi_smoke" td/examples/gen/descriptor.json
else
    echo "SKIP: run 'just build-ffi' and"
    echo "      'cargo run -p sahou -- gen td/examples/schema.sahou.yaml --out-dir td/examples/gen' first"
fi
