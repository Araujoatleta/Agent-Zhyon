import Foundation

// Hermes inside the iPhone (NotchBuddy/Sources/Phone/HermesWire.swift): the events of an answer as the host writes
// them (NotchBuddy/Hermes/app/axyon_hermes/host.py), the brains, and the conversation they fill in.
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

        print(failures == 0 ? "\nAll Hermes wire tests passed." : "\n\(failures) failed.")
        if failures > 0 { exit(1) }
    }
}
