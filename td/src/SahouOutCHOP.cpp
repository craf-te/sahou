#include "SahouOutCHOP.h"

#include <fstream>
#include <sstream>
#include <vector>

#include "envelope.h"
#include "payload.h"

extern "C" {
#include "sahou.h"  // core C ABI (validate / sample); SAHOU_CAPI is defined by the build
}
#include "sahou_transport.h"  // transport C ABI (zenoh send), from the bundled libsahou_transport

// ---------------------------------------------------------------------------
// Plugin registration — the C entry points TD's loader looks for.
// ---------------------------------------------------------------------------
extern "C" {

DLLEXPORT void FillCHOPPluginInfo(CHOP_PluginInfo* info) {
    if (!info->setAPIVersion(CHOPCPlusPlusAPIVersion)) {
        return;
    }
    // opType: capital first char, then lowercase/digits only.
    info->customOPInfo.opType->setString("Sahouout");
    info->customOPInfo.opLabel->setString("Sahou Out");
    info->customOPInfo.authorName->setString("Sahou");
    info->customOPInfo.authorEmail->setString("");
    info->customOPInfo.minInputs = 0;  // usable with no input (shows a "connect an input" hint)
    info->customOPInfo.maxInputs = 1;  // one input CHOP, whose channels become the message
}

DLLEXPORT CHOP_CPlusPlusBase* CreateCHOPInstance(const OP_NodeInfo* info) {
    return new SahouOutCHOP(info);
}

DLLEXPORT void DestroyCHOPInstance(CHOP_CPlusPlusBase* instance) {
    delete static_cast<SahouOutCHOP*>(instance);
}

}  // extern "C"

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------
namespace {

// Read a string parameter, guarding against a null return.
std::string par_str(const OP_Inputs* inputs, const char* name) {
    const char* v = inputs->getParString(name);
    return v ? std::string(v) : std::string();
}

// Read a whole file into a string ("" if it cannot be read).
std::string read_file(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) {
        return "";
    }
    std::stringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

// Add each name of a JSON string array (from the core) as a menu entry (name == label).
void add_menu_entries(OP_BuildDynamicMenuInfo* info, const char* json) {
    if (!json) {
        return;
    }
    for (const std::string& name : sahou::json_string_array(json)) {
        info->addMenuEntry(name.c_str(), name.c_str());
    }
}

// A one-line "what to wire" hint from the schema rows, e.g. "x(float), y(float), source(string)?".
std::string expected_channels(const std::vector<std::array<std::string, 4>>& fields) {
    std::string s;
    for (const std::array<std::string, 4>& f : fields) {
        if (!s.empty()) {
            s += ", ";
        }
        s += f[0] + "(" + f[1] + ")";
        if (f[2] == "no") {
            s += "?";  // optional
        }
    }
    return s;
}

}  // namespace

// ---------------------------------------------------------------------------
// SahouOutCHOP
// ---------------------------------------------------------------------------
SahouOutCHOP::SahouOutCHOP(const OP_NodeInfo*) {}

SahouOutCHOP::~SahouOutCHOP() {
    sahou_runtime_free(myRuntime);
    myRuntime = nullptr;
}

void SahouOutCHOP::getGeneralInfo(CHOP_GeneralInfo* ginfo, const OP_Inputs* inputs, void*) {
    // "Cook Every Frame" (like the OSC Out CHOP): On = run the boundary every frame even when the
    // input is static; Off (default) = only when the input changes or a cook is requested, so a
    // still input does not keep re-running (and, once send is wired, does not keep re-sending).
    ginfo->cookEveryFrame = inputs->getParInt("Cookeveryframe") != 0;
    ginfo->cookEveryFrameIfAsked = true;
    ginfo->timeslice = true;
    ginfo->inputMatchIndex = 0;
}

bool SahouOutCHOP::getOutputInfo(CHOP_OutputInfo* info, const OP_Inputs* inputs, void*) {
    // With an input, pass it through unchanged (return false => match the input's layout).
    if (inputs->getNumInputs() > 0) {
        return false;
    }
    // No input: nothing to send, output no channels.
    info->numChannels = 0;
    info->numSamples = 1;
    info->sampleRate = 60;
    return true;
}

void SahouOutCHOP::getChannelName(int32_t, OP_String* name, const OP_Inputs*, void*) {
    // Only reached when we define our own output (no input) — which has 0 channels.
    name->setString("");
}

