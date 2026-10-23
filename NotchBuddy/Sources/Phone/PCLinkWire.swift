import Foundation

// The iPhone's half of the phone link to Coucou on a Windows or Linux computer (docs/ANDROID_LINK.md, protocol v1).
// iCloud only joins the iPhone to a Mac; a PC is reached directly: TLS on the local network, the certificate pinned
// by the pairing QR code, one JSON object per line. Away from home, the same TLS goes to the addresses the computer
// gave over the link (`remote`: its router opened a port for it), with no server of anyone in between. This file is Foundation only: the pairing link, the lines both
// ways, and what they do to the screens' state, so tests/PCLinkWireTests.swift runs anywhere
// (scripts/test-pc-link.sh). PCLink.swift adds the connection, the Keychain and the notifications.
//
// Besides the sessions, the computer may share, each behind a switch of its own there: the assistant (Hermes Agent: its
// name, tone, brain, skills and the ChatGPT sign-in), Mochi's home, Mochi's outfit, the plan usage, the files an agent
// changed and the service cards. Whatever it sends is checked again here (lengths, counts, known values): a bad item
// is dropped, the rest is kept.

enum PCWire {
    static let version = 1
    /// What this iPhone understands. The computer offers each one only while its own switch is on.
    static let capabilities = ["chat", "agent", "details", "answers", "remote", "prefs", "usage", "diffs", "services",
                               "assistant", "home"]
    static let maxLineBytes = 64 * 1024
    /// An approval is offered for 2 minutes at most (the computer gives up at 115 s).
    static let approvalLifetime: TimeInterval = 120
    /// The computer refuses a longer chat message.
    static let maxChatText = 4000
    /// Keep-alive, as the other phones do.
    static let pingInterval: TimeInterval = 20
    /// The id of an assistant or home request: made by this iPhone, echoed back by the computer.
    static let maxRequestID = 64
    /// What Mochi may wear (`prefs`): the values the Mac and the PC store, nothing else.
    static let outfits = ["auto", "none", "partyHat", "beanie", "crown", "sunglasses", "roundGlasses", "bow", "scarf",
                          "witchHat", "pumpkin", "santaHat", "bunnyEars"]
}

// MARK: - Pairing

/// The computer this iPhone is paired with, without its secret: the token lives in the Keychain.
struct PCComputer: Codable, Equatable, Sendable {
    var host: String
    var port: UInt16
    /// Lowercase hex SHA-256 of the computer's TLS certificate: the one certificate this iPhone accepts.
    var fingerprint: String
    var name: String
    /// Where the computer can be reached from the internet, most direct first ("[2804:…]:47821",
    /// "198.51.100.7:47821"): what it said last over the link. Tried when the local network does not answer.
    var remote: [String] = []
    /// Optional encrypted WebSocket relay pairing, kept separately from direct TLS endpoints.
    var relay: PCRelayConfig? = nil

    private enum CodingKeys: String, CodingKey {
        case host, port, fingerprint, name, remote, relay
    }

    /// What the computer announces on the network (TXT `fp`): a hint to find it again, never a proof.
    var shortFingerprint: String { String(fingerprint.prefix(16)) }

    /// The name to show: the computer's own, or a plain word.
    var displayName: String { name.isEmpty ? "your computer" : name }
}

extension PCComputer {
    /// Written in an extension so the memberwise initializer stays. A pairing saved before `remote` existed loads
    /// with none.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        host = try c.decode(String.self, forKey: .host)
        port = try c.decode(UInt16.self, forKey: .port)
        fingerprint = try c.decode(String.self, forKey: .fingerprint)
        name = try c.decode(String.self, forKey: .name)
        remote = PCRemote.clean((try? c.decodeIfPresent([String].self, forKey: .remote)) ?? [])
        relay = try? c.decodeIfPresent(PCRelayConfig.self, forKey: .relay)
    }
}

/// Relay credentials are not a direct TCP endpoint. A relay connection requires the
/// authenticated WebSocket + end-to-end encrypted framing in docs/RELAY_LINK.md.
struct PCRelayConfig: Codable, Equatable, Sendable {
    let url: String
    let room: String
    let key: String
    let access: String

    static func parse(url raw: String, room: String, key: String, access: String) -> PCRelayConfig? {
        guard let components = URLComponents(string: raw),
              components.scheme?.lowercased() == "wss",
              let host = components.host, !host.isEmpty,
              components.user == nil, components.password == nil,
              components.path.isEmpty || components.path == "/",
              components.query == nil, components.fragment == nil,
              components.port == nil,
              raw.count <= 253,
              validBase64URL(room, count: 22),
              validBase64URL(key, count: 43),
              validBase64URL(access, count: 43) else { return nil }
        return PCRelayConfig(url: raw, room: room, key: key, access: access)
    }

    private static func validBase64URL(_ text: String, count: Int) -> Bool {
        text.count == count && text.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }
    }
}

/// The computer's addresses on the internet (`remote`). Addresses only, never a name: nothing is looked up, so no
/// one's DNS learns where the iPhone goes.
enum PCRemote {
    static let maxEndpoints = 6

    /// "198.51.100.7:47821" or "[2804:14c::10]:47821": an IP address and a port. Nil for anything else.
    static func parse(_ text: String) -> (host: String, port: UInt16)? {
        let text = text.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty, text.count <= 64, text.allSatisfy(\.isASCII) else { return nil }
        let host: Substring
        let portText: Substring
        if text.hasPrefix("[") {
            guard let close = text.firstIndex(of: "]") else { return nil }
            host = text[text.index(after: text.startIndex)..<close]
            let rest = text[text.index(after: close)...]
            guard rest.hasPrefix(":"), host.contains(":"),
                  host.allSatisfy({ $0.isHexDigit || $0 == ":" || $0 == "." }) else { return nil }
            portText = rest.dropFirst()
        } else {
            guard let colon = text.firstIndex(of: ":"), text.lastIndex(of: ":") == colon else { return nil }
            host = text[..<colon]
            portText = text[text.index(after: colon)...]
            let parts = host.split(separator: ".", omittingEmptySubsequences: false)
            guard parts.count == 4, parts.allSatisfy({ part in
                (1...3).contains(part.count) && part.allSatisfy(\.isNumber) && (Int(part) ?? 256) <= 255
            }) else { return nil }
        }
        guard portText.allSatisfy(\.isNumber), let port = UInt16(portText), port > 0 else { return nil }
        return (String(host), port)
    }

    /// The well-formed ones, each once, in the computer's order, at most six.
    static func clean(_ list: [String]) -> [String] {
        var kept: [String] = []
        for item in list where parse(item) != nil && !kept.contains(item) && kept.count < maxEndpoints {
            kept.append(item)
        }
        return kept
    }
}

/// What the QR code in Settings → Phone on the computer holds:
/// `coucou://pair?v=1&host=…&port=…&fp=…&token=…&name=…` with optional
/// `relay`, `room`, `key`, `access` credentials for the encrypted relay protocol.
struct PCPairing: Equatable, Sendable {
    var computer: PCComputer
    var token: String

