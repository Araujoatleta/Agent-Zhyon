import CryptoKit
import Foundation
import Network
import Observation
import Security
import UIKit
import UserNotifications

/// The link to Coucou on a Windows or Linux computer (docs/ANDROID_LINK.md), next to iCloud's link to a Mac
/// (PhoneLink). Paired once with the QR code in Settings → Phone on the computer, then TLS on the local network with
/// the computer's certificate pinned. Its sessions and requests join the Mac's on every screen (PhoneLink.sessions);
/// Allow, Deny, answers and the chat with Mochi go back on the same link. The wire itself is PCLinkWire.swift.
///
/// Away from home, the computer's own addresses on the internet (which it gives over the link once its router opened
/// a port for it: windows/src-tauri/src/phone_link/remote.rs) are tried after the local one, with the same pinned
/// certificate and the same token. No server of anyone is in between, and no name is looked up.
///
/// What it cannot do: wake the iPhone. iOS suspends an app soon after it leaves the screen, and only Apple's push can
/// wake it again, which needs a server and a paid developer account. So requests arrive while Coucou is open, or in
/// the half minute iOS grants after leaving it (as notifications); the link comes back by itself when Coucou opens.
@MainActor
@Observable
final class PCLink {
    static let shared = PCLink()

    enum Status: Equatable {
        /// No computer paired.
        case unpaired
        case connecting
        /// The saved address did not answer: looking for the computer on the local network.
        case searching
        case connected
        /// Not connected, and why in words (empty while the app is away from the screen).
        case offline(String)
        /// The computer refused this iPhone's token: it was paired again, or its code renewed.
        case refused
    }

    /// Mochi's pill on the computer (his hands, windows/src-tauri/src/agent), which the Mac's catalog has not.
    nonisolated static let mochiPill = "agent_mochi"
    nonisolated static let mochiColor = "#A5B4FC"

    private(set) var computer: PCComputer?
    private(set) var status: Status = .unpaired
    private(set) var model = PCLinkModel()
    /// The computer's sessions with what each waits on, as the Agents list shows them.
    private(set) var items: [SessionItem] = []
    /// What went wrong with the last decision, answer or chat message, in words.
    private(set) var lastError: String?
    /// The chat's model ("provider/model") and whether Mochi may use his hands on the computer.
    private(set) var chatModel: String
    private(set) var chatHands: Bool
    /// The link goes over the internet, to the computer's router, not the local network.
    private(set) var viaOutside = false

    @ObservationIgnored private var connection: NWConnection?
    /// The ways being tried, by their place in `plan`, until one has a link.
    @ObservationIgnored private var dialing: [Int: Attempt] = [:]
    @ObservationIgnored private var plan: [Route] = []
    @ObservationIgnored private var planned = 0
    @ObservationIgnored private var failures: [Failure] = []
    @ObservationIgnored private var dialTask: Task<Void, Never>?
    /// The way the link took (its place in `plan`).
    @ObservationIgnored private var linkID = -1
    /// The iPhone is on Wi-Fi or a wire, where the computer may be; not on cellular only.
    @ObservationIgnored private var onLocalNetwork = true
    @ObservationIgnored private var pathMonitor: NWPathMonitor?
    /// Counts connections, so what an old one reports late is ignored.
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var reader = PCLineReader()
    @ObservationIgnored private var welcomed = false
    @ObservationIgnored private var lastInbound = Date.distantPast
    /// Failed attempts in a row, for the wait before the next one.
    @ObservationIgnored private var attempt = 0
    @ObservationIgnored private var retryTask: Task<Void, Never>?
    @ObservationIgnored private var pingTask: Task<Void, Never>?
    @ObservationIgnored private var handshakeTask: Task<Void, Never>?
    @ObservationIgnored private var browser: NWBrowser?
    @ObservationIgnored private var searchTask: Task<Void, Never>?
    /// Found by its announcement on the network: its address is saved once the link works.
    @ObservationIgnored private var viaSearch = false
    @ObservationIgnored private var onScreen = true
    @ObservationIgnored private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    @ObservationIgnored private var waiters: [UUID: CheckedContinuation<Bool, Never>] = [:]
    /// Requests already notified, so a reconnection does not ring again; kept a few minutes across launches, so
    /// a notification's Allow or Deny still finds its way here after iOS ended the app.
    @ObservationIgnored private var notified: [String: Date] = [:]

    nonisolated private static let queue = DispatchQueue(label: "fr.louisraille.Coucou.pc-link")
    private static let notifiedKey = "pcLinkNotified"
    private static let chatModelKey = "pcLinkChatModel"
    private static let chatHandsKey = "pcLinkChatHands"

