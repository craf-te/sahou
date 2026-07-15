// Pure unit tests for the AcceptOutcome reader (no TD SDK, no FFI).
#include "../src/outcome.h"

#include <cstdio>
#include <string>

static int g_failures = 0;

static void check(const char* label, bool cond) {
    std::printf("%s %s\n", cond ? "ok  " : "FAIL", label);
    if (!cond) ++g_failures;
}

int main() {
    using namespace sahou;

    const std::string accept = R"({"result":"accept","payload":"{\"x\":0.5}"})";
    check("accept result", accept_result(accept) == Accepted::Accept);
    check("accept payload", accept_payload(accept) == "{\"x\":0.5}");

    const std::string reject =
        R"({"result":"reject","diags":[{"code":"required","path":"connections.motion.payload.speed","message":"missing required field 'speed'"}]})";
    check("reject result", accept_result(reject) == Accepted::Reject);
    check("reject diag", first_diag(reject).find("missing required field") != std::string::npos);

    const std::string mism = R"({"result":"hash_mismatch","sender_hash":"deadbeefdeadbeef"})";
    check("mismatch result", accept_result(mism) == Accepted::HashMismatch);
    check("mismatch hash", accept_sender_hash(mism) == "deadbeefdeadbeef");

    check("empty is none", accept_result("") == Accepted::None);

    if (g_failures) { std::printf("\n%d FAILED\n", g_failures); return 1; }
    std::printf("\nALL PASSED\n");
    return 0;
}