    /// Nil for anything that is not a well-formed v1 pairing link.
    static func parse(_ text: String) -> PCPairing? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: trimmed),
              components.scheme?.lowercased() == "coucou", components.host?.lowercased() == "pair",
              let query = components.percentEncodedQuery else { return nil }
        var fields: [String: String] = [:]
        for pair in query.split(separator: "&") {
            guard let equals = pair.firstIndex(of: "="), equals > pair.startIndex else { continue }
            let key = String(pair[..<equals])
            let raw = pair[pair.index(after: equals)...].replacingOccurrences(of: "+", with: " ")
            // The first of a repeated field counts, as on Android.
            guard fields[key] == nil, let value = raw.removingPercentEncoding else { continue }
            fields[key] = value
        }
        guard fields["v"] == String(PCWire.version) else { return nil }
        guard let host = fields["host"].map(cleanHost), !host.isEmpty, host.count <= 253,
              host.allSatisfy(isHostCharacter) else { return nil }
        guard let portText = fields["port"], let port = UInt16(portText), port > 0 else { return nil }
        guard let fingerprint = fields["fp"]?.lowercased(), isFingerprint(fingerprint) else { return nil }
        guard let token = fields["token"], (16...128).contains(token.count), token.allSatisfy(isTokenCharacter) else {
            return nil
        }
        let name = PCText.line(fields["name"] ?? "", max: 64)
        var computer = PCComputer(host: host, port: port, fingerprint: fingerprint, name: name)
        let relayFields = ["relay", "room", "key", "access"]
        let present = relayFields.filter { fields[$0] != nil }
        // Fail closed for partial or malformed relay credentials: silently dropping
        // them makes cellular pairing appear successful while it cannot connect.
        if !present.isEmpty {
            guard present.count == relayFields.count,
                  let url = fields["relay"], let room = fields["room"],
                  let key = fields["key"], let access = fields["access"],
                  let relay = PCRelayConfig.parse(url: url, room: room, key: key, access: access) else { return nil }
            computer.relay = relay
        }
        return PCPairing(computer: computer, token: token)
    }

    /// "[fe80::1]" is "fe80::1".
    private static func cleanHost(_ raw: String) -> String {
        var host = raw.trimmingCharacters(in: .whitespaces)
        if host.hasPrefix("["), host.hasSuffix("]") { host = String(host.dropFirst().dropLast()) }
        return host
    }

    private static func isHostCharacter(_ c: Character) -> Bool {
        c.isASCII && (c.isLetter || c.isNumber || ".-:%_".contains(c))
    }

    private static func isTokenCharacter(_ c: Character) -> Bool {
        c.isASCII && (c.isLetter || c.isNumber || c == "-" || c == "_")
    }

    static func isFingerprint(_ text: String) -> Bool {
        text.count == 64 && text.allSatisfy { $0.isASCII && $0.isHexDigit }
    }
}

enum PCText {
    /// One line of at most `max` characters: line breaks and control characters become spaces, ends trimmed.
    static func line(_ text: String, max: Int) -> String {
        let flat = text.components(separatedBy: .controlCharacters).joined(separator: " ")
        return String(flat.trimmingCharacters(in: .whitespaces).prefix(max))
    }

    /// At most `max` characters, line breaks kept.
    static func clip(_ text: String, max: Int) -> String {
        text.count <= max ? text : String(text.prefix(max))
    }

    /// An answer as it is read aloud: code blocks, Markdown marks and link addresses left out, each line a sentence,
    /// at most `max` characters (cut after a sentence when one ends in the second half). Same as the computer's
    /// spokenText (windows/src/core/voice.ts).
    static func spoken(_ markdown: String, max: Int = 600) -> String {
        var kept: [String] = []
        var code = false
        for raw in markdown.components(separatedBy: .newlines) {
            var line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("```") {
                code.toggle()
                continue
            }
            if code || line.isEmpty { continue }
            while let first = line.first, "#>-*•".contains(first) {
                line.removeFirst()
                line = line.trimmingCharacters(in: .whitespaces)
            }
            if line.isEmpty { continue }
            // A list item or a title is a sentence too: the voice pauses after it.
            let bare = line.replacingOccurrences(of: #"[*_`]+$"#, with: "", options: .regularExpression)
            if let last = bare.last, !".!?…:;,".contains(last) { line += "." }
            kept.append(line)
        }
        var text = kept.joined(separator: " ")
        text = text.replacingOccurrences(of: #"\[([^\]]*)\]\([^)]*\)"#, with: "$1", options: .regularExpression)
        for mark in ["**", "__", "`", "*"] {
            text = text.replacingOccurrences(of: mark, with: "")
        }
        text = text.replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression).trimmingCharacters(in: .whitespaces)
        guard text.count > max else { return text }
        let cut = String(text.prefix(max))
        if let end = cut.range(of: ". ", options: .backwards), cut.distance(from: cut.startIndex, to: end.lowerBound) > max / 2 {
            return String(cut[..<end.lowerBound]) + "."
        }
        return cut.trimmingCharacters(in: .whitespaces) + "…"
    }
}

// MARK: - Lines from the iPhone

/// The value of a home action: a percent (volume, brightness), or an app's name or a channel.
enum PCHomeValue: Equatable, Sendable {
    case number(Int)
    case text(String)
}

enum PCOutgoing: Equatable, Sendable {
    case hello(token: String, device: String, caps: [String])
    case decision(fingerprint: String, allow: Bool)
    case answer(fingerprint: String, picks: [[String]])
    case ping
    case bye
    case chatModels
    case chatSend(id: String, model: String, text: String, agent: Bool)
    case chatCancel(id: String)
    case chatReset
    /// The assistant (capability `assistant`): an `assistant` line answers the Get, an `assistantResult` with the
    /// same `id` each of the others. A nil field of `assistantSet` is left as it is on the computer.
    case assistantGet
    case assistantSet(id: String, name: String?, persona: String?, personaText: String?)
    case assistantBrain(id: String, brain: String)
    case assistantSkills(id: String, skills: [String])
    case assistantLogin(id: String)
    case assistantLoginCancel(id: String)
    case assistantGateway(id: String, on: Bool)
    /// Mochi's home (capability `home`): a `home` line answers the Get, a `homeResult` each action. `confirmed`: the
    /// owner of this iPhone said yes with Face ID, for a device or a routine that asks first.
    case homeGet
    case homeControl(id: String, device: String, action: String, value: PCHomeValue?, confirmed: Bool)
    case homeRoutine(id: String, routine: String, confirmed: Bool)
    /// The lines of a file a session changed (capability `diffs`), answered with `diff` parts.
    case getDiff(pillId: String, fileId: Int)

    /// One line of the link, newline included.
    var line: Data {
        var object: [String: Any]
        switch self {
        case .hello(let token, let device, let caps):
            object = ["type": "hello", "v": PCWire.version, "token": token, "device": device, "caps": caps]
        case .decision(let fingerprint, let allow):
            object = ["type": "decision", "fingerprint": fingerprint, "decision": allow ? "allow" : "deny"]
        case .answer(let fingerprint, let picks):
            object = ["type": "answer", "fingerprint": fingerprint, "picks": picks]
        case .ping:
            object = ["type": "ping"]
        case .bye:
            object = ["type": "bye"]
        case .chatModels:
            object = ["type": "chatModels"]
        case .chatSend(let id, let model, let text, let agent):
            object = ["type": "chatSend", "id": id, "model": model, "text": text]
            if agent { object["agent"] = true }
        case .chatCancel(let id):
            object = ["type": "chatCancel", "id": id]
        case .chatReset:
            object = ["type": "chatReset"]
        case .assistantGet:
            object = ["type": "assistantGet"]
        case .assistantSet(let id, let name, let persona, let personaText):
            object = ["type": "assistantSet", "id": id]
            if let name { object["name"] = name }
            if let persona { object["persona"] = persona }
            if let personaText { object["personaText"] = personaText }
        case .assistantBrain(let id, let brain):
            object = ["type": "assistantBrain", "id": id, "brain": brain]
        case .assistantSkills(let id, let skills):
            object = ["type": "assistantSkills", "id": id, "skills": skills]
        case .assistantLogin(let id):
            object = ["type": "assistantLogin", "id": id]
        case .assistantLoginCancel(let id):
            object = ["type": "assistantLoginCancel", "id": id]
        case .assistantGateway(let id, let on):
            object = ["type": "assistantGateway", "id": id, "on": on]
        case .homeGet:
            object = ["type": "homeGet"]
        case .homeControl(let id, let device, let action, let value, let confirmed):
            object = ["type": "homeControl", "id": id, "device": device, "action": action, "confirmed": confirmed]
            switch value {
            case .number(let number)?: object["value"] = number
            case .text(let text)?: object["value"] = text
            case nil: break
            }
        case .homeRoutine(let id, let routine, let confirmed):
            object = ["type": "homeRoutine", "id": id, "routine": routine, "confirmed": confirmed]
        case .getDiff(let pillId, let fileId):
            object = ["type": "getDiff", "pillId": pillId, "fileId": fileId]
        }
        var data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
        data.append(0x0A)
        return data
    }
}

// MARK: - Lines from the computer

/// An agent session on the computer (`sessions[]`). `steps`, `finalLine`, `project` and `color` come only with the
/// `details` capability, `files` only with `diffs`.
struct PCSession: Equatable, Sendable {
    var pillId: String
    var agent: String
    var state: String
    var statusText: String
    var stepIndex: Int
    var stepCount: Int
    var updatedAt: Date
    var steps: [String] = []
    var finalLine: String = ""
    /// A folder name, never a path.
    var project: String = ""
    var color: String = ""
    /// The files it changed, the newest last.
    var files: [PCFileChange] = []
}

/// A file a session changed (`sessions[].files`, capability `diffs`): its name only, never a path. Its lines come on
/// request (`getDiff`).
struct PCFileChange: Equatable, Sendable, Identifiable {
    var id: Int
    var name: String
    var added: Int
    var removed: Int
    /// Too large to show line by line: the counts only.
    var tooLarge = false
    /// A file written whole.
    var isNew = false

