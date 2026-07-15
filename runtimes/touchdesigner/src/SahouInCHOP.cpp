#include "SahouInCHOP.h"

#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <vector>

#include "envelope.h"
#include "outcome.h"

extern "C" {
#include "sahou.h"  // core C ABI (accept / decode / selectors); SAHOU_CAPI is defined by the build
}
#include "sahou_transport.h"  // transport C ABI (zenoh subscribe/poll), from libsahou_transport

// ---------------------------------------------------------------------------
// Plugin registration — the C entry points TD's loader looks for.
// ---------------------------------------------------------------------------
extern "C" {

DLLEXPORT void FillCHOPPluginInfo(CHOP_PluginInfo* info) {
    if (!info->setAPIVersion(CHOPCPlusPlusAPIVersion)) {
        return;
    }
    // opType: capital first char, then lowercase/digits only.
    info->customOPInfo.opType->setString("Sahouin");
    info->customOPInfo.opLabel->setString("Sahou In");
    info->customOPInfo.authorName->setString("Sahou");
    info->customOPInfo.authorEmail->setString("");
    info->customOPInfo.minInputs = 0;  // a source: receives from the network, no CHOP input
    info->customOPInfo.maxInputs = 0;
}

DLLEXPORT CHOP_CPlusPlusBase* CreateCHOPInstance(const OP_NodeInfo* info) {
    return new SahouInCHOP(info);
}

DLLEXPORT void DestroyCHOPInstance(CHOP_CPlusPlusBase* instance) {
    delete static_cast<SahouInCHOP*>(instance);
}

}  // extern "C"

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------
namespace {

std::string par_str(const OP_Inputs* inputs, const char* name) {
    const char* v = inputs->getParString(name);
    return v ? std::string(v) : std::string();
}

std::string read_file(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    if (!f) {
        return "";
    }
    std::stringstream ss;
    ss << f.rdbuf();
    return ss.str();
}

void add_menu_entries(OP_BuildDynamicMenuInfo* info, const char* json) {
    if (!json) {
        return;
    }
    for (const std::string& name : sahou::json_string_array(json)) {
        info->addMenuEntry(name.c_str(), name.c_str());
    }
}

// A one-line "what to expect" hint from the schema rows, e.g. "x(float), y(float), source(string)?".
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

bool is_numeric_type(const std::string& ty) {
    return ty == "float" || ty == "int" || ty == "bool";
}

}  // namespace

// ---------------------------------------------------------------------------
// SahouInCHOP
// ---------------------------------------------------------------------------
SahouInCHOP::SahouInCHOP(const OP_NodeInfo*) {}

SahouInCHOP::~SahouInCHOP() {
    if (!mySubscribedKey.empty()) {
        sahou_transport_unsubscribe(mySubscribedKey.c_str());
    }
    sahou_runtime_free(myRuntime);
    myRuntime = nullptr;
}

void SahouInCHOP::getGeneralInfo(CHOP_GeneralInfo* ginfo, const OP_Inputs*, void*) {
    // A source whose samples arrive asynchronously: cook every frame so the output refreshes even
    // when nothing downstream pulls it. `Active` is the user-facing "should it update" control.
    ginfo->cookEveryFrame = true;
    ginfo->cookEveryFrameIfAsked = true;
    ginfo->timeslice = false;
    ginfo->inputMatchIndex = 0;
}

bool SahouInCHOP::getOutputInfo(CHOP_OutputInfo* info, const OP_Inputs*, void*) {
    info->numChannels = static_cast<int32_t>(myChanNames.size());
    int32_t maxSamples = 1;
    for (const std::vector<float>& d : myChanData) {
        maxSamples = std::max<int32_t>(maxSamples, static_cast<int32_t>(d.size()));
    }
    info->numSamples = maxSamples;
    info->sampleRate = 60;
    return true;
}

void SahouInCHOP::getChannelName(int32_t index, OP_String* name, const OP_Inputs*, void*) {
    if (index >= 0 && index < static_cast<int32_t>(myChanNames.size())) {
        name->setString(myChanNames[index].c_str());
    } else {
        name->setString("");
    }
}

void SahouInCHOP::reloadIfNeeded(const OP_Inputs* inputs) {
    const std::string raw = par_str(inputs, "Irfile");

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
    myLoadError =
        myRuntime ? "" : ("invalid descriptor (not a Sahou gen/descriptor.json?): " + path);
}

