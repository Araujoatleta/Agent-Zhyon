import Contacts
import CoreLocation
import EventKit
import SwiftUI
import UIKit
import UserNotifications

// The screens of AXYON with Hermes inside the iPhone (the build that carries it): the start screen with Hermes' orb,
// the chat, the portfolio, the home and the settings, where a computer is one option among others. Black and Liquid
// Glass (AXYONStyle.swift); the chat's bubbles are the computer chat's (PCChatBubble).

enum AXYONTab: Hashable { case start, chat, house, wallet, settings }

/// The tabs of the build with Hermes inside. `startPath` is the start screen's navigation: an agent session of the
/// paired computer opened from a notification, a widget or the list there.
struct AXYONTabs: View {
    let link: PhoneLink
    @Binding var tab: AXYONTab
    @Binding var startPath: [String]

    var body: some View {
        TabView(selection: $tab) {
            Tab("AXYON", systemImage: "sparkles", value: AXYONTab.start) {
                NavigationStack(path: $startPath) {
                    AXYONStartView(link: link, tab: $tab)
                        .navigationDestination(for: String.self) { id in
                            SessionDetailView(link: link, sessionId: id)
                        }
                }
            }
            Tab("Chat", systemImage: "bubble.left.and.text.bubble.right.fill", value: AXYONTab.chat) {
                NavigationStack { HermesChatView() }
            }
            Tab("Home", systemImage: "house.fill", value: AXYONTab.house) {
                NavigationStack { AXYONHouseView() }
            }
            Tab("Wallet", systemImage: "chart.line.uptrend.xyaxis", value: AXYONTab.wallet) {
                NavigationStack { AXYONWalletView() }
            }
            Tab("Settings", systemImage: "gearshape.fill", value: AXYONTab.settings) {
                NavigationStack { AXYONSettingsView(link: link) }
            }
        }
        .coucouMinimizingTabBar()
        .tint(AXYONPalette.violet)
        .preferredColorScheme(.dark)
        .environment(HermesEngine.shared)
    }
}

// MARK: - The start screen

struct AXYONStartView: View {
    let link: PhoneLink
    @Binding var tab: AXYONTab
    @Environment(HermesEngine.self) private var engine
    @State private var today = AXYONToday()
    @State private var brainOpen = false
    @State private var deciding = false

    /// The agent sessions of the paired computer (Claude Code, Codex…), most urgent first.
    private var sessions: [SessionItem] { link.sessions.sortedByUrgency() }