    static let maxFiles = 20
    static let maxName = 80

    /// The last segment of a path, one line of at most 80 characters; nil when nothing is left.
    static func name(_ raw: String) -> String? {
        let segment = raw.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let name = PCText.line(segment, max: maxName)
        return name.isEmpty || name == "." || name == ".." ? nil : name
    }
}

/// A permission request waiting on the computer. `truncated`: the command is longer than what was sent, so it can
/// only be denied from here (the computer ignores an Allow for it).
struct PCApproval: Equatable, Sendable {
    var pillId: String
    var fingerprint: String
    var tool: String
    var command: String
    var createdAt: Date
    var truncated: Bool
    /// When this iPhone got it: the 2 minutes count from here, whatever the computer's clock says.
    var receivedAt: Date = .distantPast
}

/// A question Claude Code asks (`answers` capability), answered with the exact labels.
struct PCQuestion: Equatable, Sendable {
    struct Option: Equatable, Sendable {
        var label: String
        var description: String
    }

    struct Item: Equatable, Sendable {
        var question: String
        var multiSelect: Bool
        var options: [Option]
    }

    var pillId: String
    var fingerprint: String
    var createdAt: Date
    var items: [Item]
    var receivedAt: Date = .distantPast
}

/// A provider and model the computer lets the phone chat with ("provider/model").
struct PCChatModel: Equatable, Sendable, Identifiable {
    var id: String
    var provider: String
    var label: String
}

/// One thing Mochi does in a turn with his hands: "Bash · npm test", running then ok.
struct PCChatStep: Equatable, Sendable, Codable {
    var index: Int
    /// running, waiting, ok, error or denied.
    var status: String
    var text: String
}

/// Hermes Agent on the computer as the phone may change it (capability `assistant`): its name and tone, its brain,
/// its skills, the ChatGPT sign-in and the gateway. Never a key, a path, an address or the account's e-mail: the
/// computer does not send them.
struct PCAssistant: Equatable, Sendable {
    struct Brain: Equatable, Sendable, Identifiable {
        var id: String
        /// The computer's English name for it; the screens name the known ones themselves.
        var name: String
        /// Its key or address is set on the computer (a sign-in brain is ready: it signs in during setup).
        var ready: Bool
        /// It signs in with an account (ChatGPT) instead of a key.
        var login: Bool
    }

    enum Gateway: String, Equatable, Sendable {
        case off, starting, running, elsewhere, stopped
    }

    /// Setting up Hermes on the computer: a step of `total`, and what went wrong.
    struct Setup: Equatable, Sendable {
        var running = false
        var step = ""
        var done = 0
        var total = 0
        var error = ""
    }

    struct Hermes: Equatable, Sendable {
        var installed = false
        /// Coucou installed it and runs it: only then may the phone change its brain and its skills.
        var managed = false
        var gateway: Gateway = .off
        /// Why the gateway stopped.
        var reason = ""
        var setup = Setup()
    }

    /// The ChatGPT sign-in (`hermes auth add openai-codex` on the computer).
    enum Login: Equatable, Sendable {
        case unknown
        case signedOut
        /// The code to type on OpenAI's page (`url`, always on auth.openai.com), until `expiresAt`.
        case waiting(code: String, url: String, expiresAt: Date?)
        case signedIn
        case failed(reason: String)
    }

    var name = PCAssistant.defaultName
    var persona = PCAssistant.defaultPersona
    var personaText = ""
    var personas: [String] = PCAssistant.knownPersonas
    var brain = ""
    var brains: [Brain] = []
    /// The packs wanted, Mochi's home aside (that one is `house`).
    var skills: [String] = []
    var skillPacks: [String] = []
    /// Mochi's home for Hermes: shown here, changed on the computer only.
    var house = false
    var hermes = Hermes()
    var login: Login = .unknown

    static let defaultName = "Zheus"
    static let defaultPersona = "friendly"
    static let maxName = 40
    static let maxPersonaText = 1500
    static let knownPersonas = ["friendly", "direct", "professional", "playful", "butler", "custom"]
    static let customPersona = "custom"
    static let houseSkill = "home"
    static let signInHost = "auth.openai.com"
    /// Where the sign-in happens when the computer named no page of its own.
    static let defaultSignInPage = "https://auth.openai.com/codex/device"

    var current: Brain? { brains.first { $0.id == brain } }
    /// The packs the phone may tick (Mochi's home is the computer's to change).
    var optionalSkills: [String] { skillPacks.filter { $0 != Self.houseSkill } }
    /// Coucou runs Hermes there: the brain and the skills can change from here.
    var changeable: Bool { hermes.installed && hermes.managed }

    /// A page this iPhone may open for the ChatGPT sign-in: https on auth.openai.com exactly, nothing else.
    static func isSignInPage(_ text: String) -> Bool {
        guard !text.isEmpty, text.utf8.count <= 512, text.utf8.allSatisfy({ $0 > 0x20 && $0 < 0x7F }) else { return false }
        let lower = text.lowercased()
        guard lower == "https://\(signInHost)" || lower.hasPrefix("https://\(signInHost)/") else { return false }
        return URLComponents(string: text)?.host?.lowercased() == signInHost
    }

    /// A device code as OpenAI shows it ("ABCD-1234"): letters, digits, dashes and spaces, at most 32; nil otherwise.
    static func deviceCode(_ raw: String) -> String? {
        let code = raw.trimmingCharacters(in: .whitespaces)
        guard !code.isEmpty, code.count <= 32,
              code.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == " ") }) else { return nil }
        return code
    }

    /// Whole seconds until `end`, 0 once it passed.
    static func secondsLeft(until end: Date, now: Date) -> Int {
        let left = end.timeIntervalSince(now)
        return left > 0 ? Int(min(left, 86_400).rounded(.up)) : 0
    }
}

/// Mochi's home on the computer as the phone may use it (capability `home`): only the devices the user ticked for
/// Mochi there, with what each one said it can do. Never an address, a MAC address, the Home Assistant URL or a token:
/// the computer does not send them.
struct PCHome: Equatable, Sendable {
    struct Channel: Equatable, Sendable, Hashable {
        var number: String
        var name: String
    }

    /// What the device reports now; nil: it does not say.
    struct State: Equatable, Sendable {
        var online = false
        var on: Bool?
        /// Percent.
        var volume: Int?
        var muted: Bool?
        /// Percent.
        var brightness: Int?
        var app: String?
        var channel: String?
    }

    struct Device: Equatable, Sendable, Identifiable {
        /// `ha:light.sala`, `samsung:…`, `philips:…`.
        var id: String
        var name: String
        /// "" when no room is set.
        var room: String
        /// tv, speaker, light, plug or switch ("" for a kind this iPhone does not know).
        var kind: String
        /// "Home Assistant", "Samsung TV"…
        var source: String
        /// What the device itself reported it can do (`turnOn`, `volumeSet`…): nothing else is offered for it.
        var caps: [String]
        var apps: [String]
        var channels: [Channel]
        var state: State
        /// An action on it asks first: Face ID here (the computer's rule: ask-all, plugs and switches, the user's
        /// choice for this device).
        var ask: Bool

        func can(_ cap: String) -> Bool { caps.contains(cap) }
    }

