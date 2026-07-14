// FFI smoke test — TD-independent proof that the op's core path works:
//   payload_json (pure) -> sahou_prepare_publish (Rust core) -> OK / NO envelope.
// It links only against the C ABI (core/sahou.h + libsahou_core.a), exactly what the
// TouchDesigner plugin links against, so it de-risks the linkage before the .plugin build.
//
// Build & run (from repo root, after `just build-ffi`):
//   /usr/bin/c++ -std=c++17 -DSAHOU_CAPI -Icore \
//       td/src/payload.cpp td/test/ffi_smoke.cpp target/release/libsahou_core.a \
//       -framework CoreFoundation -framework Security -o /tmp/ffi_smoke \
//   && /tmp/ffi_smoke td/examples/gen/descriptor.json
#include "../src/envelope.h"
#include "../src/payload.h"

#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

extern "C" {
#include "sahou.h"
}

static int g_failures = 0;

// Read a whole file into a string.
static std::string slurp(const char* path) {
    std::ifstream f(path);
    std::stringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

// Assert the envelope's ok flag matches expectation. `want_ok` = should validation pass?
static void expect(const char* label, SahouRuntime* rt, const char* node, const char* conn,
                   const std::vector<sahou::Channel>& channels, bool want_ok) {
    std::string payload = sahou::payload_json(channels);
    char* env = sahou_prepare_publish(rt, node, conn, payload.c_str(), 0);
    std::string envelope = env ? env : "(null)";
    sahou_free(env);

    bool got_ok = envelope.find("\"ok\":true") != std::string::npos;
    if (got_ok == want_ok) {
        std::printf("ok   %-18s payload=%s\n", label, payload.c_str());
    } else {
        std::printf("FAIL %-18s payload=%s\n     -> %s\n", label, payload.c_str(),
                    envelope.c_str());
        ++g_failures;
    }
}

int main(int argc, char** argv) {
    const char* desc_path = argc > 1 ? argv[1] : "td/examples/gen/descriptor.json";
    std::string desc = slurp(desc_path);
    if (desc.empty()) {
        std::printf("FAIL could not read descriptor: %s\n", desc_path);
        return 1;
    }

    SahouRuntime* rt = sahou_runtime_new(desc.c_str());
    if (!rt) {
        std::printf("FAIL sahou_runtime_new returned null (bad descriptor?)\n");
        return 1;
    }

    // OK: all three required float fields present.
    expect("all-fields", rt, "td", "motion", {{"x", 0.5}, {"y", -1.0}, {"speed", 2.25}}, true);

    // OK (forward compat): the wire layer ignores an extra unknown field as long as the
    // defined fields are satisfied. This is intentional and distinct from the contract layer,
    // which denies unknown keys (see core/src/payload.rs "Unknown fields are dropped").
    // A typo on a *required* channel still fails below (missing-field); only extra/optional-typo is lenient.
    expect("extra-field", rt, "td", "motion", {{"x", 0.5}, {"y", 0.0}, {"speed", 0.0}, {"extra", 1.0}}, true);

    // NO: a required field is missing (e.g. a required channel was left off / mis-named).
    expect("missing-field", rt, "td", "motion", {{"x", 0.5}}, false);

    // NO: an unknown connection name.
    expect("unknown-conn", rt, "td", "nope", {{"x", 0.5}}, false);

    // Selectors that feed the Node / Connection dropdowns (parsed with the same C++ helper the op uses).
    {
        char* nj = sahou_node_list(rt);
        std::vector<std::string> nodes = sahou::json_string_array(nj ? nj : "");
        sahou_free(nj);
        bool ok = nodes.size() == 1 && nodes[0] == "td";
        std::printf("%s %-18s %s\n", ok ? "ok  " : "FAIL", "node-list", ok ? "[td]" : "(mismatch)");
        if (!ok) ++g_failures;
    }
    {
        char* cj = sahou_connections_from(rt, "td");
        std::vector<std::string> conns = sahou::json_string_array(cj ? cj : "");
        sahou_free(cj);
        bool ok = conns.size() == 1 && conns[0] == "motion";
        std::printf("%s %-18s %s\n", ok ? "ok  " : "FAIL", "connections-from",
                    ok ? "td -> [motion]" : "(mismatch)");
        if (!ok) ++g_failures;
    }
    {
        // The expected-schema panel data: 3 float fields, each as [name, type, required, detail].
        char* fj = sahou_connection_fields(rt, "motion");
        std::vector<std::string> flat = sahou::json_string_array(fj ? fj : "");
        sahou_free(fj);
        bool ok = flat.size() == 12 && flat[0] == "x" && flat[1] == "float" && flat[2] == "yes" &&
                  flat[4] == "y" && flat[8] == "speed";
        std::printf("%s %-18s %s\n", ok ? "ok  " : "FAIL", "connection-fields",
                    ok ? "motion -> x,y,speed (float)" : "(mismatch)");
        if (!ok) ++g_failures;
    }

    sahou_runtime_free(rt);

    if (g_failures) {
        std::printf("\n%d FAILED\n", g_failures);
        return 1;
    }
    std::printf("\nALL PASSED\n");
    return 0;
}
