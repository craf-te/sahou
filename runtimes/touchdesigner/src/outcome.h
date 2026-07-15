// Pure helper to read the AcceptOutcome JSON returned by the Rust core
// (`sahou_accept_sample`) for the Sahou In CHOP — no TouchDesigner SDK dependency, so it is
// unit-testable. The authoritative structure lives in the Rust core; this is display-only, mirroring
// envelope.h (which reads the send-boundary envelope). The outcome is one of:
//   {"result":"accept","payload":"<escaped JSON>"}
//   {"result":"reject","diags":[{code,path,message},...]}
//   {"result":"hash_mismatch","sender_hash":"<16 hex>"}
#ifndef SAHOU_TD_OUTCOME_H
#define SAHOU_TD_OUTCOME_H

#include <string>

#include "envelope.h"  // reuse first_diag / envelope_string for the Reject diag + simple fields

namespace sahou {

// The receive-boundary verdict. Mirrors the core AcceptOutcome tag.
enum class Accepted { None, Accept, Reject, HashMismatch };

// Read the "result" tag: accept | reject | hash_mismatch (None if absent/empty).
Accepted accept_result(const std::string& outcome_json);

// The decoded payload string on Accept ("" otherwise). The payload rides as an *escaped* JSON string
// value, so this unescapes it back to the canonical JSON text (valid to re-parse).
std::string accept_payload(const std::string& outcome_json);

// The sender_hash on HashMismatch ("" otherwise).
std::string accept_sender_hash(const std::string& outcome_json);

// Extract the JSON string value for `"<key>":"..."`, honoring backslash escapes and unescaping it
// back to the original text. "" if the key is absent. General-purpose (used for the outcome payload
// and for the transport poll's `wire` field, both of which are escaped JSON-in-a-string).
std::string json_string_field(const std::string& json, const std::string& key);

}  // namespace sahou

#endif  // SAHOU_TD_OUTCOME_H
