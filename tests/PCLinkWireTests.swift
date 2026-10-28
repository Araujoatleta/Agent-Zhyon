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
            checkEqual("hello caps", h["caps"] as? [String],
                       ["chat", "agent", "details", "answers", "remote", "prefs", "usage", "diffs", "services", "assistant", "home"])
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
            checkEqual("unknown type", line(#"{"type":"weather","claude":{}}"#), .other)
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
            check("a Tailscale address is known", PCRemote.isTailnet("100.101.102.103:47821") && PCRemote.isTailnet("100.64.0.1:1"))
            check("others are not Tailscale's", !PCRemote.isTailnet("100.128.0.1:47821") && !PCRemote.isTailnet("100.63.0.1:47821")
                && !PCRemote.isTailnet("198.51.100.7:47822") && !PCRemote.isTailnet("[2804::1]:47821") && !PCRemote.isTailnet("nonsense"))

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

        print("\nThe assistant: name, tone, brain, skills, ChatGPT sign-in (capability assistant)")
        do {
            let get = object(PCOutgoing.assistantGet.line)
            checkEqual("assistantGet", get["type"] as? String, "assistantGet")
            checkEqual("assistantGet says nothing else", get.count, 1)
            let set = object(PCOutgoing.assistantSet(id: "a1", name: "Zheus", persona: "custom", personaText: "Fala como um pirata.").line)
            checkEqual("assistantSet", [set["type"] as? String, set["id"] as? String, set["name"] as? String, set["persona"] as? String],
                       ["assistantSet", "a1", "Zheus", "custom"])
            checkEqual("assistantSet persona text", set["personaText"] as? String, "Fala como um pirata.")
            let onlyName = object(PCOutgoing.assistantSet(id: "a2", name: "Ana", persona: nil, personaText: nil).line)
            check("a field left as it is is not sent", onlyName["persona"] == nil && onlyName["personaText"] == nil && onlyName["name"] as? String == "Ana")
            let brain = object(PCOutgoing.assistantBrain(id: "a3", brain: "chatgpt").line)
            checkEqual("assistantBrain", [brain["type"] as? String, brain["id"] as? String, brain["brain"] as? String], ["assistantBrain", "a3", "chatgpt"])
            checkEqual("assistantSkills", object(PCOutgoing.assistantSkills(id: "a4", skills: ["search", "news"]).line)["skills"] as? [String], ["search", "news"])
            checkEqual("assistantSkills: none wanted", object(PCOutgoing.assistantSkills(id: "a5", skills: []).line)["skills"] as? [String], [])
            let login = object(PCOutgoing.assistantLogin(id: "a6").line)
            checkEqual("assistantLogin", [login["type"] as? String, login["id"] as? String], ["assistantLogin", "a6"])
            checkEqual("assistantLoginCancel", object(PCOutgoing.assistantLoginCancel(id: "a7").line)["type"] as? String, "assistantLoginCancel")
            let gateway = object(PCOutgoing.assistantGateway(id: "a8", on: false).line)
            checkEqual("assistantGateway", gateway["type"] as? String, "assistantGateway")
            checkEqual("assistantGateway off", gateway["on"] as? Bool, false)
            checkEqual("assistantGateway on", object(PCOutgoing.assistantGateway(id: "a9", on: true).line)["on"] as? Bool, true)

            // As docs/ANDROID_LINK.md shows it (capability `assistant`).
            let full = line(#"{"type":"assistant","name":"Zheus","persona":"friendly","personaText":"","personas":["friendly","direct","professional","playful","butler","custom"],"brain":"chatgpt","brains":[{"id":"chatgpt","name":"ChatGPT","ready":true,"login":true},{"id":"openai","name":"OpenAI","ready":false,"login":false}],"skills":["search"],"skillPacks":["home","search","news"],"house":true,"hermes":{"installed":true,"managed":true,"gateway":"running","reason":"","setup":{"running":false,"step":"","done":0,"total":0,"error":""}},"login":{"state":"waiting","code":"ABCD-1234","url":"https://auth.openai.com/codex/device","expiresAt":1900000000000}}"#)
            if case .assistant(let a)? = full {
                checkEqual("name", a.name, "Zheus")
                checkEqual("persona", a.persona, "friendly")
                checkEqual("personas in the computer's order", a.personas, PCAssistant.knownPersonas)
                checkEqual("brain", a.brain, "chatgpt")
                checkEqual("brains", a.brains, [PCAssistant.Brain(id: "chatgpt", name: "ChatGPT", ready: true, login: true),
                                                PCAssistant.Brain(id: "openai", name: "OpenAI", ready: false, login: false)])
                checkEqual("the brain in use", a.current?.id, "chatgpt")
                checkEqual("skills wanted", a.skills, ["search"])
                checkEqual("packs", a.skillPacks, ["home", "search", "news"])
                checkEqual("the packs the phone may tick", a.optionalSkills, ["search", "news"])
                check("Mochi's home for Hermes, read only", a.house)
                check("installed and managed by Coucou: the brain can change", a.changeable)
                checkEqual("gateway", a.hermes.gateway, .running)
                checkEqual("sign-in waiting for its code", a.login,
                           .waiting(code: "ABCD-1234", url: "https://auth.openai.com/codex/device", expiresAt: Date(timeIntervalSince1970: 1_900_000_000)))
            } else {
                check("assistant decoded", false)
            }
            checkEqual("nothing said: the defaults", line(#"{"type":"assistant"}"#), .assistant(PCAssistant()))
            let messy = line(#"{"type":"assistant","name":"  Zheus the Great, assistant of the whole house\nand more  ","persona":"pirate mode!","personaText":"\#(String(repeating: "x", count: 1600))","personas":["friendly","friendly","bad id!",3],"brains":[{"id":"chatgpt","name":"ChatGPT"},{"name":"no id"},{"id":"chatgpt","name":"again"},{"id":"ollama","name":"","ready":true}],"skills":["home","search","search"],"hermes":{"installed":true,"managed":false,"gateway":"exploded","reason":"why","setup":{"running":true,"step":"Installing Hermes","done":9,"total":5}}}"#)
            if case .assistant(let a)? = messy {
                check("a name is one line of at most 40 characters", a.name.count <= 40 && !a.name.contains("\n") && a.name.hasPrefix("Zheus the Great"))
                checkEqual("a tone this iPhone cannot name: the default", a.persona, "friendly")
                checkEqual("a custom text of at most 1500 characters", a.personaText.count, 1500)
                checkEqual("ids only, each once", a.personas, ["friendly"])
                checkEqual("brains without an id, or twice, are dropped", a.brains.map(\.id), ["chatgpt", "ollama"])
                check("not said ready: not ready", a.brains.first?.ready == false && a.brains.first?.login == false)
                checkEqual("no name: its id", a.brains.last?.name, "ollama")
                checkEqual("Mochi's home is not a skill the phone ticks", a.skills, ["search"])
                check("installed but not by Coucou: nothing changes from here", !a.changeable)
                checkEqual("an unknown gateway state reads as off", a.hermes.gateway, .off)
                checkEqual("a reason only when it stopped", a.hermes.reason, "")
                checkEqual("setup progress kept in its bounds", [a.hermes.setup.done, a.hermes.setup.total], [5, 5])
                check("setup running", a.hermes.setup.running && a.hermes.setup.step == "Installing Hermes")
            } else {
                check("messy assistant decoded", false)
            }
            func signIn(_ json: String) -> PCAssistant.Login? {
                if case .assistant(let a)? = line(#"{"type":"assistant","login":"# + json + "}") { return a.login }
                return nil
            }
            checkEqual("signed in", signIn(#"{"state":"signedIn"}"#), .signedIn)
            checkEqual("signed out", signIn(#"{"state":"signedOut"}"#), .signedOut)
            checkEqual("failed, with why", signIn(#"{"state":"failed","reason":"Login failed: the code expired"}"#), .failed(reason: "Login failed: the code expired"))
            checkEqual("a page that is not OpenAI's is never kept", signIn(#"{"state":"waiting","code":"ABCD-1234","url":"https://evil.example/codex/device","expiresAt":1900000000000}"#),
                       .waiting(code: "ABCD-1234", url: PCAssistant.defaultSignInPage, expiresAt: Date(timeIntervalSince1970: 1_900_000_000)))
            checkEqual("nor one that only starts like it", signIn(#"{"state":"waiting","code":"WXYZ-0000","url":"https://auth.openai.com.evil.example/x"}"#),
                       .waiting(code: "WXYZ-0000", url: PCAssistant.defaultSignInPage, expiresAt: nil))
            checkEqual("waiting without a usable code says nothing", signIn(#"{"state":"waiting","code":"<b>hi</b>"}"#), .unknown)
            checkEqual("waiting without a code", signIn(#"{"state":"waiting"}"#), .unknown)
            checkEqual("an unknown state", signIn(#"{"state":"dancing"}"#), .unknown)
            check("OpenAI's sign-in page", PCAssistant.isSignInPage("https://auth.openai.com/codex/device") && PCAssistant.isSignInPage("https://auth.openai.com"))
            let notThePage = ["http://auth.openai.com/codex/device", "https://auth.openai.com.evil.example/", "https://auth.openai.com@evil.example/",
                              "https://evil.example/https://auth.openai.com/", "https://auth.openai.com/codex device", "javascript:alert(1)",
                              "https://auth.openai.com:8443/codex", "", "https://auth.openai.com\\@evil.example/"]
            for (i, page) in notThePage.enumerated() { check("never opened #\(i)", !PCAssistant.isSignInPage(page)) }
            checkEqual("a device code", PCAssistant.deviceCode(" ABCD-1234 "), "ABCD-1234")
            check("not a device code", PCAssistant.deviceCode("ABCD\n1234") == nil && PCAssistant.deviceCode("") == nil && PCAssistant.deviceCode(String(repeating: "A", count: 33)) == nil)
            checkEqual("time left", PCAssistant.secondsLeft(until: t0.addingTimeInterval(90), now: t0), 90)
            checkEqual("time left, rounded up", PCAssistant.secondsLeft(until: t0.addingTimeInterval(0.2), now: t0), 1)
            checkEqual("expired", PCAssistant.secondsLeft(until: t0, now: t0.addingTimeInterval(5)), 0)

            checkEqual("assistantResult ok", line(#"{"type":"assistantResult","id":"a1","ok":true}"#), .assistantResult(PCRequestResult(id: "a1", ok: true)))
            checkEqual("assistantResult refused",
                       line(#"{"type":"assistantResult","id":"a2","ok":false,"reason":"not_ready","message":"Coucou has no key or address for this AI: add it in Settings first."}"#),
                       .assistantResult(PCRequestResult(id: "a2", ok: false, reason: "not_ready", message: "Coucou has no key or address for this AI: add it in Settings first.")))
            checkEqual("an answer without its id answers nothing", line(#"{"type":"assistantResult","ok":true}"#), .other)
            checkEqual("nor with an id longer than 64", line(#"{"type":"assistantResult","id":"\#(String(repeating: "i", count: 65))","ok":true}"#), .other)
            if case .assistantResult(let r)? = line(#"{"type":"assistantResult","id":"a3","ok":false,"reason":"rm -rf /"}"#) {
                checkEqual("a reason that is no code is dropped", r.reason, "")
            } else {
                check("odd result decoded", false)
            }
            checkEqual("what this iPhone found itself", PCRequestResult.local("offline"), PCRequestResult(id: "", ok: false, reason: "offline"))

            var model = PCLinkModel()
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["assistant"]), now: t0)
            if case .assistant(let a)? = full { model.apply(.assistant(a), now: t0) }
            checkEqual("kept as said last", model.assistant?.name, "Zheus")
            model.apply(.assistantResult(PCRequestResult(id: "a1", ok: true)), now: t0)
            checkEqual("the last answer kept", model.lastAssistantResult?.id, "a1")
            model.connectionLost()
            check("the link closed: still shown", model.assistant != nil)
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["chat"]), now: t0)
            check("its switch turned off on the computer: it goes", model.assistant == nil)
        }

        print("\nMochi's home (capability home)")
        do {
            checkEqual("homeGet", object(PCOutgoing.homeGet.line)["type"] as? String, "homeGet")
            let control = object(PCOutgoing.homeControl(id: "h1", device: "ha:light.sala", action: "set_brightness", value: .number(40), confirmed: false).line)
            checkEqual("homeControl", [control["type"] as? String, control["id"] as? String, control["device"] as? String, control["action"] as? String],
                       ["homeControl", "h1", "ha:light.sala", "set_brightness"])
            checkEqual("a number", control["value"] as? Int, 40)
            checkEqual("confirmed is always said", control["confirmed"] as? Bool, false)
            let app = object(PCOutgoing.homeControl(id: "h2", device: "samsung:1", action: "open_app", value: .text("YouTube"), confirmed: true).line)
            checkEqual("a name", app["value"] as? String, "YouTube")
            checkEqual("confirmed after Face ID", app["confirmed"] as? Bool, true)
            check("no value, no field", object(PCOutgoing.homeControl(id: "h3", device: "d", action: "turn_on", value: nil, confirmed: false).line)["value"] == nil)
            let routine = object(PCOutgoing.homeRoutine(id: "h4", routine: "r1", confirmed: true).line)
            checkEqual("homeRoutine", [routine["type"] as? String, routine["id"] as? String, routine["routine"] as? String], ["homeRoutine", "h4", "r1"])
            checkEqual("homeRoutine confirmed", routine["confirmed"] as? Bool, true)

            // As docs/ANDROID_LINK.md shows it (capability `home`).
            let example = line(#"{"type":"home","enabled":true,"askAll":false,"devices":[{"id":"ha:light.sala","name":"Luz da sala","room":"Sala","kind":"light","source":"Home Assistant","caps":["turnOn","turnOff","brightness"],"apps":[],"channels":[{"number":"1","name":"Globo"}],"state":{"online":true,"on":true,"volume":null,"muted":null,"brightness":80,"app":null,"channel":null},"ask":false}],"routines":[{"id":"r1","name":"Boa noite","steps":["Luz da sala · Turn off"],"ask":true,"problems":[]}],"log":[{"at":"2026-10-13 21:05:12","origin":"phone","device":"TV da sala","action":"Turn on","ok":true,"detail":"On"}]}"#)
            if case .home(let home)? = example {
                check("on, without ask-all", home.enabled && !home.askAll)
                checkEqual("device", home.devices.first,
                           PCHome.Device(id: "ha:light.sala", name: "Luz da sala", room: "Sala", kind: "light", source: "Home Assistant",
                                         caps: ["turnOn", "turnOff", "brightness"], apps: [], channels: [PCHome.Channel(number: "1", name: "Globo")],
                                         state: PCHome.State(online: true, on: true, volume: nil, muted: nil, brightness: 80, app: nil, channel: nil),
                                         ask: false))
                checkEqual("routine", home.routines.first,
                           PCHome.Routine(id: "r1", name: "Boa noite", steps: ["Luz da sala · Turn off"], ask: true, problems: []))
                checkEqual("record", home.log.first,
                           PCHome.Entry(at: "2026-10-13 21:05:12", origin: "phone", device: "TV da sala", action: "Turn on", ok: true, detail: "On"))
            } else {
                check("home decoded", false)
            }
            let devices = (0..<130).map { #"{"id":"ha:light.\#($0)","name":"L\#($0)","kind":"light","caps":["turnOn"],"state":{"online":true}}"# }.joined(separator: ",")
            let apps = (0..<50).map { "\"App \($0)\"" }.joined(separator: ",")
            let channels = (0..<70).map { #"{"number":"\#($0)","name":"C\#($0)"}"# }.joined(separator: ",")
            let routines = (0..<60).map { #"{"id":"r\#($0)","name":"R\#($0)","steps":[],"ask":false,"problems":[]}"# }.joined(separator: ",")
            let log = (0..<40).map { #"{"at":"t","origin":"phone","device":"D","action":"A\#($0)","ok":true,"detail":""}"# }.joined(separator: ",")
            let big = line(#"{"type":"home","enabled":true,"devices":[{"id":"samsung:tv","name":"TV","kind":"tv","caps":["turnOn","apps","channels","teleport"],"apps":[\#(apps)],"channels":[\#(channels)],"state":{"online":true,"volume":150,"brightness":-3}},\#(devices)],"routines":[\#(routines)],"log":[\#(log)]}"#)
            if case .home(let home)? = big {
                checkEqual("at most 100 devices", home.devices.count, 100)
                checkEqual("at most 40 apps", home.devices.first?.apps.count, 40)
                checkEqual("at most 60 channels", home.devices.first?.channels.count, 60)
                checkEqual("a capability this iPhone does not know is not offered", home.devices.first?.caps, ["turnOn", "apps", "channels"])
                check("a percent out of range says nothing", home.devices.first?.state.volume == nil && home.devices.first?.state.brightness == nil)
                checkEqual("at most 50 routines", home.routines.count, 50)
                checkEqual("at most 30 lines of record", home.log.count, 30)
            } else {
                check("a big home decoded", false)
            }
            let odd = line(#"{"type":"home","enabled":true,"devices":[{"name":"no id"},{"id":"ha:a","name":"A","kind":"toaster","caps":["turnOn"],"state":{"online":true,"on":"yes"}},{"id":"ha:a","name":"twice"},{"id":" ha:b ","name":"spaces around"},{"id":"ha:c"}],"routines":[{"name":"no id"},{"id":"r1"}],"log":[{"at":"t"},{"device":"TV","action":"Turn on"}]}"#)
            if case .home(let home)? = odd {
                checkEqual("devices without an id, twice or not exactly as sent are dropped", home.devices.map(\.id), ["ha:a", "ha:c"])
                checkEqual("a kind this iPhone does not know", home.devices.first?.kind, "")
                check("a state that is no bool says nothing", home.devices.first?.state.on == nil)
                checkEqual("no name: its id", home.devices.last?.name, "ha:c")
                check("not said whether it asks first: it asks", home.devices.last?.ask == true)
                checkEqual("routines without an id are dropped", home.routines.map(\.id), ["r1"])
                check("a routine not saying whether it asks: it asks", home.routines.first?.ask == true)
                checkEqual("an empty line of record is dropped", home.log.map(\.device), ["TV"])
            } else {
                check("an odd home decoded", false)
            }

            func device(_ id: String, room: String = "", caps: [String] = [], apps: [String] = []) -> PCHome.Device {
                PCHome.Device(id: id, name: id, room: room, kind: "tv", source: "", caps: caps, apps: apps, channels: [], state: PCHome.State(), ask: false)
            }
            let rooms = PCHome(devices: [device("a", room: "Sala"), device("b"), device("c", room: "Quarto"), device("d", room: "Sala")]).rooms
            checkEqual("rooms in the computer's order, no room last", rooms.map(\.name), ["Sala", "Quarto", ""])
            checkEqual("devices by room", rooms.first?.devices.map(\.id), ["a", "d"])
            let tv = device("samsung:tv", caps: ["turnOn", "turnOff", "volumeSet", "volumeStep", "mute", "apps", "channels"], apps: ["YouTube"])
            check("what the device said it can do", PCHome.accepts(tv, action: "turn_on", value: nil) && PCHome.accepts(tv, action: "mute", value: nil))
            check("never what it did not say", !PCHome.accepts(tv, action: "set_brightness", value: .number(50)) && !PCHome.accepts(tv, action: "channel_up", value: nil))
            check("volume from 0 to 100", PCHome.accepts(tv, action: "set_volume", value: .number(0)) && PCHome.accepts(tv, action: "set_volume", value: .number(100))
                  && !PCHome.accepts(tv, action: "set_volume", value: .number(101)) && !PCHome.accepts(tv, action: "set_volume", value: .number(-1)))
            check("volume needs its number", !PCHome.accepts(tv, action: "set_volume", value: nil) && !PCHome.accepts(tv, action: "set_volume", value: .text("50")))
            check("an app from its own list only", PCHome.accepts(tv, action: "open_app", value: .text("YouTube")) && !PCHome.accepts(tv, action: "open_app", value: .text("Netflix")))
            check("a channel", PCHome.accepts(tv, action: "set_channel", value: .text("12")) && !PCHome.accepts(tv, action: "set_channel", value: .text("")))
            check("a step takes no value", PCHome.accepts(tv, action: "volume_up", value: nil) && !PCHome.accepts(tv, action: "volume_up", value: .number(3)))
            check("an unknown action is never sent", !PCHome.accepts(tv, action: "self_destruct", value: nil))
            let lamp = device("ha:light", caps: ["brightness"])
            check("brightness from 1 to 100", PCHome.accepts(lamp, action: "set_brightness", value: .number(1)) && !PCHome.accepts(lamp, action: "set_brightness", value: .number(0)))
            check("each action needs a capability the computer names", PCHome.actions.allSatisfy { action in
                PCHome.capability(for: action).map { PCHome.capabilities.contains($0) } == true
            })

            checkEqual("homeResult ok", line(#"{"type":"homeResult","id":"h1","ok":true,"detail":"On · 40%"}"#), .homeResult(PCRequestResult(id: "h1", ok: true, detail: "On · 40%")))
            checkEqual("homeResult: it asks first", line(#"{"type":"homeResult","id":"h1","ok":false,"reason":"confirm","message":"This device asks first."}"#),
                       .homeResult(PCRequestResult(id: "h1", ok: false, reason: "confirm", message: "This device asks first.")))
            var model = PCLinkModel()
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["home"]), now: t0)
            if case .home(let home)? = example { model.apply(.home(home), now: t0) }
            checkEqual("the home as said last", model.home?.devices.count, 1)
            model.apply(.homeResult(PCRequestResult(id: "h9", ok: true)), now: t0)
            checkEqual("the last answer kept", model.lastHomeResult?.id, "h9")
            model.connectionLost()
            check("the link closed: still shown", model.home != nil)
            model.apply(.welcome(desktop: "PC", os: "windows", caps: []), now: t0)
            check("not offered any more: it goes", model.home == nil)
        }

        print("\nMochi's outfit, plan usage and service cards (capabilities prefs, usage, services)")
        do {
            checkEqual("outfit", line(#"{"type":"prefs","outfit":"beanie"}"#), .prefs(outfit: "beanie"))
            checkEqual("an outfit this iPhone does not know is ignored", line(#"{"type":"prefs","outfit":"tuxedo"}"#), .other)
            check("every outfit of the computer", PCWire.outfits.allSatisfy { line(#"{"type":"prefs","outfit":"\#($0)"}"#) == .prefs(outfit: $0) })
            var model = PCLinkModel()
            model.apply(.prefs(outfit: "crown"), now: t0)
            checkEqual("kept", model.outfit, "crown")

            let usage = line(#"{"type":"usage","claude":{"fiveHour":{"pct":42,"resetsAt":1900000000000},"sevenDay":{"pct":7,"resetsAt":1900500000000},"plan":"max","updatedAt":1899999990000},"codex":{"sevenDay":{"pct":100,"resetsAt":1900500000000},"resetCredits":2,"plan":"plus"}}"#)
            let week = Date(timeIntervalSince1970: 1_900_500_000)
            checkEqual("usage", usage, .usage(PCUsage(
                claude: PCUsage.Plan(fiveHour: PCUsage.Window(pct: 42, resetsAt: Date(timeIntervalSince1970: 1_900_000_000)),
                                     sevenDay: PCUsage.Window(pct: 7, resetsAt: week), resetCredits: nil, plan: "max",
                                     updatedAt: Date(timeIntervalSince1970: 1_899_999_990)),
                codex: PCUsage.Plan(fiveHour: nil, sevenDay: PCUsage.Window(pct: 100, resetsAt: week), resetCredits: 2, plan: "plus", updatedAt: nil))))
            checkEqual("nothing to say any more", line(#"{"type":"usage"}"#), .usage(PCUsage()))
            let oddUsage = line(#"{"type":"usage","claude":{"fiveHour":{"pct":142,"resetsAt":1900000000000},"sevenDay":{"pct":7}},"codex":{"sevenDay":{"pct":50,"resetsAt":1900500000000},"resetCredits":300,"plan":"pro <script> \u0007 max-5x and a very long name"}}"#)
            if case .usage(let u)? = oddUsage {
                check("a plan with no usable window is left out", u.claude == nil)
                check("free resets out of range say nothing", u.codex?.resetCredits == nil)
                checkEqual("a plan's name: letters, digits, spaces and dashes, 20 at most", u.codex?.plan, "pro script  max-5x a")
            } else {
                check("odd usage decoded", false)
            }
            checkEqual("a window whose reset passed counts as 0", PCUsage.Window(pct: 80, resetsAt: t0).percent(at: t0), 0)
            checkEqual("before it, its number", PCUsage.Window(pct: 80, resetsAt: t0).percent(at: t0.addingTimeInterval(-1)), 80)
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["usage", "services"]), now: t0)
            if case .usage(let u)? = usage { model.apply(.usage(u), now: t0) }
            check("the usage kept", model.usage?.claude?.fiveHour?.pct == 42)
            model.apply(.usage(PCUsage()), now: t0)
            check("a bare usage takes the panel away", model.usage == nil)

            let cards = line(#"{"type":"services","services":[{"id":"integration_stripe","title":"Stripe","headline":"12.50 EUR","reason":"Payments","items":[{"label":"Payment","detail":"+9.00 · 2m"}]}]}"#)
            checkEqual("service cards", cards, .services([PCServiceCard(id: "integration_stripe", title: "Stripe", headline: "12.50 EUR", reason: "Payments",
                                                                        items: [PCServiceCard.Item(label: "Payment", detail: "+9.00 · 2m")])]))
            let noisy = line(#"{"type":"services","services":[{"id":"integration_bank","title":"Bank"},{"id":"integration_github","title":"GitHub","headline":"\#(String(repeating: "★", count: 100))","items":[{"label":"a","detail":"1"},{"label":"","detail":"no label"},{"label":"b","detail":"2"},{"label":"c","detail":"3"},{"label":"d","detail":"4"}]},{"id":"integration_github","title":"again"},"x"]}"#)
            if case .services(let list)? = noisy {
                checkEqual("only the seven services, once each", list.map(\.id), ["integration_github"])
                checkEqual("a text is one line of at most 80 characters", list.first?.headline.count, 80)
                checkEqual("three lines at most, each with a label", list.first?.items.map(\.label), ["a", "b", "c"])
            } else {
                check("noisy services decoded", false)
            }
            checkEqual("an empty list takes the cards away", line(#"{"type":"services","services":[]}"#), .services([]))
            checkEqual("no list at all: ignored", line(#"{"type":"services"}"#), .other)
            if case .services(let list)? = cards { model.apply(.services(list), now: t0) }
            checkEqual("the cards kept", model.services.map(\.id), ["integration_stripe"])
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["usage"]), now: t0)
            check("services not offered any more: they go", model.services.isEmpty)
        }

        print("\nThe files an agent changed, and their lines (capability diffs)")
        do {
            let get = object(PCOutgoing.getDiff(pillId: "integration_claude", fileId: 7).line)
            checkEqual("getDiff", [get["type"] as? String, get["pillId"] as? String], ["getDiff", "integration_claude"])
            checkEqual("the file's id, a number", get["fileId"] as? Int, 7)

            let sessions = line(#"{"type":"sessions","sessions":[{"pillId":"integration_claude","agent":"Claude Code","state":"working","statusText":"","stepCount":0,"stepIndex":0,"updatedAt":1800000000000,"files":[{"id":7,"name":"app.ts","added":3,"removed":1},{"id":8,"name":"C:\\Users\\lea\\secret\\notes.md","added":10,"removed":0,"isNew":true},{"id":9,"name":"huge.lock","added":5000,"removed":4000,"tooLarge":true},{"name":"no id"},{"id":-1,"name":"negative"},{"id":10,"name":"/"}]}]}"#)
            if case .sessions(let list)? = sessions, let files = list.first?.files {
                checkEqual("files with an id and a name", files.map(\.id), [7, 8, 9])
                checkEqual("a name, never a path", files.map(\.name), ["app.ts", "notes.md", "huge.lock"])
                checkEqual("counts", files.first.map { [$0.added, $0.removed] }, [3, 1])
                check("written whole", files.dropFirst().first?.isNew == true)
                check("too large to show", files.last?.tooLarge == true)
            } else {
                check("sessions with files decoded", false)
            }
            let many = (0..<25).map { #"{"id":\#($0),"name":"f\#($0).swift","added":1,"removed":0}"# }.joined(separator: ",")
            if case .sessions(let list)? = line(#"{"type":"sessions","sessions":[{"pillId":"p","files":[\#(many)]}]}"#) {
                checkEqual("the newest 20", list.first?.files.map(\.id), Array(5..<25))
            } else {
                check("many files decoded", false)
            }

            let part0 = line(#"{"type":"diff","pillId":"integration_claude","fileId":7,"name":"src/app.ts","added":3,"removed":1,"tooLarge":false,"gone":false,"truncated":false,"part":0,"parts":2,"lines":[["@","@@ -1,3 +1,4 @@"],["+","let x = 1;"],["-","let y = 2;"],[" ","same"],["?","odd kind"],["+"],"broken"]}"#)
            if case .diff(let p)? = part0 {
                checkEqual("the file's name only", p.name, "app.ts")
                checkEqual("kinds, an unknown one as context", p.lines.map(\.kind), [.hunk, .added, .removed, .context, .context])
                checkEqual("texts, a broken line dropped", p.lines.map(\.text), ["@@ -1,3 +1,4 @@", "let x = 1;", "let y = 2;", "same", "odd kind"])
                checkEqual("part of", [p.part, p.parts, p.fileId], [0, 2, 7])
            } else {
                check("a diff part decoded", false)
            }
            checkEqual("three parts is not the computer", line(#"{"type":"diff","pillId":"p","fileId":1,"part":0,"parts":3,"lines":[]}"#), .other)
            checkEqual("a part out of range", line(#"{"type":"diff","pillId":"p","fileId":1,"part":2,"parts":2,"lines":[]}"#), .other)
            checkEqual("no file id", line(#"{"type":"diff","pillId":"p","part":0,"parts":1,"lines":[]}"#), .other)
            let tooMany = (0..<101).map { _ in #"["+","x"]"# }.joined(separator: ",")
            checkEqual("more than 100 lines in a part", line(#"{"type":"diff","pillId":"p","fileId":1,"part":0,"parts":1,"lines":[\#(tooMany)]}"#), .other)
            if case .diff(let p)? = line(#"{"type":"diff","pillId":"p","fileId":1,"part":0,"parts":1,"lines":[["+","\#(String(repeating: "y", count: 500))"]]}"#) {
                checkEqual("a line cut at 400 characters", p.lines.first?.text.count, 400)
            } else {
                check("a long line decoded", false)
            }

            var model = PCLinkModel()
            model.apply(.welcome(desktop: "PC", os: "windows", caps: ["diffs"]), now: t0)
            func part(_ n: Int, of parts: Int, file: Int = 7, _ texts: [String]) -> PCDiffPart {
                PCDiffPart(pillId: "integration_claude", fileId: file, name: "app.ts", added: 3, removed: 1, tooLarge: false, gone: false,
                           truncated: false, part: n, parts: parts, lines: texts.map { PCDiffLine(kind: .added, text: $0) })
            }
            model.apply(.diff(part(0, of: 1, ["x"])), now: t0)
            check("a diff nobody asked for is not kept", model.diff == nil)
            model.wantDiff(pillId: "integration_claude", fileId: 7)
            model.apply(.diff(part(1, of: 2, ["c", "d"])), now: t0)
            check("one part of two: not yet", model.diff?.complete == false)
            model.apply(.diff(part(0, of: 2, file: 8, ["other"])), now: t0)
            model.apply(.diff(part(0, of: 2, ["a", "b"])), now: t0)
            check("both parts: complete", model.diff?.complete == true)
            checkEqual("in order whatever came first, another file's left out", model.diff?.lines.map(\.text), ["a", "b", "c", "d"])
            model.closeDiff(pillId: "integration_claude", fileId: 8)
            check("closing another file's sheet changes nothing", model.diff != nil)
            model.closeDiff(pillId: "integration_claude", fileId: 7)
            check("its sheet closed: forgotten", model.diff == nil)
            model.wantDiff(pillId: "integration_claude", fileId: 9)
            model.apply(.diff(PCDiffPart(pillId: "integration_claude", fileId: 9, name: "x", added: 0, removed: 0, tooLarge: false, gone: true,
                                         truncated: false, part: 0, parts: 1, lines: [])), now: t0)
            check("gone: complete, with no line", model.diff?.complete == true && model.diff?.gone == true && model.diff?.lines.isEmpty == true)
            model.apply(.welcome(desktop: "PC", os: "windows", caps: []), now: t0)
            check("diffs not offered any more: forgotten", model.diff == nil)
        }

        print("\nThe chat kept between launches")
        do {
            var model = PCLinkModel()
            model.startChat(id: "c1", text: "liga a TV", hands: true)
            model.apply(.chatStep(id: "c1", step: PCChatStep(index: 0, status: "ok", text: "Home · TV da sala")), now: t0)
            model.apply(.chatDone(id: "c1", text: "Pronto!"), now: t0)
            model.startChat(id: "c2", text: "e o som?", hands: false)
            model.apply(.chatDelta(id: "c2", text: "Ligan"), now: t0)
            let data = PCChatArchive(entries: model.chat).data
            let back = data.map(PCChatArchive.entries(from:)) ?? []
            checkEqual("read back as it was", back, model.chat)
            if let data, let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
               let entries = json["entries"] as? [[String: Any]] {
                checkEqual("the archive's fields", Set(json.keys), ["version", "entries"])
                let allowed: Set<String> = ["id", "role", "text", "steps", "failed", "finished", "hands"]
                check("each message: its own fields only (no key, token or address)", entries.allSatisfy { Set($0.keys).isSubset(of: allowed) })
            } else {
                check("the archive is JSON", false)
            }
            var later = PCLinkModel()
            later.restoreChat(back)
            checkEqual("the messages", later.chat.map(\.text).dropLast(), ["liga a TV", "Pronto!", "e o som?"])
            checkEqual("his steps", later.chat.dropFirst().first?.steps.map(\.text), ["Home · TV da sala"])
            check("an answer being written when Coucou closed ends there, text kept",
                  later.chat.last?.finished == true && later.chat.last?.failed == true && later.chat.last?.text == "Ligan" && later.running == nil)
            var silent = PCLinkModel()
            silent.restoreChat([PCChatEntry(id: "c3", role: .mochi, text: "")])
            checkEqual("nothing written yet: says why", silent.chat.last?.text, PCLinkModel.lostText)
            check("damaged: nothing", PCChatArchive.entries(from: Data("{".utf8)).isEmpty)
            check("another version: nothing", PCChatArchive.entries(from: Data(#"{"version":2,"entries":[{"id":"c9","role":"you","text":"x"}]}"#.utf8)).isEmpty)
            checkEqual("a field missing takes its default", PCChatArchive.entries(from: Data(#"{"version":1,"entries":[{"id":"c9","role":"mochi","text":"Oi","finished":true}]}"#.utf8)),
                       [PCChatEntry(id: "c9", role: .mochi, text: "Oi", finished: true)])
            later.restoreChat((0..<120).map { PCChatEntry(id: "e\($0)", role: .you, text: "\($0)") })
            check("at most 100 messages, the newest", later.chat.count == 100 && later.chat.first?.id == "e20")
        }

        print(failures == 0 ? "\nAll PC link wire tests passed." : "\n\(failures) failure(s).")
        if failures > 0 { exit(1) }
    }
}
