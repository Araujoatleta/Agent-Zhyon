import SwiftUI
import UIKit

/// The tabs, in glass on iOS 26. With a Windows or Linux computer (paired, or a build that has nothing else to talk
/// to): the chat with Mochi there, the agents, Mochi's home and Settings, plus the Mac's services when iCloud is in
/// the build. With a Mac only: the agents, the services, the decisions you took and a search through every turn.
enum CoucouTab: Hashable { case chat, agents, home, services, history, search, settings }

extension CoucouTab {
    /// The computer's tabs: a computer is paired, or this build has no iCloud (sideloaded).
    @MainActor static var computerLayout: Bool { PCLink.shared.isPaired || !PhoneLink.cloudAvailable }
}

struct HomeView: View {
    let link: PhoneLink
    @State private var tab: CoucouTab = .agents
    @State private var agentsPath: [String] = []
    @State private var servicesPath: [String] = []
    #if HERMES
    /// Hermes inside this iPhone (NotchBuddy/Hermes/README.md): AXYON's own tabs, a computer one option among others.
    @State private var axyonTab: AXYONTab = .start
    @State private var axyonPath: [String] = []
    #endif
    @State private var introDone = HomeView.hermesInside
    /// A pairing link opened from the Camera (the QR code of a Windows or Linux computer), waiting for a yes.
    @State private var pairing: PCPairing?
    @AppStorage("onboardingDone") private var onboardingDone = false

    private var computerLayout: Bool { CoucouTab.computerLayout }

    /// This build carries Hermes: it needs no computer, so no pairing guide and no intro flying to a session's tile.
    private static let hermesInside: Bool = {
        #if HERMES
        return true
        #else
        return false
        #endif
    }()

    var body: some View {
        tabs
        // A widget tile was tapped: open that Mochi. Or the Camera read a computer's pairing code.
        .onOpenURL { url in
            if let id = SharedSession.sessionId(from: url) {
                open(id)
            } else if let found = PCPairing.parse(url.absoluteString) {
                if onboardingDone {
                    pairing = found
                } else {
                    // The first-launch guide is up: it closes, then the pairing sheet opens.
                    onboardingDone = true
                    Task {
                        try? await Task.sleep(for: .milliseconds(700))
                        pairing = found
                    }
                }
            }
        }
        .sheet(item: $pairing) { found in
            PCPairingConfirm(pairing: found, pc: PCLink.shared)
        }
        // A notification, Siri, Spotlight or the Control Center asked for a session.
        .onChange(of: link.openPillId) { _, id in
            guard let id else { return }
            open(id)
            link.openPillId = nil
        }
        .onContinueUserActivity(SpotlightIndex.activityType) { activity in
            if let id = SpotlightIndex.pillId(from: activity) { open(id) }
        }
        .sheet(isPresented: Binding(get: { link.reviewFingerprint != nil },
                                    set: { if !$0 { link.reviewFingerprint = nil } })) {
            if let fingerprint = link.reviewFingerprint {
                ReviewSheet(link: link, fingerprint: fingerprint)
            }
        }
        // First launch: how to connect the Mac (or, in a build without iCloud, the Windows or Linux computer).
        .fullScreenCover(isPresented: Binding(get: { !onboardingDone && !Self.hermesInside },
                                              set: { if !$0 { onboardingDone = true } })) {
            if PhoneLink.cloudAvailable {
                OnboardingView()
            } else {
                PCOnboardingView(pc: PCLink.shared)
            }
        }
        // The opening: Mochi alone while the app loads, then he flies to his tile.
        .overlay {
            if !introDone {
                IntroView(ready: link.firstSyncDone) { introDone = true }
                    .transition(.identity)
            }
        }
        .onAppear {
            // The first launch opens on the setup guide instead; with Hermes inside, on AXYON right away.
            if !onboardingDone || Self.hermesInside {
                introDone = true
                IntroLanding.shared.landed = true
            }
        }
        // Hermes' violet for every control, sheets included.
        .tint(CoucouPalette.accent)
    }

    @ViewBuilder private var tabs: some View {
        #if HERMES
        AXYONTabs(link: link, tab: $axyonTab, startPath: $axyonPath)
        #else
        coucouTabs
        #endif
    }