    struct Routine: Equatable, Sendable, Identifiable {
        var id: String
        var name: String
        /// "Luz da sala · Turn off", as the computer words them.
        var steps: [String]
        var ask: Bool
        /// What would stop it (a device gone, not shared any more…).
        var problems: [String]
    }

    /// One line of the home's record, newest first as the computer keeps it.
    struct Entry: Equatable, Sendable {
        var at: String
        /// Who asked: phone, mochi, settings, routine…
        var origin: String
        var device: String
        var action: String
        var ok: Bool
        var detail: String
    }

    struct Room: Equatable, Sendable, Identifiable {
        /// "" for the devices without a room.
        var name: String
        var devices: [Device]
        var id: String { name }
    }

    var enabled = false
    var askAll = false
    var devices: [Device] = []
    var routines: [Routine] = []
    var log: [Entry] = []

    static let maxDevices = 100
    static let maxApps = 40
    static let maxChannels = 60
    static let maxRoutines = 50
    static let maxLog = 30
    static let kinds = ["tv", "speaker", "light", "plug", "switch"]
    /// The serde names of the computer's `Capability`.
    static let capabilities = ["turnOn", "turnOff", "volumeSet", "volumeStep", "mute", "apps", "channels",
                               "channelStep", "brightness"]
    static let actions = ["turn_on", "turn_off", "set_volume", "volume_up", "volume_down", "mute", "unmute", "open_app",
                          "set_channel", "channel_up", "channel_down", "set_brightness"]

    /// The capability a device must have reported for an action, as the computer checks it; nil: no such action.
    static func capability(for action: String) -> String? {
        switch action {
        case "turn_on": "turnOn"
        case "turn_off": "turnOff"
        case "set_volume": "volumeSet"
        case "volume_up", "volume_down": "volumeStep"
        case "mute", "unmute": "mute"
        case "open_app": "apps"
        case "set_channel": "channels"
        case "channel_up", "channel_down": "channelStep"
        case "set_brightness": "brightness"
        default: nil
        }
    }

    /// An action the device said it can do, with the value it needs (the computer checks it all again).
    static func accepts(_ device: Device, action: String, value: PCHomeValue?) -> Bool {
        guard let cap = capability(for: action), device.can(cap) else { return false }
        switch (action, value) {
        case ("set_volume", .number(let percent)?):
            return (0...100).contains(percent)
        case ("set_brightness", .number(let percent)?):
            return (1...100).contains(percent)
        case ("open_app", .text(let app)?):
            return device.apps.contains(app)
        case ("set_channel", .text(let channel)?):
            return !channel.isEmpty && channel.count <= 100
        case ("set_volume", _), ("set_brightness", _), ("open_app", _), ("set_channel", _):
            return false
        default:
            return value == nil
        }
    }

    /// The devices by room: rooms in the computer's order, the devices without one last.
    var rooms: [Room] {
        var order: [String] = []
        var byRoom: [String: [Device]] = [:]
        for device in devices {
            if byRoom[device.room] == nil { order.append(device.room) }
            byRoom[device.room, default: []].append(device)
        }
        let sorted = order.filter { !$0.isEmpty } + order.filter(\.isEmpty)
        return sorted.map { Room(name: $0, devices: byRoom[$0] ?? []) }
    }
}

/// The computer's answer to an assistant or a home request, or what this iPhone found itself (not connected, Face ID
/// cancelled, no answer in time).
struct PCRequestResult: Equatable, Sendable {
    var id: String
    var ok: Bool
    /// From the computer: `off invalid not_installed not_ready busy rate failed`, and `unknown confirm` for the home.
    /// From this iPhone: `offline timeout declined`, and `off invalid not_installed not_ready busy` checked here first.
    var reason = ""
    /// The computer's fixed English sentence, or what went wrong with a home action, in words.
    var message = ""
    /// What a home action did ("On · 40%").
    var detail = ""

    static func local(_ reason: String, id: String = "") -> PCRequestResult {
        PCRequestResult(id: id, ok: false, reason: reason)
    }
}

/// The plan usage the island shows (capability `usage`): Claude's and Codex's windows, nothing about the account.
struct PCUsage: Equatable, Sendable {
    struct Window: Equatable, Sendable {
        /// 0 to 100.
        var pct: Int
        var resetsAt: Date

        /// A window whose reset has passed counts as 0, as on the computer.
        func percent(at now: Date) -> Int { now >= resetsAt ? 0 : pct }
    }

    struct Plan: Equatable, Sendable {
        var fiveHour: Window?
        var sevenDay: Window?
        /// Codex's free resets, 0 to 99.
        var resetCredits: Int?
        /// "plus", "max 5x": letters, digits, spaces and dashes.
        var plan = ""
        var updatedAt: Date?
    }

    var claude: Plan?
    var codex: Plan?

    var isEmpty: Bool { claude == nil && codex == nil }
}

/// One line of a file's diff.
struct PCDiffLine: Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        case added, removed, context
        /// A hunk starts: the text is "@@ <line>".
        case hunk
    }

    var kind: Kind
    var text: String
}

/// One part of a file's diff as the computer sends it: at most 2 parts of 100 lines, each line at most 400 characters.
struct PCDiffPart: Equatable, Sendable {
    var pillId: String
    var fileId: Int
    var name: String
    var added: Int
    var removed: Int
    var tooLarge: Bool
    /// The computer no longer has this change.
    var gone: Bool
    /// There were more than 200 lines: the first ones came.
    var truncated: Bool
    var part: Int
    var parts: Int
    var lines: [PCDiffLine]

    static let maxParts = 2
    static let partLines = 100
    static let maxLineCharacters = 400
}

/// A file's diff as its parts come in, held while its sheet is open and forgotten when it closes.
struct PCDiff: Equatable, Sendable {
    var pillId: String
    var fileId: Int
    var name = ""
    var added = 0
    var removed = 0
    var tooLarge = false
    var gone = false
    var truncated = false
    /// How many parts the computer sends; 0 until the first one came.
    var parts = 0
    var received: [Int: [PCDiffLine]] = [:]

    var complete: Bool { parts > 0 && received.count >= parts }
    /// The lines, in order, of the parts that came.
    var lines: [PCDiffLine] { (0..<max(0, parts)).flatMap { received[$0] ?? [] } }
}

/// A service's card (capability `services`, one tick per service on the computer): read-only, as its pill shows it.
struct PCServiceCard: Equatable, Sendable, Identifiable {
    struct Item: Equatable, Sendable {
        var label: String
        var detail: String
    }

    var id: String
    var title: String
    var headline: String
    var reason: String
    var items: [Item]

    /// The only services the link carries.
    static let ids = ["integration_stripe", "integration_github", "integration_vercel", "integration_n8n",
                      "integration_resend", "integration_notion", "integration_calcom"]
    static let maxItems = 3
    /// Each text is one line of at most this many characters (the computer keeps them shorter).
    static let maxText = 80
}

enum PCIncoming: Equatable, Sendable {
    case welcome(desktop: String, os: String, caps: Set<String>)
    case error(code: String, message: String)
    case sessions([PCSession])
    case approval(PCApproval)
    /// An approval or a question was answered (here or on the computer) or timed out.
    case resolved(fingerprint: String)
    case question(PCQuestion)
    case pong
    case chatModels([PCChatModel])
    case chatDelta(id: String, text: String)
    case chatDone(id: String, text: String?)
    case chatError(id: String, reason: String, message: String)
    case chatStep(id: String, step: PCChatStep)
    /// Where the computer can be reached from the internet now; empty: nowhere (its switch is off).
    case remote([String])
    case assistant(PCAssistant)
    case assistantResult(PCRequestResult)
    case home(PCHome)
    case homeResult(PCRequestResult)
    /// What Mochi wears on the computer: one of PCWire.outfits.
    case prefs(outfit: String)
    /// Empty: the computer has nothing to say any more.
    case usage(PCUsage)
    case diff(PCDiffPart)
    /// The cards of the services the user ticked; empty takes them away.
    case services([PCServiceCard])
    /// Anything this iPhone does not use.
    case other

