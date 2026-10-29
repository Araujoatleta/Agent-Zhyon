import Foundation

// Hermes inside the iPhone (NotchBuddy/Hermes/Swift/HermesWire.swift): the events of an answer as the host writes
// them (NotchBuddy/Hermes/app/axyon_hermes/host.py), the brains, the conversation they fill in, and the iPhone's own
// home as the host reports it (home.py).
// Foundation only: runs on macOS and Linux (scripts/test-pc-link.sh).

@main
enum HermesWireTests {
    nonisolated(unsafe) static var failures = 0

    static func check(_ label: String, _ value: Bool) {
        if value { print("  ✓ \(label)") } else { print("  ✗ \(label)"); failures += 1 }
    }

    static func checkEqual<T: Equatable>(_ label: String, _ got: T, _ expected: T) {
        if got == expected { print("  ✓ \(label)") } else { print("  ✗ \(label)  got: \(got)  expected: \(expected)"); failures += 1 }
    }

    static func main() {
        print("Events")
        checkEqual("a delta", HermesEvent.parse(#"data: {"seq": 0, "type": "delta", "text": "Já "}"#), .delta("Já "))
        checkEqual("a tool", HermesEvent.parse(#"data: {"seq": 1, "type": "tool", "name": "coucou_portfolio", "preview": ""}"#),
                   .tool(name: "coucou_portfolio", preview: ""))
        checkEqual("its end", HermesEvent.parse(#"data: {"seq": 2, "type": "tool_done", "name": "coucou_portfolio", "error": false}"#),
                   .toolDone(name: "coucou_portfolio", failed: false))
        checkEqual("done", HermesEvent.parse(#"data: {"seq": 3, "type": "done", "text": "Vale R$ 7.589,47."}"#),
                   .done(text: "Vale R$ 7.589,47.", stopped: false))
        checkEqual("stopped", HermesEvent.parse(#"data: {"type": "done", "text": "", "stopped": true}"#), .done(text: "", stopped: true))
        checkEqual("an error", HermesEvent.parse(#"data: {"type": "error", "message": "No key"}"#), .error("No key"))
        check("a keepalive is nothing", HermesEvent.parse(": keepalive") == nil)
        check("a blank line is nothing", HermesEvent.parse("") == nil)
        checkEqual("an unknown type", HermesEvent.parse(#"data: {"type": "reasoning"}"#), .other)
        check("broken JSON is nothing", HermesEvent.parse("data: {nope") == nil)

        print("\nBrains")
        check("ChatGPT first, with no key", HermesBrain.all.first?.id == "openai-codex" && HermesBrain.all.first?.keyName == nil)
        checkEqual("the keys Hermes reads", HermesBrain.keyNames,
                   ["GEMINI_API_KEY", "OPENROUTER_API_KEY", "ANTHROPIC_API_KEY", "OPENAI_API_KEY"])
        check("only the custom one needs an address", HermesBrain.all.filter(\.needsAddress).map(\.id) == ["custom"])
        let settings = HermesSettings(json: ["provider": "gemini", "model": "a-model", "baseUrl": "",
                                             "keysSet": ["GEMINI_API_KEY": true], "chatgptSignedIn": NSNull()])
        checkEqual("settings read", settings, HermesSettings(provider: "gemini", model: "a-model",
                                                             keysSet: ["GEMINI_API_KEY": true]))
        check("a brain with its key can think", settings?.canThink == true)
        check("without it, not", HermesSettings(provider: "gemini", keysSet: ["GEMINI_API_KEY": false]).canThink == false)
        check("ChatGPT once signed in", HermesSettings(provider: "openai-codex", chatGPTSignedIn: true).canThink)
        check("ChatGPT not signed in", !HermesSettings(provider: "openai-codex", chatGPTSignedIn: false).canThink)
        let signIn = HermesSignIn(json: ["state": "waiting", "code": "AB12-CD34", "url": "https://auth.openai.com/codex/device"])
        check("the sign-in's code", signIn.state == .waiting && signIn.code == "AB12-CD34")

        print("\nThe conversation")
        var c = HermesConversation(session: "axyon-iphone-test")
        var n = 0
        let answer = c.ask("qual é a minha carteira?") { n += 1; return "\($0)-\(n)" }
        check("your message and the answer to come", c.entries.count == 2 && c.entries[0].role == .you && answer == "hermes-1")
        let label = { (tool: String) in tool == "coucou_portfolio" ? "Lendo a carteira" : tool }
        c.apply(.tool(name: "coucou_portfolio", preview: ""), to: answer, label: label)
        checkEqual("the tool as a step", c.entries[1].steps, [PCChatStep(index: 0, status: "running", text: "Lendo a carteira")])
        c.apply(.toolDone(name: "coucou_portfolio", failed: false), to: answer, label: label)
        checkEqual("done", c.entries[1].steps.first?.status, "ok")
        c.apply(.delta("Vale "), to: answer, label: label)
        c.apply(.delta("R$ 7.589,47."), to: answer, label: label)
        checkEqual("the words as they come", c.entries[1].text, "Vale R$ 7.589,47.")
        check("not finished yet", !c.entries[1].finished)
        c.apply(.done(text: "Sua carteira vale R$ 7.589,47.", stopped: false), to: answer, label: label)
        check("the whole answer at the end", c.entries[1].text == "Sua carteira vale R$ 7.589,47." && c.entries[1].finished)
        let second = c.ask("e hoje?") { n += 1; return "\($0)-\(n)" }
        c.apply(.error("Your ChatGPT sign-in expired."), to: second, label: label)
        check("an error ends it, failed", c.entries[3].failed && c.entries[3].finished && c.entries[3].text.contains("expired"))
        let third = c.ask("oi") { n += 1; return "\($0)-\(n)" }
        c.apply(.tool(name: "memory", preview: ""), to: third, label: label)
        c.restored(reason: "The app closed before the answer.")
        check("an answer cut by a launch ends there", c.entries[5].finished && c.entries[5].failed && c.entries[5].steps.first?.status == "error")
        let data = try? JSONEncoder().encode(c)
        let back = data.flatMap { try? JSONDecoder().decode(HermesConversation.self, from: $0) }
        checkEqual("kept and read back", back, c)
        check("a new session each time", HermesConversation.newSession() != HermesConversation.newSession())
        var many = HermesConversation(session: "s")
        for i in 0..<70 { _ = many.ask("m\(i)") }
        checkEqual("the newest 100 kept", many.entries.count, HermesConversation.kept)

        print("\nFiles sent")
        let photo = HermesAttachment(file: "a1-foto.jpg", name: "foto.jpg", kind: .image)
        let pdf = HermesAttachment(file: "a2-nota.pdf", name: "nota.pdf", kind: .file, textFile: "a2-nota.pdf.txt.extracted")
        var sent = HermesConversation(session: "s2")
        _ = sent.ask("o que é isso?", attachments: [photo, pdf]) { n += 1; return "\($0)-\(n)" }
        let mine = sent.entries[0].id
        checkEqual("kept with your message", sent.files(of: mine), [photo, pdf])
        check("none with the answer", sent.files(of: sent.entries[1].id).isEmpty)
        let inbox = URL(fileURLWithPath: "/data/Hermes/inbox")
        checkEqual("as the host takes it, where the inbox is now", pdf.wire(inbox: inbox),
                   ["path": "/data/Hermes/inbox/a2-nota.pdf", "name": "nota.pdf", "kind": "file",
                    "text": "/data/Hermes/inbox/a2-nota.pdf.txt.extracted"])
        checkEqual("a photo has no text", photo.wire(inbox: inbox)["text"], nil)
        let kept = (try? JSONEncoder().encode(sent)).flatMap { try? JSONDecoder().decode(HermesConversation.self, from: $0) }
        checkEqual("saved and read back with its files", kept, sent)
        let older = #"{"version":1,"session":"s","entries":[{"id":"you-1","role":"you","text":"oi","finished":true}]}"#
        check("what an older app saved still reads", (try? JSONDecoder().decode(HermesConversation.self, from: Data(older.utf8)))?.entries.count == 1)
        for i in 0..<60 { _ = sent.ask("m\(i)") }
        check("files leave with their message", sent.attachments?[mine] == nil)

        print("\nGoogle and scheduled tasks")
        let google = HermesGoogle(json: ["client": true, "connected": true, "account": "dono@example.com", "services": ["email", "sheets"]])
        check("Google connected, as whom, what", google.connected && google.account == "dono@example.com" && google.services == ["email", "sheets"])
        let link = "https://accounts.google.com/o/oauth2/v2/auth?client_id=1.apps.googleusercontent.com&state=x%2Fy&scope=a+b"
        checkEqual("the sign-in link in an answer", HermesGoogle.signInLink(in: "Abra este link:\n\(link)\nDepois volte.")?.absoluteString, link)
        checkEqual("in a Markdown link too", HermesGoogle.signInLink(in: "[Entrar](\(link))")?.absoluteString, link)
        check("no other link", HermesGoogle.signInLink(in: "https://www.google.com/search?q=x") == nil)
        let tasks = HermesTask.list(["jobs": [["id": "a1", "name": "Cotação BTLG11", "schedule": "every 5m",
                                              "next": "2026-10-29T19:52:43.699152+00:00", "enabled": true],
                                             ["name": "no id"]]])
        check("a task, its schedule and next time", tasks.count == 1 && tasks[0].schedule == "every 5m"
              && tasks[0].next == Date(timeIntervalSince1970: 1_793_303_563))
        check("Hermes' times without a fraction too", HermesTask.date("2026-10-29T19:52:43+00:00") == Date(timeIntervalSince1970: 1_793_303_563))

        print("\nImages Hermes made")
        checkEqual("their paths in Hermes' home, nothing outside it",
                   HermesEvent.parse(#"data: {"type": "images", "paths": ["cache/generated/images/a.png", "/etc/x.png", "../b.png"]}"#),
                   .images(["cache/generated/images/a.png"]))
        var drawn = HermesConversation(session: "s3")
        let drawing = drawn.ask("desenha um gato") { n += 1; return "\($0)-\(n)" }
        drawn.apply(.images(["cache/generated/images/gato.png"]), to: drawing, label: label)
        drawn.apply(.done(text: "Aqui está.", stopped: false), to: drawing, label: label)
        checkEqual("kept with the answer", drawn.made(in: drawing), ["cache/generated/images/gato.png"])
        check("and the answer still ends", drawn.entries[1].finished && drawn.entries[1].text == "Aqui está.")

        print("\nThe iPhone's own home")
        let reported: [String: Any] = [
            "devices": [
                ["id": "samsung:192.168.0.12", "name": "TV da sala", "room": "Sala", "kind": "tv", "source": "Samsung TV",
                 "paired": true, "can": ["turn_on", "turn_off", "volume_up"], "apps": ["YouTube", "Netflix"],
                 "state": ["online": true, "on": true]],
                ["id": "ha:light.sala", "name": "Luz da sala", "kind": "light", "source": "Home Assistant",
                 "can": ["set_brightness", "turn_off", "turn_on"], "asksFirst": false,
                 "state": ["online": true, "on": true, "brightness": 50.0]],
                ["name": "no id"],
            ],
            "homeAssistant": ["url": "http://192.168.0.5:8123", "connected": true],
        ]
        let house = HermesHouse(json: reported)
        checkEqual("its devices, one without an id left out", house.devices.map(\.id), ["samsung:192.168.0.12", "ha:light.sala"])
        check("the TV: its apps, what it can do, on", house.own.first?.apps == ["YouTube", "Netflix"]
              && house.own.first?.can("volume_up") == true && house.own.first?.on == true)
        checkEqual("the light from Home Assistant, at half", house.assistant.first?.brightness, 50)
        check("Home Assistant connected", house.assistantConnected && house.assistantURL == "http://192.168.0.5:8123")
        let found = HermesFound.list(["found": [["driver": "android", "host": "192.168.0.20", "port": 5555, "name": "Android (ADB)"],
                                                ["driver": "samsung", "host": "192.168.0.12", "name": "TV", "added": true],
                                                ["host": "no driver"]]])
        check("what a scan found", found.count == 2 && found[0].port == 5555 && found[1].added)

        print(failures == 0 ? "\nAll Hermes wire tests passed." : "\n\(failures) failed.")
        if failures > 0 { exit(1) }
    }
}