std::string SahouInCHOP::resolveKey() {
    if (!myRuntime || myConn.empty()) {
        return "";
    }
    char* k = sahou_connection_key(myRuntime, myConn.c_str());
    std::string key = k ? k : "";
    sahou_free(k);
    return key;
}

void SahouInCHOP::syncSubscription(bool active) {
    const std::string want = active ? myKey : std::string();
    if (want == mySubscribedKey) {
        return;
    }
    if (!mySubscribedKey.empty()) {
        sahou_transport_unsubscribe(mySubscribedKey.c_str());
    }
    mySubscribedKey = want;
    myPollGen = 0;
    if (!mySubscribedKey.empty()) {
        sahou_transport_start(nullptr);  // idempotent; default peer (LAN multicast)
        sahou_transport_subscribe(mySubscribedKey.c_str());
    }
}

void SahouInCHOP::acceptWire(const std::string& wire, const std::string& attachment) {
    myReceived++;
    char* out = sahou_accept_sample(
        myRuntime, myNode.c_str(), myConn.c_str(), reinterpret_cast<const uint8_t*>(wire.data()),
        wire.size(), attachment.empty() ? nullptr : attachment.c_str(), myReceived, nullptr);
    const std::string outcome = out ? out : "";
    sahou_free(out);

    myHaveResult = true;
    myHash = attachment;
    const sahou::Accepted r = sahou::accept_result(outcome);
    if (r == sahou::Accepted::Accept) {
        myOk = true;
        myDiag.clear();
        myLastPayload = wire;  // the received wire IS the canonical payload JSON

        // Decode -> channels (name,count,v...).
        char* dj = sahou_decode_channels(myRuntime, myConn.c_str(), wire.c_str());
        std::vector<std::string> flat = sahou::json_string_array(dj ? dj : "");
        sahou_free(dj);
        myChanNames.clear();
        myChanData.clear();
        for (std::size_t i = 0; i + 1 < flat.size();) {
            const std::string name = flat[i];
            const int count = std::atoi(flat[i + 1].c_str());
            std::vector<float> samples;
            for (int k = 0; k < count && (i + 2 + static_cast<std::size_t>(k)) < flat.size(); ++k) {
                samples.push_back(static_cast<float>(std::atof(flat[i + 2 + k].c_str())));
            }
            myChanNames.push_back(name);
            myChanData.push_back(samples);
            i += 2 + (count > 0 ? static_cast<std::size_t>(count) : 0);
        }

        // Decode -> DAT rows (name,kind,value).
        char* fj = sahou_decode_fields(myRuntime, myConn.c_str(), wire.c_str());
        std::vector<std::string> ff = sahou::json_string_array(fj ? fj : "");
        sahou_free(fj);
        myDecoded.clear();
        for (std::size_t i = 0; i + 2 < ff.size(); i += 3) {
            myDecoded.push_back({ff[i], ff[i + 1], ff[i + 2]});
        }
    } else if (r == sahou::Accepted::HashMismatch) {
        myOk = false;
        myDiag = "hash_mismatch: contract version mismatch (sender_hash=" +
                 sahou::accept_sender_hash(outcome) + "; handshake is a later stage)";
    } else {  // Reject / None
        myOk = false;
        myDiag = sahou::first_diag(outcome);
    }
}

void SahouInCHOP::seedSchemaChannels() {
    // Before the first message, output the connection's numeric channels at 0 so downstream sees
    // stable channel names immediately (design §3).
    myChanNames.clear();
    myChanData.clear();
    for (const std::array<std::string, 4>& f : myFields) {
        if (is_numeric_type(f[1])) {
            myChanNames.push_back(f[0]);
            myChanData.push_back({0.0f});
        }
    }
}