    /// Nil when the line is not a JSON object: the link is broken.
    static func decode(_ line: Data) -> PCIncoming? {
        guard let object = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else { return nil }
        let text = { (key: String) in object[key] as? String ?? "" }
        switch text("type") {
        case "welcome":
            let caps = (object["caps"] as? [Any] ?? []).compactMap { $0 as? String }
            return .welcome(desktop: PCText.line(text("desktop"), max: 64), os: PCText.line(text("os"), max: 32),
                            caps: Set(caps))
        case "error":
            return .error(code: text("code"), message: PCText.line(text("message"), max: 200))
        case "sessions":
            let list = (object["sessions"] as? [Any] ?? []).compactMap { $0 as? [String: Any] }
            return .sessions(list.prefix(64).compactMap(session))
        case "approval":
            guard let approval = approval(object) else { return .other }
            return .approval(approval)
        case "approvalResolved":
            let fingerprint = text("fingerprint")
            return fingerprint.isEmpty ? .other : .resolved(fingerprint: fingerprint)
        case "question":
            guard let question = question(object) else { return .other }
            return .question(question)
        case "pong":
            return .pong
        case "chatModels":
            let list = (object["models"] as? [Any] ?? []).compactMap { $0 as? [String: Any] }
            return .chatModels(list.prefix(100).compactMap { m in
                guard let id = m["id"] as? String, !id.isEmpty else { return nil }
                let label = PCText.line(m["label"] as? String ?? "", max: 80)
                return PCChatModel(id: id, provider: PCText.line(m["provider"] as? String ?? "", max: 40),
                                   label: label.isEmpty ? id : label)
            })
        case "chatDelta":
            return .chatDelta(id: text("id"), text: text("text"))
        case "chatDone":
            return .chatDone(id: text("id"), text: object["text"] as? String)
        case "chatError":
            return .chatError(id: text("id"), reason: text("reason"), message: PCText.line(text("message"), max: 200))
        case "chatStep":
            guard let index = int(object["index"]), index >= 0 else { return .other }
            let step = PCChatStep(index: index, status: PCText.line(text("status"), max: 16),
                                  text: PCText.line(text("text"), max: 400))
            return .chatStep(id: text("id"), step: step)
        case "remote":
            guard let list = object["endpoints"] as? [Any] else { return .other }
            return .remote(PCRemote.clean(list.compactMap { $0 as? String }))
        case "assistant":
            return .assistant(assistant(object))
        case "assistantResult":
            guard let result = result(object) else { return .other }
            return .assistantResult(result)
        case "home":
            return .home(home(object))
        case "homeResult":
            guard let result = result(object) else { return .other }
            return .homeResult(result)
        case "prefs":
            // A value this iPhone does not know is not guessed: Mochi keeps what he wore.
            guard let outfit = object["outfit"] as? String, PCWire.outfits.contains(outfit) else { return .other }
            return .prefs(outfit: outfit)
        case "usage":
            return .usage(PCUsage(claude: plan(object["claude"]), codex: plan(object["codex"])))
        case "diff":
            guard let part = diffPart(object) else { return .other }
            return .diff(part)
        case "services":
            guard let list = object["services"] as? [Any] else { return .other }
            return .services(services(list))
        default:
            return .other
        }
    }

    private static func session(_ o: [String: Any]) -> PCSession? {
        guard let pillId = o["pillId"] as? String, !pillId.isEmpty else { return nil }
        let steps = (o["steps"] as? [Any] ?? []).compactMap { $0 as? String }.suffix(20).map { PCText.line($0, max: 200) }
        let color = o["color"] as? String ?? ""
        return PCSession(
            pillId: PCText.line(pillId, max: 64),
            agent: PCText.line(o["agent"] as? String ?? "", max: 64),
            state: o["state"] as? String ?? "idle",
            statusText: PCText.line(o["statusText"] as? String ?? "", max: 200),
            stepIndex: max(0, int(o["stepIndex"]) ?? 0),
            stepCount: max(0, int(o["stepCount"]) ?? 0),
            updatedAt: date(o["updatedAt"]) ?? .now,
            steps: Array(steps),
            finalLine: PCText.line(o["finalLine"] as? String ?? "", max: 200),
            project: PCText.line(o["project"] as? String ?? "", max: 64),
            color: isColor(color) ? color : "",
            files: Array(objects(o["files"]).compactMap(file).suffix(PCFileChange.maxFiles))
        )
    }

    /// A file with an id and a name, its name only even if a path came.
    private static func file(_ f: [String: Any]) -> PCFileChange? {
        guard let id = int(f["id"]), id >= 0, let name = PCFileChange.name(f["name"] as? String ?? "") else { return nil }
        return PCFileChange(id: id, name: name, added: max(0, int(f["added"]) ?? 0), removed: max(0, int(f["removed"]) ?? 0),
                            tooLarge: f["tooLarge"] as? Bool ?? false, isNew: f["isNew"] as? Bool ?? false)
    }

    // MARK: The assistant

    private static func assistant(_ o: [String: Any]) -> PCAssistant {
        var assistant = PCAssistant()
        let name = PCText.line(o["name"] as? String ?? "", max: PCAssistant.maxName)
        assistant.name = name.isEmpty ? PCAssistant.defaultName : name
        assistant.persona = token(o["persona"]) ?? PCAssistant.defaultPersona
        assistant.personaText = PCText.clip(o["personaText"] as? String ?? "", max: PCAssistant.maxPersonaText)
        let personas = tokens(o["personas"])
        assistant.personas = personas.isEmpty ? PCAssistant.knownPersonas : personas
        assistant.brain = token(o["brain"]) ?? ""
        var brains: [PCAssistant.Brain] = []
        for b in objects(o["brains"]) where brains.count < 16 {
            guard let id = token(b["id"]), !brains.contains(where: { $0.id == id }) else { continue }
            let label = PCText.line(b["name"] as? String ?? "", max: 60)
            brains.append(PCAssistant.Brain(id: id, name: label.isEmpty ? id : label,
                                            ready: b["ready"] as? Bool ?? false, login: b["login"] as? Bool ?? false))
        }
        assistant.brains = brains
        assistant.skillPacks = tokens(o["skillPacks"])
        assistant.skills = tokens(o["skills"]).filter { $0 != PCAssistant.houseSkill }
        assistant.house = o["house"] as? Bool ?? false
        assistant.hermes = hermes(o["hermes"] as? [String: Any] ?? [:])
        assistant.login = login(o["login"] as? [String: Any])
        return assistant
    }

    private static func hermes(_ o: [String: Any]) -> PCAssistant.Hermes {
        let s = o["setup"] as? [String: Any] ?? [:]
        let total = min(max(0, int(s["total"]) ?? 0), 1000)
        let setup = PCAssistant.Setup(running: s["running"] as? Bool ?? false,
                                      step: PCText.line(s["step"] as? String ?? "", max: 200),
                                      done: min(max(0, int(s["done"]) ?? 0), total), total: total,
                                      error: PCText.line(s["error"] as? String ?? "", max: 300))
        let gateway = PCAssistant.Gateway(rawValue: o["gateway"] as? String ?? "") ?? .off
        return PCAssistant.Hermes(installed: o["installed"] as? Bool ?? false, managed: o["managed"] as? Bool ?? false,
                                  gateway: gateway,
                                  reason: gateway == .stopped ? PCText.line(o["reason"] as? String ?? "", max: 200) : "",
                                  setup: setup)
    }

    /// The sign-in's state. A waiting one without a usable code says nothing; a page that is not OpenAI's is never kept.
    private static func login(_ o: [String: Any]?) -> PCAssistant.Login {
        guard let o else { return .unknown }
        switch o["state"] as? String ?? "" {
        case "signedOut":
            return .signedOut
        case "signedIn":
            return .signedIn
        case "waiting":
            guard let code = PCAssistant.deviceCode(o["code"] as? String ?? "") else { return .unknown }
            let page = o["url"] as? String ?? ""
            return .waiting(code: code, url: PCAssistant.isSignInPage(page) ? page : PCAssistant.defaultSignInPage,
                            expiresAt: date(o["expiresAt"]))
        case "failed":
            return .failed(reason: PCText.line(o["reason"] as? String ?? "", max: 200))
        default:
            return .unknown
        }
    }