void SahouOutCHOP::reloadIfNeeded(const OP_Inputs* inputs) {
    const std::string raw = par_str(inputs, "Irfile");

    // Empty path: no IR loaded (not an error — execute() shows a hint).
    if (raw.empty()) {
        if (myRuntime) {
            sahou_runtime_free(myRuntime);
            myRuntime = nullptr;
        }
        myLoadedPath.clear();
        myLoadError.clear();
        myForceReload = false;
        return;
    }

    // Resolve to an absolute path (relative paths resolve against the .toe location).
    const char* abs = inputs->getParFilePath("Irfile");
    const std::string path = abs ? std::string(abs) : raw;

    if (!myForceReload && myRuntime && path == myLoadedPath) {
        return;  // already loaded and unchanged
    }
    myForceReload = false;

    if (myRuntime) {
        sahou_runtime_free(myRuntime);
        myRuntime = nullptr;
    }

    const std::string json = read_file(path);
    if (json.empty()) {
        myLoadError = "cannot read IR file: " + path;
        myLoadedPath = path;
        return;
    }

    myRuntime = sahou_runtime_new(json.c_str());
    myLoadedPath = path;
    myLoadError = myRuntime ? "" : ("invalid descriptor (not a Sahou gen/descriptor.json?): " + path);
}

void SahouOutCHOP::execute(CHOP_Output* output, const OP_Inputs* inputs, void*) {
    myExecuteCount++;

    // Reset per-cook status.
    myValidated = false;
    myOk = false;
    myNode = par_str(inputs, "Node");
    myConn = par_str(inputs, "Connection");
    myKey.clear();
    myHash.clear();
    myLastPayload.clear();
    myDiag.clear();
    myWarning.clear();

    reloadIfNeeded(inputs);

    // Node/Connection are meaningless without a loaded IR: keep them disabled until it loads,
    // and keep Connection disabled until a Node is picked.
    const bool haveIr = (myRuntime != nullptr);
    inputs->enablePar("Node", haveIr);
    inputs->enablePar("Connection", haveIr && !myNode.empty());

    // Expected schema of the selected connection (drives the Info DAT + the pre-wire hint).
    myFields.clear();
    if (myRuntime && !myConn.empty()) {
        char* fj = sahou_connection_fields(myRuntime, myConn.c_str());
        const std::vector<std::string> flat = sahou::json_string_array(fj ? fj : "");
        sahou_free(fj);
        for (std::size_t i = 0; i + 3 < flat.size(); i += 4) {
            myFields.push_back({flat[i], flat[i + 1], flat[i + 2], flat[i + 3]});
        }
    }

    const OP_CHOPInput* cin = inputs->getNumInputs() > 0 ? inputs->getInputCHOP(0) : nullptr;

    // Pass the input through unchanged so the node is transparent in the chain.
    if (cin) {
        for (int32_t i = 0; i < output->numChannels && i < cin->numChannels; ++i) {
            for (int32_t j = 0; j < output->numSamples; ++j) {
                output->channels[i][j] = cin->getChannelData(i)[j];
            }
        }
    }

    // Test pulse: publish an IR-valid sample of the selected connection over zenoh (works with no
    // input — a quick connectivity check you can watch with `sahou tap`). Independent of the input.
    if (myDoTest) {
        myDoTest = false;
        if (!myRuntime) {
            myTestStatus = "load an IR File first";
        } else if (myNode.empty() || myConn.empty()) {
            myTestStatus = "set Node and Connection first";
        } else {
            sahou_transport_start(nullptr);  // idempotent; default peer (LAN multicast)
            char* smp = sahou_sample(myRuntime, myConn.c_str());
            const std::string sample = smp ? std::string(smp) : std::string("{}");
            sahou_free(smp);
            char* env = sahou_prepare_publish(myRuntime, myNode.c_str(), myConn.c_str(),
                                              sample.c_str(), mySeq++);
            const std::string envelope = env ? std::string(env) : std::string();
            sahou_free(env);
            if (sahou::envelope_ok(envelope)) {
                const std::string key = sahou::envelope_string(envelope, "key");
                const std::string att = sahou::envelope_string(envelope, "attachment");
                sahou_transport_publish(key.c_str(), sample.c_str(), att.c_str());
                char* st = sahou_transport_status();
                const std::string status = st ? std::string(st) : std::string();
                sahou_transport_free(st);
                myTestStatus = "sent " + key + " " + sample + "  " + status;
            } else {
                myTestStatus = "sample rejected: " + sahou::first_diag(envelope);
            }
        }
    }

    // Not-yet-ready conditions surface as warnings (getErrorString handles the hard load error).
    if (!myLoadError.empty()) {
        return;
    }
    if (!myRuntime) {
        myWarning = "set 'IR File' to a Sahou descriptor.json";
        return;
    }
    if (myNode.empty() || myConn.empty()) {
        myWarning = "set 'Node' and 'Connection'";
        return;
    }
    if (!cin || cin->numChannels == 0) {
        myWarning = myFields.empty()
                        ? "connect an input CHOP; its channels become the message"
                        : "expected channels: " + expected_channels(myFields);
        return;
    }

    // Build the payload from the latest sample of each input channel.
    std::vector<sahou::Channel> channels;
    channels.reserve(static_cast<std::size_t>(cin->numChannels));
    const int32_t last = cin->numSamples > 0 ? cin->numSamples - 1 : 0;
    for (int32_t i = 0; i < cin->numChannels; ++i) {
        const double value = cin->numSamples > 0 ? cin->getChannelData(i)[last] : 0.0;
        channels.push_back({cin->getChannelName(i), value});
    }
    myLastPayload = sahou::payload_json(channels);

    // The send boundary: the Rust core is the single source of "NO".
    char* env = sahou_prepare_publish(myRuntime, myNode.c_str(), myConn.c_str(),
                                      myLastPayload.c_str(), mySeq++);
    const std::string envelope = env ? std::string(env) : std::string();
    sahou_free(env);

    myValidated = true;
    myOk = sahou::envelope_ok(envelope);
    if (myOk) {
        // Validate-only: on OK we would publish over Zenoh here (next stage).
        myKey = sahou::envelope_string(envelope, "key");
        myHash = sahou::envelope_string(envelope, "attachment");
    } else {
        myDiag = sahou::first_diag(envelope);
    }
}

