import Contacts
import CoreLocation
import EventKit
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import UserNotifications

// The screens of AXYON with Hermes inside the iPhone (the build that carries it): the portfolio first, the chat, the
// home and the settings, where a computer is one option among others. Black and Liquid Glass (AXYONStyle.swift); the
// chat's bubbles are the computer chat's (PCChatBubble).

enum AXYONTab: Hashable {
    case wallet, chat, house, settings

    /// The tab AXYON opens on: the portfolio; in the Simulator, the one CI's screenshots ask for
    /// (AXYON_TEST_TAB, Hermes/tests/simulator.sh).
    static var launch: AXYONTab {
        #if targetEnvironment(simulator)
        switch ProcessInfo.processInfo.environment["AXYON_TEST_TAB"] {
        case "chat": return .chat
        case "house": return .house
        case "settings": return .settings
        default: return .wallet
        }
        #else
        return .wallet
        #endif
    }
}

/// The tabs of the build with Hermes inside, the portfolio first. `sessionPath` is an agent session of the paired
/// computer opened from a notification or a widget, shown over the tabs; a command of one waiting for the user's OK
/// shows above every tab.
struct AXYONTabs: View {
    let link: PhoneLink
    @Binding var tab: AXYONTab
    @Binding var sessionPath: [String]

    var body: some View {
        TabView(selection: $tab) {
            Tab("Wallet", systemImage: "chart.pie.fill", value: AXYONTab.wallet) {
                // The investing screen (AXYONInvestViews.swift); the positions, as before, under its History.
                NavigationStack { AXYONInvestView() }
            }
            Tab("Chat", systemImage: "bubble.left.and.bubble.right.fill", value: AXYONTab.chat) {
                NavigationStack { HermesChatView() }
            }
            Tab("Home", systemImage: "house.fill", value: AXYONTab.house) {
                NavigationStack { AXYONHouseView() }
            }
            Tab("Settings", systemImage: "gearshape.fill", value: AXYONTab.settings) {
                NavigationStack { AXYONSettingsView(link: link) }
            }
        }
        .coucouMinimizingTabBar()
        .safeAreaInset(edge: .top, spacing: 0) { AXYONApprovalBanner(link: link) }
        .sheet(isPresented: Binding(get: { !sessionPath.isEmpty }, set: { if !$0 { sessionPath = [] } })) {
            if let id = sessionPath.first {
                NavigationStack { SessionDetailView(link: link, sessionId: id) }
            }
        }
        // White for every control, as Apple's own dark apps; switches stay the system's green.
        .tint(.white)
        .toggleStyle(AXYONSwitch())
        .preferredColorScheme(.dark)
        .environment(HermesEngine.shared)
    }
}

extension HermesEngine.State {
    /// The dot beside Hermes' status: white when he is ready, grey while he starts, red when he could not.
    var dot: Color {
        switch self {
        case .ready: .white
        case .failed: AXYONPalette.down
        case .off, .starting: AXYONPalette.faint
        }
    }
}

/// A command of an agent on the paired computer (Claude Code, Codex…) waits for the user's OK: Allow (Face ID) and
/// Deny, above whichever tab is open. Nothing is allowed without the tap.
struct AXYONApprovalBanner: View {
    let link: PhoneLink
    @State private var deciding = false

    /// The session waiting for an OK this iPhone can give.
    private var waiting: SessionItem? {
        link.sessions.sortedByUrgency().first { $0.needsApproval && !$0.approvalFingerprint.isEmpty }
    }