    private init() {
        // Locals first: an @Observable property cannot be read before every stored property is set.
        let paired = PCLinkStore.computer()
        let recent = UserDefaults.standard.dictionary(forKey: Self.notifiedKey) as? [String: Date] ?? [:]
        computer = paired
        status = paired == nil ? .unpaired : .offline("")
        chatModel = UserDefaults.standard.string(forKey: Self.chatModelKey) ?? ""
        chatHands = UserDefaults.standard.object(forKey: Self.chatHandsKey) as? Bool ?? true
        notified = recent.filter { Date.now.timeIntervalSince($0.value) < 600 }
        pathMonitor = Self.watchPath(link: self)
    }

    var isPaired: Bool { computer != nil }
    var name: String { computer?.displayName ?? "your computer" }

    // MARK: Pairing

    /// Pairs with the computer of a `coucou://pair` link, in place of any computer paired before.
    @discardableResult
    func pair(_ pairing: PCPairing) -> Bool {
        guard PCLinkStore.save(pairing) else {
            lastError = "Couldn't keep the pairing in the Keychain."
            return false
        }
        disconnect(sayBye: true)
        computer = pairing.computer
        model = PCLinkModel()
        rebuild()
        lastError = nil
        attempt = 0
        connect()
        return true
    }

    /// Forgets the computer and its token. The computer keeps its own code until "Pair again" there.
    func unpair() {
        disconnect(sayBye: true)
        stopSearching()
        retryTask?.cancel()
        PCLinkStore.forget()
        computer = nil
        model = PCLinkModel()
        rebuild()
        status = .unpaired
    }

    // MARK: The app on screen or away

    /// Coucou opened or came back on screen: connect now rather than after the wait.
    func appBecameActive() {
        onScreen = true
        endBackgroundTime()
        guard computer != nil else { status = .unpaired; return }
        if welcomed { send(.ping) } else { reconnectIfNeeded() }
    }

    /// A link that is down tries again now (the app came back, or a pull to refresh).
    func reconnectIfNeeded() {
        guard computer != nil, !busy, status != .refused else { return }
        retryTask?.cancel()
        attempt = 0
        connect()
    }

