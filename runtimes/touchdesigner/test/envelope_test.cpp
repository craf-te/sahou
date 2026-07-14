// Zero-dependency unit test for the pure envelope helpers (TDD).
// Build & run:  /usr/bin/c++ -std=c++17 runtimes/touchdesigner/src/envelope.cpp runtimes/touchdesigner/test/envelope_test.cpp -o /tmp/envelope_test && /tmp/envelope_test
#include "../src/envelope.h"

#include <cstdio>
#include <string>

static int g_failures = 0;

static void check_str(const char* label, const std::string& got, const std::string& want) {
    if (got != want) {
        std::printf("FAIL %s\n  got : %s\n  want: %s\n", label, got.c_str(), want.c_str());
        ++g_failures;
    } else {
        std::printf("ok   %s\n", label);
    }
}

static void check_bool(const char* label, bool got, bool want) {
    if (got != want) {
        std::printf("FAIL %s  got=%d want=%d\n", label, got, want);
        ++g_failures;
    } else {
        std::printf("ok   %s\n", label);
    }
}

int main() {
    using sahou::envelope_ok;
    using sahou::first_diag;

    const std::string ok = R"({"ok":true,"msg":{"key":"sahou/motion","wire":"{\"x\":0.5}"}})";
    const std::string no =
        R"({"ok":false,"diags":[{"code":"required","path":"connections.motion.payload.y","message":"missing required field 'y'"}]})";
    const std::string multi =
        R"({"ok":false,"diags":[{"code":"a","path":"p1","message":"m1"},{"code":"b","path":"p2","message":"m2"}]})";

    // ok flag
    check_bool("ok-true", envelope_ok(ok), true);
    check_bool("ok-false", envelope_ok(no), false);

    // no diagnostic when accepted
    check_str("diag-empty-when-ok", first_diag(ok), "");

    // first diagnostic, formatted
    check_str("diag-single", first_diag(no),
              "required at connections.motion.payload.y: missing required field 'y'");

    // multiple diagnostics -> the first one
    check_str("diag-first-of-many", first_diag(multi), "a at p1: m1");

    // envelope_string: pull the resolved keyexpr and schema_hash off an ok envelope
    using sahou::envelope_string;
    const std::string ok_full =
        R"({"ok":true,"msg":{"key":"sahou/motion","wire":"{\"x\":0.5}","attachment":"62f306a291600aac"}})";
    check_str("string-key", envelope_string(ok_full, "key"), "sahou/motion");
    check_str("string-attachment", envelope_string(ok_full, "attachment"), "62f306a291600aac");
    check_str("string-absent", envelope_string(ok_full, "nope"), "");

    // json_string_array: parse the node/connection selector lists
    using sahou::json_string_array;
    auto join = [](const std::vector<std::string>& v) {
        std::string s;
        for (std::size_t i = 0; i < v.size(); ++i) {
            if (i) s += "|";
            s += v[i];
        }
        return s;
    };
    check_str("arr-empty", join(json_string_array("[]")), "");
    check_str("arr-one", join(json_string_array(R"(["td"])")), "td");
    check_str("arr-many", join(json_string_array(R"(["debug_tap","points","touch"])")),
              "debug_tap|points|touch");

    if (g_failures) {
        std::printf("\n%d FAILED\n", g_failures);
        return 1;
    }
    std::printf("\nALL PASSED\n");
    return 0;
}