    /// `assistantResult` and `homeResult`: the id this iPhone gave, and how it went.
    private static func result(_ o: [String: Any]) -> PCRequestResult? {
        guard let id = o["id"] as? String, !id.isEmpty, id.count <= PCWire.maxRequestID else { return nil }
        return PCRequestResult(id: id, ok: o["ok"] as? Bool ?? false, reason: token(o["reason"]) ?? "",
                               message: PCText.line(o["message"] as? String ?? "", max: 300),
                               detail: PCText.line(o["detail"] as? String ?? "", max: 200))
    }

    // MARK: Mochi's home

    private static func home(_ o: [String: Any]) -> PCHome {
        var home = PCHome(enabled: o["enabled"] as? Bool ?? false, askAll: o["askAll"] as? Bool ?? false)
        for raw in objects(o["devices"]) where home.devices.count < PCHome.maxDevices {
            guard let item = device(raw), !home.devices.contains(where: { $0.id == item.id }) else { continue }
            home.devices.append(item)
        }
        for raw in objects(o["routines"]) where home.routines.count < PCHome.maxRoutines {
            guard let item = routine(raw), !home.routines.contains(where: { $0.id == item.id }) else { continue }
            home.routines.append(item)
        }
        home.log = Array(objects(o["log"]).compactMap(entry).prefix(PCHome.maxLog))
        return home
    }

    private static func device(_ o: [String: Any]) -> PCHome.Device? {
        guard let rawID = o["id"] as? String else { return nil }
        let id = PCText.line(rawID, max: 200)
        guard !id.isEmpty, id == rawID else { return nil }
        let name = PCText.line(o["name"] as? String ?? "", max: 80)
        let kind = (o["kind"] as? String).flatMap { PCHome.kinds.contains($0) ? $0 : nil } ?? ""
        let caps = unique(strings(o["caps"]).filter { PCHome.capabilities.contains($0) })
        let apps = unique(strings(o["apps"]).map { PCText.line($0, max: 100) }.filter { !$0.isEmpty })
        var channels: [PCHome.Channel] = []
        for c in objects(o["channels"]) where channels.count < PCHome.maxChannels {
            let number = (c["number"] as? String).map { PCText.line($0, max: 16) } ?? int(c["number"]).map { String($0) } ?? ""
            let channel = PCHome.Channel(number: number, name: PCText.line(c["name"] as? String ?? "", max: 80))
            if !(number.isEmpty && channel.name.isEmpty), !channels.contains(channel) { channels.append(channel) }
        }
        let s = o["state"] as? [String: Any] ?? [:]
        let state = PCHome.State(online: s["online"] as? Bool ?? false, on: s["on"] as? Bool,
                                 volume: percent(s["volume"]), muted: s["muted"] as? Bool,
                                 brightness: percent(s["brightness"]),
                                 app: optionalLine(s["app"], max: 100), channel: optionalLine(s["channel"], max: 100))
        return PCHome.Device(id: id, name: name.isEmpty ? id : name, room: PCText.line(o["room"] as? String ?? "", max: 60),
                             kind: kind, source: PCText.line(o["source"] as? String ?? "", max: 40), caps: caps,
                             apps: Array(apps.prefix(PCHome.maxApps)), channels: channels, state: state,
                             // Not said: it asks, which is the safe side.
                             ask: o["ask"] as? Bool ?? true)
    }

    private static func routine(_ o: [String: Any]) -> PCHome.Routine? {
        guard let rawID = o["id"] as? String else { return nil }
        let id = PCText.line(rawID, max: 64)
        guard !id.isEmpty, id == rawID else { return nil }
        let name = PCText.line(o["name"] as? String ?? "", max: 80)
        return PCHome.Routine(id: id, name: name.isEmpty ? id : name,
                              steps: Array(strings(o["steps"]).map { PCText.line($0, max: 200) }.filter { !$0.isEmpty }.prefix(20)),
                              ask: o["ask"] as? Bool ?? true,
                              problems: Array(strings(o["problems"]).map { PCText.line($0, max: 200) }.filter { !$0.isEmpty }.prefix(10)))
    }

    private static func entry(_ o: [String: Any]) -> PCHome.Entry? {
        let entry = PCHome.Entry(at: PCText.line(o["at"] as? String ?? "", max: 32),
                                 origin: PCText.line(o["origin"] as? String ?? "", max: 16),
                                 device: PCText.line(o["device"] as? String ?? "", max: 80),
                                 action: PCText.line(o["action"] as? String ?? "", max: 80),
                                 ok: o["ok"] as? Bool ?? false,
                                 detail: PCText.line(o["detail"] as? String ?? "", max: 200))
        return entry.device.isEmpty && entry.action.isEmpty ? nil : entry
    }

    // MARK: Plan usage, diffs, service cards

