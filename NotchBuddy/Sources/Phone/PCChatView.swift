import SwiftUI
import UIKit

// The chat with Mochi on the paired Windows or Linux computer: a tab of its own, and still behind the computer button
// of the Agents tab. Mochi's answers in glass with his face drawn in code, yours tinted; what he does with his hands
// shows as steps, and a request that waits for your OK is allowed (Face ID) or denied right under his answer, through
// the same decision as the Agents tab. Answers are cut into blocks here (code, lists, quotes, titles, tables) and each
// block's bold, italics, code and links are SwiftUI's own Markdown.

/// Mochi on the computer, from the iPhone: the computer's chat, with his hands when they are on there (each step
/// shows up as he works; what changes something waits for Allow, here or on the computer).
struct PCChatView: View {
    let pc: PCLink
    /// The Chat tab. Pushed from the Agents tab instead, the tab bar hides as for a conversation.
    var inTab = false
    @State private var text = ""
    @FocusState private var focused: Bool
    /// Dictation as in the instruction composer (on the iPhone when it can); a pause sends what was said.
    @State private var dictation = Dictation()
    @State private var silence: Task<Void, Never>?
    /// The message being answered was spoken: the answer is read aloud.
    @State private var spoken = false
    @AppStorage("pcReadAnswersAloud") private var readAloud = true
    /// The end of the conversation is on screen: new words keep it there. Scrolled up, they never pull you down.
    @State private var atEnd = true
    /// Requests answered from here (fingerprint → allowed), until the computer says they are done.
    @State private var answered: [String: Bool] = [:]
    @State private var answering: String?
    @State private var approvalError: String?
    @State private var scanning = false
    @Namespace private var glass

    private static let end = "pc-chat-end"

    private var handsOn: Bool { pc.model.handsAllowed && pc.chatHands }
    private var canSend: Bool {
        pc.status == .connected && pc.model.chatAllowed && !pc.chatModels.isEmpty && !pc.chatRunning
    }

    /// What grows while Mochi answers: his words, his steps, a request for your OK.
    private var followKey: String {
        let last = pc.model.chat.last
        return "\(last?.id ?? "")|\(last?.text.count ?? 0)|\(last?.steps.map(\.status).joined() ?? "")|\(last?.finished == true)|\(pc.mochiApprovals.count)"
    }

    /// Mochi's answer being written, which a request for your OK belongs to: Allow and Deny go right under it.
    private var waitingEntryID: String? {
        guard !pc.mochiApprovals.isEmpty,
              let last = pc.model.chat.last(where: { $0.role == .mochi }), !last.finished else { return nil }
        return last.id
    }

    /// The newest of Mochi's messages: the only one whose face moves (the others are drawn still).
    private var liveEntryID: String? {
        pc.model.chat.last(where: { $0.role == .mochi })?.id
    }