// --- self status: Info CHOP -------------------------------------------------
int32_t SahouOutCHOP::getNumInfoCHOPChans(void*) {
    return 3;
}

void SahouOutCHOP::getInfoCHOPChan(int32_t index, OP_InfoCHOPChan* chan, void*) {
    switch (index) {
        case 0:
            chan->name->setString("valid");
            chan->value = myValidated ? (myOk ? 1.0f : 0.0f) : -1.0f;  // -1 = not checked
            break;
        case 1:
            chan->name->setString("seq");
            chan->value = static_cast<float>(mySeq);
            break;
        case 2:
            chan->name->setString("execute_count");
            chan->value = static_cast<float>(myExecuteCount);
            break;
        default:
            break;
    }
}

// --- self status: Info DAT --------------------------------------------------
// Layout (4 columns): 6 status rows (key | value | | ), then, when a connection is selected,
// a header row and one row per expected field (field | type | required | detail).
bool SahouOutCHOP::getInfoDATSize(OP_InfoDATSize* infoSize, void*) {
    const int32_t schemaRows = myFields.empty() ? 0 : 1 + static_cast<int32_t>(myFields.size());
    infoSize->rows = 7 + schemaRows;  // 7 status rows, then (header + one row per field)
    infoSize->cols = 4;
    infoSize->byColumn = false;
    return true;
}