    private var coucouTabs: some View {
        // The app opens on the agents (where the intro's Mochi lands), in both layouts.
        TabView(selection: $tab) {
            if computerLayout {
                Tab("Chat", systemImage: "bubble.left.and.bubble.right.fill", value: CoucouTab.chat) {
                    NavigationStack { PCChatView(pc: PCLink.shared, inTab: true) }
                }
            }
            Tab("Agents", systemImage: "sparkles", value: CoucouTab.agents) {
                AgentsTab(link: link, path: $agentsPath, settingsInToolbar: !computerLayout,
                          openChat: { tab = .chat })
            }
            if computerLayout {
                Tab("Home", systemImage: "house.fill", value: CoucouTab.home) {
                    NavigationStack { PCHomeView() }
                }
            }
            if !computerLayout || PhoneLink.cloudAvailable {
                Tab("Services", systemImage: "square.grid.2x2", value: CoucouTab.services) {
                    ServicesTab(link: link, path: $servicesPath)
                }
            }
            if computerLayout {
                Tab("Settings", systemImage: "gearshape.fill", value: CoucouTab.settings) {
                    NavigationStack { AboutView(link: link) }
                }
            }
            if !computerLayout {
                Tab("History", systemImage: "clock.arrow.circlepath", value: CoucouTab.history) {
                    NavigationStack { HistoryView(link: link) }
                }
            }
            if !computerLayout {
                Tab(value: CoucouTab.search, role: .search) {
                    SearchTab(link: link)
                }
            }
        }
        .coucouMinimizingTabBar()
        // Pairing or forgetting the computer changes the tabs: back to the agents, which both layouts have.
        .onChange(of: computerLayout) { tab = .agents }
    }

    private func open(_ id: String) {
        #if HERMES
        // The computer's agent sessions open from AXYON's start screen.
        if PillCatalog.isSession(id) {
            axyonTab = .start
            axyonPath = [id]
        }
        #else
        if PillCatalog.isSession(id) {
            tab = .agents
            agentsPath = [id]
        } else if !computerLayout || PhoneLink.cloudAvailable {
            tab = .services
            servicesPath = [id]
        }
        #endif
    }
}

/// Every agent session from the Mac and the paired computer, most urgent first, each a glass card. Swipe right to
/// allow (Face ID), left to deny; press and hold for a peek at the last turn. Below them, the service cards the
/// computer shares.
struct AgentsTab: View {
    let link: PhoneLink
    @Binding var path: [String]
    /// Settings has no tab of its own (a Mac only): the gear opens it.
    var settingsInToolbar = true
    /// The chat with Mochi on the computer, in its own tab.
    var openChat: () -> Void = {}
    @Namespace private var zoom

    var body: some View {
        NavigationStack(path: $path) {
            List {
                Section {
                    NotchHeader(link: link)
                        // Shrinks and fades as the list scrolls, like Wallet.
                        .visualEffect { content, proxy in
                            let y = proxy.frame(in: .scrollView).minY
                            let pull = min(0, y - 8)
                            return content
                                .scaleEffect(max(0.86, 1 + pull / 500), anchor: .top)
                                .opacity(max(0, 1 + pull / 160))
                        }
                    TodayCard(link: link)
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
                .listRowSeparator(.hidden)

                Section {
                    if link.sessions.isEmpty {
                        EmptySessionsView(status: link.status)
                            .listRowBackground(Color.clear)
                            .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                            .listRowSeparator(.hidden)
                    }
                    ForEach(link.sessions.sortedByUrgency()) { session in
                        NavigationLink(value: session.id) {
                            SessionRow(session: session, zoom: zoom)
                        }
                        .listRowBackground(GlassRowCard(tint: session.isWaitingForYou ? session.statusColor : nil))
                        .listRowInsets(EdgeInsets(top: 10, leading: 28, bottom: 10, trailing: 24))
                        .listRowSeparator(.hidden)
                        .swipeActions(edge: .leading, allowsFullSwipe: true) {
                            if session.needsApproval && !session.approvalFingerprint.isEmpty {
                                Button {
                                    Task { _ = await QuickDecision.allow(session, link: link) }
                                } label: {
                                    Label("Allow", systemImage: "faceid")
                                }
                                .tint(CoucouPalette.ok)
                            }
                        }
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            if session.needsApproval && !session.approvalFingerprint.isEmpty {
                                Button(role: .destructive) {
                                    Task { _ = await QuickDecision.deny(session, link: link) }
                                } label: {
                                    Label("Deny", systemImage: "xmark")
                                }
                            }
                        }
                        .contextMenu {
                            SessionMenu(link: link, session: session)
                        } preview: {
                            SessionPeek(session: session, turn: link.turns[session.id])
                        }
                    }
                }

                // What the computer's service Mochi say (read-only), when it shares any (an empty row would leave a gap).
                if PCLink.shared.isPaired, !PCLink.shared.model.services.isEmpty {
                    PCServicesSection()
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
                        .listRowSeparator(.hidden)
                }
            }
            .listStyle(.plain)
            .deepBackdrop()
            // Sessions move to their new place and change status smoothly.
            .animation(.spring(duration: 0.5, bounce: 0.2),
                       value: link.sessions.map { "\($0.id)|\($0.statusText)" })
            .refreshable { await link.refresh() }
            .navigationTitle("Coucou")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: String.self) { id in
                SessionDetailView(link: link, sessionId: id)
                    // The Mochi tile grows into the session, and shrinks back.
                    .navigationTransition(.zoom(sourceID: id, in: zoom))
            }
            .toolbar {
                if settingsInToolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        NavigationLink {
                            AboutView(link: link)
                        } label: {
                            Image(systemName: "gearshape")
                        }
                        .accessibilityLabel("Settings")
                    }
                }
                // Mochi on the paired Windows or Linux computer: his chat, which has its own tab now.
                if PCLink.shared.isPaired {
                    ToolbarItem(placement: .topBarLeading) {
                        Button(action: openChat) {
                            Image(systemName: "desktopcomputer")
                        }
                        .accessibilityLabel("Chat with Mochi")
                    }
                }
                #if DEBUG
                // Test screens, in builds run from Xcode only.
                ToolbarItem(placement: .topBarLeading) {
                    NavigationLink {
                        LinkTestView(link: link)
                    } label: {
                        Image(systemName: "stethoscope")
                    }
                }
                #endif
            }
        }
    }
}

