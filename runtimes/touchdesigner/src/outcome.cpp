#include "outcome.h"

namespace sahou {

// Extract a JSON string value for `"<key>":"..."`, honoring backslash escapes, and unescape it.
// Returns "" if the key/opening quote is absent. Handles the escapes serde_json emits for a string
// (\" \\ \/ \n \t \r \b \f and \uXXXX for the ASCII range); a \u outside ASCII becomes '?'.
std::string json_string_field(const std::string& json, const std::string& key) {
    const std::string pat = "\"" + key + "\":\"";
    const std::size_t p = json.find(pat);
    if (p == std::string::npos) {
        return "";
    }
    std::string out;
    std::size_t i = p + pat.size();
    while (i < json.size()) {
        const char c = json[i];
        if (c == '"') {
            break;  // unescaped closing quote
        }
        if (c == '\\' && i + 1 < json.size()) {
            const char e = json[i + 1];
            switch (e) {
                case '"': out.push_back('"'); break;
                case '\\': out.push_back('\\'); break;
                case '/': out.push_back('/'); break;
                case 'n': out.push_back('\n'); break;
                case 't': out.push_back('\t'); break;
                case 'r': out.push_back('\r'); break;
                case 'b': out.push_back('\b'); break;
                case 'f': out.push_back('\f'); break;
                case 'u': {
                    // \uXXXX — parse the 4 hex digits; emit ASCII directly, else '?'.
                    if (i + 5 < json.size()) {
                        int code = 0;
                        bool ok = true;
                        for (int k = 0; k < 4; ++k) {
                            const char h = json[i + 2 + k];
                            code <<= 4;
                            if (h >= '0' && h <= '9') code += h - '0';
                            else if (h >= 'a' && h <= 'f') code += h - 'a' + 10;
                            else if (h >= 'A' && h <= 'F') code += h - 'A' + 10;
                            else { ok = false; break; }
                        }
                        out.push_back(ok && code < 128 ? static_cast<char>(code) : '?');
                        i += 6;
                        continue;
                    }
                    out.push_back('?');
                    break;
                }
                default: out.push_back(e); break;
            }
            i += 2;
            continue;
        }
        out.push_back(c);
        ++i;
    }
    return out;
}

Accepted accept_result(const std::string& outcome_json) {
    // "result" is a bare identifier value (accept/reject/hash_mismatch) — no escaping to worry about.
    const std::string r = envelope_string(outcome_json, "result");
    if (r == "accept") return Accepted::Accept;
    if (r == "reject") return Accepted::Reject;
    if (r == "hash_mismatch") return Accepted::HashMismatch;
    return Accepted::None;
}

std::string accept_payload(const std::string& outcome_json) {
    return json_string_field(outcome_json, "payload");
}

std::string accept_sender_hash(const std::string& outcome_json) {
    // A 16-hex string — no escaping, envelope_string is enough.
    return envelope_string(outcome_json, "sender_hash");
}

}  // namespace sahou
