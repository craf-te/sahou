// Pure payload mapping for the Sahou Out CHOP — no TouchDesigner SDK dependency,
// so it is unit-testable on its own (TDD). It only turns named scalar channel
// values into a JSON object string; the real type check happens in the Rust core
// (`sahou_prepare_publish`), which is the single source of "NO".
#ifndef SAHOU_TD_PAYLOAD_H
#define SAHOU_TD_PAYLOAD_H

#include <string>
#include <vector>

namespace sahou {

// One input CHOP channel projected to its current scalar value.
struct Channel {
    std::string name;
    double value;
};

// Build a JSON object payload from named scalar channel values.
// - Keys keep the input order.
// - Numbers use the shortest decimal that round-trips the double
//   (integral values drop the trailing ".0", e.g. 3.0 -> 3).
// - Non-finite values (NaN / +-Inf) become JSON null, so the string stays
//   valid JSON and the Rust boundary reports a clean type NO instead.
// - Keys are JSON-escaped.
std::string payload_json(const std::vector<Channel>& channels);

}  // namespace sahou

#endif  // SAHOU_TD_PAYLOAD_H
