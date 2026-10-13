import Foundation

// The iPhone's link to a Windows or Linux computer (NotchBuddy/Sources/Phone/PCLinkWire.swift): the pairing link,
// the lines both ways (as windows/src-tauri/src/phone_link writes and reads them) and what they do to the screens.
// Foundation only: runs on macOS and Linux (scripts/test-pc-link.sh).

@main
enum PCLinkWireTests {

    static var failures = 0

    static func check(_ label: String, _ value: Bool) {
        if value { print("  ✓ \(label)") }
        else      { print("  ✗ \(label)"); failures += 1 }
    }

    static func checkEqual<T: Equatable>(_ label: String, _ got: T, _ expected: T) {
        if got == expected { print("  ✓ \(label)") }
        else { print("  ✗ \(label)  got: \(got)  expected: \(expected)"); failures += 1 }
    }

    static func object(_ data: Data) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }

    static func line(_ json: String) -> PCIncoming? { PCIncoming.decode(Data(json.utf8)) }

    static let fp = String(repeating: "ab", count: 32)
    static let t0 = Date(timeIntervalSince1970: 1_800_000_000)

    static func main() {

        // ── Pairing link ──────────────────────────────────────────────────────
        print("Pairing link")
        do {
            // The link windows/src-tauri/src/phone_link/pairing.rs builds in its own test.
            let link = "coucou://pair?v=1&host=192.168.1.20&port=47821&fp=\(fp.uppercased())&token=tok_en-1234567890abcd&name=L%C3%A9a%27s%20PC%20%26%20co"
            let p = PCPairing.parse(link)
            check("parses", p != nil)
            checkEqual("host", p?.computer.host, "192.168.1.20")
            checkEqual("port", p?.computer.port, 47821)
            checkEqual("fingerprint in lowercase", p?.computer.fingerprint, fp)
            checkEqual("token", p?.token, "tok_en-1234567890abcd")
            checkEqual("name decoded", p?.computer.name, "Léa's PC & co")
            checkEqual("short fingerprint", p?.computer.shortFingerprint, String(fp.prefix(16)))
            check("spaces around are fine", PCPairing.parse("  \(link)\n") != nil)
            // The relay's fields are for Android; the iPhone keeps the local network part.
            let relay = link + "&relay=wss%3A%2F%2Fr.example&room=abc&key=k&access=a"
            checkEqual("relay fields ignored", PCPairing.parse(relay)?.computer.host, "192.168.1.20")
            let noName = "coucou://pair?v=1&host=pc.local&port=1&fp=\(fp)&token=abcdefghijklmnop"
            checkEqual("no name: empty", PCPairing.parse(noName)?.computer.name, "")
            checkEqual("no name: a plain word", PCPairing.parse(noName)?.computer.displayName, "your computer")
            checkEqual("IPv6 in brackets", PCPairing.parse("coucou://pair?v=1&host=%5Bfe80%3A%3A1%5D&port=9&fp=\(fp)&token=abcdefghijklmnop")?.computer.host, "fe80::1")
            let bad = [
                "https://pair?v=1&host=a&port=1&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://mochi/x",
                "coucou://pair?v=2&host=a&port=1&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://pair?v=1&port=1&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a%20b&port=1&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a&port=0&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a&port=70000&fp=\(fp)&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a&port=1&fp=abcd&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a&port=1&fp=\(String(repeating: "zz", count: 32))&token=abcdefghijklmnop",
                "coucou://pair?v=1&host=a&port=1&fp=\(fp)&token=short",
                "coucou://pair?v=1&host=a&port=1&fp=\(fp)&token=abcdefghijklmnop!",
                "",
            ]
            for (i, text) in bad.enumerated() { check("refused #\(i)", PCPairing.parse(text) == nil) }
            // A name is one short line.
            let longName = "coucou://pair?v=1&host=a&port=1&fp=\(fp)&token=abcdefghijklmnop&name=\(String(repeating: "x", count: 90))%0Ay"
            checkEqual("name: 64 characters, one line", PCPairing.parse(longName)?.computer.name.count, 64)
        }

        // ── Lines from the iPhone ─────────────────────────────────────────────
        print("Lines from the iPhone")
        do {
            let hello = PCOutgoing.hello(token: "abcdefghijklmnop", device: "iPhone", caps: PCWire.capabilities).line
            check("ends with a newline", hello.last == 0x0A)
            let h = object(hello)
            checkEqual("hello type", h["type"] as? String, "hello")
            checkEqual("hello v", h["v"] as? Int, 1)
            checkEqual("hello token", h["token"] as? String, "abcdefghijklmnop")
            checkEqual("hello caps", h["caps"] as? [String], ["chat", "agent", "details", "answers", "remote"])
            let allow = object(PCOutgoing.decision(fingerprint: "f1", allow: true).line)
            checkEqual("allow", allow["decision"] as? String, "allow")
            checkEqual("deny", object(PCOutgoing.decision(fingerprint: "f1", allow: false).line)["decision"] as? String, "deny")
            let send = object(PCOutgoing.chatSend(id: "abc", model: "anthropic/claude", text: "hi", agent: true).line)
            checkEqual("chatSend with hands", send["agent"] as? Bool, true)
            checkEqual("chatSend model", send["model"] as? String, "anthropic/claude")
            let plain = object(PCOutgoing.chatSend(id: "abc", model: "m", text: "hi", agent: false).line)
            check("chatSend without hands has no agent field", plain["agent"] == nil)
            let answer = object(PCOutgoing.answer(fingerprint: "q1", picks: [["A"], ["B", "C"]]).line)
            checkEqual("answer picks", answer["picks"] as? [[String]], [["A"], ["B", "C"]])
            checkEqual("ping", object(PCOutgoing.ping.line)["type"] as? String, "ping")
            check("one line each", !String(decoding: PCOutgoing.chatSend(id: "a", model: "m", text: "two\nlines", agent: false).line.dropLast(), as: UTF8.self).contains("\n"))
            let id = PCLinkModel.newChatID()
            check("chat id: 16 letters and digits", id.count == 16 && id.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber) })
        }

        // ── Lines from the computer ───────────────────────────────────────────
        print("Lines from the computer")
        do {
            checkEqual("welcome", line(#"{"type":"welcome","v":1,"desktop":"Léa's PC","os":"windows","caps":["chat","agent"]}"#),
                       .welcome(desktop: "Léa's PC", os: "windows", caps: ["chat", "agent"]))
            checkEqual("welcome without caps", line(#"{"type":"welcome","v":1,"desktop":"PC","os":"linux"}"#),
                       .welcome(desktop: "PC", os: "linux", caps: []))
            checkEqual("auth error", line(#"{"type":"error","code":"auth","message":"bad token or version"}"#),
                       .error(code: "auth", message: "bad token or version"))
            // The v1 fields, then the details.
            let sessions = line(##"{"type":"sessions","sessions":[{"agent":"Claude Code","pillId":"integration_claude","state":"working","statusText":"Bash · npm test","stepCount":4,"stepIndex":2,"updatedAt":1800000000000},{"agent":"Mochi","pillId":"agent_mochi","state":"approval","statusText":"Edit · a.rs","stepCount":1,"stepIndex":0,"updatedAt":1800000001000,"steps":["Read · a.rs","Edit · a.rs"],"project":"coucou","color":"#A5B4FC","finalLine":"Done"},{"pillId":""},{"agent":"x","pillId":"agent_codex","state":"idle","statusText":"","stepCount":0,"stepIndex":0,"updatedAt":1,"color":"red"}]}"##)
            if case .sessions(let list)? = sessions {
                checkEqual("sessions: the one without a pill is dropped", list.count, 3)
                checkEqual("pill", list[0].pillId, "integration_claude")
                checkEqual("status", list[0].statusText, "Bash · npm test")
                checkEqual("updatedAt in ms", list[0].updatedAt, t0)
                checkEqual("steps", list[1].steps, ["Read · a.rs", "Edit · a.rs"])
                checkEqual("project", list[1].project, "coucou")
                checkEqual("color", list[1].color, "#A5B4FC")
                checkEqual("final line", list[1].finalLine, "Done")
                checkEqual("a colour that is not #RRGGBB is dropped", list[2].color, "")
            } else {
                check("sessions decoded", false)
            }
            let approval = line(#"{"type":"approval","pillId":"agent_mochi","fingerprint":"f1","tool":"Bash","command":"npm test","truncated":false,"createdAt":1800000000000}"#)
            checkEqual("approval", approval, .approval(PCApproval(pillId: "agent_mochi", fingerprint: "f1", tool: "Bash", command: "npm test", createdAt: t0, truncated: false)))
            if case .approval(let a)? = line(#"{"type":"approval","pillId":"p","fingerprint":"f2","tool":"Bash","command":"x","truncated":true,"createdAt":1}"#) {
                check("truncated", a.truncated)
            } else {
                check("truncated approval decoded", false)
            }
            checkEqual("approval without a fingerprint", line(#"{"type":"approval","pillId":"p","tool":"Bash","command":"x"}"#), .other)
            checkEqual("resolved", line(#"{"type":"approvalResolved","fingerprint":"f1"}"#), .resolved(fingerprint: "f1"))
            let question = line(#"{"type":"question","pillId":"integration_claude","fingerprint":"q1","createdAt":1800000000000,"questions":[{"question":"Which branch?","multiSelect":false,"options":[{"label":"main","description":"the default"},{"label":"dev","description":""}]}]}"#)
            if case .question(let q)? = question {
                checkEqual("question text", q.items.first?.question, "Which branch?")
                checkEqual("options", q.items.first?.options.map(\.label), ["main", "dev"])
                checkEqual("option description", q.items.first?.options.first?.description, "the default")
            } else {
                check("question decoded", false)
            }
            checkEqual("a question with no option is left to the computer",
                       line(#"{"type":"question","pillId":"p","fingerprint":"q2","createdAt":1,"questions":[{"question":"?","multiSelect":false,"options":[]}]}"#), .other)
            checkEqual("models", line(#"{"type":"chatModels","models":[{"id":"anthropic/claude-opus-5","provider":"anthropic","label":"Claude Opus 5"},{"id":""}]}"#),
                       .chatModels([PCChatModel(id: "anthropic/claude-opus-5", provider: "anthropic", label: "Claude Opus 5")]))
            checkEqual("step", line(#"{"type":"chatStep","id":"c1","index":2,"status":"running","text":"Bash · npm test"}"#),
                       .chatStep(id: "c1", step: PCChatStep(index: 2, status: "running", text: "Bash · npm test")))
            checkEqual("done with the full text", line(#"{"type":"chatDone","id":"c1","text":"All green."}"#), .chatDone(id: "c1", text: "All green."))
            checkEqual("done", line(#"{"type":"chatDone","id":"c1"}"#), .chatDone(id: "c1", text: nil))
            checkEqual("chat error", line(#"{"type":"chatError","id":"c1","reason":"busy","message":"Busy."}"#), .chatError(id: "c1", reason: "busy", message: "Busy."))
            checkEqual("pong", line(#"{"type":"pong"}"#), .pong)
            checkEqual("unknown type", line(#"{"type":"usage","claude":{}}"#), .other)
            check("not JSON", line("hello") == nil)
            check("not an object", line("[1,2]") == nil)
        }

        // ── Cutting the stream into lines ─────────────────────────────────────
        print("Line reader")
        do {
            var reader = PCLineReader()
            checkEqual("half a line: nothing yet", reader.feed(Data(#"{"type":"po"#.utf8))?.count, 0)
            let lines = reader.feed(Data("ng\"}\r\n{\"type\":\"pong\"}\n\n{\"ty".utf8))
            checkEqual("two lines, \\r dropped, blank skipped", lines?.map { String(decoding: $0, as: UTF8.self) },
                       [#"{"type":"pong"}"#, #"{"type":"pong"}"#])
            checkEqual("the rest waits", reader.feed(Data("pe\":\"pong\"}\n".utf8))?.count, 1)
            var long = PCLineReader()
            check("a line over 64 KiB breaks the link", long.feed(Data(repeating: 0x41, count: PCWire.maxLineBytes + 1)) == nil)
            var fits = PCLineReader()
            var exact = Data(repeating: 0x41, count: PCWire.maxLineBytes)
            exact.append(0x0A)
            checkEqual("64 KiB exactly is fine", fits.feed(exact)?.count, 1)
        }

        // ── What the screens show ─────────────────────────────────────────────
        print("Requests")
        do {
            var model = PCLinkModel()
            let a1 = PCApproval(pillId: "integration_claude", fingerprint: "f1", tool: "Bash", command: "rm -rf build", createdAt: t0, truncated: false)
            var effects = model.apply(.approval(a1), now: t0)
            checkEqual("a new request is news", effects, [.newApproval(PCApproval(pillId: "integration_claude", fingerprint: "f1", tool: "Bash", command: "rm -rf build", createdAt: t0, truncated: false, receivedAt: t0))])
            effects = model.apply(.approval(a1), now: t0.addingTimeInterval(30))
            checkEqual("the same one again (a reconnection) is not", effects, [])
            checkEqual("received when first seen", model.approvals.first?.receivedAt, t0)
            let a2 = PCApproval(pillId: "integration_claude", fingerprint: "f2", tool: "Bash", command: "ls", createdAt: t0, truncated: false)
            effects = model.apply(.approval(a2), now: t0.addingTimeInterval(40))
            checkEqual("a new one for the same pill replaces it", effects.first, .resolved("f1"))
            checkEqual("one per pill", model.approvals.map(\.fingerprint), ["f2"])
            let mochi = PCApproval(pillId: "agent_mochi", fingerprint: "m1", tool: "Edit", command: "a.rs", createdAt: t0, truncated: false)
            model.apply(.approval(mochi), now: t0.addingTimeInterval(41))
            checkEqual("Mochi's next to Claude Code's", model.approvals.map(\.fingerprint), ["m1", "f2"])
            checkEqual("answered elsewhere", model.apply(.resolved(fingerprint: "f2"), now: t0.addingTimeInterval(50)), [.resolved("f2")])
            checkEqual("unknown fingerprint: nothing", model.apply(.resolved(fingerprint: "zz"), now: t0), [])
            checkEqual("not expired at 119 s", model.expire(now: t0.addingTimeInterval(41 + 119)), [])
            checkEqual("expired at 120 s", model.expire(now: t0.addingTimeInterval(41 + 120)), [.resolved("m1")])
            check("gone", model.approvals.isEmpty)
            let q = PCQuestion(pillId: "integration_claude", fingerprint: "q1", createdAt: t0,
                               items: [PCQuestion.Item(question: "Which?", multiSelect: false, options: [PCQuestion.Option(label: "a", description: "")])])
            if case .newQuestion(let got)? = model.apply(.question(q), now: t0).first {
                checkEqual("a question is news", got.fingerprint, "q1")
            } else {
                check("question effect", false)
            }
            model.apply(.approval(a1), now: t0)
            model.connectionLost()
            check("the link closed: they come again with the next one", model.approvals.isEmpty && model.questions.isEmpty)
        }

        print("Chat")
        do {
            var model = PCLinkModel()
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["chat"]), now: t0)
            check("chat without hands", model.chatAllowed && !model.handsAllowed)
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["chat", "agent"]), now: t0)
            check("hands with the chat", model.handsAllowed)
            model.startChat(id: "c1", text: "run the tests", hands: true)
            checkEqual("your line, then Mochi's", model.chat.map(\.role), [.you, .mochi])
            checkEqual("running", model.running, "c1")
            model.apply(.chatStep(id: "c1", step: PCChatStep(index: 0, status: "running", text: "Bash · npm test")), now: t0)
            model.apply(.chatStep(id: "c1", step: PCChatStep(index: 1, status: "waiting", text: "Edit · a.rs")), now: t0)
            model.apply(.chatStep(id: "c1", step: PCChatStep(index: 0, status: "ok", text: "Bash · npm test")), now: t0)
            checkEqual("a step's later state replaces it", model.chat.last?.steps.map(\.status), ["ok", "waiting"])
            model.apply(.chatDelta(id: "c1", text: "All "), now: t0)
            model.apply(.chatDelta(id: "c1", text: "green."), now: t0)
            checkEqual("pieces add up", model.chat.last?.text, "All green.")
            model.apply(.chatDone(id: "c1", text: nil), now: t0)
            check("finished", model.chat.last?.finished == true && model.running == nil)
            model.apply(.chatDelta(id: "other", text: "stray"), now: t0)
            checkEqual("another id changes nothing", model.chat.last?.text, "All green.")

            model.startChat(id: "c2", text: "again", hands: true)
            model.apply(.chatError(id: "c2", reason: "hands_off", message: "Mochi's hands are off on the computer."), now: t0)
            check("hands off says where to turn them on", model.chat.last?.failed == true && model.chat.last?.text.contains("Settings → Mochi's hands") == true)
            model.startChat(id: "c3", text: "and again", hands: false)
            model.apply(.chatDelta(id: "c3", text: "Half"), now: t0)
            model.connectionLost()
            check("the link closed mid-answer: it ends, text kept", model.chat.last?.failed == true && model.chat.last?.text == "Half" && model.running == nil)
            model.startChat(id: "c4", text: "x", hands: false)
            model.connectionLost()
            checkEqual("nothing written yet: says why", model.chat.last?.text, "The link to the computer was lost.")
            for i in 0..<60 { model.startChat(id: "n\(i)", text: "x", hands: false) }
            checkEqual("at most 100 messages", model.chat.count, 100)
            model.resetChat()
            check("new chat", model.chat.isEmpty && model.running == nil)
        }

        print("\nAn answer read aloud")
        do {
            checkEqual("marks, code and link addresses are left out",
                       PCText.spoken("**Pronto!** A TV da sala está `ligada`.\n\n- YouTube aberto\n```\nnpm test\n```\nVeja [o guia](https://example.com)."),
                       "Pronto! A TV da sala está ligada. YouTube aberto. Veja o guia.")
            checkEqual("a title and a quote are sentences", PCText.spoken("# Feito\n> nada mais"), "Feito. nada mais.")
            checkEqual("a long answer stops after a sentence",
                       PCText.spoken("A TV está ligada. O YouTube está aberto na tela da sala agora mesmo. " + String(repeating: "x", count: 200), max: 80),
                       "A TV está ligada. O YouTube está aberto na tela da sala agora mesmo.")
            checkEqual("or is cut", PCText.spoken(String(repeating: "y", count: 30), max: 10), "yyyyyyyyyy…")
            checkEqual("nothing to say", PCText.spoken("```\ncode\n```"), "")
        }

        print("\nAway from home: the computer's addresses on the internet")
        do {
            check("this iPhone asks for them", PCWire.capabilities.contains("remote"))
            // As windows/src-tauri/src/phone_link/hub.rs writes it (remote_line).
            let told = line(#"{"type":"remote","endpoints":["[2804:14c:1:2::10]:47821","198.51.100.7:47822"]}"#)
            checkEqual("decoded, IPv6 first as sent", told, .remote(["[2804:14c:1:2::10]:47821", "198.51.100.7:47822"]))
            checkEqual("none: the switch is off on the computer", line(#"{"type":"remote","endpoints":[]}"#), .remote([]))
            checkEqual("no list at all: ignored", line(#"{"type":"remote"}"#), .other)
            let noisy = line(#"{"type":"remote","endpoints":["pc.example.com:47821","198.51.100.7:47822","198.51.100.7:47822",5,"[2804::1]:0","1.2.3.4:99999","10.0.0.1:47821","[fe80::1%en0]:47821","1.2.3:4","256.1.1.1:5","[2804::2]:+80"]}"#)
            checkEqual("addresses only, each once, nothing malformed", noisy, .remote(["198.51.100.7:47822", "10.0.0.1:47821"]))
            let many = (1...9).map { "198.51.100.\($0):47821" }
            checkEqual("at most six", PCRemote.clean(many).count, PCRemote.maxEndpoints)
            check("host and port of an IPv6 one", PCRemote.parse("[2804:14c:1:2::10]:47821").map { $0.host == "2804:14c:1:2::10" && $0.port == 47821 } == true)
            check("host and port of an IPv4 one", PCRemote.parse("198.51.100.7:47822").map { $0.host == "198.51.100.7" && $0.port == 47822 } == true)
            check("a name is never taken", PCRemote.parse("evil.example:443") == nil)
            check("an IPv6 without brackets is not taken", PCRemote.parse("2804::1:47821") == nil)

            var model = PCLinkModel()
            let effects = model.apply(.remote(["198.51.100.7:47822"]), now: t0)
            check("the screens' state is not touched", effects.isEmpty && model == PCLinkModel())

            // A pairing kept before this existed still loads, with no address outside.
            let old = Data(#"{"host":"192.168.1.20","port":47821,"fingerprint":"\#(fp)","name":"PC"}"#.utf8)
            let loaded = try? JSONDecoder().decode(PCComputer.self, from: old)
            checkEqual("an older saved pairing loads", loaded?.host, "192.168.1.20")
            checkEqual("with no address outside", loaded?.remote, [])
            var computer = PCComputer(host: "192.168.1.20", port: 47821, fingerprint: fp, name: "PC")
            computer.remote = ["198.51.100.7:47822"]
            let again = (try? JSONEncoder().encode(computer)).flatMap { try? JSONDecoder().decode(PCComputer.self, from: $0) }
            checkEqual("kept across launches", again, computer)
            let tampered = Data(#"{"host":"h","port":1,"fingerprint":"\#(fp)","name":"","remote":["x.example:1","198.51.100.7:1"]}"#.utf8)
            checkEqual("what was saved is checked again", (try? JSONDecoder().decode(PCComputer.self, from: tampered))?.remote, ["198.51.100.7:1"])
        }

        print(failures == 0 ? "\nAll PC link wire tests passed." : "\n\(failures) failure(s).")
        if failures > 0 { exit(1) }
    }
}
