// Zero-dependency unit test for the pure payload mapping (TDD).
// Build & run:  /usr/bin/c++ -std=c++17 runtimes/touchdesigner/src/payload.cpp runtimes/touchdesigner/test/payload_test.cpp -o /tmp/payload_test && /tmp/payload_test
#include "../src/payload.h"

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

static int g_failures = 0;

static void check(const char* label, const std::string& got, const std::string& want) {
    if (got != want) {
        std::printf("FAIL %s\n  got : %s\n  want: %s\n", label, got.c_str(), want.c_str());
        ++g_failures;
    } else {
        std::printf("ok   %s\n", label);
    }
}

int main() {
    using sahou::payload_json;

    // No channels -> empty object.
    check("empty", payload_json({}), "{}");

    // A single scalar channel.
    check("single", payload_json({{"x", 0.5}}), R"({"x":0.5})");

    // Keys keep the input order.
    check("order", payload_json({{"a", 1.0}, {"b", 2.0}}), R"({"a":1,"b":2})");

    // Integral values drop the trailing ".0".
    check("integral", payload_json({{"n", 3.0}}), R"({"n":3})");

    // Fractional values keep the shortest round-tripping form.
    check("fraction", payload_json({{"f", 0.25}}), R"({"f":0.25})");

    // Negative values.
    check("negative", payload_json({{"g", -2.5}}), R"({"g":-2.5})");

    // Non-finite -> null (keeps the JSON valid; the Rust boundary then says NO on type).
    check("nan", payload_json({{"bad", std::nan("")}}), R"({"bad":null})");
    check("inf", payload_json({{"bad", HUGE_VAL}}), R"({"bad":null})");

    // A key that needs JSON escaping.
    check("escape-key", payload_json({{"a\"b", 1.0}}), R"({"a\"b":1})");

    if (g_failures) {
        std::printf("\n%d FAILED\n", g_failures);
        return 1;
    }
    std::printf("\nALL PASSED\n");
    return 0;
}
