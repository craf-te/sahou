// FFI smoke test — TD-independent proof of the Sahou In CHOP core path:
//   sahou_sample -> sahou_prepare_publish (wire+attachment) -> sahou_accept_sample -> Accept
//   -> sahou_decode_channels -> numeric channels.
// Links only against the C ABI (core/sahou.h + libsahou_core.a), exactly what the plugin links,
// so it de-risks the linkage before the .plugin build.
//
// Build & run (from repo root, after `just build-ffi` + generating the demo descriptor):
//   /usr/bin/c++ -std=c++17 -DSAHOU_CAPI -Icore \
//       runtimes/touchdesigner/src/outcome.cpp runtimes/touchdesigner/src/envelope.cpp \
//       runtimes/touchdesigner/test/ffi_in_smoke.cpp target/release/libsahou_core.a \
//       -framework CoreFoundation -framework Security -o /tmp/ffi_in_smoke \
//   && /tmp/ffi_in_smoke runtimes/touchdesigner/examples/gen/descriptor.json
#include "../src/envelope.h"
#include "../src/outcome.h"

#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

extern "C" {
#include "sahou.h"
}

static int g_failures = 0;
static std::string slurp(const char* p) {
    std::ifstream f(p);
    std::stringstream ss;
    ss << f.rdbuf();
    return ss.str();
}
static void check(const char* label, bool cond, const std::string& detail = "") {
    std::printf("%s %-22s %s\n", cond ? "ok  " : "FAIL", label, detail.c_str());
    if (!cond) ++g_failures;
}

int main(int argc, char** argv) {
    const char* desc_path =
        argc > 1 ? argv[1] : "runtimes/touchdesigner/examples/gen/descriptor.json";
    std::string desc = slurp(desc_path);
    if (desc.empty()) {
        std::printf("FAIL could not read descriptor: %s\n", desc_path);
        return 1;
    }
    SahouRuntime* rt = sahou_runtime_new(desc.c_str());
    if (!rt) {
        std::printf("FAIL sahou_runtime_new returned null\n");
        return 1;
    }

    // Receiver selectors feed the In CHOP dropdowns.
    {
        char* nj = sahou_subscribing_nodes(rt);
        std::vector<std::string> nodes = sahou::json_string_array(nj ? nj : "");
        sahou_free(nj);
        check("subscribing-nodes", nodes.size() == 1 && nodes[0] == "viz",
              nodes.empty() ? "(none)" : nodes[0]);
    }
    {
        char* cj = sahou_connections_to(rt, "viz");
        std::vector<std::string> conns = sahou::json_string_array(cj ? cj : "");
        sahou_free(cj);
        check("connections-to", conns.size() == 1 && conns[0] == "motion",
              conns.empty() ? "(none)" : conns[0]);
    }
    {
        char* k = sahou_connection_key(rt, "motion");
        std::string key = k ? k : "";
        sahou_free(k);
        check("connection-key", key == "sahou/motion", key);
    }

    // Build a valid wire+attachment for `motion` via the send boundary (what a real sender emits).
    char* smp = sahou_sample(rt, "motion");
    std::string sample = smp ? smp : "{}";
    sahou_free(smp);
    char* env = sahou_prepare_publish(rt, "td", "motion", sample.c_str(), 0);
    std::string envelope = env ? env : "";
    sahou_free(env);
    std::string att = sahou::envelope_string(envelope, "attachment");

    // Receive boundary: accept the wire as `viz` on `motion`.
    char* out = sahou_accept_sample(rt, "viz", "motion",
                                    reinterpret_cast<const uint8_t*>(sample.data()), sample.size(),
                                    att.c_str(), 0, nullptr);
    std::string outcome = out ? out : "";
    sahou_free(out);
    check("accept ok", sahou::accept_result(outcome) == sahou::Accepted::Accept, outcome);

    // Decode the accepted payload into numeric channels (name,count,value groups).
    std::string payload = sahou::accept_payload(outcome);
    char* dj = sahou_decode_channels(rt, "motion", payload.c_str());
    std::vector<std::string> flat = sahou::json_string_array(dj ? dj : "");
    sahou_free(dj);
    // motion = x,y,speed floats -> 3 groups of (name,"1",value) = 9 elements.
    check("decode-channels",
          flat.size() == 9 && flat[0] == "x" && flat[1] == "1" && flat[3] == "y" &&
              flat[6] == "speed",
          std::to_string(flat.size()) + " elems");

    // A wrong-node receive is a NO (viz receives motion; td is the sender, not a receiver).
    char* bad = sahou_accept_sample(rt, "td", "motion",
                                    reinterpret_cast<const uint8_t*>(sample.data()), sample.size(),
                                    att.c_str(), 0, nullptr);
    std::string badout = bad ? bad : "";
    sahou_free(bad);
    check("wrong-node rejected", sahou::accept_result(badout) != sahou::Accepted::Accept, badout);

    sahou_runtime_free(rt);
    if (g_failures) {
        std::printf("\n%d FAILED\n", g_failures);
        return 1;
    }
    std::printf("\nALL PASSED\n");
    return 0;
}
