#include "envelope.h"

namespace sahou {

bool envelope_ok(const std::string& envelope) {
    return envelope.find("\"ok\":true") != std::string::npos;
}

std::string envelope_string(const std::string& envelope, const std::string& key) {
    const std::string pat = "\"" + key + "\":\"";
    const std::size_t p = envelope.find(pat);
    if (p == std::string::npos) {
        return "";
    }
    const std::size_t start = p + pat.size();
    const std::size_t end = envelope.find('"', start);
    if (end == std::string::npos) {
        return "";
    }
    return envelope.substr(start, end - start);
}

std::vector<std::string> json_string_array(const std::string& json) {
    std::vector<std::string> out;
    std::size_t p = 0;
    while (true) {
        const std::size_t open = json.find('"', p);
        if (open == std::string::npos) {
            break;
        }
        const std::size_t close = json.find('"', open + 1);
        if (close == std::string::npos) {
            break;
        }
        out.push_back(json.substr(open + 1, close - open - 1));
        p = close + 1;
    }
    return out;
}

std::string first_diag(const std::string& envelope) {
    if (envelope.find("\"diags\":") == std::string::npos) {
        return "";
    }
    // code / path / message are unique to diagnostics, so a plain scan finds the first diag's fields.
    const std::string code = envelope_string(envelope, "code");
    const std::string path = envelope_string(envelope, "path");
    const std::string message = envelope_string(envelope, "message");
    if (code.empty() && path.empty() && message.empty()) {
        return "";
    }
    return code + " at " + path + ": " + message;
}

}  // namespace sahou
