import Foundation

// The iPhone's half of the phone link to Coucou on a Windows or Linux computer (docs/ANDROID_LINK.md, protocol v1).
// iCloud only joins the iPhone to a Mac; a PC is reached directly: TLS on the local network, the certificate pinned
// by the pairing QR code, one JSON object per line. Away from home, the same TLS goes to the addresses the computer
// gave over the link (`remote`: its router opened a port for it), with no server of anyone in between. This file is Foundation only: the pairing link, the lines both
// ways, and what they do to the screens' state, so tests/PCLinkWireTests.swift runs anywhere
// (scripts/test-pc-link.sh). PCLink.swift adds the connection, the Keychain and the notifications.

enum PCWire {
    static let version = 1
    /// What this iPhone understands. The computer offers each one only while its own switch is on.
    static let capabilities = ["chat", "agent", "details", "answers", "remote"]
    static let maxLineBytes = 64 * 1024
    /// An approval is offered for 2 minutes at most (the computer gives up at 115 s).
    static let approvalLifetime: TimeInterval = 120
    /// The computer refuses a longer chat message.
    static let maxChatText = 4000
    /// Keep-alive, as the other phones do.
    static let pingInterval: TimeInterval = 20
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

    private enum CodingKeys: String, CodingKey {
        case host, port, fingerprint, name, remote
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
/// `coucou://pair?v=1&host=…&port=…&fp=…&token=…&name=…`. Relay fields, if any, are ignored: the iPhone uses the
/// local network only.
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
        return PCPairing(computer: PCComputer(host: host, port: port, fingerprint: fingerprint, name: name), token: token)
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
        }
        var data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
        data.append(0x0A)
        return data
    }
}

// MARK: - Lines from the computer

/// An agent session on the computer (`sessions[]`). `steps`, `finalLine`, `project` and `color` come only with the
/// `details` capability.
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
struct PCChatStep: Equatable, Sendable {
    var index: Int
    /// running, waiting, ok, error or denied.
    var status: String
    var text: String
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
    /// Anything this iPhone does not use (prefs, usage, diffs, services…).
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
            color: isColor(color) ? color : ""
        )
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
struct PCChatEntry: Identifiable, Equatable, Sendable {
    enum Role: Equatable, Sendable { case you, mochi }

    let id: String
    let role: Role
    var text: String
    /// What Mochi did on the computer in this turn (his hands), in order.
    var steps: [PCChatStep] = []
    var failed = false
    var finished = false
    /// The turn was sent with Mochi's hands.
    var hands = false
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
                $0.text = $0.text.isEmpty ? "The link to the computer was lost." : $0.text
                $0.failed = true
                $0.finished = true
            }
            running = nil
        }
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