/// The service Mochi (GitHub, Stripe…), each with what it saw.
struct ServicesTab: View {
    let link: PhoneLink
    @Binding var path: [String]
    @Namespace private var zoom

    var body: some View {
        NavigationStack(path: $path) {
            ScrollView {
                ServicesList(services: link.services)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
            }
            .deepBackdrop()
            .refreshable { await link.refresh() }
            .navigationTitle("Services")
            .navigationDestination(for: String.self) { id in
                ServiceDetailView(link: link, pillId: id)
            }
        }
    }
}

/// Allow (Face ID) and Deny without opening the session.
@MainActor
enum QuickDecision {
    static func allow(_ session: SessionItem, link: PhoneLink) async -> Bool {
        guard await OwnerCheck.confirm(reason: String(localized: "Allow this command on \(session.hostName)")) else {
            Haptics.warning()
            return false
        }
        let summary = session.approvalCommand.isEmpty ? String(localized: "Permission") : session.approvalCommand
        let sent = await link.decide(.allow, fingerprint: session.approvalFingerprint, pillId: session.id,
                                     summary: String(summary.prefix(200)))
        if sent { Haptics.success() } else { Haptics.error() }
        return sent
    }

    static func deny(_ session: SessionItem, link: PhoneLink) async -> Bool {
        let summary = session.approvalCommand.isEmpty ? String(localized: "Permission") : session.approvalCommand
        let sent = await link.decide(.deny, fingerprint: session.approvalFingerprint, pillId: session.id,
                                     summary: String(summary.prefix(200)))
        if sent { Haptics.impact() } else { Haptics.error() }
        return sent
    }
}

/// What a long press on a session offers.
struct SessionMenu: View {
    let link: PhoneLink
    let session: SessionItem

    var body: some View {
        if session.needsApproval && !session.approvalFingerprint.isEmpty {
            Button {
                Task { _ = await QuickDecision.allow(session, link: link) }
            } label: {
                Label("Allow", systemImage: "faceid")
            }
            Button(role: .destructive) {
                Task { _ = await QuickDecision.deny(session, link: link) }
            } label: {
                Label("Deny", systemImage: "xmark")
            }
        }
        if !session.approvalCommand.isEmpty {
            Button {
                UIPasteboard.general.string = session.approvalCommand
            } label: {
                Label("Copy the command", systemImage: "doc.on.doc")
            }
        }
        if let answer = link.turns[session.id]?.finalMessage, !answer.isEmpty {
            Button {
                UIPasteboard.general.string = answer
            } label: {
                Label("Copy Claude's answer", systemImage: "text.quote")
            }
        }
        Button {
            link.openPillId = session.id
        } label: {
            Label("Open", systemImage: "arrow.up.forward.app")
        }
    }
}

