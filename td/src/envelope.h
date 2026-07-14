// Pure helpers to read the JSON envelope returned by the Rust core
// (`sahou_prepare_publish`) for status display — no TouchDesigner SDK dependency,
// so they are unit-testable. Display-only, deliberately tolerant string scanning
// (the authoritative structure lives in the Rust core).
#ifndef SAHOU_TD_ENVELOPE_H
#define SAHOU_TD_ENVELOPE_H

#include <string>
#include <vector>

namespace sahou {

// Did the core accept the boundary? envelope = `{"ok":true,...}` | `{"ok":false,...}`.
bool envelope_ok(const std::string& envelope);

// One-line summary of the first diagnostic as "code at path: message",
// or "" when the envelope is ok / carries no diagnostics.
std::string first_diag(const std::string& envelope);

// Value of the first `"key":"..."` string field in the envelope, or "" if absent.
// Used to surface the resolved keyexpr ("key") and schema_hash ("attachment") on an ok envelope.
std::string envelope_string(const std::string& envelope, const std::string& key);

// Parse a JSON string array (`["a","b"]`) into its elements. Tolerant scan; assumes elements have
// no embedded double quote (true for Sahou identifiers). Feeds the Node / Connection selectors.
std::vector<std::string> json_string_array(const std::string& json);

}  // namespace sahou

#endif  // SAHOU_TD_ENVELOPE_H