    /// Coucou left the screen: the link stays up for the time iOS grants, then says goodbye.
    func appWentAway() {
        onScreen = false
        guard busy, backgroundTask == .invalid else { return }
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Coucou computer link") { [weak self] in
            MainActor.assumeIsolated { self?.backgroundTimeOver() }
        }
    }

    private func backgroundTimeOver() {
        disconnect(sayBye: true)
        model.connectionLost()
        rebuild()
        if computer != nil, status != .refused { status = .offline("") }
        endBackgroundTime()
    }

    private func endBackgroundTime() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }

    // MARK: Connecting

    /// One way to the computer: an address, and whether it goes over the internet.
    private struct Route {
        let endpoint: NWEndpoint
        let outside: Bool
    }

    /// A way being tried.
    private struct Attempt {
        let connection: NWConnection
        let outside: Bool
    }

    private struct Failure {
        let reason: String
        let outside: Bool
    }

    /// The next way starts when the one before has not answered for this long, or at once when it failed.
    private static let headStart: Double = 1.5

    /// A link is up or being made.
    private var busy: Bool { connection != nil || !dialing.isEmpty }

    /// Where to try, in order: the address on the local network (skipped on cellular when there is another way), then
    /// the computer's addresses on the internet, if it gave any.
    private func routes(to computer: PCComputer) -> [Route] {
        let outside: [Route] = computer.remote.compactMap { text in
            guard let parsed = PCRemote.parse(text), let port = NWEndpoint.Port(rawValue: parsed.port) else { return nil }
            return Route(endpoint: .hostPort(host: NWEndpoint.Host(parsed.host), port: port), outside: true)
        }
        let local = Route(endpoint: .hostPort(host: NWEndpoint.Host(computer.host),
                                              port: NWEndpoint.Port(rawValue: computer.port) ?? 47821),
                          outside: false)
        return onLocalNetwork || outside.isEmpty ? [local] + outside : outside
    }

    private func connect(to endpoint: NWEndpoint? = nil) {
        guard let computer else { status = .unpaired; return }
        guard PCLinkStore.token() != nil else { status = .refused; return }
        // One attempt at a time: the saved address and then the ways over the internet, or the one the search found.
        stopSearching()
        disconnect(sayBye: false)
        generation += 1
        let generation = generation
        viaSearch = endpoint != nil
        if !viaSearch { status = .connecting }
        plan = endpoint.map { [Route(endpoint: $0, outside: false)] } ?? routes(to: computer)
        planned = 0
        failures = []
        reader = PCLineReader()
        welcomed = false
        lastInbound = .now
        dialNext(generation: generation)
        // No welcome within 10 s, and a little more per way to try (a wrong address, a computer asleep): later.
        let wait = 10 + Self.headStart * Double(max(0, plan.count - 1))
        handshakeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self, self.generation == generation, !self.welcomed else { return }
            self.lost(self.unreachableReason())
        }
    }

    /// Starts the next way of the plan, and the one after it if this one has not answered within its head start.
    private func dialNext(generation: Int) {
        guard generation == self.generation, connection == nil, planned < plan.count, let computer else { return }
        let id = planned
        let route = plan[id]
        planned += 1
        let candidate = NWConnection(to: route.endpoint, using: Self.parameters(pinned: computer.fingerprint))
        dialing[id] = Attempt(connection: candidate, outside: route.outside)
        Self.watch(candidate, generation: generation, id: id, link: self)
        candidate.start(queue: Self.queue)
        dialTask?.cancel()
        guard planned < plan.count else { return }
        dialTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.headStart))
            guard !Task.isCancelled, let self else { return }
            self.dialNext(generation: generation)
        }
    }

    /// The first way whose certificate checked out: the link goes on there, the others are dropped.
    private func adopt(_ trial: Attempt, id: Int, generation: Int) {
        dialing[id] = nil
        for other in dialing.values {
            other.connection.stateUpdateHandler = nil
            other.connection.cancel()
        }
        dialing = [:]
        dialTask?.cancel()
        let chosen = trial.connection
        connection = chosen
        linkID = id
        viaOutside = trial.outside
        guard let token = PCLinkStore.token() else { status = .refused; disconnect(sayBye: false); return }
        if viaSearch, let address = Self.address(of: chosen.currentPath?.remoteEndpoint), var computer {
            // The certificate matched: this is the paired computer, at a new address.
            computer.host = address.host
            computer.port = address.port
            self.computer = computer
            PCLinkStore.save(computer)
        }
        Self.receive(on: chosen, generation: generation, link: self)
        send(.hello(token: token, device: UIDevice.current.name, caps: PCWire.capabilities))
    }

    /// Why no way worked, in words. Away from home the local address may be another device's (a café's Wi-Fi), so
    /// what it answered is not taken for the computer's, except the iPhone's own setting for the local network.
    private func unreachableReason() -> String {
        let reasons = failures.map(\.reason)
        if reasons.contains(LinkEvent.noLocalNetwork) { return LinkEvent.noLocalNetwork }
        if !reasons.isEmpty, reasons.allSatisfy({ $0 == LinkEvent.otherCertificate }) { return LinkEvent.otherCertificate }
        if plan.contains(where: { $0.outside }) {
            return "\(name) can't be reached, at home or over the internet. Is it awake, with Coucou running?"
        }
        return failures.first?.reason ?? LinkEvent.unreachable
    }

    /// TLS 1.2 or later, and exactly one certificate: the one whose SHA-256 the QR code carried. No authority, no
    /// host name check (the computer's certificate is its own, self-signed). Made outside the main actor: the check
    /// runs on the link's queue.
    nonisolated private static func parameters(pinned: String) -> NWParameters {
        let tls = NWProtocolTLS.Options()
        let security = tls.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(security, .TLSv12)
        sec_protocol_options_set_verify_block(security, { _, trust, complete in
            complete(matches(sec_trust_copy_ref(trust).takeRetainedValue(), pinned: pinned))
        }, queue)
        let tcp = NWProtocolTCP.Options()
        tcp.connectionTimeout = 6
        tcp.noDelay = true
        tcp.enableKeepalive = true
        tcp.keepaliveIdle = 30
        let parameters = NWParameters(tls: tls, tcp: tcp)
        parameters.includePeerToPeer = false
        return parameters
    }

    /// The leaf certificate's SHA-256 is the pinned one (compared in constant time).
    nonisolated private static func matches(_ trust: SecTrust, pinned: String) -> Bool {
        guard let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate], let leaf = chain.first else { return false }
        let digest = SHA256.hash(data: SecCertificateCopyData(leaf) as Data)
        let actual = Array(digest.map { String(format: "%02x", $0) }.joined().utf8)
        let expected = Array(pinned.lowercased().utf8)
        guard actual.count == expected.count else { return false }
        return zip(actual, expected).reduce(UInt8(0)) { $0 | ($1.0 ^ $1.1) } == 0
    }

    nonisolated private static func watch(_ connection: NWConnection, generation: Int, id: Int, link: PCLink) {
        connection.stateUpdateHandler = { [weak link] state in
            guard let event = LinkEvent(state), let link else { return }
            Task { @MainActor in link.connectionChanged(event, generation: generation, id: id) }
        }
    }

    /// Which network the iPhone is on, as it changes.
    nonisolated private static func watchPath(link: PCLink) -> NWPathMonitor {
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak link] path in
            let local = path.usesInterfaceType(.wifi) || path.usesInterfaceType(.wiredEthernet)
            let up = path.status == .satisfied
            guard let link else { return }
            Task { @MainActor in link.pathChanged(local: local, up: up) }
        }
        monitor.start(queue: queue)
        return monitor
    }

    /// The iPhone changed network. Leaving the Wi-Fi while linked through it moves the link to the internet now (when
    /// the computer gave addresses for it); a network coming back is tried at once rather than after the wait.
    private func pathChanged(local: Bool, up: Bool) {
        let left = onLocalNetwork && !local
        onLocalNetwork = local
        guard let computer, up, status != .refused, onScreen || backgroundTask != .invalid else { return }
        if left, welcomed, !viaOutside, !computer.remote.isEmpty {
            lost("")
            retryTask?.cancel()
            attempt = 0
            connect()
        } else if !busy, browser == nil {
            reconnectIfNeeded()
        }
    }

    nonisolated private static func receive(on connection: NWConnection, generation: Int, link: PCLink) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65_536) { [weak link] data, _, isComplete, error in
            guard let link else { return }
            let ended = isComplete || error != nil
            Task { @MainActor in link.received(data, ended: ended, generation: generation) }
        }
    }

    nonisolated private static func write(_ data: Data, on connection: NWConnection) {
        connection.send(content: data, completion: .contentProcessed { _ in })
    }

    private func connectionChanged(_ event: LinkEvent, generation: Int, id: Int) {
        guard generation == self.generation else { return }
        if let trial = dialing[id] {
            switch event {
            case .ready:
                adopt(trial, id: id, generation: generation)
            case .waiting(let reason), .failed(let reason):
                // This way does not answer: the next one now, or, when none is left, try again later.
                trial.connection.stateUpdateHandler = nil
                trial.connection.cancel()
                dialing[id] = nil
                failures.append(Failure(reason: reason, outside: trial.outside))
                guard dialing.isEmpty else { return }
                if planned < plan.count {
                    dialNext(generation: generation)
                } else {
                    lost(unreachableReason())
                }
            case .cancelled:
                break
            }
            return
        }
        guard connection != nil, id == linkID else { return }
        switch event {
        case .ready, .cancelled:
            break
        case .waiting(let reason), .failed(let reason):
            lost(reason)
        }
    }

    private func received(_ data: Data?, ended: Bool, generation: Int) {
        guard generation == self.generation, let connection else { return }
        if let data, !data.isEmpty {
            lastInbound = .now
            guard let lines = reader.feed(data) else {
                lost("The computer sent something this iPhone can't read.")
                return
            }
            for line in lines {
                handle(line)
                // An error line or a broken one closes the link: the rest is not read.
                guard generation == self.generation else { return }
            }
        }
        if ended {
            lost(welcomed ? "The computer closed the link." : LinkEvent.unreachable)
            return
        }
        Self.receive(on: connection, generation: generation, link: self)
    }

    private func handle(_ line: Data) {
        guard let message = PCIncoming.decode(line) else {
            lost("The computer sent something this iPhone can't read.")
            return
        }
        switch message {
        case .remote(let endpoints):
            // Where to try from outside next time (none: the computer's switch is off). Kept with the pairing.
            if var computer, computer.remote != endpoints {
                computer.remote = endpoints
                self.computer = computer
                PCLinkStore.save(computer)
            }
        case .welcome:
            welcomed = true
            attempt = 0
            handshakeTask?.cancel()
            stopSearching()
            status = .connected
            startPinging()
            clearStaleBanners()
        case .error(let code, _):
            if code == "auth" {
                // The token is not the computer's any more: retrying would only be refused again.
                disconnect(sayBye: false)
                model.connectionLost()
                rebuild()
                status = .refused
                finishWaiters(false)
                return
            }
            lost("The computer closed the link.")
            return
        default:
            break
        }
        let effects = model.apply(message, now: .now)
        if case .welcome = message {
            if model.chatAllowed { send(.chatModels) }
            finishWaiters(true)
        }
        if case .chatModels(let list) = message, !list.contains(where: { $0.id == chatModel }), let first = list.first {
            setChatModel(first.id)
        }
        switch message {
        case .sessions, .approval, .question, .resolved:
            rebuild()
            notifyEvents()
        default:
            break
        }
        react(to: effects)
    }

    /// The link is down: everything that waited on it learns so, and the next try is planned.
    private func lost(_ reason: String) {
        let wasUp = welcomed
        disconnect(sayBye: false)
        model.connectionLost()
        rebuild()
        finishWaiters(false)
        guard computer != nil else { status = .unpaired; return }
        if case .refused = status { return }
        status = .offline(onScreen ? reason : "")
        if wasUp { attempt = 0 }
        scheduleRetry()
    }

    /// 1, 2, 4… up to 30 s between tries, on screen only (away, the next try is when Coucou opens). After two
    /// misses at the saved address, the computer is looked for on the network: its address may have changed.
    private func scheduleRetry() {
        retryTask?.cancel()
        guard onScreen || backgroundTask != .invalid else { return }
        let delays: [Double] = [1, 2, 4, 8, 16, 30]
        let delay = delays[min(attempt, delays.count - 1)]
        attempt += 1
        // On cellular only, the local network cannot be searched.
        let search = onLocalNetwork && (attempt == 2 || attempt % 4 == 0)
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled, let self, !self.busy else { return }
            if search { self.search() } else { self.connect() }
        }
    }

    private func disconnect(sayBye: Bool) {
        handshakeTask?.cancel()
        pingTask?.cancel()
        dialTask?.cancel()
        for trial in dialing.values {
            trial.connection.stateUpdateHandler = nil
            trial.connection.cancel()
        }
        dialing = [:]
        if viaOutside { viaOutside = false }
        if let connection {
            if sayBye && welcomed { Self.write(PCOutgoing.bye.line, on: connection) }
            connection.stateUpdateHandler = nil
            connection.cancel()
        }
        connection = nil
        welcomed = false
        generation += 1
    }

    private func send(_ message: PCOutgoing) {
        guard let connection else { return }
        Self.write(message.line, on: connection)
    }

    /// A ping every 20 s; a computer silent for 50 s is gone. Requests older than 2 minutes expire meanwhile.
    private func startPinging() {
        pingTask?.cancel()
        let generation = generation
        pingTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(PCWire.pingInterval))
                guard !Task.isCancelled, let self, self.generation == generation else { return }
                if Date.now.timeIntervalSince(self.lastInbound) > PCWire.pingInterval * 2.5 {
                    self.lost("The computer stopped answering.")
                    return
                }
                self.send(.ping)
                let expired = self.model.expire(now: .now)
                if !expired.isEmpty {
                    self.rebuild()
                    self.react(to: expired)
                }
            }
        }
    }

    /// Waits until the link works, connecting if needed: an Allow tapped in a notification may come after iOS
    /// closed the link.
    private func ensureConnected(within seconds: Double = 8) async -> Bool {
        if welcomed, connection != nil { return true }
        guard computer != nil, status != .refused else { return false }
        if !busy {
            retryTask?.cancel()
            attempt = 0
            connect()
        }
        let id = UUID()
        return await withCheckedContinuation { continuation in
            waiters[id] = continuation
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(seconds))
                self?.finishWaiter(id, false)
            }
        }
    }

    private func finishWaiter(_ id: UUID, _ ok: Bool) {
        waiters.removeValue(forKey: id)?.resume(returning: ok)
    }

    private func finishWaiters(_ ok: Bool) {
        let all = waiters
        waiters = [:]
        for continuation in all.values { continuation.resume(returning: ok) }
    }

    // MARK: Finding the computer again

    /// Looks for the computer's announcement (`_coucou._tcp`, TXT `fp` = the first 16 characters of the pinned
    /// fingerprint) for 12 s. The announcement is only a hint: the TLS check against the full fingerprint decides.
    private func search() {
        guard let computer, browser == nil else { return }
        status = .searching
        browser = Self.browse(for: computer.shortFingerprint, link: self)
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(12))
            guard !Task.isCancelled, let self, self.browser != nil else { return }
            self.stopSearching()
            if !self.busy {
                self.status = .offline(self.onScreen ? "\(self.name) isn't on this network. Same Wi-Fi? Computer awake and Coucou running? Guest networks and hotspots can keep devices apart." : "")
                self.scheduleRetry()
            }
        }
    }

    nonisolated private static func browse(for wanted: String, link: PCLink) -> NWBrowser {
        let parameters = NWParameters()
        parameters.includePeerToPeer = false
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_coucou._tcp", domain: nil), using: parameters)
        browser.browseResultsChangedHandler = { [weak link] results, _ in
            guard let link else { return }
            for result in results {
                guard case .bonjour(let txt) = result.metadata,
                      txt["fp"]?.lowercased() == wanted, (txt["v"] ?? "1") == "1",
                      case .service(let name, let type, let domain, _) = result.endpoint else { continue }
                Task { @MainActor in link.found(name: name, type: type, domain: domain) }
                return
            }
        }
        browser.start(queue: queue)
        return browser
    }

    private func found(name: String, type: String, domain: String) {
        guard browser != nil else { return }
        stopSearching()
        connect(to: .service(name: name, type: type, domain: domain, interface: nil))
    }

    private func stopSearching() {
        searchTask?.cancel()
        browser?.browseResultsChangedHandler = nil
        browser?.cancel()
        browser = nil
    }

    nonisolated private static func address(of endpoint: NWEndpoint?) -> (host: String, port: UInt16)? {
        guard case .hostPort(let host, let port)? = endpoint else { return nil }
        switch host {
        case .ipv4(let address): return ("\(address)", port.rawValue)
        case .ipv6(let address): return ("\(address)", port.rawValue)
        case .name(let name, _): return (name, port.rawValue)
        @unknown default: return nil
        }
    }

    // MARK: Answers

    /// A request or a question of this computer (on screen now, or notified a moment ago).
    func owns(_ fingerprint: String) -> Bool {
        guard computer != nil, !fingerprint.isEmpty else { return false }
        return model.approvals.contains { $0.fingerprint == fingerprint }
            || model.questions.contains { $0.fingerprint == fingerprint }
            || notified[fingerprint] != nil
    }

    /// Allow or Deny. Face ID for Allow is asked before (ApprovalCard, QuickDecision), or iOS asked to unlock (a
    /// notification's Allow). The computer applies it only if the request still waits.
    func decide(allow: Bool, fingerprint: String) async -> Bool {
        lastError = nil
        if allow, model.approvals.first(where: { $0.fingerprint == fingerprint })?.truncated == true {
            lastError = "This command is longer than what reached the iPhone: deny it here, or answer it on \(name)."
            return false
        }
        guard await ensureConnected() else {
            lastError = "Can't reach \(name). Is Coucou open on it, on the same Wi-Fi?"
            return false
        }
        send(.decision(fingerprint: fingerprint, allow: allow))
        removeNotification(fingerprint)
        return true
    }

    /// The labels picked, one list per question, exactly as the computer sent them.
    func answer(fingerprint: String, picks: [[String]]) async -> Bool {
        lastError = nil
        guard await ensureConnected() else {
            lastError = "Can't reach \(name). Is Coucou open on it, on the same Wi-Fi?"
            return false
        }
        guard model.caps.contains("answers") else {
            lastError = "Answer on \(name): turn on Settings → Phone → \"Let the phone answer Claude Code's questions\" there to answer from here."
            return false
        }
        send(.answer(fingerprint: fingerprint, picks: picks))
        removeNotification(fingerprint)
        return true
    }

    // MARK: Chat with Mochi on the computer

    var chatModels: [PCChatModel] { model.models }
    var chatRunning: Bool { model.running != nil }

    func setChatModel(_ id: String) {
        chatModel = id
        UserDefaults.standard.set(id, forKey: Self.chatModelKey)
    }

    func setChatHands(_ on: Bool) {
        chatHands = on
        UserDefaults.standard.set(on, forKey: Self.chatHandsKey)
    }

    /// Sends a message to Mochi on the computer, with his hands if they are on there and here.
    func sendChat(_ text: String) {
        let text = PCText.clip(text.trimmingCharacters(in: .whitespacesAndNewlines), max: PCWire.maxChatText)
        guard !text.isEmpty, model.running == nil else { return }
        guard welcomed, connection != nil else {
            lastError = "Not connected to \(name)."
            return
        }
        guard model.chatAllowed else {
            lastError = "The chat is off on \(name): Settings → Phone → \"Let the phone chat with my AI providers\"."
            return
        }
        guard !chatModel.isEmpty else {
            lastError = "Pick a model first."
            return
        }
        lastError = nil
        let id = PCLinkModel.newChatID()
        let hands = chatHands && model.handsAllowed
        model.startChat(id: id, text: text, hands: hands)
        send(.chatSend(id: id, model: chatModel, text: text, agent: hands))
    }

    func stopChat() {
        guard let id = model.running else { return }
        send(.chatCancel(id: id))
    }

    /// A new conversation: the computer forgets the phone's one and stops an answer being written.
    func newChat() {
        if welcomed { send(.chatReset) }
        model.resetChat()
    }

    func refreshModels() {
        if welcomed, model.chatAllowed { send(.chatModels) }
    }

    // MARK: Sessions for the screens

    /// The computer's sessions as the Agents list shows them; a request whose session the computer did not list
    /// (yet) still gets its row.
    private func rebuild() {
        guard let computer else {
            items = []
            PhoneLink.shared.computerSessionsChanged()
            return
        }
        var list: [SessionItem] = []
        var pills = Set<String>()
        for session in model.sessions where pills.insert(session.pillId).inserted {
            list.append(SessionItem(computer: session,
                                    approval: model.approvals.first { $0.pillId == session.pillId },
                                    question: model.questions.first { $0.pillId == session.pillId },
                                    computerName: computer.displayName))
        }
        let waiting = Set(model.approvals.map(\.pillId) + model.questions.map(\.pillId))
        for pill in waiting where pills.insert(pill).inserted {
            let approval = model.approvals.first { $0.pillId == pill }
            let question = model.questions.first { $0.pillId == pill }
            let agent = pill == Self.mochiPill ? "Mochi" : PillCatalog.definition(for: pill)?.name ?? "Agent"
            let session = PCSession(pillId: pill, agent: agent, state: approval != nil ? "approval" : "question",
                                    statusText: "", stepIndex: 0, stepCount: 0,
                                    updatedAt: approval?.receivedAt ?? question?.receivedAt ?? .now)
            list.append(SessionItem(computer: session, approval: approval, question: question,
                                    computerName: computer.displayName))
        }
        items = list.sorted { $0.updatedAt > $1.updatedAt }
        PhoneLink.shared.computerSessionsChanged()
    }

    // MARK: Notifications (only while Coucou is away from the screen)

    private func react(to effects: [PCLinkModel.Effect]) {
        for effect in effects {
            switch effect {
            case .newApproval(let approval):
                Task { await notify(approval) }
            case .newQuestion:
                break   // AgentNotifier tells about questions, as it does for the Mac's
            case .resolved(let fingerprint):
                removeNotification(fingerprint)
            }
        }
    }

    private func notify(_ approval: PCApproval) async {
        guard notified[approval.fingerprint] == nil else { return }
        remember(approval.fingerprint)
        guard UIApplication.shared.applicationState != .active else { return }
        let session = items.first { $0.approvalFingerprint == approval.fingerprint }
        let content = UNMutableNotificationContent()
        content.title = session.map { "\($0.pillName) · \($0.title)" } ?? name
        let what = session?.approvalCommand ?? approval.command
        content.body = what.isEmpty ? "An agent is waiting for your OK" : "Waiting for your OK: \(what.prefix(140))"
        content.sound = PhoneSettings.sound("approval")
        content.categoryIdentifier = NotificationActions.approvalCategory
        content.userInfo = ["fingerprint": approval.fingerprint, "pillId": approval.pillId]
        content.threadIdentifier = approval.pillId
        let request = UNNotificationRequest(identifier: PhoneLink.approvalNotificationID(approval.fingerprint),
                                            content: content, trigger: nil)
        try? await UNUserNotificationCenter.current().add(request)
    }

    /// Finished, failed and question notifications for the computer's sessions, as for the Mac's.
    private func notifyEvents() {
        let events = items.compactMap { AgentNotifier.event(for: $0) }
        guard !events.isEmpty else { return }
        for event in events {
            if case .question(let session, _) = event { remember(session.questionFingerprint) }
        }
        Task {
            for event in events { await AgentNotifier.post(event, turn: nil) }
        }
    }

    private func remember(_ fingerprint: String) {
        guard !fingerprint.isEmpty else { return }
        notified[fingerprint] = .now
        notified = notified.filter { Date.now.timeIntervalSince($0.value) < 600 }
        UserDefaults.standard.set(notified, forKey: Self.notifiedKey)
    }

    /// Once the computer has said again what still waits (it does right after the welcome), the banners of requests
    /// answered at the desk while the link was down go.
    private func clearStaleBanners() {
        let generation = generation
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(3))
            guard let self, self.generation == generation, self.welcomed else { return }
            let pending = Set(self.model.approvals.map(\.fingerprint) + self.model.questions.map(\.fingerprint))
            let stale = self.notified.keys.filter { !pending.contains($0) }
            guard !stale.isEmpty else { return }
            UNUserNotificationCenter.current().removeDeliveredNotifications(
                withIdentifiers: stale.map { PhoneLink.approvalNotificationID($0) })
        }
    }

    private func removeNotification(_ fingerprint: String) {
        UNUserNotificationCenter.current().removeDeliveredNotifications(
            withIdentifiers: [PhoneLink.approvalNotificationID(fingerprint)])
    }
}