    /// The session waiting for an OK this iPhone can give.
    private var waiting: SessionItem? {
        sessions.first { $0.needsApproval && !$0.approvalFingerprint.isEmpty }
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 22) {
                hero
                if let waiting { approvalCard(waiting) }
                askBar
                if !engine.canChat, engine.state == .ready { brainCard }
                if case .failed(let why) = engine.state { troubleCard(why) }
                actions
                walletCard
                todayCard
                if !sessions.isEmpty { agentsCard }
                computerCard
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 30)
        }
        .scrollIndicators(.hidden)
        .axyonBackdrop()
        .toolbarVisibility(.hidden, for: .navigationBar)
        .sheet(isPresented: $brainOpen) { NavigationStack { HermesBrainView() } }
        .task {
            await today.load()
            await engine.refreshPortfolio()
        }
        .refreshable {
            await today.load()
            await engine.refreshPortfolio()
        }
    }

    private var greeting: String {
        let hour = Calendar.current.component(.hour, from: .now)
        switch hour {
        case 5..<12: return String(localized: "Good morning")
        case 12..<18: return String(localized: "Good afternoon")
        default: return String(localized: "Good evening")
        }
    }

    private var statusLine: String {
        switch engine.state {
        case .off: return String(localized: "Hermes is asleep")
        case .starting: return String(localized: "Hermes is waking up inside the iPhone…")
        case .failed: return String(localized: "Hermes could not start")
        case .ready:
            guard let settings = engine.settings else { return String(localized: "Hermes is ready") }
            let brain = HermesBrain.named(settings.provider)?.name ?? settings.provider
            return settings.canThink ? String(localized: "Hermes inside the iPhone · \(brain)")
                : String(localized: "Hermes is ready: choose its brain")
        }
    }

    private var hero: some View {
        VStack(spacing: 6) {
            AXYONOrb(mood: engine.mood, size: 116)
                .padding(.top, 34)
            Text(greeting)
                .font(.system(.largeTitle, design: .rounded, weight: .bold))
                .foregroundStyle(LinearGradient(colors: [.white, AXYONPalette.cyan.opacity(0.85)],
                                                startPoint: .leading, endPoint: .trailing))
            HStack(spacing: 6) {
                Circle().fill(engine.state == .ready ? AXYONPalette.up : AXYONPalette.gold).frame(width: 7, height: 7)
                    .shadow(color: engine.state == .ready ? AXYONPalette.up : AXYONPalette.gold, radius: 4)
                Text(statusLine)
            }
            .font(.footnote.weight(.medium))
            .foregroundStyle(AXYONPalette.dim)
        }
        .frame(maxWidth: .infinity)
    }

    private var askBar: some View {
        Button {
            tab = .chat
            Haptics.impact()
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "sparkle")
                    .foregroundStyle(AXYONPalette.cyan)
                Text("Ask AXYON anything…")
                    .foregroundStyle(AXYONPalette.dim)
                Spacer()
                Image(systemName: "mic.fill")
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 36)
                    .coucouGlass(.prominent, in: Circle(), tint: AXYONPalette.violet)
            }
            .padding(.leading, 18)
            .padding(.trailing, 6)
            .padding(.vertical, 6)
            .axyonCard(cornerRadius: 26, glow: AXYONPalette.violet)
        }
        .buttonStyle(PressableButtonStyle())
    }

    private var brainCard: some View {
        Button { brainOpen = true } label: {
            HStack(spacing: 14) {
                Image(systemName: "brain.head.profile")
                    .font(.title2)
                    .foregroundStyle(AXYONPalette.magenta)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Give Hermes a brain").font(.headline).foregroundStyle(.white)
                    Text("Sign in with ChatGPT, or paste a free Gemini key.")
                        .font(.footnote).foregroundStyle(AXYONPalette.dim)
                }
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(AXYONPalette.faint)
            }
            .padding(18)
            .axyonCard(tint: AXYONPalette.magenta, glow: AXYONPalette.magenta)
        }
        .buttonStyle(PressableButtonStyle())
    }

    private func troubleCard(_ why: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Hermes did not start", systemImage: "exclamationmark.triangle.fill")
                .font(.headline)
                .foregroundStyle(AXYONPalette.down)
            Text(why)
                .font(.caption.monospaced())
                .foregroundStyle(AXYONPalette.dim)
                .textSelection(.enabled)
                .lineLimit(8)
            Text("Close AXYON from the app switcher and open it again.")
                .font(.footnote)
                .foregroundStyle(.white)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .axyonCard(tint: AXYONPalette.down)
    }

    private var actions: some View {
        VStack(spacing: 10) {
            AXYONHeader(title: String(localized: "Do it for me"), systemImage: "bolt.fill")
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10),
                                GridItem(.flexible(), spacing: 10)], spacing: 10) {
                action(String(localized: "Remind me"), "bell.badge.fill", AXYONPalette.gold,
                       String(localized: "Remind me at 6pm to "))
                action(String(localized: "My day"), "calendar", AXYONPalette.cyan,
                       String(localized: "What do I have today and tomorrow?"))
                action(String(localized: "Portfolio"), "chart.pie.fill", AXYONPalette.up,
                       String(localized: "How is my portfolio doing today?"))
                action(String(localized: "Home"), "tv.fill", AXYONPalette.violet,
                       String(localized: "Turn on the living room TV"))
                action(String(localized: "Shortcut"), "square.on.square.dashed", AXYONPalette.magenta,
                       String(localized: "Run my shortcut "))
                action(String(localized: "Search"), "globe", .white,
                       String(localized: "Search the web: "))
            }
        }
    }

    private func action(_ title: String, _ icon: String, _ color: Color, _ prompt: String) -> some View {
        Button {
            HermesDraft.shared.text = prompt
            tab = .chat
            Haptics.impact()
        } label: {
            VStack(spacing: 8) {
                Image(systemName: icon)
                    .font(.title3)
                    .foregroundStyle(color)
                    .shadow(color: color.opacity(0.7), radius: 6)
                Text(title)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 16)
            .axyonCard(cornerRadius: 22)
        }
        .buttonStyle(PressableButtonStyle())
    }

    private var walletCard: some View {
        let report = engine.portfolio.map(WalletReport.init)
        return Button { tab = .wallet } label: {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    AXYONHeader(title: String(localized: "Portfolio"), systemImage: "chart.line.uptrend.xyaxis")
                    Spacer()
                    if engine.portfolioLoading { ProgressView().controlSize(.small) }
                }
                if let report, !report.rows.isEmpty {
                    HStack(alignment: .bottom) {
                        AXYONFigure(value: AXYONMoney.text(report.total), label: String(localized: "Total now"))
                        Spacer()
                        if let day = report.dayMoney {
                            AXYONFigure(value: AXYONMoney.signed(day), label: String(localized: "Today"),
                                        color: day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                        }
                    }
                    HStack(spacing: 8) {
                        ForEach(report.movers.prefix(3), id: \.ticker) { row in
                            AXYONChip(title: "\(row.ticker) \(AXYONMoney.percent(row.dayPct ?? 0))",
                                      systemImage: (row.dayPct ?? 0) >= 0 ? "arrow.up.right" : "arrow.down.right",
                                      tint: (row.dayPct ?? 0) >= 0 ? AXYONPalette.up : AXYONPalette.down)
                        }
                    }
                } else {
                    Text("Your portfolio comes from AXYON on your computer once, or tell AXYON your positions in the chat.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .multilineTextAlignment(.leading)
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .axyonCard(glow: AXYONPalette.up)
        }
        .buttonStyle(PressableButtonStyle())
    }

    private var todayCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            AXYONHeader(title: String(localized: "Today"), systemImage: "sun.max.fill")
            if today.allowed {
                if today.events.isEmpty {
                    Text("Nothing on your calendar today.").font(.footnote).foregroundStyle(AXYONPalette.dim)
                }
                ForEach(today.events) { event in
                    HStack(spacing: 12) {
                        Capsule().fill(AXYONPalette.cyan).frame(width: 3, height: 30)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(event.title).font(.subheadline.weight(.semibold)).foregroundStyle(.white).lineLimit(1)
                            Text(event.when).font(.caption).foregroundStyle(AXYONPalette.dim)
                        }
                        Spacer()
                    }
                }
                if today.reminders > 0 {
                    Label("Open reminders: \(today.reminders)", systemImage: "checklist")
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(AXYONPalette.gold)
                }
            } else {
                Button {
                    Task { await today.ask() }
                } label: {
                    Label("Let AXYON see your calendar and reminders", systemImage: "calendar.badge.plus")
                        .font(.footnote.weight(.semibold))
                }
                .coucouGlassButton()
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(glow: AXYONPalette.cyan)
    }

    /// A command of an agent on the computer waits for your OK: Allow (Face ID) and Deny right here.
    private func approvalCard(_ session: SessionItem) -> some View {
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
        .axyonCard(tint: .orange, glow: .orange)
    }

    /// The computer's agents (Claude Code, Codex…): a tap opens the session, its last turn and what it asks.
    private var agentsCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            AXYONHeader(title: String(localized: "Agents on the computer"), systemImage: "terminal.fill")
            VStack(spacing: 0) {
                ForEach(Array(sessions.prefix(5).enumerated()), id: \.element.id) { index, session in
                    if index > 0 { Divider().overlay(Color.white.opacity(0.06)).padding(.leading, 64) }
                    NavigationLink(value: session.id) {
                        SessionRow(session: session)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, 6)
            .axyonCard(cornerRadius: 24)
        }
    }

    private var computerCard: some View {
        let pc = PCLink.shared
        return Button { tab = .settings } label: {
            HStack(spacing: 14) {
                Image(systemName: "desktopcomputer")
                    .font(.title3)
                    .foregroundStyle(pc.status == .connected ? AXYONPalette.up : AXYONPalette.faint)
                VStack(alignment: .leading, spacing: 2) {
                    Text(pc.isPaired ? pc.name : String(localized: "Computer (optional)"))
                        .font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    Text(pc.isPaired ? pc.status.shortText : String(localized: "AXYON works on its own. Pair a computer to reach it too."))
                        .font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(2)
                }
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(AXYONPalette.faint)
            }
            .padding(16)
            .axyonCard(cornerRadius: 22)
        }
        .buttonStyle(PressableButtonStyle())
    }
}