    var body: some View {
        if let session = waiting {
            ApprovalPanel(session: session, disabled: deciding) {
                Task {
                    deciding = true
                    _ = await QuickDecision.deny(session, link: link)
                    deciding = false
                }
            } allow: {
                Task {
                    deciding = true
                    _ = await QuickDecision.allow(session, link: link)
                    deciding = false
                }
            }
            .axyonCard(cornerRadius: 22, tint: CoucouPalette.attention)
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}

// MARK: - The chat

struct HermesChatView: View {
    @Environment(HermesEngine.self) private var engine
    @State private var text = ""
    @FocusState private var focused: Bool
    @State private var dictation = Dictation()
    @State private var silence: Task<Void, Never>?
    @State private var spoken = false
    @State private var brainOpen = false
    @AppStorage("hermesReadAloud") private var readAloud = true
    /// The photos and files that go with the next message (in Hermes' inbox already).
    @State private var pending: [HermesAttachment] = []
    /// Files being read into the inbox.
    @State private var adding = 0
    @State private var attachProblem: String?
    @State private var photosOpen = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var cameraOpen = false
    @State private var filesOpen = false
    /// A page opened inside the app (Google's sign-in).
    @State private var page: AXYONPage?

    private static let end = "hermes-chat-end"

    private var followKey: String {
        let last = engine.conversation.entries.last
        return "\(last?.id ?? "")|\(last?.text.count ?? 0)|\(last?.steps.count ?? 0)|\(last?.finished == true)"
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    if engine.conversation.entries.isEmpty { intro }
                    if !engine.canChat { setupCard }
                    ForEach(engine.conversation.entries) { entry in
                        message(entry)
                            .contextMenu {
                                Button {
                                    UIPasteboard.general.string = entry.text
                                } label: {
                                    Label("Copy", systemImage: "doc.on.doc")
                                }
                                Button {
                                    MochiVoice.shared.say(entry.text, language: dictation.localeID)
                                } label: {
                                    Label("Read aloud", systemImage: "speaker.wave.2")
                                }
                            }
                            .id(entry.id)
                    }
                    if let ask = engine.mailAsk {
                        AXYONMailCard(ask: ask) { engine.answerMail($0) }
                    }
                    Color.clear.frame(height: 1).id(Self.end)
                }
                .padding(.horizontal, 16)
                .padding(.top, 12)
                .padding(.bottom, 8)
            }
            .scrollDismissesKeyboard(.interactively)
            .onAppear { proxy.scrollTo(Self.end, anchor: .bottom) }
            .onChange(of: followKey) { proxy.scrollTo(Self.end, anchor: .bottom) }
            .onChange(of: engine.mailAsk?.id) { proxy.scrollTo(Self.end, anchor: .bottom) }
        }
        .axyonBackdrop(intensity: 0.6)
        .safeAreaInset(edge: .bottom) { composer }
        .navigationTitle("AXYON")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) { title }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    engine.newChat()
                    Haptics.impact()
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .disabled(engine.conversation.entries.isEmpty || engine.answering != nil)
                .accessibilityLabel("New chat")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button { brainOpen = true } label: { Label("Brain", systemImage: "brain.head.profile") }
                    Toggle(isOn: $readAloud) {
                        Label("Read answers to spoken messages aloud", systemImage: "speaker.wave.2")
                    }
                    Picker("Dictation language", selection: $dictation.localeID) {
                        ForEach(Dictation.languages, id: \.self) { id in
                            Text(Locale.current.localizedString(forIdentifier: id) ?? id).tag(id)
                        }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
            }
        }
        .sheet(isPresented: $brainOpen) { NavigationStack { HermesBrainView() } }
        .sheet(item: $page, onDismiss: { Task { await engine.refreshGoogle() } }) { page in
            AXYONSafari(url: page.url).ignoresSafeArea()
        }
        .photosPicker(isPresented: $photosOpen, selection: $photoItems, maxSelectionCount: 6, matching: .images)
        .onChange(of: photoItems) { _, items in
            guard !items.isEmpty else { return }
            photoItems = []
            for (index, item) in items.enumerated() {
                attach {
                    guard let data = try await item.loadTransferable(type: Data.self) else { throw CocoaError(.fileReadUnknown) }
                    return try await AXYONInbox.addPhoto(data, name: "photo-\(index + 1)")
                }
            }
        }
        .fullScreenCover(isPresented: $cameraOpen) {
            AXYONCamera { image in
                cameraOpen = false
                guard let data = image?.jpegData(compressionQuality: 0.9) else { return }
                attach { try await AXYONInbox.addPhoto(data, name: "camera") }
            }
            .ignoresSafeArea()
        }
        .fileImporter(isPresented: $filesOpen, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let files) = result else { return }
            for file in files.prefix(6) {
                attach { try await AXYONInbox.addFile(file) }
            }
        }
        .onDisappear {
            dictation.stop()
            MochiVoice.shared.stop()
        }
        .onChange(of: dictation.transcript) { _, words in
            guard dictation.isRecording else { return }
            text = dictation.prefix + words
            silence?.cancel()
            guard !words.trimmingCharacters(in: .whitespaces).isEmpty else { return }
            silence = Task { @MainActor in
                try? await Task.sleep(for: .seconds(1.8))
                guard !Task.isCancelled, dictation.isRecording else { return }
                dictation.stop()
                send(spoken: true)
            }
        }
        .onChange(of: engine.answering) { was, now in
            guard was != nil, now == nil, spoken else { return }
            spoken = false
            if readAloud, !dictation.isRecording, let answer = engine.conversation.entries.last,
               answer.role == .mochi, !answer.failed {
                MochiVoice.shared.say(answer.text, language: dictation.localeID)
            }
        }
    }

    private var title: some View {
        VStack(spacing: 1) {
            Text(verbatim: "AXYON").font(.headline)
            HStack(spacing: 5) {
                Circle().fill(engine.state.dot).frame(width: 6, height: 6)
                Text(engine.state == .ready ? String(localized: "Hermes inside the iPhone") : String(localized: "Starting…"))
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
    }

    private var intro: some View {
        VStack(spacing: 14) {
            AXYONOrb(mood: engine.mood, size: 88)
            Text("AXYON, inside your iPhone")
                .font(.title3.weight(.semibold))
            Text("Hermes runs here: ask it to remind you, plan your day, look at your portfolio, turn on the TV, run a shortcut or search the web.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 30)
    }

    @ViewBuilder private var setupCard: some View {
        switch engine.state {
        case .ready:
            Button { brainOpen = true } label: {
                Label("Choose AXYON's brain to start", systemImage: "brain.head.profile")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(WhiteCapsuleButtonStyle())
            .controlSize(.large)
        case .starting, .off:
            HStack(spacing: 12) {
                ProgressView()
                Text("Hermes is waking up inside the iPhone…").font(.footnote).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(16)
            .axyonCard()
        case .failed(let why):
            Text(why)
                .font(.caption.monospaced())
                .foregroundStyle(AXYONPalette.down)
                .padding(16)
                .axyonCard(tint: AXYONPalette.down)
        }
    }

    /// A message, with the files sent with it above, the images Hermes made below, and Google's sign-in to open
    /// inside the app when an answer gives its link.
    @ViewBuilder private func message(_ entry: PCChatEntry) -> some View {
        let files = engine.conversation.files(of: entry.id)
        let made = engine.conversation.made(in: entry.id)
        VStack(alignment: entry.role == .you ? .trailing : .leading, spacing: 8) {
            if !files.isEmpty {
                AXYONAttachmentChips(files: files)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            if entry.role == .mochi || !entry.text.isEmpty {
                PCChatBubble(entry: entry, live: entry.id == engine.answering)
            }
            if !made.isEmpty {
                AXYONMadeImages(paths: made)
            }
            if entry.role == .mochi, entry.finished, let link = HermesGoogle.signInLink(in: entry.text) {
                Button {
                    page = AXYONPage(url: link)
                } label: {
                    Label("Sign in with Google", systemImage: "person.badge.key.fill")
                        .font(.subheadline.weight(.semibold))
                }
                .buttonStyle(WhiteCapsuleButtonStyle())
            }
        }
    }

    /// A photo or a file on its way into the inbox, then into the next message.
    private func attach(_ work: @escaping @MainActor () async throws -> HermesAttachment) {
        adding += 1
        attachProblem = nil
        Task {
            do {
                pending.append(try await work())
            } catch {
                attachProblem = String(localized: "That file could not be added.")
            }
            adding -= 1
        }
    }

    private var composer: some View {
        let empty = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && pending.isEmpty
        let running = engine.answering != nil
        return VStack(alignment: .leading, spacing: 8) {
            if let error = engine.lastError ?? attachProblem {
                Label(error, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
            }
            if !pending.isEmpty || adding > 0 {
                HStack(spacing: 8) {
                    AXYONAttachmentChips(files: pending) { file in
                        pending.removeAll { $0.id == file.id }
                    }
                    if adding > 0 {
                        ProgressView().controlSize(.small)
                    }
                }
                .padding(.horizontal, 6)
            }
            CoucouGlassGroup(spacing: 8) {
                HStack(alignment: .bottom, spacing: 8) {
                    HStack(alignment: .bottom, spacing: 2) {
                        Menu {
                            Button {
                                photosOpen = true
                            } label: {
                                Label("Photos", systemImage: "photo.on.rectangle")
                            }
                            if UIImagePickerController.isSourceTypeAvailable(.camera) {
                                Button {
                                    cameraOpen = true
                                } label: {
                                    Label("Camera", systemImage: "camera")
                                }
                            }
                            Button {
                                filesOpen = true
                            } label: {
                                Label("Files", systemImage: "folder")
                            }
                        } label: {
                            Image(systemName: "plus")
                                .font(.body.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .frame(width: 40, height: 44)
                        }
                        .disabled(running || !engine.canChat)
                        .accessibilityLabel("Attach a photo or a file")
                        .padding(.leading, 4)
                        TextField("Message AXYON…", text: $text, axis: .vertical)
                            .lineLimit(1...8)
                            .focused($focused)
                            .padding(.leading, 2)
                            .padding(.vertical, 12)
                        Button {
                            MochiVoice.shared.stop()
                            Task { await dictation.toggle(startingFrom: text) }
                        } label: {
                            Image(systemName: dictation.isRecording ? "waveform" : "mic.fill")
                                .foregroundStyle(dictation.isRecording ? AXYONPalette.down : Color.secondary)
                                .symbolEffect(.variableColor.iterative, isActive: dictation.isRecording)
                                .frame(width: 40, height: 44)
                        }
                        .disabled(running || !engine.canChat)
                        .accessibilityLabel(dictation.isRecording ? "Stop dictation" : "Dictate")
                        .padding(.trailing, 4)
                    }
                    .coucouGlass(.regular, cornerRadius: 24)
                    .overlay {
                        RoundedRectangle(cornerRadius: 24, style: .continuous)
                            .strokeBorder(AXYONPalette.rim, lineWidth: 0.8)
                            .allowsHitTesting(false)
                    }
                    if running || !empty {
                        Button {
                            if running {
                                engine.stop()
                                Haptics.impact()
                            } else {
                                send()
                            }
                        } label: {
                            Image(systemName: running ? "stop.fill" : "arrow.up")
                                .font(.body.weight(.bold))
                                .foregroundStyle(running ? Color.white : Color.black)
                                .frame(width: 46, height: 46)
                                .contentTransition(.symbolEffect(.replace))
                                .coucouGlass(.prominent, in: Circle(), tint: running ? AXYONPalette.down : .white)
                        }
                        .buttonStyle(PressableButtonStyle())
                        .disabled(!running && (!engine.canChat || adding > 0))
                        .transition(.scale(scale: 0.5).combined(with: .opacity))
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 6)
        .animation(.spring(duration: 0.35, bounce: 0.2), value: running || !empty)
        .background(alignment: .bottom) {
            LinearGradient(colors: [.clear, .black.opacity(0.85), .black], startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()
                .allowsHitTesting(false)
        }
    }

    private func send(spoken voice: Bool = false) {
        silence?.cancel()
        dictation.stop()
        guard engine.canChat else {
            brainOpen = true
            return
        }
        guard adding == 0 else { return }
        engine.send(text, attachments: pending)
        text = ""
        pending = []
        spoken = voice
        Haptics.impact()
    }
}

// MARK: - The brain

struct HermesBrainView: View {
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var chosen = "openai-codex"
    @State private var model = ""
    @State private var address = ""
    @State private var key = ""
    @State private var saving = false
    @State private var saved = false

    private var brain: HermesBrain { HermesBrain.named(chosen) ?? HermesBrain.all[0] }

    var body: some View {
        List {
            Section {
                ForEach(HermesBrain.all) { option in
                    Button {
                        chosen = option.id
                        model = engine.settings?.provider == option.id ? (engine.settings?.model ?? "") : option.defaultModel
                        key = ""
                        Haptics.impact()
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: icon(option.id))
                                .frame(width: 28)
                                .foregroundStyle(chosen == option.id ? Color.white : AXYONPalette.faint)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(option.id == "custom" ? String(localized: "Another OpenAI-compatible") : option.name)
                                    .foregroundStyle(.white)
                                Text(blurb(option.id)).font(.caption).foregroundStyle(AXYONPalette.dim)
                            }
                            Spacer()
                            if chosen == option.id {
                                Image(systemName: "checkmark").font(.body.weight(.semibold)).foregroundStyle(.white)
                            }
                        }
                    }
                }
            } header: {
                Text("AXYON's brain")
            } footer: {
                Text("Hermes runs on this iPhone and thinks with the brain chosen here. Keys stay in this iPhone's Keychain.")
            }
            .listRowBackground(GlassRowBackground())

            if brain.id == "openai-codex" {
                chatGPTSection
            } else {
                Section {
                    if brain.needsAddress {
                        TextField("https://…/v1", text: $address)
                            .keyboardType(.URL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                    }
                    SecureField(engine.settings?.keysSet[brain.keyName ?? ""] == true
                                ? String(localized: "Key saved (paste a new one to change it)")
                                : String(localized: "Paste the key"), text: $key)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    if let page = brain.keyPage, let url = URL(string: page) {
                        Button {
                            openURL(url)
                        } label: {
                            Label(brain.id == "gemini" ? String(localized: "Get a free key from Google AI Studio")
                                  : String(localized: "Get a key"), systemImage: "key.fill")
                        }
                    }
                } header: {
                    Text("Key")
                }
                .listRowBackground(GlassRowBackground())
            }

            Section {
                TextField(brain.defaultModel.isEmpty ? String(localized: "Hermes' own choice") : brain.defaultModel,
                          text: $model)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            } header: {
                Text("Model")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Button {
                    Task { await save() }
                } label: {
                    HStack {
                        Spacer()
                        if saving { ProgressView() } else {
                            Label(saved ? String(localized: "Saved") : String(localized: "Use this brain"),
                                  systemImage: saved ? "checkmark" : "bolt.fill")
                                .font(.headline)
                        }
                        Spacer()
                    }
                }
                .disabled(saving || engine.state != .ready)
                if let error = engine.lastError {
                    Text(error).font(.footnote).foregroundStyle(AXYONPalette.down)
                }
                if engine.state != .ready {
                    Text("Hermes is still starting: a moment.").font(.footnote).foregroundStyle(.secondary)
                }
            }
            .listRowBackground(GlassRowBackground(tint: .white))
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle("Brain")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button("Done") { dismiss() }
            }
        }
        .onAppear {
            if let settings = engine.settings {
                chosen = settings.provider
                model = settings.model
                address = settings.baseURL
            }
        }
    }

    @ViewBuilder private var chatGPTSection: some View {
        Section {
            if engine.settings?.provider == "openai-codex", engine.settings?.chatGPTSignedIn == true {
                Label("Signed in to ChatGPT", systemImage: "checkmark.seal.fill").foregroundStyle(AXYONPalette.up)
            }
            switch engine.signIn.state {
            case .waiting:
                VStack(alignment: .leading, spacing: 10) {
                    Text("On OpenAI's page, type this code:").font(.footnote).foregroundStyle(.secondary)
                    Text(engine.signIn.code)
                        .font(.system(.largeTitle, design: .monospaced, weight: .bold))
                        .foregroundStyle(.white)
                        .textSelection(.enabled)
                    HStack {
                        Button {
                            UIPasteboard.general.string = engine.signIn.code
                            Haptics.success()
                        } label: { Label("Copy", systemImage: "doc.on.doc") }
                            .coucouGlassButton()
                        Button {
                            UIPasteboard.general.string = engine.signIn.code
                            if let url = URL(string: engine.signIn.url) { openURL(url) }
                        } label: { Label("Open OpenAI", systemImage: "safari") }
                            .coucouGlassButton(prominent: true)
                    }
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text("Waiting for the sign-in…").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 6)
            case .done:
                Label("Signed in. Hermes thinks with your ChatGPT.", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(AXYONPalette.up)
            case .failed:
                Text(engine.signIn.message).font(.footnote).foregroundStyle(AXYONPalette.down)
                signInButton
            case .idle:
                signInButton
            }
        } header: {
            Text("ChatGPT")
        } footer: {
            Text("Your ChatGPT subscription, with its own sign-in on this iPhone (separate from the computer's).")
        }
        .listRowBackground(GlassRowBackground())
    }

    private var signInButton: some View {
        Button {
            Task { await engine.startChatGPTSignIn() }
        } label: {
            Label("Sign in with ChatGPT", systemImage: "person.badge.key.fill")
        }
        .disabled(engine.state != .ready)
    }

    private func save() async {
        saving = true
        defer { saving = false }
        let ok = await engine.setBrain(brain, model: model, address: address, key: key)
        if ok {
            key = ""
            saved = true
            Haptics.success()
            try? await Task.sleep(for: .seconds(1.2))
            saved = false
        } else {
            Haptics.error()
        }
    }

    private func icon(_ id: String) -> String {
        switch id {
        case "openai-codex": "bubble.left.and.bubble.right.fill"
        case "gemini": "sparkles"
        case "openrouter": "point.3.connected.trianglepath.dotted"
        case "anthropic": "asterisk"
        default: "server.rack"
        }
    }

    private func blurb(_ id: String) -> String {
        switch id {
        case "openai-codex": String(localized: "Your ChatGPT subscription, no key")
        case "gemini": String(localized: "Free key, generous limits")
        case "openrouter": String(localized: "Hundreds of models, some free")
        case "anthropic": String(localized: "Claude, with your API key")
        default: String(localized: "Your own server or another provider")
        }
    }
}

// MARK: - The portfolio

/// The portfolio as the host reports it (`GET /v1/portfolio`, axyon_hermes/coucou.py portfolio_report).
struct WalletReport {
    struct Row {
        let ticker: String
        let assetClass: String
        let quantity: Double
        let price: Double?
        let value: Double?
        let dayPct: Double?
        let dayMoney: Double?
        let gainPct: Double?
        let ceiling: Double?
        let vsCeilingPct: Double?
        let priceFrom: String
    }

    let rows: [Row]
    let watched: [Row]
    let total: Double
    let invested: Double
    let dayMoney: Double?
    let copyDate: String
    /// The positions are the superpowers' own record on this iPhone, rather than the computer's copy.
    let keptOnIphone: Bool
    let pricesAt: String
    let notes: [String]
    let error: String?

    init(_ object: [String: Any]) {
        func row(_ o: [String: Any]) -> Row {
            Row(ticker: o["ticker"] as? String ?? "", assetClass: o["class"] as? String ?? "",
                quantity: o["quantity"] as? Double ?? 0, price: o["price"] as? Double, value: o["value"] as? Double,
                dayPct: o["dayChangePct"] as? Double, dayMoney: o["dayChangeMoney"] as? Double,
                gainPct: o["gainPct"] as? Double, ceiling: o["ceiling"] as? Double,
                vsCeilingPct: o["vsCeilingPct"] as? Double, priceFrom: o["priceFrom"] as? String ?? "")
        }
        rows = (object["positions"] as? [[String: Any]] ?? []).map(row)
        watched = (object["watchedWithCeiling"] as? [[String: Any]] ?? []).map(row)
        total = object["totalValue"] as? Double ?? 0
        invested = object["invested"] as? Double ?? 0
        dayMoney = object["dayChangeMoney"] as? Double
        copyDate = object["copyFromComputer"] as? String ?? ""
        keptOnIphone = object["keptOnIphone"] as? Bool ?? false
        pricesAt = object["pricesReadAt"] as? String ?? ""
        notes = (object["notesInAxyonTab"] as? [[String: Any]] ?? []).map {
            "\($0["ticker"] as? String ?? ""): \($0["text"] as? String ?? "")"
        }
        // No portfolio yet is said by the screen itself, in the user's language.
        error = object["empty"] as? Bool == true ? nil : object["error"] as? String
    }

    /// The day's biggest moves, either way.
    var movers: [Row] {
        rows.filter { $0.dayPct != nil }.sorted { abs($0.dayPct ?? 0) > abs($1.dayPct ?? 0) }
    }
}

enum AXYONMoney {
    static func text(_ value: Double) -> String {
        value.formatted(.currency(code: "BRL").locale(Locale(identifier: "pt_BR")))
    }

    static func signed(_ value: Double) -> String {
        (value >= 0 ? "+" : "−") + text(abs(value))
    }

    static func percent(_ value: Double) -> String {
        (value >= 0 ? "+" : "−") + abs(value).formatted(.number.precision(.fractionLength(1))) + "%"
    }
}

struct AXYONWalletView: View {
    @Environment(HermesEngine.self) private var engine

    var body: some View {
        let report = engine.portfolio.map(WalletReport.init)
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if let report, report.error == nil, !report.rows.isEmpty {
                    summary(report)
                    section(String(localized: "Positions"), report.rows)
                    if !report.watched.isEmpty {
                        section(String(localized: "Watched with your ceiling"), report.watched)
                    }
                    if !report.notes.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            AXYONHeader(title: String(localized: "To check in AXYON's tab"))
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(report.notes, id: \.self) { note in
                                    Label(note, systemImage: "exclamationmark.circle")
                                        .font(.footnote)
                                        .foregroundStyle(.white)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(16)
                            .axyonCard(cornerRadius: 22)
                        }
                    }
                    Text(report.keptOnIphone
                         ? String(localized: "Positions kept on this iPhone · prices \(report.pricesAt), about 15 min late")
                         : String(localized: "Positions from AXYON on your computer (\(report.copyDate)) · prices \(report.pricesAt), about 15 min late"))
                        .font(.caption2)
                        .foregroundStyle(AXYONPalette.faint)
                        .frame(maxWidth: .infinity)
                        .multilineTextAlignment(.center)
                } else {
                    empty(report?.error)
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
        }
        .axyonBackdrop()
        .navigationTitle("Wallet")
        .refreshable { await engine.refreshPortfolio() }
        .task { await engine.refreshPortfolio() }
    }

    /// The total in large type, the day and what it gained since bought under it.
    private func summary(_ report: WalletReport) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(AXYONMoney.text(report.total))
                .font(.system(size: 40, weight: .bold))
                .monospacedDigit()
                .foregroundStyle(.white)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .contentTransition(.numericText())
            HStack(spacing: 24) {
                if let day = report.dayMoney {
                    AXYONFigure(value: AXYONMoney.signed(day), label: String(localized: "Today"),
                                color: day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
                if report.invested > 0 {
                    let gain = report.total - report.invested
                    AXYONFigure(value: AXYONMoney.signed(gain), label: String(localized: "Since you bought"),
                                color: gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .axyonCard()
    }

    private func section(_ title: String, _ rows: [WalletReport.Row]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            AXYONHeader(title: title)
            VStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.ticker) { index, row in
                    if index > 0 { Divider().overlay(Color.white.opacity(0.08)).padding(.leading, 16) }
                    positionRow(row)
                }
            }
            .axyonCard(cornerRadius: 22)
        }
    }

    /// A line of the Stocks app: the ticker and what it is on the left, the value and the day on the right.
    private func positionRow(_ row: WalletReport.Row) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(row.ticker).font(.headline).foregroundStyle(.white)
                Text(row.quantity > 0 ? "\(row.quantity.formatted()) · \(row.assetClass.uppercased())" : row.assetClass.uppercased())
                    .font(.caption).foregroundStyle(AXYONPalette.dim)
                if let ceiling = row.ceiling {
                    let near = (row.vsCeilingPct ?? 1) <= 0
                    Label(String(localized: "Ceiling \(AXYONMoney.text(ceiling))")
                            + (row.vsCeilingPct.map { " · " + AXYONMoney.percent($0) } ?? ""),
                          systemImage: near ? "target" : "scope")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(near ? AXYONPalette.up : AXYONPalette.faint)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 3) {
                if let value = row.value {
                    Text(AXYONMoney.text(value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                }
                if let price = row.price {
                    Text(AXYONMoney.text(price)).font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                }
                if let day = row.dayPct {
                    Text(AXYONMoney.percent(day))
                        .font(.caption.weight(.bold))
                        .monospacedDigit()
                        .foregroundStyle(day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private func empty(_ error: String?) -> some View {
        VStack(spacing: 14) {
            Image(systemName: "chart.line.uptrend.xyaxis")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.white)
            Text("Your portfolio, here").font(.title3.weight(.semibold))
            Text(error ?? String(localized: "Open AXYON once at home with your computer on: it gives this iPhone your positions and ceilings. Or tell AXYON in the chat what you hold."))
                .font(.callout)
                .foregroundStyle(AXYONPalette.dim)
                .multilineTextAlignment(.center)
        }
        .padding(28)
        .frame(maxWidth: .infinity)
        .axyonCard()
        .padding(.top, 40)
    }
}

// The home is AXYONHouse.swift.

// MARK: - Settings

struct AXYONSettingsView: View {
    let link: PhoneLink
    @Environment(HermesEngine.self) private var engine
    @State private var permissions = AXYONPermissions()
    @AppStorage(AXYONAlerts.enabledKey) private var alertsOn = false
    @State private var looking = false
    /// How many alerts the last look by hand sent.
    @State private var looked: Int?

    var body: some View {
        let pc = PCLink.shared
        List {
            Section {
                HStack(spacing: 14) {
                    AXYONOrb(mood: engine.mood, size: 46)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: "AXYON").font(.title3.weight(.bold))
                        Text("Hermes inside this iPhone").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }
            .listRowBackground(Color.clear)

            Section {
                NavigationLink {
                    HermesBrainView()
                } label: {
                    LabeledContent {
                        Text(engine.settings.flatMap { HermesBrain.named($0.provider)?.name } ?? "—")
                            .foregroundStyle(.secondary)
                    } label: {
                        Label("Brain", systemImage: "brain.head.profile")
                    }
                }
                Button {
                    engine.newChat()
                    Haptics.impact()
                } label: {
                    Label("New conversation", systemImage: "square.and.pencil")
                }
                .disabled(engine.answering != nil)
            } header: {
                Text("Hermes")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                permissionRow(String(localized: "Notifications"), "bell.fill", permissions.notifications) {
                    await permissions.askNotifications()
                }
                permissionRow(String(localized: "Calendar"), "calendar", permissions.calendar) {
                    await permissions.askCalendar()
                }
                permissionRow(String(localized: "Reminders"), "checklist", permissions.reminders) {
                    await permissions.askReminders()
                }
                permissionRow(String(localized: "Contacts"), "person.crop.circle", permissions.contacts) {
                    await permissions.askContacts()
                }
                permissionRow(String(localized: "Location"), "location.fill", permissions.location) {
                    permissions.askLocation()
                }
            } header: {
                Text("What AXYON may use on this iPhone")
            } footer: {
                Text("AXYON asks the first time it needs each one. Messages, e-mails and calls always wait for your tap.")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Toggle(isOn: $alertsOn) {
                    Label("Alerts with AXYON closed", systemImage: "bell.badge.fill")
                }
                .tint(AXYONPalette.up)
                .onChange(of: alertsOn) { _, on in
                    looked = nil
                    if on { Task { await permissions.askNotifications() } }
                    AXYONAlerts.schedule()
                }
                if alertsOn {
                    Button {
                        looking = true
                        Task {
                            looked = await AXYONAlerts.look()
                            looking = false
                        }
                    } label: {
                        HStack {
                            Label("Look now", systemImage: "arrow.clockwise")
                            Spacer()
                            if looking {
                                ProgressView().controlSize(.small)
                            } else if let looked {
                                Text(looked == 0 ? String(localized: "Nothing to tell right now.")
                                                 : String(localized: "Alerts sent: \(looked)"))
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .disabled(looking)
                }
            } header: {
                Text("Superpowers")
            } footer: {
                Text("A notification when an asset goes under your ceiling or back above it, on a strong day for an asset or your portfolio, and when one is under its ceiling; in the morning, your day. iOS decides when AXYON may look, a few times a day. Only B3, the CVM, FundosNet, the Banco Central and Google Finance are read; nothing about you is sent.")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                NavigationLink {
                    AXYONGoogleView()
                } label: {
                    LabeledContent {
                        Text(googleState)
                            .foregroundStyle(.secondary)
                    } label: {
                        Label("Google", systemImage: "envelope.open")
                    }
                }
                NavigationLink {
                    AXYONTasksView()
                } label: {
                    Label("Scheduled tasks", systemImage: "clock.arrow.circlepath")
                }
            } header: {
                Text("Connections")
            } footer: {
                Text("Gmail, Sheets, Drive, Docs and Calendar with your own Google sign-in, and the tasks AXYON runs for you on a schedule.")
            }
            .listRowBackground(GlassRowBackground())

            // The paired computer's agent sessions (Claude Code, Codex…): a tap opens the session, its last turn and
            // what it asks.
            if !link.sessions.isEmpty {
                Section {
                    ForEach(Array(link.sessions.sortedByUrgency().prefix(8))) { session in
                        NavigationLink {
                            SessionDetailView(link: link, sessionId: session.id)
                        } label: {
                            SessionRow(session: session)
                        }
                    }
                } header: {
                    Text("Agents on the computer")
                }
                .listRowBackground(GlassRowBackground())
            }

            Section {
                NavigationLink {
                    PCLinkSettingsView(pc: pc)
                } label: {
                    LabeledContent {
                        Text(pc.status.shortText)
                            .foregroundStyle(.secondary)
                    } label: {
                        Label("Windows or Linux computer", systemImage: "desktopcomputer")
                    }
                }
                if pc.isPaired {
                    NavigationLink {
                        PCChatView(pc: pc)
                    } label: {
                        Label("Chat with the computer's AXYON", systemImage: "bubble.left.and.bubble.right")
                    }
                    NavigationLink {
                        PCAssistantSettingsView()
                    } label: {
                        Label("The computer's assistant", systemImage: "sparkles")
                    }
                }
                NavigationLink {
                    AboutView(link: link)
                } label: {
                    Label("More settings", systemImage: "slider.horizontal.3")
                }
            } header: {
                Text("Computer (optional)")
            } footer: {
                Text("AXYON works without one. A paired computer adds its own home, agents and chat, at home or through Tailscale.")
            }
            .listRowBackground(GlassRowBackground())
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle("Settings")
        .task {
            await permissions.load()
            await engine.refreshGoogle()
        }
    }

    private var googleState: String {
        guard let google = engine.google, google.connected else { return String(localized: "Not connected") }
        return google.account.isEmpty ? String(localized: "Connected") : google.account
    }

    private func permissionRow(_ title: String, _ icon: String, _ granted: Bool?,
                               ask: @escaping @MainActor () async -> Void) -> some View {
        HStack {
            Label(title, systemImage: icon)
            Spacer()
            if granted == true {
                Image(systemName: "checkmark").font(.body.weight(.semibold)).foregroundStyle(.secondary)
            } else if granted == false {
                Button("Open Settings") {
                    if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                }
                .font(.footnote)
            } else {
                Button("Allow") { Task { await ask() } }
                    .font(.footnote.weight(.semibold))
            }
        }
    }
}

/// Which of the iPhone's permissions AXYON has: nil not asked yet, false refused (only Settings can change it).
@MainActor
@Observable
final class AXYONPermissions {
    private(set) var notifications: Bool?
    private(set) var calendar: Bool?
    private(set) var reminders: Bool?
    private(set) var contacts: Bool?
    private(set) var location: Bool?
    @ObservationIgnored private let locationManager = CLLocationManager()

    func load() async {
        let alerts = await Self.notificationStatus()
        notifications = Self.state(alerts == .authorized || alerts == .provisional, asked: alerts != .notDetermined)
        calendar = Self.event(EKEventStore.authorizationStatus(for: .event))
        reminders = Self.event(EKEventStore.authorizationStatus(for: .reminder))
        let contactStatus = CNContactStore.authorizationStatus(for: .contacts)
        contacts = Self.state(contactStatus == .authorized || contactStatus == .limited, asked: contactStatus != .notDetermined)
        let place = locationManager.authorizationStatus
        location = Self.state(place == .authorizedWhenInUse || place == .authorizedAlways, asked: place != .notDetermined)
    }

    func askNotifications() async {
        _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
        await load()
    }

    func askCalendar() async {
        await Self.requestEvents()
        await load()
    }

    func askReminders() async {
        await Self.requestReminders()
        await load()
    }

    func askContacts() async {
        await Self.requestContacts()
        await load()
    }

    // The stores live and are asked off the main actor, where their answers come.

    nonisolated private static func notificationStatus() async -> UNAuthorizationStatus {
        await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }

    nonisolated static func requestEvents() async {
        _ = try? await EKEventStore().requestFullAccessToEvents()
    }

    nonisolated static func requestReminders() async {
        _ = try? await EKEventStore().requestFullAccessToReminders()
    }

    nonisolated private static func requestContacts() async {
        _ = try? await CNContactStore().requestAccess(for: .contacts)
    }

    func askLocation() {
        locationManager.requestWhenInUseAuthorization()
        Task {
            try? await Task.sleep(for: .seconds(2))
            await load()
        }
    }

    private static func state(_ granted: Bool, asked: Bool) -> Bool? {
        granted ? true : (asked ? false : nil)
    }

    private static func event(_ status: EKAuthorizationStatus) -> Bool? {
        switch status {
        case .fullAccess: true
        case .notDetermined: nil
        default: false
        }
    }
}