/// What a connection reports, reduced to what the link needs: made on the link's queue, used on the main actor.
private enum LinkEvent: Sendable {
    case ready
    case waiting(String)
    case failed(String)
    case cancelled

    static let unreachable = "The computer can't be reached. Same Wi-Fi? Computer awake and Coucou running?"
    static let otherCertificate = "The computer's certificate doesn't match the pairing. Pair again with the code in Settings → Phone on the computer."
    static let noLocalNetwork = "Coucou may not use the local network: iPhone Settings → Privacy & Security → Local Network → Coucou."

    init?(_ state: NWConnection.State) {
        switch state {
        case .ready: self = .ready
        case .waiting(let error): self = .waiting(Self.describe(error))
        case .failed(let error): self = .failed(Self.describe(error))
        case .cancelled: self = .cancelled
        default: return nil
        }
    }

    static func describe(_ error: NWError) -> String {
        switch error {
        case .tls:
            return otherCertificate
        case .dns(let code) where code == -65570:
            return noLocalNetwork
        case .posix(let code) where code == .ECONNREFUSED:
            return "The computer refused the connection. Is the phone link on, in Coucou's Settings → Phone?"
        default:
            return unreachable
        }
    }
}

/// Where the pairing is kept: the computer (address, certificate fingerprint, name) in the app's defaults, and the
/// token, the only secret, in the Keychain of this iPhone only (not in backups, not in iCloud Keychain).
enum PCLinkStore {
    private static let computerKey = "pcLinkComputer"
    private static let service = "fr.louisraille.Coucou.pc-link"
    private static let account = "token"

