import SwiftUI
import UIKit

// The screens of the link to a Windows or Linux computer (PCLink): pairing with the code Coucou shows there, the
// link's state, the chat with Mochi on the computer (his hands included) and, in a sideloaded build that has no
// Mac to talk to, the first-launch guide. New screens in the app's own style; the Mac's screens are unchanged.

extension PCPairing: Identifiable {
    var id: String { computer.fingerprint + token }
}

extension SessionItem {
    /// Where the agent runs, for "sent to …" and Face ID's reason: the Mac, or the paired computer by its name.
    var hostName: String {
        guard fromComputer else { return "your Mac" }
        return macName.isEmpty ? "your computer" : macName
    }
}

extension PCLink.Status {
    var text: String {
        switch self {
        case .unpaired: "Not paired"
        case .connecting: "Connecting…"
        case .searching: "Looking for it on the network…"
        case .connected: "Connected"
        case .offline(let reason): reason.isEmpty ? "Not connected" : reason
        case .refused: "It refused this iPhone: pair again with the code it shows now."
        }
    }

    var color: Color {
        switch self {
        case .connected: .green
        case .connecting, .searching: .secondary
        case .refused: .red
        case .unpaired, .offline: .orange
        }
    }

    /// One or two words, for the Settings row.
    var shortText: String {
        switch self {
        case .unpaired: "Not paired"
        case .connecting, .searching: "Connecting…"
        case .connected: "Connected"
        case .offline: "Not connected"
        case .refused: "Pair again"
        }
    }

    var mochi: BotState {
        switch self {
        case .connected: .idle
        case .connecting, .searching: .searching
        case .refused: .error
        case .unpaired, .offline: .sleeping
        }
    }
}

/// Settings → Windows or Linux computer.
struct PCLinkSettingsView: View {
    let pc: PCLink
    @State private var link = ""
    @State private var pending: PCPairing?
    @State private var error: String?
    @State private var scanning = false

    var body: some View {
        List {
            Section {
                HStack(spacing: 14) {
                    MochiLive(state: pc.status.mochi)
                        .padding(6)
                        .frame(width: 56, height: 56)
                        .background(Color(white: 0.16), in: RoundedRectangle(cornerRadius: 14))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(pc.computer.map { $0.name.isEmpty ? "Your computer" : $0.name } ?? "No computer yet")
                            .font(.headline)
                        Text(pc.status.text)
                            .font(.subheadline)
                            .foregroundStyle(pc.status.color)
                    }
                }
                .padding(.vertical, 4)
                if pc.isPaired, ![.connected, .connecting, .searching, .refused].contains(pc.status) {
                    Button("Try again now") { pc.reconnectIfNeeded() }
                }
            }

            if let computer = pc.computer {
                Section {
                    LabeledContent("Address", value: "\(computer.host):\(computer.port)")
                    LabeledContent("Certificate") {
                        Text(computer.shortFingerprint).font(.callout.monospaced())
                    }
                    if !pc.model.os.isEmpty {
                        LabeledContent("System", value: pc.model.os.capitalized)
                    }
                } header: {
                    Text("Link")
                } footer: {
                    Text("Only this computer is accepted: its certificate is checked at every connection. If its address changes, Coucou finds it again on the local network.")
                }

                if pc.status == .connected {
                    Section {
                        capability("Sessions and permission requests", on: true)
                        capability("Chat with Mochi", on: pc.model.chatAllowed)
                        capability("Mochi's hands from the phone", on: pc.model.handsAllowed)
                        capability("Answer Claude Code's questions", on: pc.model.caps.contains("answers"))
                        capability("Session details", on: pc.model.caps.contains("details"))
                    } header: {
                        Text("Allowed by the computer")
                    } footer: {
                        Text("Each one is a switch in Coucou's Settings → Phone on the computer, off until you turn it on there. Mochi's hands also need Settings → Mochi's hands.")
                    }
                }
            }

            Section {
                Button {
                    scanning = true
                } label: {
                    Label("Scan the code", systemImage: "qrcode.viewfinder")
                }
                TextField("coucou://pair?…", text: $link, axis: .vertical)
                    .font(.callout.monospaced())
                    .lineLimit(1...4)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Paste the link") {
                    link = UIPasteboard.general.string ?? ""
                    review()
                }
                Button("Pair") { review() }
                    .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.red)
                }
            } header: {
                Text(pc.isPaired ? "Pair again" : "Pair")
            } footer: {
                Text("On the computer: Coucou → Settings → Phone (Android and iPhone) → turn the link on and show the pairing code, then scan it here. Or copy the link there and paste it here. (The iPhone's Camera app can't open Coucou codes.)")
            }

            if pc.isPaired {
                Section {
                    Button("Forget this computer", role: .destructive) {
                        pc.unpair()
                        Haptics.impact()
                    }
                }
            }

            Section {
                Text("Requests reach the iPhone while Coucou is open, and for about half a minute after you leave it: iOS then pauses the app, and only Apple's push could wake it, which a direct link has not. Coucou connects again as soon as you open it.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(Color.black)
        .navigationTitle("Windows or Linux computer")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $pending) { pairing in
            PCPairingConfirm(pairing: pairing, pc: pc) { link = "" }
        }
        .sheet(isPresented: $scanning) {
            PCScanSheet(pc: pc) { link = "" }
        }
    }

    private func review() {
        error = nil
        guard let pairing = PCPairing.parse(link) else {
            error = "This isn't a Coucou pairing link. It starts with coucou://pair?v=1"
            Haptics.warning()
            return
        }
        pending = pairing
    }

    private func capability(_ title: String, on: Bool) -> some View {
        LabeledContent(title) {
            Image(systemName: on ? "checkmark.circle.fill" : "minus.circle")
                .foregroundStyle(on ? Color.green : Color.secondary)
        }
    }
}