/// A prompt the start screen puts in the chat's field.
@MainActor
@Observable
final class HermesDraft {
    static let shared = HermesDraft()
    var text = ""
}

/// Today's calendar and open reminders, read on the iPhone itself for the start screen.
@MainActor
@Observable
final class AXYONToday {
    struct Event: Identifiable, Sendable {
        let id: String
        let title: String
        let when: String
    }

    private(set) var allowed = false
    private(set) var events: [Event] = []
    private(set) var reminders = 0

    func load() async {
        allowed = EKEventStore.authorizationStatus(for: .event) == .fullAccess
        guard allowed else { return }
        let (found, open) = await Self.read()
        events = found
        reminders = open
    }

    func ask() async {
        await AXYONPermissions.requestEvents()
        await AXYONPermissions.requestReminders()
        await load()
    }

    nonisolated private static func read() async -> ([Event], Int) {
        let store = EKEventStore()
        let start = Date.now
        let end = Calendar.current.startOfDay(for: .now).addingTimeInterval(86_400)
        let found = store.events(matching: store.predicateForEvents(withStart: start, end: end, calendars: nil))
            .prefix(4)
            .map { event in
                Event(id: event.eventIdentifier ?? UUID().uuidString, title: event.title ?? "",
                      when: event.isAllDay ? String(localized: "All day")
                        : event.startDate.formatted(date: .omitted, time: .shortened))
            }
        var open = 0
        if EKEventStore.authorizationStatus(for: .reminder) == .fullAccess {
            let predicate = store.predicateForIncompleteReminders(withDueDateStarting: nil, ending: end, calendars: nil)
            open = await withCheckedContinuation { continuation in
                store.fetchReminders(matching: predicate) { @Sendable found in
                    continuation.resume(returning: found?.count ?? 0)
                }
            }
        }
        return (Array(found), open)
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
                        PCChatBubble(entry: entry, live: entry.id == engine.answering)
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
                    Color.clear.frame(height: 1).id(Self.end)
                }
                .padding(.horizontal, 16)
                .padding(.top, 12)
                .padding(.bottom, 8)
            }
            .scrollDismissesKeyboard(.interactively)
            .onAppear { proxy.scrollTo(Self.end, anchor: .bottom) }
            .onChange(of: followKey) { proxy.scrollTo(Self.end, anchor: .bottom) }
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
        .onAppear {
            if !HermesDraft.shared.text.isEmpty {
                text = HermesDraft.shared.text
                HermesDraft.shared.text = ""
                focused = true
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
                Circle().fill(engine.state == .ready ? AXYONPalette.up : AXYONPalette.gold).frame(width: 6, height: 6)
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
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(16)
                    .axyonCard(tint: AXYONPalette.magenta, glow: AXYONPalette.magenta)
            }
            .buttonStyle(PressableButtonStyle())
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

    private var composer: some View {
        let empty = text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let running = engine.answering != nil
        return VStack(alignment: .leading, spacing: 8) {
            if let error = engine.lastError {
                Label(error, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
            }
            CoucouGlassGroup(spacing: 8) {
                HStack(alignment: .bottom, spacing: 8) {
                    HStack(alignment: .bottom, spacing: 2) {
                        TextField("Message AXYON…", text: $text, axis: .vertical)
                            .lineLimit(1...8)
                            .focused($focused)
                            .padding(.leading, 16)
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
                                .foregroundStyle(.white)
                                .frame(width: 46, height: 46)
                                .contentTransition(.symbolEffect(.replace))
                                .coucouGlass(.prominent, in: Circle(), tint: running ? AXYONPalette.down : AXYONPalette.violet)
                                .shadow(color: AXYONPalette.violet.opacity(0.6), radius: 10)
                        }
                        .buttonStyle(PressableButtonStyle())
                        .disabled(!running && !engine.canChat)
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
        engine.send(text)
        text = ""
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
                                .foregroundStyle(chosen == option.id ? AXYONPalette.cyan : AXYONPalette.faint)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(option.id == "custom" ? String(localized: "Another OpenAI-compatible") : option.name)
                                    .foregroundStyle(.white)
                                Text(blurb(option.id)).font(.caption).foregroundStyle(AXYONPalette.dim)
                            }
                            Spacer()
                            if chosen == option.id {
                                Image(systemName: "checkmark.circle.fill").foregroundStyle(AXYONPalette.violet)
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
            .listRowBackground(GlassRowBackground(tint: AXYONPalette.violet))
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
                        .foregroundStyle(AXYONPalette.cyan)
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
            VStack(spacing: 16) {
                if let report, report.error == nil, !report.rows.isEmpty {
                    summary(report)
                    AXYONHeader(title: String(localized: "Positions"), systemImage: "list.bullet.rectangle")
                    ForEach(report.rows, id: \.ticker) { positionRow($0) }
                    if !report.watched.isEmpty {
                        AXYONHeader(title: String(localized: "Watched with your ceiling"), systemImage: "eye.fill")
                        ForEach(report.watched, id: \.ticker) { positionRow($0) }
                    }
                    if !report.notes.isEmpty {
                        AXYONHeader(title: String(localized: "To check in AXYON's tab"), systemImage: "exclamationmark.bubble")
                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(report.notes, id: \.self) { Text($0).font(.footnote).foregroundStyle(AXYONPalette.gold) }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(16)
                        .axyonCard(tint: AXYONPalette.gold)
                    }
                    Text(report.keptOnIphone
                         ? String(localized: "Positions kept on this iPhone · prices \(report.pricesAt), about 15 min late")
                         : String(localized: "Positions from AXYON on your computer (\(report.copyDate)) · prices \(report.pricesAt), about 15 min late"))
                        .font(.caption2)
                        .foregroundStyle(AXYONPalette.faint)
                        .multilineTextAlignment(.center)
                } else {
                    empty(report?.error)
                }
            }
            .padding(16)
        }
        .axyonBackdrop()
        .navigationTitle("Wallet")
        .refreshable { await engine.refreshPortfolio() }
        .task { await engine.refreshPortfolio() }
    }

    private func summary(_ report: WalletReport) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(AXYONMoney.text(report.total))
                .font(.system(size: 40, weight: .bold, design: .rounded))
                .foregroundStyle(.white)
                .contentTransition(.numericText())
            HStack(spacing: 18) {
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
        .axyonCard(glow: AXYONPalette.up)
    }

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
                if let value = row.value { Text(AXYONMoney.text(value)).font(.subheadline.weight(.semibold)).foregroundStyle(.white) }
                if let price = row.price { Text(AXYONMoney.text(price)).font(.caption).foregroundStyle(AXYONPalette.dim) }
                if let day = row.dayPct {
                    Text(AXYONMoney.percent(day))
                        .font(.caption.weight(.bold))
                        .foregroundStyle(day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private func empty(_ error: String?) -> some View {
        VStack(spacing: 14) {
            Image(systemName: "chart.pie")
                .font(.system(size: 44))
                .foregroundStyle(AXYONPalette.up)
                .shadow(color: AXYONPalette.up.opacity(0.6), radius: 12)
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
                            .foregroundStyle(engine.settings?.canThink == true ? AXYONPalette.up : .secondary)
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
                NavigationLink {
                    PCLinkSettingsView(pc: pc)
                } label: {
                    LabeledContent {
                        Text(pc.status.shortText)
                            .foregroundStyle(pc.status == .connected ? AXYONPalette.up : Color.secondary)
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
        .task { await permissions.load() }
    }

    private func permissionRow(_ title: String, _ icon: String, _ granted: Bool?,
                               ask: @escaping @MainActor () async -> Void) -> some View {
        HStack {
            Label(title, systemImage: icon)
            Spacer()
            if granted == true {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(AXYONPalette.up)
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