    static func computer() -> PCComputer? {
        guard let data = UserDefaults.standard.data(forKey: computerKey),
              let computer = try? JSONDecoder().decode(PCComputer.self, from: data),
              PCPairing.isFingerprint(computer.fingerprint) else { return nil }
        return computer
    }

    static func save(_ computer: PCComputer) {
        if let data = try? JSONEncoder().encode(computer) { UserDefaults.standard.set(data, forKey: computerKey) }
    }

    @discardableResult
    static func save(_ pairing: PCPairing) -> Bool {
        let base: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                   kSecAttrService as String: service,
                                   kSecAttrAccount as String: account]
        SecItemDelete(base as CFDictionary)
        var item = base
        item[kSecValueData as String] = Data(pairing.token.utf8)
        // Readable after the first unlock, so a notification's answer works with the iPhone locked again.
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        guard SecItemAdd(item as CFDictionary, nil) == errSecSuccess else { return false }
        save(pairing.computer)
        return true
    }

    static func token() -> String? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service,
                                    kSecAttrAccount as String: account,
                                    kSecReturnData as String: true,
                                    kSecMatchLimit as String: kSecMatchLimitOne]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data, let token = String(data: data, encoding: .utf8), !token.isEmpty else { return nil }
        return token
    }

    static func forget() {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service,
                                    kSecAttrAccount as String: account]
        SecItemDelete(query as CFDictionary)
        UserDefaults.standard.removeObject(forKey: computerKey)
    }
}

