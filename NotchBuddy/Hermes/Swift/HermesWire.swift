import Foundation

// What the iPhone and Hermes running inside it say to each other (NotchBuddy/Hermes/app/axyon_hermes/host.py): the
// events of an answer, the brains on offer, and the conversation as the AXYON tab shows it. Foundation only, like
// PCLinkWire.swift: tests/HermesWireTests.swift runs it on macOS and Linux.

/// One event of an answer being written (`GET /v1/runs/<id>/events`, server-sent).
enum HermesEvent: Equatable, Sendable {
    case delta(String)
    /// A tool Hermes started: its name, and what it is about (a few words, maybe empty).
    case tool(name: String, preview: String)
    case toolDone(name: String, failed: Bool)
    /// The answer, whole; `stopped` when it was stopped from here.
    case done(text: String, stopped: Bool)
    case error(String)
    case other

    /// `data: {…}` lines only; nil for a keepalive, a blank line or anything that is not an event.
    static func parse(_ line: String) -> HermesEvent? {
        guard line.hasPrefix("data: "),
              let object = (try? JSONSerialization.jsonObject(with: Data(line.dropFirst(6).utf8))) as? [String: Any]
        else { return nil }
        let text = { (key: String) in object[key] as? String ?? "" }
        switch text("type") {
        case "delta": return .delta(text("text"))
        case "tool": return .tool(name: text("name"), preview: text("preview"))
        case "tool_done": return .toolDone(name: text("name"), failed: object["error"] as? Bool ?? false)
        case "done": return .done(text: text("text"), stopped: object["stopped"] as? Bool ?? false)
        case "error": return .error(text("message"))
        default: return .other
        }
    }
}

/// The brains Hermes can think with here. ChatGPT (the user's subscription) has no key: Hermes signs in to it itself.
/// The others' keys stay in this iPhone's Keychain and reach Hermes in memory only.
struct HermesBrain: Equatable, Sendable, Identifiable {
    let id: String
    let name: String
    /// The variable Hermes reads its key from; nil for ChatGPT.
    let keyName: String?
    /// The model when none is chosen: "", Hermes then picks the provider's own cost-safe default.
    let defaultModel: String
    /// Where the user gets a key.
    let keyPage: String?
    /// The address must be typed (an OpenAI-compatible server of the user's own).
    let needsAddress: Bool

    static let all: [HermesBrain] = [
        HermesBrain(id: "openai-codex", name: "ChatGPT", keyName: nil, defaultModel: "", keyPage: nil, needsAddress: false),
        HermesBrain(id: "gemini", name: "Google Gemini", keyName: "GEMINI_API_KEY", defaultModel: "",
                    keyPage: "https://aistudio.google.com/apikey", needsAddress: false),
        HermesBrain(id: "openrouter", name: "OpenRouter", keyName: "OPENROUTER_API_KEY", defaultModel: "",
                    keyPage: "https://openrouter.ai/keys", needsAddress: false),
        HermesBrain(id: "anthropic", name: "Anthropic", keyName: "ANTHROPIC_API_KEY", defaultModel: "",
                    keyPage: "https://platform.claude.com/settings/keys", needsAddress: false),
        HermesBrain(id: "custom", name: "OpenAI-compatible", keyName: "OPENAI_API_KEY", defaultModel: "",
                    keyPage: nil, needsAddress: true),
    ]

    static func named(_ id: String) -> HermesBrain? { all.first { $0.id == id } }
    static var keyNames: [String] { all.compactMap(\.keyName) }
}

/// What `GET /v1/settings` says of the brain now.
struct HermesSettings: Equatable, Sendable {
    var provider: String
    var model: String
    var baseURL: String
    /// Whether each of this brain's keys reached Hermes.
    var keysSet: [String: Bool]
    /// nil when the brain is not ChatGPT.
    var chatGPTSignedIn: Bool?

    init(provider: String, model: String = "", baseURL: String = "", keysSet: [String: Bool] = [:],
         chatGPTSignedIn: Bool? = nil) {
        self.provider = provider
        self.model = model
        self.baseURL = baseURL
        self.keysSet = keysSet
        self.chatGPTSignedIn = chatGPTSignedIn
    }

    init?(json object: [String: Any]) {
        guard let provider = object["provider"] as? String else { return nil }
        self.provider = provider
        model = object["model"] as? String ?? ""
        baseURL = object["baseUrl"] as? String ?? ""
        keysSet = object["keysSet"] as? [String: Bool] ?? [:]
        chatGPTSignedIn = object["chatgptSignedIn"] as? Bool
    }