    /// A plan with at least one usable window; nil otherwise.
    private static func plan(_ value: Any?) -> PCUsage.Plan? {
        guard let o = value as? [String: Any] else { return nil }
        let fiveHour = window(o["fiveHour"])
        let sevenDay = window(o["sevenDay"])
        guard fiveHour != nil || sevenDay != nil else { return nil }
        let name = (o["plan"] as? String ?? "").filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == " " || $0 == "-") }
        return PCUsage.Plan(fiveHour: fiveHour, sevenDay: sevenDay,
                            resetCredits: int(o["resetCredits"]).flatMap { (0...99).contains($0) ? $0 : nil },
                            plan: String(name.prefix(20)).trimmingCharacters(in: .whitespaces),
                            updatedAt: date(o["updatedAt"]))
    }

    private static func window(_ value: Any?) -> PCUsage.Window? {
        guard let o = value as? [String: Any], let pct = int(o["pct"]), (0...100).contains(pct),
              let resetsAt = date(o["resetsAt"]) else { return nil }
        return PCUsage.Window(pct: pct, resetsAt: resetsAt)
    }

    /// A part of a diff, or nil when it breaks the limits the computer keeps (so it cannot be a real one).
    private static func diffPart(_ o: [String: Any]) -> PCDiffPart? {
        guard let pillId = o["pillId"] as? String, !pillId.isEmpty, pillId.count <= 64,
              let fileId = int(o["fileId"]), fileId >= 0,
              let parts = int(o["parts"]), (1...PCDiffPart.maxParts).contains(parts),
              let part = int(o["part"]), (0..<parts).contains(part),
              let rows = o["lines"] as? [Any], rows.count <= PCDiffPart.partLines else { return nil }
        let lines: [PCDiffLine] = rows.compactMap { row in
            guard let pair = row as? [Any], pair.count >= 2, let mark = pair[0] as? String, let text = pair[1] as? String else {
                return nil
            }
            // An unknown kind is read as context.
            let kind: PCDiffLine.Kind = switch mark {
            case "+": .added
            case "-": .removed
            case "@": .hunk
            default: .context
            }
            return PCDiffLine(kind: kind, text: PCText.clip(text, max: PCDiffPart.maxLineCharacters))
        }
        return PCDiffPart(pillId: pillId, fileId: fileId, name: PCFileChange.name(o["name"] as? String ?? "") ?? "",
                          added: max(0, int(o["added"]) ?? 0), removed: max(0, int(o["removed"]) ?? 0),
                          tooLarge: o["tooLarge"] as? Bool ?? false, gone: o["gone"] as? Bool ?? false,
                          truncated: o["truncated"] as? Bool ?? false, part: part, parts: parts, lines: lines)
    }

    /// The cards of the seven services this iPhone knows, once each, cut to size; anything else is skipped.
    private static func services(_ list: [Any]) -> [PCServiceCard] {
        var cards: [PCServiceCard] = []
        let limit = PCServiceCard.maxText
        for case let o as [String: Any] in list {
            guard let id = o["id"] as? String, PCServiceCard.ids.contains(id), !cards.contains(where: { $0.id == id }) else {
                continue
            }
            let items: [PCServiceCard.Item] = objects(o["items"]).compactMap { item in
                let label = PCText.line(item["label"] as? String ?? "", max: limit)
                return label.isEmpty ? nil : PCServiceCard.Item(label: label, detail: PCText.line(item["detail"] as? String ?? "", max: limit))
            }
            cards.append(PCServiceCard(id: id, title: PCText.line(o["title"] as? String ?? "", max: limit),
                                       headline: PCText.line(o["headline"] as? String ?? "", max: limit),
                                       reason: PCText.line(o["reason"] as? String ?? "", max: limit),
                                       items: Array(items.prefix(PCServiceCard.maxItems))))
        }
        return cards
    }

    // MARK: Pieces

    private static func objects(_ value: Any?) -> [[String: Any]] {
        (value as? [Any] ?? []).compactMap { $0 as? [String: Any] }
    }

    private static func strings(_ value: Any?) -> [String] {
        (value as? [Any] ?? []).compactMap { $0 as? String }
    }

    private static func unique(_ list: [String]) -> [String] {
        var seen = Set<String>()
        return list.filter { seen.insert($0).inserted }
    }

    /// An id such as `chatgpt` or `not_ready`: 1 to 32 ASCII letters, digits, `_` and `-`.
    private static func token(_ value: Any?) -> String? {
        guard let text = value as? String, (1...32).contains(text.count),
              text.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "_" || $0 == "-") }) else { return nil }
        return text
    }

    /// The well-formed ids of a list, each once, at most 16.
    private static func tokens(_ value: Any?) -> [String] {
        Array(unique(strings(value).compactMap { token($0) }).prefix(16))
    }

    /// A whole percent, 0 to 100; nil for anything else (null included).
    private static func percent(_ value: Any?) -> Int? {
        int(value).flatMap { (0...100).contains($0) ? $0 : nil }
    }

    /// One line, nil when empty or absent.
    private static func optionalLine(_ value: Any?, max: Int) -> String? {
        guard let text = value as? String else { return nil }
        let line = PCText.line(text, max: max)
        return line.isEmpty ? nil : line
    }

    private static func approval(_ o: [String: Any]) -> PCApproval? {
        guard let pillId = o["pillId"] as? String, !pillId.isEmpty,
              let fingerprint = o["fingerprint"] as? String, !fingerprint.isEmpty else { return nil }
        return PCApproval(pillId: PCText.line(pillId, max: 64), fingerprint: fingerprint,
                          tool: PCText.line(o["tool"] as? String ?? "", max: 40),
                          command: PCText.clip(o["command"] as? String ?? "", max: 2000),
                          createdAt: date(o["createdAt"]) ?? .now,
                          truncated: o["truncated"] as? Bool ?? false)
    }

    private static func question(_ o: [String: Any]) -> PCQuestion? {
        guard let pillId = o["pillId"] as? String, !pillId.isEmpty,
              let fingerprint = o["fingerprint"] as? String, !fingerprint.isEmpty else { return nil }
        let items: [PCQuestion.Item] = (o["questions"] as? [Any] ?? []).compactMap { raw in
            guard let q = raw as? [String: Any], let question = q["question"] as? String else { return nil }
            let options: [PCQuestion.Option] = (q["options"] as? [Any] ?? []).compactMap { raw in
                guard let option = raw as? [String: Any], let label = option["label"] as? String else { return nil }
                return PCQuestion.Option(label: label, description: option["description"] as? String ?? "")
            }
            return PCQuestion.Item(question: question, multiSelect: q["multiSelect"] as? Bool ?? false, options: options)
        }
        // Answers carry the exact texts back: a question that does not fit is left to the computer, never cut.
        guard !items.isEmpty, items.count <= 4, items.allSatisfy({ !$0.options.isEmpty && $0.options.count <= 8 }) else {
            return nil
        }
        return PCQuestion(pillId: PCText.line(pillId, max: 64), fingerprint: fingerprint,
                          createdAt: date(o["createdAt"]) ?? .now, items: items)
    }

    /// A whole number, written as an integer or as a float with nothing after the point.
    private static func int(_ value: Any?) -> Int? {
        if let n = value as? Int { return n }
        if let d = value as? Double, d.isFinite, d.rounded() == d, abs(d) < 9e15 { return Int(d) }
        return nil
    }

    /// Milliseconds since 1970.
    private static func date(_ value: Any?) -> Date? {
        guard let ms = int(value), ms > 0 else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    }

    private static func isColor(_ c: String) -> Bool {
        c.count == 7 && c.hasPrefix("#") && c.dropFirst().allSatisfy { $0.isASCII && $0.isHexDigit }
    }
}

/// Cuts the bytes of the link into lines. A line longer than 64 KiB breaks the link.
struct PCLineReader: Sendable {
    private var buffer = Data()

    /// The complete lines in what came so far, or nil when a line is too long.
    mutating func feed(_ data: Data) -> [Data]? {
        buffer.append(data)
        var lines: [Data] = []
        while let newline = buffer.firstIndex(of: 0x0A) {
            var line = Data(buffer[buffer.startIndex..<newline])
            buffer.removeSubrange(buffer.startIndex...newline)
            if line.count > PCWire.maxLineBytes { return nil }
            if line.last == 0x0D { line.removeLast() }
            if !line.isEmpty { lines.append(line) }
        }
        return buffer.count > PCWire.maxLineBytes ? nil : lines
    }
}

// MARK: - What the screens show

/// One message of the chat with Mochi on the computer.
struct PCChatEntry: Identifiable, Equatable, Sendable, Codable {
    enum Role: String, Equatable, Sendable, Codable { case you, mochi }

    let id: String
    let role: Role
    var text: String
    /// What Mochi did on the computer in this turn (his hands), in order.
    var steps: [PCChatStep] = []
    var failed = false
    var finished = false
    /// The turn was sent with Mochi's hands.
    var hands = false

    private enum CodingKeys: String, CodingKey {
        case id, role, text, steps, failed, finished, hands
    }
}

extension PCChatEntry {
    /// Written in an extension so the memberwise initializer stays. A field missing from what was saved takes its
    /// default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        role = try c.decode(Role.self, forKey: .role)
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        steps = try c.decodeIfPresent([PCChatStep].self, forKey: .steps) ?? []
        failed = try c.decodeIfPresent(Bool.self, forKey: .failed) ?? false
        finished = try c.decodeIfPresent(Bool.self, forKey: .finished) ?? false
        hands = try c.decodeIfPresent(Bool.self, forKey: .hands) ?? false
    }
}

/// The chat as this iPhone keeps it between launches (PCLink saves it to Application Support/pc-chat.json): the
/// messages and Mochi's steps as shown. Nothing else: no key, token, address or model.
struct PCChatArchive: Codable, Equatable, Sendable {
    static let currentVersion = 1

    var version = PCChatArchive.currentVersion
    var entries: [PCChatEntry]

    var data: Data? { try? JSONEncoder().encode(self) }

    /// What was saved, or nothing when it cannot be read (another version, a damaged file).
    static func entries(from data: Data) -> [PCChatEntry] {
        guard let archive = try? JSONDecoder().decode(PCChatArchive.self, from: data),
              archive.version == currentVersion else { return [] }
        return archive.entries
    }
}

/// What the paired computer has said, as the screens need it. A value: PCLink holds one, the tests drive one.
struct PCLinkModel: Equatable, Sendable {
    var desktop = ""
    var os = ""
    var caps: Set<String> = []
    var sessions: [PCSession] = []
    /// One per pill at most, the newest first.
    var approvals: [PCApproval] = []
    var questions: [PCQuestion] = []
    var models: [PCChatModel] = []
    var chat: [PCChatEntry] = []
    /// The chat answer being written, if any.
    var running: String?
    /// The assistant on the computer (capability `assistant`), as it said last.
    var assistant: PCAssistant?
    var lastAssistantResult: PCRequestResult?
    /// Mochi's home (capability `home`), as it said last.
    var home: PCHome?
    var lastHomeResult: PCRequestResult?
    /// What Mochi wears on the computer (`prefs`). Kept only: Mochi on the iPhone wears nothing (no wardrobe here).
    var outfit: String?
    /// The plan usage (capability `usage`); nil: nothing to show.
    var usage: PCUsage?
    /// The service cards (capability `services`).
    var services: [PCServiceCard] = []
    /// The diff being looked at (capability `diffs`): asked for with `wantDiff`, filled as its parts come.
    var diff: PCDiff?