void SahouInCHOP::execute(CHOP_Output* output, const OP_Inputs* inputs, void*) {
    myExecuteCount++;
    myNode = par_str(inputs, "Node");
    myConn = par_str(inputs, "Connection");
    myWarning.clear();

    reloadIfNeeded(inputs);

    const bool haveIr = (myRuntime != nullptr);
    inputs->enablePar("Node", haveIr);
    inputs->enablePar("Connection", haveIr && !myNode.empty());

    // Expected schema of the selected connection (drives the Info DAT + the pre-ready hint).
    myFields.clear();
    if (myRuntime && !myConn.empty()) {
        char* fj = sahou_connection_fields(myRuntime, myConn.c_str());
        const std::vector<std::string> flat = sahou::json_string_array(fj ? fj : "");
        sahou_free(fj);
        for (std::size_t i = 0; i + 3 < flat.size(); i += 4) {
            myFields.push_back({flat[i], flat[i + 1], flat[i + 2], flat[i + 3]});
        }
    }

    // Resolve keyexpr + (de)subscribe based on Active.
    const bool active = inputs->getParInt("Active") != 0;
    myKey = resolveKey();
    syncSubscription(active && myRuntime && !myNode.empty() && !myConn.empty());

    // Inject Sample: feed an IR-valid sample locally, no network (test downstream with no publisher).
    if (myDoInject) {
        myDoInject = false;
        if (myRuntime && !myNode.empty() && !myConn.empty()) {
            char* smp = sahou_sample(myRuntime, myConn.c_str());
            const std::string sample = smp ? smp : "{}";
            sahou_free(smp);
            // The per-connection hash rides on a send-boundary envelope's "attachment".
            char* env = sahou_prepare_publish(myRuntime, myNode.c_str(), myConn.c_str(),
                                              sample.c_str(), 0);
            const std::string att = sahou::envelope_string(env ? env : "", "attachment");
            sahou_free(env);
            acceptWire(sample, att);
        } else {
            myWarning = "set 'Node' and 'Connection' first";
        }
    }

    // Poll the transport for a newer received sample (Active only).
    if (active && !mySubscribedKey.empty()) {
        char* pj = sahou_transport_poll(mySubscribedKey.c_str(), myPollGen);
        const std::string poll = pj ? pj : "{}";
        sahou_transport_free(pj);
        const std::size_t gp = poll.find("\"generation\":");
        if (gp != std::string::npos) {
            myPollGen = std::strtoull(poll.c_str() + gp + 13, nullptr, 10);
            const std::string wire = sahou::json_string_field(poll, "wire");
            const std::string att = sahou::envelope_string(poll, "attachment");
            acceptWire(wire, att);
        }
    }

    // Not-yet-ready conditions surface as warnings (getErrorString handles the hard load error).
    if (myLoadError.empty()) {
        if (!myRuntime) {
            myWarning = "set 'IR File' to a Sahou descriptor.json";
        } else if (myNode.empty() || myConn.empty()) {
            myWarning = "set 'Node' and 'Connection'";
        } else if (!myHaveResult) {
            myWarning = myFields.empty() ? "waiting for first message"
                                         : "waiting for first message; expected: " +
                                               expected_channels(myFields);
        }
    }

    // Before any message, seed schema-derived zero channels so downstream sees stable names.
    if (!myHaveResult && myChanNames.empty()) {
        seedSchemaChannels();
    }

    // Emit the latest decoded channels.
    for (int32_t i = 0; i < output->numChannels && i < static_cast<int32_t>(myChanData.size());
         ++i) {
        const std::vector<float>& d = myChanData[i];
        for (int32_t j = 0; j < output->numSamples; ++j) {
            output->channels[i][j] =
                d.empty() ? 0.0f : d[std::min<int32_t>(j, static_cast<int32_t>(d.size()) - 1)];
        }
    }
}

// --- self status: Info CHOP -------------------------------------------------
int32_t SahouInCHOP::getNumInfoCHOPChans(void*) {
    return 3;
}