    /// Hermes can think: signed in to ChatGPT, or the brain's key reached it.
    var canThink: Bool {
        if provider == "openai-codex" { return chatGPTSignedIn == true }
        return !keysSet.isEmpty && keysSet.values.allSatisfy { $0 }
    }
}

/// The ChatGPT sign-in, as `hermes auth add openai-codex` runs it: a code to type on OpenAI's page.
struct HermesSignIn: Equatable, Sendable {
    enum State: String, Sendable { case idle, waiting, done, failed }
    var state: State
    var code = ""
    var url = ""
    var message = ""

    init(state: State, code: String = "", url: String = "", message: String = "") {
        self.state = state
        self.code = code
        self.url = url
        self.message = message
    }

    init(json object: [String: Any]) {
        state = State(rawValue: object["state"] as? String ?? "") ?? .idle
        code = object["code"] as? String ?? ""
        url = object["url"] as? String ?? ""
        message = object["message"] as? String ?? ""
    }
}

/// A number of a JSON object as JSONSerialization gives it, on Apple's platforms and on Linux alike.
enum HermesJSON {
    static func int(_ value: Any?) -> Int? {
        if let n = value as? Int { return n }
        if let d = value as? Double, d.isFinite { return Int(d.rounded()) }
        return nil
    }
}

/// One device the iPhone drives itself (`GET /v1/home`, axyon_hermes/home.py): a TV, an Android box, or an entity
/// of the user's Home Assistant.
struct HermesDevice: Equatable, Sendable, Identifiable {
    var id: String
    var name: String
    var room: String
    /// tv, light, switch, fan, cover, scene, routine, climate.
    var kind: String
    /// "Samsung TV", "Android", "Home Assistant".
    var source: String
    /// The actions it takes (`turn_on`, `volume_up`…): nothing else is offered for it.
    var can: [String]
    var apps: [String]
    var paired: Bool
    var asksFirst: Bool
    /// nil: it does not say (or did not answer quickly).
    var online: Bool?
    var on: Bool?
    var volume: Int?
    var brightness: Int?

    init(id: String, name: String, room: String = "", kind: String = "tv", source: String = "", can: [String] = [],
         apps: [String] = [], paired: Bool = true, asksFirst: Bool = false, online: Bool? = nil, on: Bool? = nil,
         volume: Int? = nil, brightness: Int? = nil) {
        self.id = id
        self.name = name
        self.room = room
        self.kind = kind
        self.source = source
        self.can = can
        self.apps = apps
        self.paired = paired
        self.asksFirst = asksFirst
        self.online = online
        self.on = on
        self.volume = volume
        self.brightness = brightness
    }

    init?(json o: [String: Any]) {
        guard let id = o["id"] as? String, !id.isEmpty else { return nil }
        let state = o["state"] as? [String: Any] ?? [:]
        self.init(id: id, name: o["name"] as? String ?? id, room: o["room"] as? String ?? "",
                  kind: o["kind"] as? String ?? "", source: o["source"] as? String ?? "",
                  can: (o["can"] as? [Any] ?? []).compactMap { $0 as? String },
                  apps: (o["apps"] as? [Any] ?? []).compactMap { $0 as? String },
                  paired: o["paired"] as? Bool ?? true, asksFirst: o["asksFirst"] as? Bool ?? false,
                  online: state["online"] as? Bool, on: state["on"] as? Bool,
                  volume: HermesJSON.int(state["volume"]), brightness: HermesJSON.int(state["brightness"]))
    }

    /// A Home Assistant entity (its actions go through the user's Home Assistant).
    var fromAssistant: Bool { id.hasPrefix("ha:") }
    func can(_ action: String) -> Bool { can.contains(action) }
}

/// The iPhone's own home as the host reports it: its devices and, when set, the user's Home Assistant.
struct HermesHouse: Equatable, Sendable {
    var devices: [HermesDevice] = []
    var assistantURL: String?
    var assistantConnected = false
    var assistantProblem: String?

    init(devices: [HermesDevice] = [], assistantURL: String? = nil, assistantConnected: Bool = false,
         assistantProblem: String? = nil) {
        self.devices = devices
        self.assistantURL = assistantURL
        self.assistantConnected = assistantConnected
        self.assistantProblem = assistantProblem
    }

