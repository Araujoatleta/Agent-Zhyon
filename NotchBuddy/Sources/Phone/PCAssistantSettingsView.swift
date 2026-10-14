import SwiftUI
import UIKit

// Settings → Assistant: Hermes Agent on the paired Windows or Linux computer, as Coucou sets it up and runs it there
// (capability `assistant`, docs/ANDROID_LINK.md): its name and tone, its brain with the ChatGPT sign-in, its skill
// packs, and whether Coucou runs it. Every change goes through PCLink and the computer checks it again; another brain
// and the sign-in ask Face ID first. No key is typed here: a brain whose key or server is not on the computer yet is
// shown, not offered, and the iPhone never installs anything there.

struct PCAssistantSettingsView: View {
    @State private var draft = Draft()
    /// What the computer said when the draft was last filled: a draft still equal to it follows the computer.
    @State private var shown: Draft?
    /// The skill packs and the gateway switch as just asked, until the computer says them back.
    @State private var pendingSkills: [String]?
    @State private var pendingGateway: Bool?
    /// Counts the changes asked, so an older one's wait does not undo a newer one.
    @State private var asked = 0
    @State private var working: Work?
    @State private var problem: String?
    @State private var saved = false
    @State private var copied = false
    @FocusState private var typing: Bool

    private var pc: PCLink { PCLink.shared }
    private var connected: Bool { pc.status == .connected }

    /// The name and the tone as typed here.
    private struct Draft: Equatable {
        var name = ""
        var persona = PCAssistant.defaultPersona
        var text = ""
    }

    /// The change on its way to the computer: one at a time.
    private enum Work: Equatable {
        case save, brain(String), skills, signIn, cancelSignIn, gateway
    }