    static let lostText = "The link to the computer was lost."

    enum Effect: Equatable, Sendable {
        /// A request the user had not seen yet: worth a notification when the app is not on screen.
        case newApproval(PCApproval)
        case newQuestion(PCQuestion)
        /// Answered or timed out: its notification goes.
        case resolved(String)
    }

    var chatAllowed: Bool { caps.contains("chat") }
    var handsAllowed: Bool { caps.contains("chat") && caps.contains("agent") }

    @discardableResult
    mutating func apply(_ message: PCIncoming, now: Date) -> [Effect] {
        switch message {
        case .welcome(let desktop, let os, let caps):
            self.desktop = desktop
            self.os = os
            self.caps = caps
            if !caps.contains("chat") { models = [] }
            // What the computer stopped offering goes from the screens; what it still offers stays until it says more.
            if !caps.contains("assistant") { assistant = nil }
            if !caps.contains("home") { home = nil }
            if !caps.contains("usage") { usage = nil }
            if !caps.contains("services") { services = [] }
            if !caps.contains("diffs") { diff = nil }
            return []
        case .sessions(let list):
            sessions = list
            return []
        case .approval(var approval):
            let known = approvals.first { $0.fingerprint == approval.fingerprint }
            approval.receivedAt = known?.receivedAt ?? now
            // A pill waits on one request at a time: a new one replaces the one before.
            let replaced = approvals.filter { $0.pillId == approval.pillId && $0.fingerprint != approval.fingerprint }
            approvals.removeAll { $0.pillId == approval.pillId || $0.fingerprint == approval.fingerprint }
            approvals.insert(approval, at: 0)
            return replaced.map { .resolved($0.fingerprint) } + (known == nil ? [.newApproval(approval)] : [])
        case .question(var question):
            let known = questions.first { $0.fingerprint == question.fingerprint }
            question.receivedAt = known?.receivedAt ?? now
            let replaced = questions.filter { $0.pillId == question.pillId && $0.fingerprint != question.fingerprint }
            questions.removeAll { $0.pillId == question.pillId || $0.fingerprint == question.fingerprint }
            questions.insert(question, at: 0)
            return replaced.map { .resolved($0.fingerprint) } + (known == nil ? [.newQuestion(question)] : [])
        case .resolved(let fingerprint):
            let had = approvals.contains { $0.fingerprint == fingerprint } || questions.contains { $0.fingerprint == fingerprint }
            approvals.removeAll { $0.fingerprint == fingerprint }
            questions.removeAll { $0.fingerprint == fingerprint }
            return had ? [.resolved(fingerprint)] : []
        case .chatModels(let list):
            models = list
            return []
        case .chatDelta(let id, let text):
            update(id) { $0.text += text }
            return []
        case .chatDone(let id, let text):
            update(id) {
                if let text { $0.text = text }
                $0.finished = true
            }
            if running == id { running = nil }
            return []
        case .chatError(let id, let reason, let message):
            update(id) {
                $0.text = Self.chatErrorText(reason: reason, message: message)
                $0.failed = true
                $0.finished = true
            }
            if running == id { running = nil }
            return []
        case .chatStep(let id, let step):
            update(id) { entry in
                entry.steps.removeAll { $0.index == step.index }
                entry.steps.append(step)
                entry.steps.sort { $0.index < $1.index }
            }
            return []
        case .assistant(let assistant):
            self.assistant = assistant
            return []
        case .assistantResult(let result):
            lastAssistantResult = result
            return []
        case .home(let home):
            self.home = home
            return []
        case .homeResult(let result):
            lastHomeResult = result
            return []
        case .prefs(let outfit):
            self.outfit = outfit
            return []
        case .usage(let usage):
            self.usage = usage.isEmpty ? nil : usage
            return []
        case .services(let list):
            services = list
            return []
        case .diff(let part):
            merge(part)
            return []
        case .error, .pong, .remote, .other:
            return []
        }
    }

    /// The link closed. What still waits on the computer comes again with the next connection (its notification
    /// stays meanwhile); an answer being written stopped there when the phone left.
    mutating func connectionLost() {
        approvals = []
        questions = []
        if let id = running {
            update(id) {
                $0.text = $0.text.isEmpty ? Self.lostText : $0.text
                $0.failed = true
                $0.finished = true
            }
            running = nil
        }
    }

    /// The chat kept on this iPhone, back after a launch (the newest 100 messages). An answer that was being written
    /// when Coucou closed ends there, as when the link closes.
    mutating func restoreChat(_ entries: [PCChatEntry]) {
        chat = Array(entries.suffix(100))
        running = nil
        for index in chat.indices where chat[index].role == .mochi && !chat[index].finished {
            if chat[index].text.isEmpty { chat[index].text = Self.lostText }
            chat[index].failed = true
            chat[index].finished = true
        }
    }

    /// The lines of this file are wanted: its parts are kept as they come, any other file's are not.
    mutating func wantDiff(pillId: String, fileId: Int) {
        diff = PCDiff(pillId: pillId, fileId: fileId)
    }

    /// Its sheet closed: the lines are forgotten (nothing is kept on the iPhone).
    mutating func closeDiff(pillId: String, fileId: Int) {
        if diff?.pillId == pillId, diff?.fileId == fileId { diff = nil }
    }

    private mutating func merge(_ part: PCDiffPart) {
        guard var current = diff, current.pillId == part.pillId, current.fileId == part.fileId else { return }
        if current.parts != part.parts {
            current.parts = part.parts
            current.received = [:]
        }
        current.name = part.name
        current.added = part.added
        current.removed = part.removed
        current.tooLarge = part.tooLarge
        current.gone = part.gone
        current.truncated = part.truncated
        current.received[part.part] = part.lines
        diff = current
    }

    /// Requests this iPhone got more than 2 minutes ago: the computer has given up on them.
    @discardableResult
    mutating func expire(now: Date) -> [Effect] {
        let isOld = { (received: Date) in now.timeIntervalSince(received) >= PCWire.approvalLifetime }
        let old = approvals.filter { isOld($0.receivedAt) }.map(\.fingerprint) + questions.filter { isOld($0.receivedAt) }.map(\.fingerprint)
        approvals.removeAll { isOld($0.receivedAt) }
        questions.removeAll { isOld($0.receivedAt) }
        return old.map { Effect.resolved($0) }
    }

    /// A message sent from the chat: the user's line and Mochi's answer to come.
    mutating func startChat(id: String, text: String, hands: Bool) {
        chat.append(PCChatEntry(id: "you-\(id)", role: .you, text: text))
        chat.append(PCChatEntry(id: id, role: .mochi, text: "", hands: hands))
        // At most 100 messages on screen, the newest.
        if chat.count > 100 { chat.removeFirst(chat.count - 100) }
        running = id
    }

    mutating func resetChat() {
        chat = []
        running = nil
    }

    private mutating func update(_ id: String, _ change: (inout PCChatEntry) -> Void) {
        guard !id.isEmpty, let index = chat.lastIndex(where: { $0.id == id && $0.role == .mochi }) else { return }
        change(&chat[index])
    }

    /// The computer's fixed sentence for a failure, with a hint where the iPhone can say more.
    static func chatErrorText(reason: String, message: String) -> String {
        switch reason {
        case "hands_off":
            return "Mochi's hands are off on the computer. Turn on Settings → Mochi's hands, and Settings → Phone → \"Let the phone use Mochi's hands\"."
        case "off":
            return "The chat is off on the computer: Settings → Phone → \"Let the phone chat with my AI providers\"."
        case "not_allowed":
            return "The computer does not let the phone use this model. Pick another one, or tick it in Settings → Phone."
        default:
            return message.isEmpty ? "Something went wrong on the computer." : message
        }
    }

    /// A chat id the computer accepts: 16 letters and digits.
    static func newChatID() -> String {
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
        return String((0..<16).map { _ in alphabet.randomElement() ?? "x" })
    }
}