/// Before pairing: which computer, where, and what pairing means. Nothing is paired without this tap.
struct PCPairingConfirm: View {
    let pairing: PCPairing
    let pc: PCLink
    var paired: () -> Void = {}
    @Environment(\.dismiss) private var dismiss
    @State private var failed = false

    private var replaces: Bool {
        guard let current = pc.computer else { return false }
        return current.fingerprint != pairing.computer.fingerprint
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                MochiLive(state: .question)
                    .frame(width: 72, height: 72)
                Text("Pair with \(pairing.computer.name.isEmpty ? "this computer" : pairing.computer.name)?")
                    .font(.title2.weight(.bold))
                Text("This iPhone will show the agents running on it and answer their permission requests, on your local network only. Allow always asks for Face ID.")
                    .foregroundStyle(.secondary)
                VStack(spacing: 8) {
                    LabeledContent("Address", value: "\(pairing.computer.host):\(pairing.computer.port)")
                    LabeledContent("Certificate") {
                        Text(pairing.computer.shortFingerprint).font(.callout.monospaced())
                    }
                }
                .font(.callout)
                .padding(14)
                .glassCard(cornerRadius: 16)
                if replaces {
                    Text("It replaces \(pc.name), paired before.")
                        .font(.footnote)
                        .foregroundStyle(.orange)
                }
                if failed {
                    Text(pc.lastError ?? "Couldn't pair.")
                        .font(.footnote)
                        .foregroundStyle(.red)
                }
            }
            .padding(24)
        }
        .background(Color.black)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                Button {
                    if pc.pair(pairing) {
                        Haptics.success()
                        paired()
                        dismiss()
                    } else {
                        failed = true
                        Haptics.error()
                    }
                } label: {
                    Text("Pair").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                Button("Cancel", role: .cancel) { dismiss() }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
        }
        .presentationDetents([.large])
        .preferredColorScheme(.dark)
    }
}

/// Mochi on the computer, from the iPhone: the computer's chat, with his hands when they are on there (each step
/// shows up as he works; what changes something waits for Allow, here or on the computer).
struct PCChatView: View {
    let pc: PCLink
    @State private var text = ""
    @FocusState private var focused: Bool

    private var scrollKey: String {
        let last = pc.model.chat.last
        return "\(pc.model.chat.count)|\(last?.text.count ?? 0)|\(last?.steps.map(\.status).joined() ?? "")"
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    if pc.model.chat.isEmpty { intro }
                    ForEach(pc.model.chat) { entry in
                        PCChatBubble(entry: entry).id(entry.id)
                    }
                }
                .padding(16)
            }
            .onChange(of: scrollKey) {
                guard let id = pc.model.chat.last?.id else { return }
                withAnimation(.easeOut(duration: 0.25)) { proxy.scrollTo(id, anchor: .bottom) }
            }
        }
        .scrollDismissesKeyboard(.immediately)
        .background(Color.black)
        .safeAreaInset(edge: .bottom) { composer }
        .toolbarVisibility(.hidden, for: .tabBar)
        .navigationTitle("Mochi")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { menu }
        }
        .onAppear { pc.refreshModels() }
    }

    private var intro: some View {
        VStack(spacing: 12) {
            MochiLive(state: pc.status == .connected ? .idle : .sleeping)
                .frame(width: 80, height: 80)
            Text(pc.model.handsAllowed && pc.chatHands
                 ? "Ask Mochi to do something on \(pc.name): look through a folder, change a file, run the tests…"
                 : "Chat with Mochi, with the AI providers set up on \(pc.name).")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 60)
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
            Button {
                pc.newChat()
                Haptics.impact()
            } label: {
                Label("New chat", systemImage: "square.and.pencil")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
    }

    private var composer: some View {
        let empty = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let running = pc.chatRunning
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .bottom, spacing: 8) {
                TextField(pc.model.handsAllowed && pc.chatHands ? "Ask Mochi to do something…" : "Message Mochi…",
                          text: $text, axis: .vertical)
                    .lineLimit(1...8)
                    .focused($focused)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 11)
                    .glassPill(RoundedRectangle(cornerRadius: 22, style: .continuous))
                Button {
                    if running {
                        pc.stopChat()
                        Haptics.impact()
                    } else {
                        pc.sendChat(text)
                        if pc.lastError == nil {
                            text = ""
                            Haptics.impact()
                        } else {
                            Haptics.warning()
                        }
                    }
                } label: {
                    Image(systemName: running ? "stop.fill" : "arrow.up")
                        .font(.body.weight(.bold))
                        .foregroundStyle(running || !empty ? Color.black : Color.secondary)
                        .frame(width: 44, height: 44)
                        .background(running || !empty ? Color.white : Color.white.opacity(0.12), in: Circle())
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(PressableButtonStyle())
                .disabled(!running && (empty || pc.status != .connected))
            }
            if let note {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 14)
        .padding(.bottom, 6)
        .background(alignment: .bottom) {
            LinearGradient(colors: [.clear, .black.opacity(0.85), .black], startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
                .allowsHitTesting(false)
        }
    }

    /// What stands in the way, or which model answers.
    private var note: String? {
        if let error = pc.lastError { return error }
        if pc.status != .connected { return pc.status.text }
        if !pc.model.chatAllowed {
            return "The chat is off on \(pc.name): Settings → Phone → \"Let the phone chat with my AI providers\"."
        }
        if pc.chatModels.isEmpty { return "No model allowed for the phone yet: tick one in Settings → Phone on the computer." }
        let model = pc.chatModels.first { $0.id == pc.chatModel }?.label ?? pc.chatModel
        let hands = pc.model.handsAllowed && pc.chatHands ? " · with his hands" : ""
        return "\(model)\(hands)"
    }
}

