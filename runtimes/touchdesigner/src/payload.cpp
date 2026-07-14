#include "payload.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>

namespace sahou {
namespace {

// Escape a string for use as a JSON key/value: quotes, backslash, and control chars.
std::string escape(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 2);
    for (char c : s) {
        switch (c) {
            case '"':  out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof buf, "\\u%04x", static_cast<unsigned char>(c));
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out;
}

// Format a double as the shortest decimal that round-trips it. Non-finite -> "null".
std::string number(double v) {
    if (!std::isfinite(v)) {
        return "null";
    }
    char buf[32];
    for (int prec = 1; prec <= 17; ++prec) {
        std::snprintf(buf, sizeof buf, "%.*g", prec, v);
        if (std::strtod(buf, nullptr) == v) {
            break;
        }
    }
    return std::string(buf);
}

}  // namespace

std::string payload_json(const std::vector<Channel>& channels) {
    std::string out = "{";
    bool first = true;
    for (const auto& ch : channels) {
        if (!first) {
            out += ",";
        }
        first = false;
        out += "\"";
        out += escape(ch.name);
        out += "\":";
        out += number(ch.value);
    }
    out += "}";
    return out;
}

}  // namespace sahou
