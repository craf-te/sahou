// Sahou Out CHOP — publishes the input CHOP's channels as a Sahou connection message.
// This increment is validate-only: it runs the send boundary through the Rust core
// (sahou_prepare_publish) and reports OK / NO on the node. The actual Zenoh publish is
// the next stage. All contract interpretation lives in the Rust core; this C++ is thin glue.
#ifndef SAHOU_TD_SAHOU_OUT_CHOP_H
#define SAHOU_TD_SAHOU_OUT_CHOP_H

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include "CHOP_CPlusPlusBase.h"  // vendored TD SDK (Derivative Shared Use License, gitignored)

using namespace TD;

// Opaque Rust runtime handle (defined in the C ABI, core/sahou.h). Forward-declared so the
// header does not pull in the C ABI; the .cpp includes sahou.h.
struct SahouRuntime;

class SahouOutCHOP : public CHOP_CPlusPlusBase {
public:
    explicit SahouOutCHOP(const OP_NodeInfo* info);
    ~SahouOutCHOP() override;

    void getGeneralInfo(CHOP_GeneralInfo*, const OP_Inputs*, void*) override;
    bool getOutputInfo(CHOP_OutputInfo*, const OP_Inputs*, void*) override;
    void getChannelName(int32_t index, OP_String* name, const OP_Inputs*, void*) override;
    void execute(CHOP_Output*, const OP_Inputs*, void*) override;

    int32_t getNumInfoCHOPChans(void*) override;
    void getInfoCHOPChan(int32_t index, OP_InfoCHOPChan* chan, void*) override;
    bool getInfoDATSize(OP_InfoDATSize*, void*) override;
    void getInfoDATEntries(int32_t index, int32_t nEntries, OP_InfoDATEntries*, void*) override;

    void setupParameters(OP_ParameterManager*, void*) override;
    void pulsePressed(const char* name, void*) override;
    void buildDynamicMenu(const OP_Inputs*, OP_BuildDynamicMenuInfo*, void*) override;
    void getWarningString(OP_String* warning, void*) override;
    void getErrorString(OP_String* error, void*) override;

private:
    // (Re)load the descriptor from the IR File parameter when it changed or a Reload was pulsed.
    void reloadIfNeeded(const OP_Inputs* inputs);

    SahouRuntime* myRuntime = nullptr;  // owned; freed in dtor / on reload
    std::string myLoadedPath;           // absolute path the runtime was loaded from
    std::string myLoadError;            // non-empty => the IR failed to load (hard error)
    bool myForceReload = false;         // set by the Reload pulse
    bool myDoTest = false;              // set by the Test pulse (perform a sample send next cook)
    std::string myTestStatus;           // last Test-send result (shown in the Info DAT)

    // Per-cook status, computed in execute() and read back by the accessor overrides.
    int32_t myExecuteCount = 0;
    uint64_t mySeq = 0;
    bool myValidated = false;  // did a boundary check run this cook?
    bool myOk = false;         // last verdict
    std::string myNode;
    std::string myConn;
    std::string myKey;          // resolved keyexpr (ok envelope)
    std::string myHash;         // schema_hash / attachment (ok envelope)
    std::string myLastPayload;  // the JSON that was validated
    std::string myDiag;         // first diagnostic on NO (shown as error)
    std::string myWarning;      // soft status (not-yet-ready conditions)

    // Expected payload schema of the selected connection, as [name, type, required, detail] rows.
    // Feeds the Info DAT ("what should I send?") and the pre-wire warning guide.
    std::vector<std::array<std::string, 4>> myFields;
};

#endif  // SAHOU_TD_SAHOU_OUT_CHOP_H