/// The peek of a long press: who, where it is, and the last turn in short.
struct SessionPeek: View {
    let session: SessionItem
    let turn: TurnSnapshot?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                MochiStill(state: session.state, bodyHex: session.color)
                    .padding(6)
                    .frame(width: 48, height: 48)
                    .background(Color.agentTile(hex: session.color), in: RoundedRectangle(cornerRadius: 13))
                VStack(alignment: .leading, spacing: 2) {
                    Text(session.title).font(.headline)
                    Text(session.statusText).font(.subheadline.weight(.semibold)).foregroundStyle(session.statusColor)
                }
            }
            if session.needsApproval, !session.approvalCommand.isEmpty {
                Text(session.approvalCommand)
                    .font(.callout.monospaced())
                    .lineLimit(5)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
            }
            if let turn {
                Text(turn.headline)
                    .font(.callout)
                    .lineLimit(3)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(CoucouPalette.accent.opacity(0.22), in: RoundedRectangle(cornerRadius: 12))
                HStack(spacing: 14) {
                    Label("\(turn.actions.count)", systemImage: "wrench.and.screwdriver")
                    Label("\(turn.files.count)", systemImage: "doc.text")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                if !turn.finalMessage.isEmpty {
                    Text(turn.finalMessage).font(.callout).foregroundStyle(.secondary).lineLimit(6)
                }
            }
        }
        .padding(18)
        .frame(width: 340, alignment: .leading)
        .background(CoucouPalette.night)
        .environment(\.colorScheme, .dark)
    }
}

/// The island at the top: Mochi with the most urgent state and a summary, in
/// glass. When a command waits for your OK, Allow and Deny grow out of it
/// (Liquid Glass morphing on iOS 26, a slide before) and fold back once answered.
struct NotchHeader: View {
    let link: PhoneLink
    @Namespace private var glass
    @State private var sending = false
    @State private var approved = false

    private var sessions: [SessionItem] { link.sessions }

    /// The session waiting for an OK this iPhone can give.
    private var waiting: SessionItem? {
        sessions.sortedByUrgency().first { $0.needsApproval && !$0.approvalFingerprint.isEmpty }
    }