    var body: some View {
        List {
            if let assistant = pc.model.assistant {
                sections(assistant)
            } else {
                Section {
                    PCNotice.missing("assistant", off: String(localized: "On \(pc.name), open Coucou → Settings → Phone and turn on “Let the phone change the assistant (name, tone, brain, skills)”."))
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 8, leading: 0, bottom: 8, trailing: 0))
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background { DeepBackdrop() }
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle("Assistant")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("Done") { typing = false }
            }
        }
        .onAppear {
            pc.refreshAssistant()
            sync(pc.model.assistant)
        }
        .onChange(of: pc.model.assistant) { _, assistant in
            sync(assistant)
        }
        .onChange(of: draft) {
            saved = false
        }
    }

    @ViewBuilder
    private func sections(_ assistant: PCAssistant) -> some View {
        header(assistant)
        if let problem {
            Section {
                Label(problem, systemImage: "exclamationmark.triangle.fill")
                    .font(.footnote)
                    .foregroundStyle(CoucouPalette.danger)
            }
            .listRowBackground(PCSettingsRowBackground())
        }
        identity(assistant)
        brains(assistant)
        if showsSignIn(assistant) {
            Section {
                signInCard(assistant)
            } header: {
                Text("ChatGPT")
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
        }
        skills(assistant)
        hermes(assistant)
    }

    // MARK: Who it is

    private func header(_ assistant: PCAssistant) -> some View {
        Section {
            HStack(spacing: 14) {
                Image(systemName: "sparkles")
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(CoucouPalette.accent)
                    .frame(width: 52, height: 52)
                    .coucouGlass(.regular, cornerRadius: 16, tint: CoucouPalette.accent)
                VStack(alignment: .leading, spacing: 3) {
                    Text(verbatim: assistant.name)
                        .font(.title3.weight(.semibold))
                    Text("Hermes Agent on \(pc.name)")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Text(verbatim: PCAssistantWords.gateway(assistant.hermes))
                        .font(.caption)
                        .foregroundStyle(gatewayColor(assistant.hermes))
                }
            }
            .padding(.vertical, 4)
            if !connected {
                HStack(spacing: 10) {
                    Label(pc.status.text, systemImage: "wifi.slash")
                        .font(.footnote)
                        .foregroundStyle(CoucouPalette.warn)
                    Spacer(minLength: 8)
                    if pc.status != .connecting, pc.status != .searching, pc.status != .refused {
                        Button("Try again now") { pc.reconnectIfNeeded() }
                            .font(.footnote.weight(.semibold))
                    }
                }
            }
        }
        .listRowBackground(Color.clear)
    }

    private func identity(_ assistant: PCAssistant) -> some View {
        Section {
            LabeledContent {
                TextField(text: $draft.name, prompt: Text(verbatim: PCAssistant.defaultName)) {
                    Text("Name")
                }
                .multilineTextAlignment(.trailing)
                .textInputAutocapitalization(.words)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .focused($typing)
            } label: {
                Text("Name")
            }
            Picker("Tone", selection: $draft.persona) {
                ForEach(assistant.personas, id: \.self) { id in
                    Text(verbatim: PCAssistantWords.persona(id)).tag(id)
                }
            }
            if draft.persona == PCAssistant.customPersona {
                TextField("Describe the tone in your own words", text: $draft.text, axis: .vertical)
                    .lineLimit(3...8)
                    .focused($typing)
                Text(verbatim: "\(draft.text.count)/\(PCAssistant.maxPersonaText)")
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            Button {
                Task { await save() }
            } label: {
                HStack {
                    Text("Save name and tone")
                    Spacer(minLength: 8)
                    if working == .save {
                        ProgressView()
                    } else if saved {
                        Image(systemName: "checkmark")
                            .foregroundStyle(CoucouPalette.ok)
                    }
                }
            }
            .disabled(!canSave)
        } header: {
            Text("Name and tone")
        } footer: {
            Text(verbatim: PCAssistantWords.personaStyle(draft.persona))
        }
        .listRowBackground(PCSettingsRowBackground())
        // One line of at most 40 characters, a tone of at most 1500, as the computer takes them.
        .onChange(of: draft.name) { _, typed in
            if typed.count > PCAssistant.maxName { draft.name = String(typed.prefix(PCAssistant.maxName)) }
        }
        .onChange(of: draft.text) { _, typed in
            if typed.count > PCAssistant.maxPersonaText { draft.text = String(typed.prefix(PCAssistant.maxPersonaText)) }
        }
    }

    private var canSave: Bool {
        guard connected, working == nil, let shown, draft != shown else { return false }
        // The user's own tone needs its words.
        return draft.persona != PCAssistant.customPersona
            || !draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    // MARK: The brain

    private func brains(_ assistant: PCAssistant) -> some View {
        Section {
            ForEach(assistant.brains) { brain in
                Button {
                    Task { await choose(brain) }
                } label: {
                    brainRow(brain, current: brain.id == assistant.brain)
                }
                .disabled(!canChoose(brain, in: assistant))
            }
            if assistant.brains.isEmpty {
                Text("The computer listed no brain.")
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Brain")
        } footer: {
            Text(verbatim: brainFooter(assistant))
        }
        .listRowBackground(PCSettingsRowBackground())
    }

    private func brainRow(_ brain: PCAssistant.Brain, current: Bool) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: PCAssistantWords.brain(brain))
                    .foregroundStyle(.primary)
                if let detail = brainDetail(brain) {
                    Text(verbatim: detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            if working == .brain(brain.id) {
                ProgressView()
            } else if current {
                Image(systemName: "checkmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(CoucouPalette.accent)
                    .accessibilityLabel("In use")
            }
        }
        .contentShape(Rectangle())
    }

    private func canChoose(_ brain: PCAssistant.Brain, in assistant: PCAssistant) -> Bool {
        connected && working == nil && assistant.changeable && !assistant.hermes.setup.running
            && (brain.ready || brain.id == assistant.brain)
    }

    private func brainDetail(_ brain: PCAssistant.Brain) -> String? {
        if brain.login { return String(localized: "Needs ChatGPT Plus or Pro. No key: you sign in with your account.") }
        guard !brain.ready else { return nil }
        return ["ollama", "lmstudio"].contains(brain.id)
            ? String(localized: "Set its server address on the computer first.")
            : String(localized: "Add its key on the computer first.")
    }

    private func brainFooter(_ assistant: PCAssistant) -> String {
        if !assistant.hermes.installed {
            return String(localized: "Hermes Agent isn't installed on \(pc.name). Install it there, in Coucou → Settings → Hermes Agent: the iPhone never installs anything.")
        }
        if !assistant.hermes.managed {
            return String(localized: "Coucou doesn't run Hermes on \(pc.name): turn on “Coucou runs Hermes” below to change its brain and skills from here.")
        }
        return String(localized: "Another brain asks Face ID first. Keys stay on the computer: add one there, in Coucou → Settings → Hermes Agent.")
    }

    // MARK: The ChatGPT sign-in

    private func showsSignIn(_ assistant: PCAssistant) -> Bool {
        if case .waiting = assistant.login { return true }
        return assistant.current?.login == true
    }

    private func signInCard(_ assistant: PCAssistant) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Sign in with ChatGPT", systemImage: "person.crop.circle.badge.checkmark")
                .font(.headline)
            switch assistant.login {
            case .waiting(let code, let url, let expiresAt):
                waiting(code: code, url: url, expiresAt: expiresAt)
            case .signedIn:
                Label("Signed in with your ChatGPT account.", systemImage: "checkmark.seal.fill")
                    .font(.callout)
                    .foregroundStyle(CoucouPalette.ok)
                Text("To sign out, use Coucou → Settings → Hermes Agent on the computer.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            case .failed(let reason):
                Text(verbatim: reason.isEmpty ? String(localized: "The sign-in didn't work.") : reason)
                    .font(.callout)
                    .foregroundStyle(CoucouPalette.danger)
                startButton(assistant, title: String(localized: "Try again"))
            case .signedOut, .unknown:
                Text("Use your ChatGPT Plus or Pro subscription: Coucou starts the sign-in on \(pc.name), and you type a code on OpenAI's page here. No key needed.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                startButton(assistant, title: String(localized: "Sign in with ChatGPT"))
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .coucouGlass(.regular, cornerRadius: 24, tint: CoucouPalette.accent)
    }

    /// The code to type on OpenAI's page, large, with Copy, the page in Safari, the time left and Cancel.
    @ViewBuilder
    private func waiting(code: String, url: String, expiresAt: Date?) -> some View {
        Text("On OpenAI's page, sign in with your ChatGPT account and type this code:")
            .font(.callout)
            .foregroundStyle(.secondary)
        Text(verbatim: code)
            .font(.system(size: 38, weight: .bold, design: .monospaced))
            .tracking(2)
            .lineLimit(1)
            .minimumScaleFactor(0.5)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
            .accessibilityLabel(Text(verbatim: code.map { String($0) }.joined(separator: " ")))
        if let expiresAt {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                let left = PCAssistant.secondsLeft(until: expiresAt, now: context.date)
                Text(verbatim: left > 0 ? String(localized: "Time left: \(Self.clock(left))")
                                        : String(localized: "The code expired: cancel and start again."))
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(left > 60 ? Color.secondary : CoucouPalette.warn)
                    .frame(maxWidth: .infinity)
            }
        }
        HStack(spacing: 10) {
            Button {
                UIPasteboard.general.string = code
                copied = true
                Haptics.success()
            } label: {
                if copied {
                    Label("Copied", systemImage: "checkmark")
                        .frame(maxWidth: .infinity)
                } else {
                    Label("Copy code", systemImage: "doc.on.doc")
                        .frame(maxWidth: .infinity)
                }
            }
            .buttonStyle(.bordered)
            Button {
                openSignInPage(url)
            } label: {
                Label("Open sign-in page", systemImage: "safari")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(CoucouPalette.accent)
            .disabled(!PCAssistant.isSignInPage(url))
        }
        Button(role: .destructive) {
            Task { await cancelSignIn() }
        } label: {
            HStack {
                if working == .cancelSignIn { ProgressView() }
                Text("Cancel sign-in")
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .disabled(!connected || working != nil)
    }

    private func startButton(_ assistant: PCAssistant, title: String) -> some View {
        Button {
            Task { await startSignIn() }
        } label: {
            HStack {
                if working == .signIn { ProgressView() }
                Text(verbatim: title)
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .tint(CoucouPalette.accent)
        .disabled(!connected || working != nil || !assistant.hermes.installed)
    }

    /// "4:59".
    private static func clock(_ seconds: Int) -> String {
        String(format: "%d:%02d", seconds / 60, seconds % 60)
    }

    /// OpenAI's own page only, in Safari: the computer sends no other, and this iPhone opens no other.
    private func openSignInPage(_ page: String) {
        guard PCAssistant.isSignInPage(page), let url = URL(string: page) else { return }
        UIApplication.shared.open(url)
    }

    // MARK: Skills

    private func skills(_ assistant: PCAssistant) -> some View {
        Section {
            ForEach(assistant.optionalSkills, id: \.self) { pack in
                Toggle(isOn: skillBinding(pack, in: assistant)) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: PCAssistantWords.skill(pack))
                        if let detail = PCAssistantWords.skillDetail(pack) {
                            Text(verbatim: detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .tint(CoucouPalette.accent)
                .disabled(!connected || !assistant.changeable || working != nil)
            }
            LabeledContent {
                if assistant.house {
                    Text("On").foregroundStyle(CoucouPalette.ok)
                } else {
                    Text("Off").foregroundStyle(.secondary)
                }
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: PCAssistantWords.skill(PCAssistant.houseSkill))
                    Text("Changed on the computer, in Settings → Hermes Agent.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Text("Skills")
        } footer: {
            Text("Hermes also comes with its own skills (Google Workspace, Notion, maps, Obsidian, e-mail triage, reminders): already included, nothing to install.")
        }
        .listRowBackground(PCSettingsRowBackground())
    }

    private func skillBinding(_ pack: String, in assistant: PCAssistant) -> Binding<Bool> {
        Binding(
            get: { (pendingSkills ?? assistant.skills).contains(pack) },
            set: { on in
                var wanted = pendingSkills ?? assistant.skills
                wanted.removeAll { $0 == pack }
                if on { wanted.append(pack) }
                Task { await setSkills(wanted) }
            })
    }

    // MARK: Hermes and its gateway

    private func hermes(_ assistant: PCAssistant) -> some View {
        Section {
            Toggle(isOn: gatewayBinding(assistant)) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Coucou runs Hermes")
                    Text(verbatim: PCAssistantWords.gateway(assistant.hermes))
                        .font(.caption)
                        .foregroundStyle(gatewayColor(assistant.hermes))
                }
            }
            .tint(CoucouPalette.accent)
            .disabled(!connected || !assistant.hermes.installed || working != nil)
            let setup = assistant.hermes.setup
            if setup.running {
                VStack(alignment: .leading, spacing: 6) {
                    if setup.total > 0 {
                        ProgressView(value: Double(setup.done), total: Double(setup.total))
                            .tint(CoucouPalette.accent)
                    } else {
                        ProgressView()
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    Text(verbatim: setup.step.isEmpty ? String(localized: "Setting up…") : setup.step)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if !setup.error.isEmpty {
                Label(setup.error, systemImage: "exclamationmark.triangle.fill")
                    .font(.footnote)
                    .foregroundStyle(CoucouPalette.danger)
            }
        } header: {
            Text("Hermes Agent")
        } footer: {
            Text("Off: Hermes stays installed on the computer, its gateway stops, and its brain and skills can't change from here.")
        }
        .listRowBackground(PCSettingsRowBackground())
    }

    private func gatewayBinding(_ assistant: PCAssistant) -> Binding<Bool> {
        Binding(
            get: { pendingGateway ?? assistant.hermes.managed },
            set: { on in Task { await setGateway(on) } })
    }

    private func gatewayColor(_ hermes: PCAssistant.Hermes) -> Color {
        switch hermes.gateway {
        case .running, .elsewhere: CoucouPalette.ok
        case .starting: CoucouPalette.warn
        case .stopped: CoucouPalette.danger
        case .off: Color.secondary
        }
    }

    // MARK: Changes

    /// The computer said how the assistant is: an untouched draft follows it (what is being typed stays), and what
    /// was just asked is shown as it is once it says the same.
    private func sync(_ assistant: PCAssistant?) {
        guard let assistant else { return }
        let now = Draft(name: assistant.name, persona: assistant.persona, text: assistant.personaText)
        if shown == nil || draft == shown { draft = now }
        shown = now
        if let pending = pendingSkills, Set(pending) == Set(assistant.skills) { pendingSkills = nil }
        if let pending = pendingGateway, pending == assistant.hermes.managed { pendingGateway = nil }
    }

    private func save() async {
        guard let shown else { return }
        typing = false
        problem = nil
        let name = PCText.line(draft.name, max: PCAssistant.maxName)
        let newName = name != shown.name ? name : nil
        let newPersona = draft.persona != shown.persona ? draft.persona : nil
        let newText = draft.text != shown.text ? draft.text : nil
        draft.name = name
        // Only spaces changed: nothing to send.
        guard newName != nil || newPersona != nil || newText != nil else { return }
        working = .save
        let result = await pc.setAssistant(name: newName, persona: newPersona, personaText: newText)
        working = nil
        if result.ok {
            saved = true
            Haptics.success()
        } else {
            show(result)
        }
    }

    private func choose(_ brain: PCAssistant.Brain) async {
        guard let assistant = pc.model.assistant, brain.id != assistant.brain else { return }
        problem = nil
        working = .brain(brain.id)
        let result = await pc.setAssistantBrain(brain.id)
        working = nil
        if result.ok { Haptics.success() } else { show(result) }
    }

    private func startSignIn() async {
        problem = nil
        copied = false
        working = .signIn
        let result = await pc.signInWithChatGPT()
        working = nil
        if !result.ok { show(result) }
    }

    private func cancelSignIn() async {
        problem = nil
        working = .cancelSignIn
        let result = await pc.cancelChatGPTSignIn()
        working = nil
        if !result.ok { show(result) }
    }

    private func setSkills(_ wanted: [String]) async {
        problem = nil
        asked += 1
        let mine = asked
        pendingSkills = wanted
        working = .skills
        let result = await pc.setAssistantSkills(wanted)
        working = nil
        guard result.ok else {
            pendingSkills = nil
            show(result)
            return
        }
        // The computer says the packs back in a moment; should it not, the switches show what it said last.
        try? await Task.sleep(for: .seconds(5))
        if asked == mine { pendingSkills = nil }
    }

    private func setGateway(_ on: Bool) async {
        problem = nil
        asked += 1
        let mine = asked
        pendingGateway = on
        working = .gateway
        let result = await pc.setAssistantGateway(on)
        working = nil
        guard result.ok else {
            pendingGateway = nil
            show(result)
            return
        }
        try? await Task.sleep(for: .seconds(5))
        if asked == mine { pendingGateway = nil }
    }

    private func show(_ result: PCRequestResult) {
        guard let text = PCRequestWords.explain(result, cap: "assistant", computer: pc.name) else { return }
        problem = text
        Haptics.warning()
    }
}

/// The words for the assistant's ids, as Settings → Hermes Agent says them on the computer.
enum PCAssistantWords {
    static func brain(_ brain: PCAssistant.Brain) -> String {
        switch brain.id {
        case "chatgpt": String(localized: "ChatGPT (your subscription)")
        case "openai": String(localized: "OpenAI (API key)")
        case "anthropic": String(localized: "Anthropic Claude (API key)")
        case "google": String(localized: "Google Gemini (API key)")
        case "openrouter": String(localized: "OpenRouter (API key)")
        case "ollama": String(localized: "Ollama (on your computer)")
        case "lmstudio": String(localized: "LM Studio (on your computer)")
        default: brain.name
        }
    }

    static func persona(_ id: String) -> String {
        switch id {
        case "friendly": String(localized: "Friendly")
        case "direct": String(localized: "Direct")
        case "professional": String(localized: "Professional")
        case "playful": String(localized: "Playful")
        case "butler": String(localized: "Butler (formal, witty)")
        case "custom": String(localized: "Custom")
        default: id
        }
    }

    /// How the tone answers (the computer writes the same, in English, to Hermes' SOUL.md).
    static func personaStyle(_ id: String) -> String {
        switch id {
        case "friendly": String(localized: "Warm and friendly, like a helpful friend: short, clear answers, a light touch of humour when it fits.")
        case "direct": String(localized: "Direct and brief: the answer first, one line when one line is enough, no small talk.")
        case "professional": String(localized: "Professional and precise, like an executive assistant: structured answers, careful wording, and a clear note of what was done.")
        case "playful": String(localized: "Playful and upbeat, with energy and the odd emoji, while staying accurate and useful.")
        case "butler": String(localized: "Calm, courteous and quietly witty, like a seasoned butler who is also an engineer: the next step anticipated and offered in one line.")
        case "custom": String(localized: "Your own words, kept as you type them.")
        default: ""
        }
    }

    static func skill(_ id: String) -> String {
        switch id {
        case "home": String(localized: "Mochi's home (TVs, lights…)")
        case "search": String(localized: "Free web search (DuckDuckGo)")
        case "news": String(localized: "News and feeds (RSS)")
        default: id
        }
    }

    static func skillDetail(_ id: String) -> String? {
        switch id {
        case "search": String(localized: "Installed in Hermes from its official skills.")
        case "news": String(localized: "Installed in Hermes from its official skills.")
        default: nil
        }
    }

    static func gateway(_ hermes: PCAssistant.Hermes) -> String {
        guard hermes.installed else { return String(localized: "Hermes isn't installed on the computer") }
        switch hermes.gateway {
        case .off: return String(localized: "Off")
        case .starting: return String(localized: "Starting…")
        case .running: return String(localized: "Running")
        case .elsewhere: return String(localized: "Running outside Coucou")
        case .stopped:
            return hermes.reason.isEmpty ? String(localized: "Stopped") : String(localized: "Stopped: \(hermes.reason)")
        }
    }
}

/// A settings row on the deep backdrop, frosted with one surface per section, as on the other settings screens.
private struct PCSettingsRowBackground: View {
    var body: some View {
        ZStack {
            Rectangle().fill(.ultraThinMaterial)
            CoucouPalette.surface.opacity(0.55)
        }
    }
}