/// One message of the chat: yours on the right; Mochi's on the left, with what he did on the computer.
struct PCChatBubble: View {
    let entry: PCChatEntry

    var body: some View {
        switch entry.role {
        case .you:
            HStack {
                Spacer(minLength: 48)
                Text(entry.text)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color.accentColor.opacity(0.85), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .foregroundStyle(.white)
                    .textSelection(.enabled)
            }
        case .mochi:
            VStack(alignment: .leading, spacing: 8) {
                if !entry.steps.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(entry.steps, id: \.index) { step in
                            PCStepRow(step: step)
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .glassCard(cornerRadius: 16)
                }
                if entry.text.isEmpty && !entry.finished {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text(entry.steps.last?.status == "waiting" ? "Waiting for your OK…" : "Mochi is on it…")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                } else if !entry.text.isEmpty {
                    Text(Self.rich(entry.text))
                        .foregroundStyle(entry.failed ? Color.orange : Color.primary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.trailing, 24)
        }
    }

    /// Bold, italics, code and links, as the island shows them; plain text when it does not parse.
    private static func rich(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
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
            Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        case "error":
            Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
        case "denied":
            Image(systemName: "hand.raised.fill").foregroundStyle(.secondary)
        case "waiting":
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
        default:
            ProgressView().controlSize(.mini)
        }
    }
}

/// The first launch of a build with no Mac to talk to (sideloaded): how to pair with Coucou on Windows or Linux.
struct PCOnboardingView: View {
    let pc: PCLink
    @Environment(\.dismiss) private var dismiss
    @State private var pending: PCPairing?
    @State private var error: String?
    @State private var scanning = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                VStack(alignment: .leading, spacing: 12) {
                    MochiLive(state: .finished)
                        .frame(width: 88, height: 88)
                    Text("Your agents, in your pocket")
                        .font(.largeTitle.weight(.bold))
                    Text("Coucou on iPhone shows what Claude Code and Mochi are doing on your Windows or Linux computer, and answers their requests. Three things to set up, once.")
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                step(1, "Coucou on the computer",
                     "Install Coucou for Windows or Linux and set up its Claude Code hooks.",
                     icon: "desktopcomputer")
                step(2, "Turn on the phone link",
                     "On the computer: Coucou Settings → Phone (Android and iPhone) → turn it on and show the pairing code. Same Wi-Fi as this iPhone.",
                     icon: "switch.2")
                step(3, "Scan the code",
                     "Tap Scan the pairing code below and point the camera at the computer's screen. Or copy the link on the computer and paste it here.",
                     icon: "qrcode.viewfinder")
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.red)
                }
            }
            .padding(24)
        }
        .background(Color.black)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                Button {
                    scanning = true
                } label: {
                    Label("Scan the pairing code", systemImage: "qrcode.viewfinder")
                        .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                HStack(spacing: 24) {
                    Button("Paste the link") {
                        guard let pairing = PCPairing.parse(UIPasteboard.general.string ?? "") else {
                            error = "No pairing link to paste. Copy it on the computer first (Settings → Phone)."
                            Haptics.warning()
                            return
                        }
                        pending = pairing
                    }
                    Button("Later") { dismiss() }
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
        }
        .sheet(item: $pending) { pairing in
            PCPairingConfirm(pairing: pairing, pc: pc) { dismiss() }
        }
        .sheet(isPresented: $scanning) {
            PCScanSheet(pc: pc) { dismiss() }
        }
        .preferredColorScheme(.dark)
    }

    private func step(_ number: Int, _ title: String, _ text: String, icon: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 44, height: 44)
                .background(Color(white: 0.14), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 3) {
                Text("\(number). \(title)").font(.headline)
                Text(text).font(.callout).foregroundStyle(.secondary)
            }
        }
    }
}