    var body: some View {
        CoucouGlassGroup(spacing: 12) {
            VStack(spacing: 12) {
                summary
                    .coucouGlass(.regular, cornerRadius: 28)
                    .coucouGlassID("island", in: glass)
                if let waiting {
                    panel(waiting)
                        .coucouGlass(.regular, cornerRadius: 26, tint: .orange)
                        .coucouGlassID("panel", in: glass)
                        .transition(Self.panelTransition)
                }
            }
        }
        .overlay {
            if approved {
                DrawnCheckmark(size: 64)
                    .padding(14)
                    .background(.ultraThinMaterial, in: Circle())
                    .transition(.scale.combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.55, bounce: 0.3), value: waiting?.approvalFingerprint)
        .animation(.spring(duration: 0.5, bounce: 0.2), value: "\(headline)|\(detail ?? "")")
        .animation(.spring(duration: 0.4, bounce: 0.3), value: approved)
        .sensoryFeedback(.success, trigger: approved) { _, new in new }
    }

    /// iOS 26 morphs the glass of the island into the panel; before, the panel slides down from it.
    private static var panelTransition: AnyTransition {
        if #available(iOS 26.0, *) { return .opacity }
        return .move(edge: .top).combined(with: .opacity)
    }

    private func panel(_ session: SessionItem) -> some View {
        ApprovalPanel(session: session, disabled: sending) {
            Task {
                sending = true
                _ = await QuickDecision.deny(session, link: link)
                sending = false
            }
        } allow: {
            Task {
                sending = true
                if await QuickDecision.allow(session, link: link) {
                    approved = true
                    try? await Task.sleep(for: .seconds(1.4))
                    approved = false
                }
                sending = false
            }
        }
    }

    private var summary: some View {
        HStack(spacing: 14) {
            MochiLive(state: sessions.leadState)
                .frame(width: 52, height: 52)
                .introLanding(.header)
            VStack(alignment: .leading, spacing: 3) {
                Text(headline)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .contentTransition(.interpolate)
                if let detail {
                    Text(detail)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.6))
                        .lineLimit(1)
                        .contentTransition(.interpolate)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
    }

    private var headline: String {
        switch link.status {
        case .noAccount: return String(localized: "iCloud not connected")
        case .failed: return String(localized: "Can't reach iCloud")
        default: break
        }
        return sessions.summary ?? (sessions.isEmpty ? String(localized: "Nothing yet") : String(localized: "All quiet"))
    }

    private var detail: String? {
        guard let lead = sessions.sortedByUrgency().first else { return nil }
        return "\(lead.title) · \(lead.statusText)"
    }
}

struct SessionRow: View {
    let session: SessionItem
    var zoom: Namespace.ID? = nil

    var body: some View {
        HStack(spacing: 12) {
            tile
            VStack(alignment: .leading, spacing: 2) {
                Text(session.title)
                    .font(.body.weight(.semibold))
                    .lineLimit(1)
                // The agent under the project name; for a session without a project
                // name (title is the agent already), what runs in it.
                Text(session.title == session.pillName
                     ? (PillCatalog.definition(for: session.id)?.sessionSubtitle ?? session.pillName)
                     : session.pillName)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                HStack(spacing: 5) {
                    StateSymbol(session: session).font(.caption)
                    Text(session.statusText)
                        .font(.subheadline.weight(session.isWaitingForYou ? .semibold : .regular))
                        .foregroundStyle(session.statusColor)
                        .contentTransition(.interpolate)
                }
                Text(session.updatedAt, style: .relative)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }

    @ViewBuilder private var tile: some View {
        let base = mochi
            .padding(4)
            .frame(width: 40, height: 40)
            .background(Color.agentTile(hex: session.color), in: RoundedRectangle(cornerRadius: 11))
        if let zoom {
            base.matchedTransitionSource(id: session.id, in: zoom)
        } else {
            base
        }
    }

    /// VS Code's Mochi is where the intro's Mochi lands.
    @ViewBuilder private var mochi: some View {
        if session.id == "integration_claude" {
            MochiLive(state: session.state, fps: 60).introLanding(.tile)
        } else {
            MochiLive(state: session.state, fps: 60)
        }
    }
}

struct EmptySessionsView: View {
    let status: PhoneLink.Status

    var body: some View {
        VStack(spacing: 8) {
            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(24)
        .glassCard()
    }

    private var message: String {
        // A paired Windows or Linux computer, or a build that has nothing else to talk to.
        let pc = PCLink.shared
        if pc.isPaired && (status == .ready || !PhoneLink.cloudAvailable) {
            switch pc.status {
            case .connected:
                return String(localized: "No session yet. Start Claude Code on \(pc.name), or chat with Mochi there in the Chat tab.")
            case .connecting, .searching:
                return String(localized: "Connecting to \(pc.name)…")
            default:
                return pc.status.text
            }
        }
        if !PhoneLink.cloudAvailable {
            return String(localized: "Pair with Coucou on your Windows or Linux computer: Settings → Windows or Linux computer.")
        }
        return switch status {
        case .starting: String(localized: "Connecting to iCloud…")
        case .noAccount(let reason): String(localized: "\(reason) Sign in to iCloud with the same account as your Mac.")
        case .zoneMissing: String(localized: "Open Coucou on your Mac: your sessions will show up here.")
        case .failed(let message): message
        case .ready: String(localized: "No session yet. Start Claude Code, Cursor or Codex on your Mac.")
        }
    }
}

/// Today, as this iPhone saw it: turns finished, files and lines changed,
/// commands you answered. Counted on the iPhone, never sent anywhere.
struct TodayCard: View {
    let link: PhoneLink

    private var decisionsToday: Int {
        link.history.filter { Calendar.current.isDateInToday($0.date) }.count
    }

    var body: some View {
        let tally = link.archive.currentTally
        if !tally.isEmpty || decisionsToday > 0 {
            HStack(spacing: 0) {
                stat("\(tally.turns)", tally.turns == 1 ? "turn" : "turns")
                stat("\(tally.files)", tally.files == 1 ? "file" : "files")
                VStack(spacing: 2) {
                    HStack(spacing: 4) {
                        Text(verbatim: "+\(tally.added)").foregroundStyle(CoucouPalette.ok)
                        Text(verbatim: "−\(tally.removed)").foregroundStyle(CoucouPalette.danger)
                    }
                    .font(.headline.monospacedDigit())
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                    Text("lines").font(.caption2).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
                stat("\(decisionsToday)", decisionsToday == 1 ? "OK given" : "OKs given")
            }
            .padding(.vertical, 12)
            .glassCard()
            .overlay(alignment: .topLeading) {
                Text("TODAY")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 10)
                    .offset(y: -16)
            }
            .padding(.top, 10)
        }
    }

    private func stat(_ value: String, _ label: LocalizedStringKey) -> some View {
        VStack(spacing: 2) {
            Text(value).font(.headline.monospacedDigit())
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
    }
}