    var body: some View {
        ScrollViewReader { proxy in
            ZStack(alignment: .bottomTrailing) {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        if pc.model.chat.isEmpty { intro }
                        ForEach(pc.model.chat) { entry in
                            message(entry).id(entry.id)
                        }
                        // A request of Mochi's that no answer here is waiting on (a turn started on the computer).
                        if waitingEntryID == nil {
                            ForEach(pc.mochiApprovals, id: \.fingerprint) { approval in
                                approvalCard(approval)
                            }
                        }
                        Color.clear.frame(height: 1).id(Self.end)
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
                    .padding(.bottom, 8)
                }
                .scrollDismissesKeyboard(.interactively)
                .onScrollGeometryChange(for: ChatScroll.self) { geometry in
                    let bottom = geometry.contentOffset.y + geometry.containerSize.height - geometry.contentInsets.bottom
                    let visible = geometry.containerSize.height - geometry.contentInsets.top - geometry.contentInsets.bottom
                    return ChatScroll(atEnd: bottom >= geometry.contentSize.height - 72, visible: visible)
                } action: { old, new in
                    if new.visible < old.visible - 1, old.atEnd {
                        // The keyboard came up, or the field grew, while the end was on screen: it stays there.
                        proxy.scrollTo(Self.end, anchor: .bottom)
                        atEnd = true
                    } else {
                        atEnd = new.atEnd
                    }
                }
                if !atEnd {
                    Button {
                        withAnimation(.spring(duration: 0.4)) { proxy.scrollTo(Self.end, anchor: .bottom) }
                    } label: {
                        Image(systemName: "arrow.down")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(.white)
                            .frame(width: 42, height: 42)
                            .coucouGlass(.regular, in: Circle())
                    }
                    .buttonStyle(PressableButtonStyle())
                    .accessibilityLabel("Go to the latest message")
                    .padding(.trailing, 16)
                    .padding(.bottom, 10)
                    .transition(.scale(scale: 0.6).combined(with: .opacity))
                }
            }
            .animation(.spring(duration: 0.3), value: atEnd)
            .onAppear { proxy.scrollTo(Self.end, anchor: .bottom) }
            // A new message (yours, and his answer to come): to the end, smoothly.
            .onChange(of: pc.model.chat.count) {
                withAnimation(.easeOut(duration: 0.3)) { proxy.scrollTo(Self.end, anchor: .bottom) }
            }
            // His words as they come: followed only while the end is on screen, and without an animation each time,
            // so the conversation doesn't jump at every word.
            .onChange(of: followKey) {
                if atEnd { proxy.scrollTo(Self.end, anchor: .bottom) }
            }
        }
        .background { DeepBackdrop() }
        .safeAreaInset(edge: .bottom) { composer }
        .toolbarVisibility(inTab ? .automatic : .hidden, for: .tabBar)
        .navigationTitle("Mochi")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) { title }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    pc.newChat()
                    answered = [:]
                    Haptics.impact()
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .disabled(pc.model.chat.isEmpty)
                .accessibilityLabel("New chat")
            }
            ToolbarItem(placement: .topBarTrailing) { menu }
        }
        .onAppear { pc.refreshModels() }
        .onDisappear {
            dictation.stop()
            MochiVoice.shared.stop()
        }
        .onChange(of: dictation.transcript) { _, words in
            guard dictation.isRecording else { return }
            text = dictation.prefix + words
            // A short pause after the words: done talking, the message goes.
            silence?.cancel()
            guard !words.trimmingCharacters(in: .whitespaces).isEmpty else { return }
            silence = Task { @MainActor in
                try? await Task.sleep(for: .seconds(1.8))
                guard !Task.isCancelled, dictation.isRecording else { return }
                dictation.stop()
                send(spoken: true)
            }
        }
        .onChange(of: pc.chatRunning) { was, now in
            guard was, !now, spoken else { return }
            spoken = false
            // Not over your voice: talking again to Mochi while he answered wins.
            if readAloud, !dictation.isRecording, let answer = pc.model.chat.last, answer.role == .mochi, !answer.failed {
                MochiVoice.shared.say(answer.text, language: dictation.localeID)
            }
        }
        .sheet(isPresented: $scanning) {
            PCScanSheet(pc: pc)
        }
    }

    // MARK: Messages

    @ViewBuilder private func message(_ entry: PCChatEntry) -> some View {
        switch entry.role {
        case .you:
            PCChatBubble(entry: entry)
                .contextMenu { menu(for: entry) }
        case .mochi:
            VStack(alignment: .leading, spacing: 10) {
                PCChatBubble(entry: entry, live: entry.id == liveEntryID)
                    .contextMenu { menu(for: entry) }
                if entry.id == waitingEntryID {
                    ForEach(pc.mochiApprovals, id: \.fingerprint) { approval in
                        approvalCard(approval)
                    }
                }
            }
        }
    }

    /// Long press on a message: copy it, hear it, or send it again (for his answer, the message it answered).
    @ViewBuilder private func menu(for entry: PCChatEntry) -> some View {
        if !entry.text.isEmpty {
            Button {
                UIPasteboard.general.string = entry.text
            } label: {
                Label("Copy", systemImage: "doc.on.doc")
            }
            Button {
                dictation.stop()
                MochiVoice.shared.say(entry.text, language: dictation.localeID)
            } label: {
                Label("Read aloud", systemImage: "speaker.wave.2")
            }
            if MochiVoice.shared.isSpeaking {
                Button {
                    MochiVoice.shared.stop()
                } label: {
                    Label("Stop reading", systemImage: "speaker.slash")
                }
            }
        }
        if let again = prompt(for: entry) {
            Button {
                sendAgain(again)
            } label: {
                Label("Send again", systemImage: "arrow.clockwise")
            }
            .disabled(!canSend)
        }
    }

    /// What "Send again" sends: your message itself; for Mochi's answer, the message of yours it answered.
    private func prompt(for entry: PCChatEntry) -> String? {
        switch entry.role {
        case .you:
            return entry.text.isEmpty ? nil : entry.text
        case .mochi:
            guard let index = pc.model.chat.firstIndex(where: { $0.id == entry.id }) else { return nil }
            return pc.model.chat[..<index].last(where: { $0.role == .you })?.text
        }
    }

    // MARK: Requests for your OK

    /// Mochi's request with Allow (Face ID, as everywhere) and Deny, sent the way the Agents tab sends them.
    private func approvalCard(_ approval: PCApproval) -> some View {
        let decided = answered[approval.fingerprint]
        return VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Circle().fill(Color.orange).frame(width: 8, height: 8)
                    .shadow(color: .orange.opacity(0.8), radius: 4)
                Text("Mochi asks to run")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.orange)
                Spacer(minLength: 0)
                Text(pc.name)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Text(Self.command(approval))
                .font(.callout.monospaced())
                .foregroundStyle(.white.opacity(0.92))
                .lineLimit(6)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let decided {
                Label(decided ? "Allowed, sent to \(pc.name)" : "Denied, sent to \(pc.name)",
                      systemImage: decided ? "checkmark.circle.fill" : "xmark.circle.fill")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(decided ? CoucouPalette.ok : CoucouPalette.danger)
                    .transition(.scale(scale: 0.8).combined(with: .opacity))
            } else {
                ApprovalChoiceButtons(disabled: answering != nil) {
                    Task { await decide(approval, allow: false) }
                } allow: {
                    Task { await decide(approval, allow: true) }
                }
                Text("Allow asks for Face ID. The request expires after 2 minutes.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let approvalError {
                Text(approvalError).font(.caption).foregroundStyle(CoucouPalette.danger)
            }
        }
        .padding(16)
        .coucouGlass(.regular, cornerRadius: 22, tint: .orange)
        .padding(.leading, 44)
        .animation(.spring(duration: 0.45, bounce: 0.3), value: decided)
    }

    /// "Bash · npm test" as the Agents tab shows it; "…" when the computer sent only the start of it.
    private static func command(_ approval: PCApproval) -> String {
        let what = approval.tool.isEmpty || approval.tool == "Bash" ? approval.command : "\(approval.tool) · \(approval.command)"
        if what.isEmpty { return String(localized: "A permission") }
        return approval.truncated ? what + "…" : what
    }

    private func decide(_ approval: PCApproval, allow: Bool) async {
        approvalError = nil
        if allow, approval.truncated {
            // The computer ignores an Allow for a command it did not send whole.
            approvalError = String(localized: "This command is longer than what reached the iPhone: deny it here, or answer it on \(pc.name).")
            Haptics.warning()
            return
        }
        answering = approval.fingerprint
        defer { answering = nil }
        if allow {
            guard await OwnerCheck.confirm(reason: String(localized: "Allow this command on \(pc.name)")) else {
                approvalError = String(localized: "Face ID didn't confirm. Nothing was sent.")
                Haptics.warning()
                return
            }
        }
        let link = PhoneLink.shared
        let summary = String(Self.command(approval).prefix(200))
        if await link.decide(allow ? .allow : .deny, fingerprint: approval.fingerprint, pillId: approval.pillId,
                             summary: summary) {
            answered[approval.fingerprint] = allow
            if allow { Haptics.success() } else { Haptics.impact() }
        } else {
            approvalError = link.lastPong ?? String(localized: "Can't reach \(pc.name).")
            Haptics.error()
        }
    }

    // MARK: Sending

    private func send(spoken voice: Bool = false) {
        silence?.cancel()
        dictation.stop()
        pc.sendChat(text)
        if pc.lastError == nil {
            text = ""
            spoken = voice
            Haptics.impact()
        } else {
            Haptics.warning()
        }
    }

    private func sendAgain(_ message: String) {
        guard canSend else { return }
        pc.sendChat(message)
        if pc.lastError == nil {
            spoken = false
            Haptics.impact()
        } else {
            Haptics.warning()
        }
    }

    // MARK: Around the conversation

    /// "Mochi" with the computer and its link under it.
    private var title: some View {
        VStack(spacing: 1) {
            Text("Mochi").font(.headline)
            HStack(spacing: 5) {
                Circle().fill(pc.status.color).frame(width: 6, height: 6)
                Text(pc.isPaired ? pc.name : String(localized: "Not paired"))
                    .lineLimit(1)
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }

    private var intro: some View {
        VStack(spacing: 14) {
            MochiLive(state: pc.status == .connected ? .idle : .sleeping, bodyHex: PCLink.mochiColor)
                .frame(width: 78, height: 78)
                .padding(16)
                .coucouGlass(.clear, in: Circle())
            Text(pc.isPaired ? "Mochi on \(pc.name)" : "Chat with Mochi")
                .font(.title3.weight(.semibold))
                .multilineTextAlignment(.center)
            Text(introText)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            if !pc.isPaired {
                Button {
                    scanning = true
                } label: {
                    Label("Scan the pairing code", systemImage: "qrcode.viewfinder")
                        .font(.headline)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 4)
                }
                .coucouGlassButton(prominent: true)
                .padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 12)
        .padding(.top, 44)
    }

    private var introText: LocalizedStringKey {
        if !pc.isPaired {
            return "Pair this iPhone with Coucou on your Windows or Linux computer, then talk to Mochi from here, by text or by voice."
        }
        return handsOn
            ? "Ask Mochi to do something on \(pc.name): look through a folder, change a file, run the tests…"
            : "Chat with Mochi, with the AI providers set up on \(pc.name)."
    }

    private var menu: some View {
        Menu {
            if !pc.chatModels.isEmpty {
                Picker("Model", selection: Binding(get: { pc.chatModel }, set: { pc.setChatModel($0) })) {
                    ForEach(pc.chatModels) { model in
                        Text(model.label).tag(model.id)
                    }
                }
            }
            if pc.model.handsAllowed {
                Toggle(isOn: Binding(get: { pc.chatHands }, set: { pc.setChatHands($0) })) {
                    Label("Mochi's hands", systemImage: "hand.raised")
                }
            }
            Toggle(isOn: $readAloud) {
                Label("Read answers to spoken messages aloud", systemImage: "speaker.wave.2")
            }
            Picker("Dictation language", selection: $dictation.localeID) {
                ForEach(Dictation.languages, id: \.self) { id in
                    Text(Locale.current.localizedString(forIdentifier: id) ?? id).tag(id)
                }
            }
            Button {
                pc.newChat()
                answered = [:]
                Haptics.impact()
            } label: {
                Label("New chat", systemImage: "square.and.pencil")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .accessibilityLabel("Chat options")
    }

    // MARK: Composer

    /// Quick prompts on an empty chat, the model and hands chips, then one glass capsule with the field and the mic,
    /// and the send button that turns into stop while Mochi answers.
    private var composer: some View {
        let empty = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let running = pc.chatRunning
        let showsSend = running || !empty
        return VStack(alignment: .leading, spacing: 8) {
            if pc.model.chat.isEmpty, canSend { prompts }
            CoucouGlassGroup(spacing: 8) {
                VStack(alignment: .leading, spacing: 8) {
                    chips
                    HStack(alignment: .bottom, spacing: 8) {
                        field
                        if showsSend {
                            sendButton(running: running)
                                .transition(.scale(scale: 0.5).combined(with: .opacity))
                        }
                    }
                }
            }
            if let note {
                Label(note, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 6)
        .animation(.spring(duration: 0.35, bounce: 0.2), value: showsSend)
        .background(alignment: .bottom) {
            LinearGradient(colors: [.clear, CoucouPalette.abyss.opacity(0.85), CoucouPalette.abyss],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
                .allowsHitTesting(false)
        }
    }

    private var field: some View {
        HStack(alignment: .bottom, spacing: 2) {
            TextField(handsOn ? "Ask Mochi to do something…" : "Message Mochi…", text: $text, axis: .vertical)
                .lineLimit(1...8)
                .focused($focused)
                .padding(.leading, 16)
                .padding(.vertical, 11)
            // Talk to Mochi: "liga a TV da sala e abre o YouTube". A pause sends it.
            Button {
                MochiVoice.shared.stop()
                Task { await dictation.toggle(startingFrom: text) }
            } label: {
                Image(systemName: dictation.isRecording ? "waveform" : "mic.fill")
                    .font(.body)
                    .foregroundStyle(dictation.isRecording ? CoucouPalette.danger : Color.secondary)
                    .symbolEffect(.variableColor.iterative, isActive: dictation.isRecording)
                    .frame(width: 38, height: 42)
            }
            .disabled(pc.chatRunning || pc.status != .connected)
            .accessibilityLabel(dictation.isRecording ? "Stop dictation" : "Dictate")
            .padding(.trailing, 4)
        }
        .coucouGlass(.regular, cornerRadius: 22)
        .coucouGlassID("field", in: glass)
    }

    private func sendButton(running: Bool) -> some View {
        Button {
            if running {
                pc.stopChat()
                Haptics.impact()
            } else {
                send()
            }
        } label: {
            Image(systemName: running ? "stop.fill" : "arrow.up")
                .font(.body.weight(.bold))
                .foregroundStyle(.white)
                .frame(width: 44, height: 44)
                .contentTransition(.symbolEffect(.replace))
                .coucouGlass(.prominent, in: Circle(), tint: running ? CoucouPalette.danger : CoucouPalette.accent)
                .coucouGlassID("send", in: glass)
        }
        .buttonStyle(PressableButtonStyle())
        .disabled(!running && pc.status != .connected)
        .accessibilityLabel(running ? "Stop" : "Send")
    }

    /// Which model answers and whether he may use his hands, a tap away above the field.
    @ViewBuilder private var chips: some View {
        if !pc.chatModels.isEmpty || pc.model.handsAllowed {
            HStack(spacing: 8) {
                if !pc.chatModels.isEmpty {
                    Menu {
                        Picker("Model", selection: Binding(get: { pc.chatModel }, set: { pc.setChatModel($0) })) {
                            ForEach(pc.chatModels) { model in
                                Text(model.label).tag(model.id)
                            }
                        }
                    } label: {
                        chip(modelLabel, systemImage: "cpu", on: false)
                    }
                    .disabled(pc.chatRunning)
                    .accessibilityLabel("Model")
                }
                if pc.model.handsAllowed {
                    Button {
                        pc.setChatHands(!pc.chatHands)
                        Haptics.impact()
                    } label: {
                        chip(pc.chatHands ? String(localized: "With his hands") : String(localized: "Without his hands"),
                             systemImage: pc.chatHands ? "hand.raised.fill" : "hand.raised.slash", on: pc.chatHands)
                    }
                    .buttonStyle(PressableButtonStyle())
                    .disabled(pc.chatRunning)
                }
                Spacer(minLength: 0)
            }
        }
    }

    private var modelLabel: String {
        pc.chatModels.first { $0.id == pc.chatModel }?.label ?? pc.chatModel
    }

    private func chip(_ title: String, systemImage: String, on: Bool) -> some View {
        Label(title, systemImage: systemImage)
            .font(.footnote.weight(.semibold))
            .lineLimit(1)
            .foregroundStyle(on ? Color.white : Color.primary)
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .coucouGlass(.regular, in: Capsule(), tint: on ? CoucouPalette.accent : nil, interactive: true)
    }

    /// Ideas for a first message: a tap puts one in the field, for you to finish or send.
    private var prompts: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            CoucouGlassGroup(spacing: 8) {
                HStack(spacing: 8) {
                    ForEach(quickPrompts, id: \.self) { prompt in
                        Button {
                            text = prompt
                            focused = true
                            Haptics.impact()
                        } label: {
                            Text(prompt)
                                .font(.footnote.weight(.medium))
                                .lineLimit(1)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 9)
                                .coucouGlass(.regular, in: Capsule(), interactive: true)
                        }
                        .buttonStyle(PressableButtonStyle())
                    }
                }
                .padding(.horizontal, 2)
                .padding(.vertical, 2)
            }
        }
        .scrollClipDisabled()
    }

    private var quickPrompts: [String] {
        guard handsOn else {
            return [String(localized: "Explain this error: "),
                    String(localized: "Write a short message that says "),
                    String(localized: "Summarize this text: "),
                    String(localized: "Give me three ideas for ")]
        }
        var list = [String(localized: "What's in my Downloads folder?"),
                    String(localized: "Find the biggest files in my Documents"),
                    String(localized: "Run the tests and tell me what fails")]
        if pc.offers("home") { list.insert(String(localized: "Which devices of my home are on?"), at: 0) }
        return list
    }

    /// What stands in the way of chatting, if anything.
    private var note: String? {
        if let error = pc.lastError { return error }
        guard pc.isPaired else { return nil }
        if pc.status != .connected { return pc.status.text }
        if !pc.model.chatAllowed {
            return String(localized: "The chat is off on \(pc.name): Settings → Phone → \"Let the phone chat with my AI providers\".")
        }
        if pc.chatModels.isEmpty {
            return String(localized: "No model allowed for the phone yet: tick one in Settings → Phone on the computer.")
        }
        return nil
    }
}

/// Where the conversation's scroll is, as the chat needs it.
private struct ChatScroll: Equatable, Sendable {
    var atEnd: Bool
    var visible: CGFloat
}

/// One message of the chat: yours on the right, tinted; Mochi's on the left in glass, with his face, what he did on
/// the computer, and his answer.
struct PCChatBubble: View {
    let entry: PCChatEntry
    /// The newest of Mochi's messages: his face moves. Older ones are drawn still, costing nothing.
    var live = false

    var body: some View {
        switch entry.role {
        case .you:
            HStack {
                Spacer(minLength: 56)
                Text(entry.text)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .coucouGlass(.prominent, cornerRadius: 20, tint: CoucouPalette.accent)
            }
        case .mochi:
            HStack(alignment: .top, spacing: 10) {
                avatar
                VStack(alignment: .leading, spacing: 10) {
                    if !entry.steps.isEmpty {
                        PCStepList(steps: entry.steps)
                    }
                    answer
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .coucouGlass(.regular, cornerRadius: 20, tint: entry.failed ? CoucouPalette.warn : nil)
                Spacer(minLength: 20)
            }
        }
    }

    /// Mochi himself, drawn in code: thinking, working, waiting for you, or done.
    private var avatar: some View {
        Group {
            if live {
                MochiLive(state: mood, bodyHex: PCLink.mochiColor, fps: 30)
            } else {
                MochiStill(state: mood, bodyHex: PCLink.mochiColor, showBadge: false)
            }
        }
        .padding(5)
        .frame(width: 34, height: 34)
        .coucouGlass(.regular, in: Circle())
    }

    private var mood: BotState {
        if entry.failed { return .error }
        if entry.finished { return .idle }
        if entry.steps.last?.status == "waiting" { return .approval }
        return entry.text.isEmpty ? .thinking : .working
    }

    @ViewBuilder private var answer: some View {
        if entry.text.isEmpty && !entry.finished {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text(entry.steps.last?.status == "waiting" ? "Waiting for your OK…" : "Mochi is on it…")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        } else if entry.failed {
            Label {
                Text(entry.text)
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill")
            }
            .font(.callout)
            .foregroundStyle(CoucouPalette.warn)
        } else if entry.text.isEmpty {
            Text("No answer.")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else {
            ChatMarkdownView(text: entry.text)
        }
    }
}

/// What Mochi did in a turn, step by step; a long list folds to its last steps.
struct PCStepList: View {
    let steps: [PCChatStep]
    @State private var open = false
    private let folded = 4

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if steps.count > folded {
                Button {
                    withAnimation(.easeOut(duration: 0.2)) { open.toggle() }
                } label: {
                    Label(open ? "Show less" : "\(steps.count) steps · show all",
                          systemImage: open ? "chevron.up" : "chevron.down")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
            ForEach(open || steps.count <= folded ? steps : Array(steps.suffix(folded)), id: \.index) { step in
                PCStepRow(step: step)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

/// "Bash · npm test" with how it went.
struct PCStepRow: View {
    let step: PCChatStep

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            mark.frame(width: 16)
            Text(step.text)
                .font(.footnote.monospaced())
                .lineLimit(2)
                .foregroundStyle(step.status == "denied" ? Color.secondary : Color.primary)
        }
    }

    @ViewBuilder private var mark: some View {
        switch step.status {
        case "ok":
            Image(systemName: "checkmark.circle.fill").foregroundStyle(CoucouPalette.ok)
        case "error":
            Image(systemName: "xmark.circle.fill").foregroundStyle(CoucouPalette.danger)
        case "denied":
            Image(systemName: "hand.raised.fill").foregroundStyle(.secondary)
        case "waiting":
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
                .symbolEffect(.pulse)
        default:
            ProgressView().controlSize(.mini)
        }
    }
}

// MARK: - Markdown in blocks

/// A chat answer cut into what a chat shows differently. Made for text that is still arriving: a code block not
/// closed yet runs to the end, half a table is a table.
enum ChatBlock: Equatable, Sendable {
    case paragraph(String)
    case heading(level: Int, text: String)
    case bullet(depth: Int, text: String)
    case numbered(number: String, depth: Int, text: String)
    case quote(String)
    case code(language: String, text: String)
    case table([String])
    case rule
}

enum ChatMarkdown {
    static func blocks(_ markdown: String) -> [ChatBlock] {
        var reader = Reader()
        for line in markdown.components(separatedBy: "\n") {
            reader.read(line.hasSuffix("\r") ? String(line.dropLast()) : line)
        }
        return reader.finish()
    }

    /// Bold, italics, code and links inside a block, as the island shows them. Links open only web pages and mail
    /// (nothing that opens an app, Coucou's own pairing links included); plain text when it does not parse.
    static func inline(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        guard var rich = try? AttributedString(markdown: text, options: options) else { return AttributedString(text) }
        let blocked = rich.runs.compactMap { run -> Range<AttributedString.Index>? in
            guard let url = run.link else { return nil }
            return ["https", "http", "mailto"].contains(url.scheme?.lowercased() ?? "") ? nil : run.range
        }
        for range in blocked { rich[range].link = nil }
        return rich
    }

    private struct Reader {
        var blocks: [ChatBlock] = []
        var paragraph: [String] = []
        var quote: [String] = []
        var table: [String] = []
        var code: (language: String, lines: [String])?

        mutating func read(_ line: String) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if var open = code {
                if trimmed.hasPrefix("```") {
                    blocks.append(.code(language: open.language, text: open.lines.joined(separator: "\n")))
                    code = nil
                } else {
                    open.lines.append(line)
                    code = open
                }
                return
            }
            if trimmed.hasPrefix("```") {
                flush()
                code = (language: String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces), lines: [])
                return
            }
            if trimmed.isEmpty {
                flush()
                return
            }
            if trimmed.hasPrefix("|") {
                flushParagraph()
                flushQuote()
                table.append(trimmed)
                return
            }
            flushTable()
            if trimmed.hasPrefix(">") {
                flushParagraph()
                quote.append(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces))
                return
            }
            flushQuote()
            if let heading = ChatMarkdown.heading(trimmed) {
                flushParagraph()
                blocks.append(heading)
            } else if ChatMarkdown.isRule(trimmed) {
                flushParagraph()
                blocks.append(.rule)
            } else if let item = ChatMarkdown.item(line) {
                flushParagraph()
                blocks.append(item)
            } else {
                paragraph.append(trimmed)
            }
        }

        mutating func finish() -> [ChatBlock] {
            if let open = code {
                blocks.append(.code(language: open.language, text: open.lines.joined(separator: "\n")))
                code = nil
            }
            flush()
            return blocks
        }

        private mutating func flush() {
            flushParagraph()
            flushQuote()
            flushTable()
        }

        private mutating func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            blocks.append(.paragraph(paragraph.joined(separator: "\n")))
            paragraph = []
        }

        private mutating func flushQuote() {
            guard !quote.isEmpty else { return }
            blocks.append(.quote(quote.joined(separator: "\n")))
            quote = []
        }

        private mutating func flushTable() {
            guard !table.isEmpty else { return }
            blocks.append(.table(table))
            table = []
        }
    }

    /// "## Title" (a space after the marks, at most six).
    fileprivate static func heading(_ line: String) -> ChatBlock? {
        let marks = line.prefix { $0 == "#" }.count
        guard (1...6).contains(marks) else { return nil }
        let rest = line.dropFirst(marks)
        guard rest.first == " " else { return nil }
        return .heading(level: marks, text: rest.trimmingCharacters(in: .whitespaces))
    }

    /// "---", "***", "___" (spaces allowed between them).
    fileprivate static func isRule(_ line: String) -> Bool {
        let marks = line.filter { $0 != " " }
        guard marks.count >= 3, let first = marks.first, "-*_".contains(first) else { return false }
        return marks.allSatisfy { $0 == first }
    }

    /// "- item", "* item", "• item", "1. item", "2) item"; two spaces of indent per level.
    fileprivate static func item(_ line: String) -> ChatBlock? {
        var indent = 0
        var body = Substring(line)
        while let first = body.first, first == " " || first == "\t" {
            indent += first == "\t" ? 4 : 1
            body = body.dropFirst()
        }
        let depth = min(indent / 2, 3)
        if let first = body.first, "-*+•".contains(first), body.dropFirst().first == " " {
            return .bullet(depth: depth, text: body.dropFirst(2).trimmingCharacters(in: .whitespaces))
        }
        let digits = body.prefix { $0.isASCII && $0.isNumber }
        guard (1...3).contains(digits.count) else { return nil }
        let rest = body.dropFirst(digits.count)
        guard let mark = rest.first, mark == "." || mark == ")", rest.dropFirst().first == " " else { return nil }
        return .numbered(number: String(digits), depth: depth, text: rest.dropFirst(2).trimmingCharacters(in: .whitespaces))
    }
}

/// An answer in blocks.
struct ChatMarkdownView: View {
    let text: String

    var body: some View {
        let blocks = ChatMarkdown.blocks(text)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                ChatBlockView(block: block)
            }
        }
    }
}

private struct ChatBlockView: View {
    let block: ChatBlock

    var body: some View {
        switch block {
        case .paragraph(let text):
            Text(ChatMarkdown.inline(text))
                .fixedSize(horizontal: false, vertical: true)
        case .heading(let level, let text):
            Text(ChatMarkdown.inline(text))
                .font(Self.font(heading: level))
                .padding(.top, 2)
        case .bullet(let depth, let text):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                if text.hasPrefix("[ ] ") || text.hasPrefix("[x] ") || text.hasPrefix("[X] ") {
                    let done = !text.hasPrefix("[ ] ")
                    Image(systemName: done ? "checkmark.square.fill" : "square")
                        .foregroundStyle(done ? CoucouPalette.ok : Color.secondary)
                    Text(ChatMarkdown.inline(String(text.dropFirst(4))))
                } else {
                    Text(verbatim: depth == 0 ? "•" : "◦")
                        .foregroundStyle(.secondary)
                    Text(ChatMarkdown.inline(text))
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            .padding(.leading, CGFloat(depth) * 16)
        case .numbered(let number, let depth, let text):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(verbatim: "\(number).")
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                Text(ChatMarkdown.inline(text))
            }
            .fixedSize(horizontal: false, vertical: true)
            .padding(.leading, CGFloat(depth) * 16)
        case .quote(let text):
            HStack(alignment: .top, spacing: 10) {
                Capsule()
                    .fill(CoucouPalette.accent.opacity(0.7))
                    .frame(width: 3)
                Text(ChatMarkdown.inline(text))
                    .foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
        case .code(let language, let text):
            ChatCodeBlock(language: language, code: text)
        case .table(let rows):
            ChatTable(rows: rows)
        case .rule:
            Divider().padding(.vertical, 2)
        }
    }

    private static func font(heading level: Int) -> Font {
        switch level {
        case 1: .title3.weight(.bold)
        case 2: .headline
        default: .subheadline.weight(.semibold)
        }
    }
}

/// A block of code: its language, Copy, and the code scrolling sideways rather than wrapping.
struct ChatCodeBlock: View {
    let language: String
    let code: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? String(localized: "Code") : language)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 8)
                Button {
                    UIPasteboard.general.string = code
                    copied = true
                    Haptics.success()
                } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(copied ? CoucouPalette.ok : Color.secondary)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            Rectangle().fill(CoucouPalette.hairline).frame(height: 0.75)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(verbatim: code.isEmpty ? " " : code)
                    .font(.footnote.monospaced())
                    .foregroundStyle(.white.opacity(0.9))
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(12)
            }
        }
        .background(Color.black.opacity(0.38), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(CoucouPalette.hairline, lineWidth: 0.75))
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(1.5))
            copied = false
        }
    }
}

/// A Markdown table, its first row as titles, scrolling sideways when it is wide.
struct ChatTable: View {
    let rows: [String]

    /// The cells of each row, without the line of dashes under the titles.
    private var cells: [[String]] {
        rows.compactMap { row -> [String]? in
            var parts = row.split(separator: "|", omittingEmptySubsequences: false)
                .map { $0.trimmingCharacters(in: .whitespaces) }
            if parts.first == "" { parts.removeFirst() }
            if parts.last == "" { parts.removeLast() }
            let underline = !parts.isEmpty && parts.allSatisfy { part in
                part.contains("-") && part.allSatisfy { $0 == "-" || $0 == ":" }
            }
            return underline || parts.isEmpty ? nil : parts
        }
    }

    var body: some View {
        let table = cells
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 6) {
                ForEach(Array(table.enumerated()), id: \.offset) { index, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                            Text(ChatMarkdown.inline(cell))
                                .font(index == 0 ? .footnote.weight(.semibold) : .footnote)
                        }
                    }
                    if index == 0, table.count > 1 {
                        Divider().gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .padding(12)
        }
        .background(Color.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}