    init(json o: [String: Any]) {
        devices = (o["devices"] as? [Any] ?? []).compactMap { ($0 as? [String: Any]).flatMap(HermesDevice.init(json:)) }
        if let assistant = o["homeAssistant"] as? [String: Any] {
            assistantURL = assistant["url"] as? String
            assistantConnected = assistant["connected"] as? Bool ?? false
            assistantProblem = assistant["problem"] as? String
        }
    }

    /// The TVs and boxes the iPhone reaches itself, then the Home Assistant entities.
    var own: [HermesDevice] { devices.filter { !$0.fromAssistant } }
    var assistant: [HermesDevice] { devices.filter(\.fromAssistant) }
}

/// What a scan of the iPhone's Wi-Fi found (`POST /v1/home/scan`).
struct HermesFound: Equatable, Sendable, Identifiable {
    /// samsung, android or homeassistant.
    var driver: String
    var host: String
    var port: Int?
    var name: String
    var model: String
    var url: String
    var added: Bool
    var id: String { "\(driver):\(host):\(port ?? 0)" }

    init?(json o: [String: Any]) {
        guard let driver = o["driver"] as? String, let host = o["host"] as? String else { return nil }
        self.driver = driver
        self.host = host
        port = HermesJSON.int(o["port"])
        name = o["name"] as? String ?? host
        model = o["model"] as? String ?? ""
        url = o["url"] as? String ?? ""
        added = o["added"] as? Bool ?? false
    }

    static func list(_ o: [String: Any]) -> [HermesFound] {
        (o["found"] as? [Any] ?? []).compactMap { ($0 as? [String: Any]).flatMap(HermesFound.init(json:)) }
    }
}

/// The conversation with Hermes on this iPhone as the AXYON tab shows it, in the chat's own entries (PCChatEntry, so
/// the bubbles are the computer chat's), and the Hermes session it belongs to.
struct HermesConversation: Equatable, Sendable, Codable {
    static let currentVersion = 1
    static let kept = 100

    var version = HermesConversation.currentVersion
    /// Hermes' session for this conversation: it keeps the history and remembers across launches.
    var session: String
    var entries: [PCChatEntry] = []

    static func newSession() -> String { "axyon-iphone-" + UUID().uuidString.lowercased().prefix(12) }

    static func fresh() -> HermesConversation { HermesConversation(session: newSession()) }

    /// Your message and the answer to come, which events fill in.
    mutating func ask(_ text: String, now id: (String) -> String = { $0 + "-" + UUID().uuidString }) -> String {
        let answer = id("hermes")
        entries.append(PCChatEntry(id: id("you"), role: .you, text: text, finished: true))
        entries.append(PCChatEntry(id: answer, role: .mochi, text: ""))
        if entries.count > Self.kept { entries.removeFirst(entries.count - Self.kept) }
        return answer
    }

    /// One event of the answer `id`. `label` words a tool for the steps.
    mutating func apply(_ event: HermesEvent, to id: String, label: (String) -> String) {
        guard let index = entries.firstIndex(where: { $0.id == id }) else { return }
        var entry = entries[index]
        switch event {
        case .delta(let text):
            entry.text += text
        case .tool(let name, _):
            entry.steps.append(PCChatStep(index: entry.steps.count, status: "running", text: label(name)))
        case .toolDone(let name, let failed):
            let wanted = label(name)
            if let step = entry.steps.lastIndex(where: { $0.status == "running" && $0.text == wanted }) {
                entry.steps[step].status = failed ? "error" : "ok"
            }
        case .done(let text, _):
            if !text.isEmpty { entry.text = text }
            entry.finished = true
            entry.steps = entry.steps.map { var s = $0; if s.status == "running" { s.status = "ok" }; return s }
        case .error(let message):
            entry.text = message.isEmpty ? entry.text : message
            entry.failed = true
            entry.finished = true
        case .other:
            return
        }
        entries[index] = entry
    }

    /// The answer stopped without its end (the app left, the host stopped): it ends there, with `reason` when it has
    /// no words yet.
    mutating func interrupt(_ id: String, reason: String) {
        guard let index = entries.firstIndex(where: { $0.id == id }), !entries[index].finished else { return }
        if entries[index].text.isEmpty { entries[index].text = reason }
        entries[index].failed = entries[index].text == reason
        entries[index].finished = true
        entries[index].steps = entries[index].steps.map { var s = $0; if s.status == "running" { s.status = "error" }; return s }
    }

    /// Back after a launch: an answer that was being written ends there.
    mutating func restored(reason: String) {
        for entry in entries where entry.role == .mochi && !entry.finished {
            interrupt(entry.id, reason: reason)
        }
    }
}