void SahouOutCHOP::getInfoDATEntries(int32_t index, int32_t, OP_InfoDATEntries* entries, void*) {
    auto set_row = [&](const char* c0, const std::string& c1, const char* c2, const char* c3) {
        entries->values[0]->setString(c0);
        entries->values[1]->setString(c1.c_str());
        entries->values[2]->setString(c2);
        entries->values[3]->setString(c3);
    };

    // Status section (rows 0..6).
    if (index < 7) {
        switch (index) {
            case 0:
                set_row("status",
                        !myLoadError.empty() ? "error"
                        : !myWarning.empty() ? "waiting"
                        : !myValidated       ? "idle"
                        : myOk               ? "ok"
                                             : "NO",
                        "", "");
                break;
            case 1: set_row("node", myNode, "", ""); break;
            case 2: set_row("connection", myConn, "", ""); break;
            case 3: set_row("keyexpr", myKey, "", ""); break;
            case 4: set_row("schema_hash", myHash, "", ""); break;
            case 5:
                set_row("detail",
                        !myLoadError.empty() ? myLoadError
                        : !myDiag.empty()    ? myDiag
                        : !myWarning.empty() ? myWarning
                                             : myLastPayload,
                        "", "");
                break;
            case 6: set_row("test", myTestStatus, "", ""); break;
            default: break;
        }
        return;
    }

    // Expected-schema section header (row 7).
    if (index == 7) {
        set_row("field", "type", "required", "detail");
        return;
    }

    // Expected-schema rows (row 8+).
    const std::size_t f = static_cast<std::size_t>(index - 8);
    if (f < myFields.size()) {
        for (int c = 0; c < 4; ++c) {
            entries->values[c]->setString(myFields[f][c].c_str());
        }
    }
}

// --- self status: node warning / error strings ------------------------------
void SahouOutCHOP::getWarningString(OP_String* warning, void*) {
    if (!myWarning.empty()) {
        warning->setString(myWarning.c_str());
    }
}

void SahouOutCHOP::getErrorString(OP_String* error, void*) {
    // A load failure or a boundary NO turns the node red (Sahou: "say NO in the right place").
    if (!myLoadError.empty()) {
        error->setString(myLoadError.c_str());
    } else if (myValidated && !myOk) {
        const std::string msg = myDiag.empty() ? "payload rejected by the contract" : myDiag;
        error->setString(msg.c_str());
    }
}

// --- parameters -------------------------------------------------------------
void SahouOutCHOP::setupParameters(OP_ParameterManager* manager, void*) {
    {
        OP_StringParameter sp;
        sp.name = "Irfile";
        sp.label = "IR File";
        manager->appendFile(sp);
    }
    // Node / Connection are dynamic menus populated from the loaded IR (see buildDynamicMenu).
    // They stay disabled until an IR is loaded (see execute's enablePar calls).
    {
        OP_StringParameter sp;
        sp.name = "Node";
        sp.label = "Node";
        manager->appendDynamicStringMenu(sp);
    }
    {
        OP_StringParameter sp;
        sp.name = "Connection";
        sp.label = "Connection";
        manager->appendDynamicStringMenu(sp);
    }
    // Cook cadence (mirrors the OSC Out CHOP's "Cook Every Frame"). Default On: TD is pull-based, so
    // a send/sink node with nothing pulling its output would otherwise MISS input changes. Redundant
    // work when idle is avoided at the send stage (Send Every Cook = on-change), not by not cooking.
    // Turn Off only when something downstream pulls this node every frame.
    {
        OP_NumericParameter np;
        np.name = "Cookeveryframe";
        np.label = "Cook Every Frame";
        np.defaultValues[0] = 1.0;
        manager->appendToggle(np);
    }

    // Test send: publish one IR-valid sample of the selected connection over zenoh (connectivity check).
    {
        OP_NumericParameter np;
        np.name = "Test";
        np.label = "Test Send";
        manager->appendPulse(np);
    }
    // Reload stays at the bottom of the parameter list.
    {
        OP_NumericParameter np;
        np.name = "Reload";
        np.label = "Reload";
        manager->appendPulse(np);
    }
}

void SahouOutCHOP::pulsePressed(const char* name, void*) {
    if (!name) {
        return;
    }
    const std::string n = name;
    if (n == "Reload") {
        myForceReload = true;
    } else if (n == "Test") {
        myDoTest = true;
    }
}

// Populate the Node / Connection dropdowns from the loaded IR.
void SahouOutCHOP::buildDynamicMenu(const OP_Inputs* inputs, OP_BuildDynamicMenuInfo* info, void*) {
    if (!info || !info->name) {
        return;
    }
    reloadIfNeeded(inputs);
    if (!myRuntime) {
        return;  // no IR -> empty menu
    }
    if (std::string(info->name) == "Node") {
        char* json = sahou_node_list(myRuntime);
        add_menu_entries(info, json);
        sahou_free(json);
    } else if (std::string(info->name) == "Connection") {
        const std::string node = par_str(inputs, "Node");
        if (!node.empty()) {
            char* json = sahou_connections_from(myRuntime, node.c_str());
            add_menu_entries(info, json);
            sahou_free(json);
        }
    }
}