void SahouInCHOP::getInfoCHOPChan(int32_t index, OP_InfoCHOPChan* chan, void*) {
    switch (index) {
        case 0:
            chan->name->setString("valid");
            chan->value = myHaveResult ? (myOk ? 1.0f : 0.0f) : -1.0f;  // -1 = none yet
            break;
        case 1:
            chan->name->setString("seq");
            chan->value = static_cast<float>(myReceived);
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
// Layout (4 cols): 7 status rows, then (schema header + one row per expected field), then
// (decoded header + one row per decoded field of the last accepted message).
bool SahouInCHOP::getInfoDATSize(OP_InfoDATSize* infoSize, void*) {
    const int32_t schemaRows = myFields.empty() ? 0 : 1 + static_cast<int32_t>(myFields.size());
    const int32_t decodedRows = myDecoded.empty() ? 0 : 1 + static_cast<int32_t>(myDecoded.size());
    infoSize->rows = 7 + schemaRows + decodedRows;
    infoSize->cols = 4;
    infoSize->byColumn = false;
    return true;
}

void SahouInCHOP::getInfoDATEntries(int32_t index, int32_t, OP_InfoDATEntries* entries, void*) {
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
                        !myLoadError.empty()  ? "error"
                        : !myWarning.empty()  ? "waiting"
                        : !myHaveResult       ? "idle"
                        : myOk                ? "ok"
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
            case 6: set_row("received", std::to_string(myReceived), "", ""); break;
            default: break;
        }
        return;
    }

    const int32_t schemaRows = myFields.empty() ? 0 : 1 + static_cast<int32_t>(myFields.size());

    // Expected-schema section (header at row 7, then one row per field).
    if (!myFields.empty() && index < 7 + schemaRows) {
        if (index == 7) {
            set_row("field", "type", "required", "detail");
            return;
        }
        const std::size_t f = static_cast<std::size_t>(index - 8);
        if (f < myFields.size()) {
            for (int c = 0; c < 4; ++c) {
                entries->values[c]->setString(myFields[f][c].c_str());
            }
        }
        return;
    }

    // Decoded-payload section (header, then one row per decoded field: field | value).
    const int32_t base = 7 + schemaRows;
    if (index == base) {
        set_row("field", "value", "", "");
        return;
    }
    const std::size_t d = static_cast<std::size_t>(index - base - 1);
    if (d < myDecoded.size()) {
        // rows are [name, kind, value] -> show name | value (kind in col 2).
        set_row(myDecoded[d][0].c_str(), myDecoded[d][2], myDecoded[d][1].c_str(), "");
    }
}

// --- self status: node warning / error strings ------------------------------
void SahouInCHOP::getWarningString(OP_String* warning, void*) {
    if (!myWarning.empty()) {
        warning->setString(myWarning.c_str());
    }
}

void SahouInCHOP::getErrorString(OP_String* error, void*) {
    // A load failure or a rejected/mismatched last message turns the node red.
    if (!myLoadError.empty()) {
        error->setString(myLoadError.c_str());
    } else if (myHaveResult && !myOk) {
        const std::string msg = myDiag.empty() ? "message rejected by the contract" : myDiag;
        error->setString(msg.c_str());
    }
}

// --- parameters -------------------------------------------------------------
void SahouInCHOP::setupParameters(OP_ParameterManager* manager, void*) {
    {
        OP_StringParameter sp;
        sp.name = "Irfile";
        sp.label = "IR File";
        manager->appendFile(sp);
    }
    // Node / Connection are dynamic menus populated from the loaded IR (see buildDynamicMenu).
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
    // Active: On = subscribe and refresh on new messages; Off = hold the last output.
    {
        OP_NumericParameter np;
        np.name = "Active";
        np.label = "Active";
        np.defaultValues[0] = 1.0;
        manager->appendToggle(np);
    }
    // Inject Sample: feed one IR-valid sample locally (no network), to test downstream wiring.
    {
        OP_NumericParameter np;
        np.name = "Inject";
        np.label = "Inject Sample";
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

void SahouInCHOP::pulsePressed(const char* name, void*) {
    if (!name) {
        return;
    }
    const std::string n = name;
    if (n == "Reload") {
        myForceReload = true;
    } else if (n == "Inject") {
        myDoInject = true;
    }
}

// Populate the Node / Connection dropdowns from the loaded IR (receiver side).
void SahouInCHOP::buildDynamicMenu(const OP_Inputs* inputs, OP_BuildDynamicMenuInfo* info, void*) {
    if (!info || !info->name) {
        return;
    }
    reloadIfNeeded(inputs);
    if (!myRuntime) {
        return;  // no IR -> empty menu
    }
    if (std::string(info->name) == "Node") {
        char* json = sahou_subscribing_nodes(myRuntime);
        add_menu_entries(info, json);
        sahou_free(json);
    } else if (std::string(info->name) == "Connection") {
        const std::string node = par_str(inputs, "Node");
        if (!node.empty()) {
            char* json = sahou_connections_to(myRuntime, node.c_str());
            add_menu_entries(info, json);
            sahou_free(json);
        }
    }
}