extension SessionItem {
    /// A session of the paired computer, with the request or the question it waits on.
    init(computer session: PCSession, approval: PCApproval?, question: PCQuestion?, computerName: String) {
        id = session.pillId
        name = session.project
        color = !session.color.isEmpty ? session.color
            : session.pillId == PCLink.mochiPill ? PCLink.mochiColor
            : PillCatalog.definition(for: session.pillId)?.defaultColor ?? "#C0C4CC"
        let waitsForOK = approval != nil
        state = waitsForOK ? .approval : question != nil ? .question
            : BotState(rawValue: session.state) ?? .idle
        // The latest steps (with session details on the computer), else what it does now.
        steps = !session.steps.isEmpty ? session.steps : session.statusText.isEmpty ? [] : [session.statusText]
        stepIndex = max(0, steps.count - 1)
        needsApproval = waitsForOK
        if let approval {
            let what = approval.tool.isEmpty || approval.tool == "Bash" ? approval.command : "\(approval.tool) · \(approval.command)"
            approvalCommand = approval.truncated ? what + "…" : what
            approvalFingerprint = approval.fingerprint
        } else {
            approvalCommand = ""
            approvalFingerprint = ""
        }
        // The computer runs no instruction sent from a phone.
        acceptsInstructions = false
        if let question {
            self.question = question.items.first?.question ?? ""
            questionPayload = QuestionPayload(items: question.items.map { item in
                QuestionPayload.Item(question: item.question, header: "",
                                     options: item.options.map { QuestionPayload.Option(label: $0.label, description: $0.description) },
                                     multiSelect: item.multiSelect)
            })
            questionFingerprint = question.fingerprint
        } else {
            self.question = ""
            questionPayload = nil
            questionFingerprint = ""
        }
        finalLine = session.finalLine
        // Never a path from the computer, only the folder's name (in `name`).
        cwd = ""
        updatedAt = max(session.updatedAt, approval?.receivedAt ?? .distantPast, question?.receivedAt ?? .distantPast)
        macName = computerName
        fromComputer = true
        agentName = session.pillId == PCLink.mochiPill ? "Mochi" : session.agent
    }
}
