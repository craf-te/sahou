// Sahou In CHOP — subscribes to a Sahou pub_sub connection, runs the receive boundary
// (sahou_accept_sample) through the Rust core, and outputs the accepted payload's numeric fields as
// channels. All contract interpretation lives in the Rust core; this C++ is thin glue. The mirror
// of SahouOutCHOP: receiver-side selectors, Zenoh subscribe (via the bundled transport), and a
// local "Inject Sample" pulse to test downstream wiring with no publisher.
#ifndef SAHOU_TD_SAHOU_IN_CHOP_H
#define SAHOU_TD_SAHOU_IN_CHOP_H

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include "CHOP_CPlusPlusBase.h"  // vendored TD SDK (Derivative Shared Use License, gitignored)

using namespace TD;

// Opaque Rust runtime handle (defined in the C ABI, core/sahou.h). Forward-declared here.
struct SahouRuntime;

class SahouInCHOP : public CHOP_CPlusPlusBase {
public:
    explicit SahouInCHOP(const OP_NodeInfo* info);
    ~SahouInCHOP() override;

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
    // Resolve the selected connection's keyexpr from the IR (via the core).
    std::string resolveKey();
    // Re-subscribe when the resolved key changes / Active toggles.
    void syncSubscription(bool active);
    // Run the receive boundary on a wire+attachment and refresh channels/status.
    void acceptWire(const std::string& wire, const std::string& attachment);
    // Populate schema-derived zero channels (numeric fields) before the first message.
    void seedSchemaChannels();

    SahouRuntime* myRuntime = nullptr;  // owned; freed in dtor / on reload
    std::string myLoadedPath;
    std::string myLoadError;
    bool myForceReload = false;
    bool myDoInject = false;

    std::string myNode;
    std::string myConn;
    std::string myWantKey;        // the key we want subscribed (retry target until it sticks)
    std::string mySubscribedKey;  // the key actually subscribed (empty = none / pending)
    uint64_t myPollGen = 0;       // last transport generation consumed

    int32_t myExecuteCount = 0;
    uint64_t myReceived = 0;  // accepted+rejected messages processed

    bool myHaveResult = false;  // has any message been processed?
    bool myOk = false;          // last verdict accept?
    std::string myKey;          // resolved keyexpr
    std::string myHash;         // schema_hash (attachment)
    std::string myDiag;         // first diag on reject / hash-mismatch note
    std::string myWarning;      // soft not-ready status
    std::string myLastPayload;  // last accepted payload JSON

    // Latest decoded output: parallel channel names + per-channel sample vectors.
    std::vector<std::string> myChanNames;
    std::vector<std::vector<float>> myChanData;

    // Expected schema rows [name,type,required,detail] + decoded payload rows [name,kind,value].
    std::vector<std::array<std::string, 4>> myFields;
    std::vector<std::array<std::string, 3>> myDecoded;
};

#endif  // SAHOU_TD_SAHOU_IN_CHOP_H
