import Foundation
import Observation
import os
import Security
import UIKit
import UserNotifications

/// Hermes Agent inside this iPhone (NotchBuddy/Hermes/README.md): CPython embedded in the app runs Hermes itself, its
/// memory, skills and sessions in the app's own folder, behind a small API on 127.0.0.1 only
/// (NotchBuddy/Hermes/app/axyon_hermes/host.py) with a token made for each launch. The iPhone is its body: what
/// Hermes asks of it (a reminder, an event, a shortcut, a device of the home) comes back here over the bridge and is
/// done by HermesArms. No computer is needed; a paired one only adds its own home and agents.
///
/// Only the sideload build made with Hermes/build-ios.sh carries Python (`HERMES`); in any other build `available`
/// is false and nothing here starts.
@MainActor
@Observable
final class HermesEngine {
    static let shared = HermesEngine()

    /// This build carries Hermes.
    nonisolated static let available: Bool = {
        #if HERMES
        return true
        #else
        return false
        #endif
    }()

    /// How Hermes' start went, in the system log (Console, a device's sysdiagnose, the Simulator test).
    nonisolated static let log = Logger(subsystem: "fr.louisraille.Coucou", category: "Hermes")

    enum State: Equatable {
        case off
        case starting
        case ready
        case failed(String)
    }

    private(set) var state: State = .off
    /// The brain, as Hermes says it is set (nil until it answered).
    private(set) var settings: HermesSettings?
    private(set) var conversation: HermesConversation
    /// The answer being written (its entry's id).
    private(set) var answering: String?
    private(set) var signIn = HermesSignIn(state: .idle)
    private(set) var lastError: String?
    /// When the computer last gave this iPhone the portfolio (nil: never, or forgotten).
    private(set) var portfolioDate: Date?
    /// The portfolio with today's prices, as the Wallet tab shows it.
    private(set) var portfolio: [String: Any]?
    private(set) var portfolioLoading = false
    /// What Hermes is doing now, for the orb.
    private(set) var acting = false
    /// The devices this iPhone drives itself (Hermes/app/axyon_hermes/home.py), as last read.
    private(set) var house: HermesHouse?
    private(set) var houseLoading = false
    /// The user's Google account in Hermes, as last read.
    private(set) var google: HermesGoogle?
    /// An e-mail Hermes wants to send, waiting for the user's tap in the chat.
    private(set) var mailAsk: HermesMailAsk?

    @ObservationIgnored private var port: UInt16 = 0
    @ObservationIgnored private var token = ""
    @ObservationIgnored private var runID: String?
    @ObservationIgnored private var runTask: Task<Void, Never>?
    @ObservationIgnored private var bridgeTask: Task<Void, Never>?
    @ObservationIgnored private var signInTask: Task<Void, Never>?
    @ObservationIgnored private var mailWaiting: CheckedContinuation<Bool, Never>?
    @ObservationIgnored private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    /// Python returned: it cannot start again in this launch.
    @ObservationIgnored private var pythonStopped = false
    @ObservationIgnored private let session: URLSession

    nonisolated static let cutText = "The answer stopped there: AXYON closed before it ended."

    private init() {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 90
        config.timeoutIntervalForResource = 60 * 60
        config.waitsForConnectivity = false
        config.connectionProxyDictionary = [:]
        session = URLSession(configuration: config)
        var saved = HermesChatStore.load() ?? .fresh()
        saved.restored(reason: String(localized: "The answer stopped there: AXYON closed before it ended."))
        conversation = saved
        portfolioDate = HermesPortfolio.receivedAt()
    }

    var mood: AXYONMood {
        switch state {
        case .off: return .sleeping
        case .starting: return .starting
        case .failed: return .trouble
        case .ready:
            if acting { return .acting }
            return answering != nil ? .thinking : .idle
        }
    }

    /// Hermes can answer: running, with a brain it can use.
    var canChat: Bool { state == .ready && settings?.canThink == true }

    // MARK: Starting

    /// Starts Python and Hermes in it, once per launch (CPython cannot start twice in one process).
    func start() {
        guard Self.available, state == .off else { return }
        state = .starting
        token = Self.randomToken()
        port = Self.freePort() ?? 47_830
        #if targetEnvironment(simulator)
        // CI's test of Hermes inside iOS (Hermes/tests/simulator.sh) knows where to knock. The Simulator only.
        let environment = ProcessInfo.processInfo.environment
        if let wanted = environment["AXYON_TEST_PORT"].flatMap(UInt16.init) { port = wanted }
        if let wanted = environment["AXYON_TEST_TOKEN"], wanted.count >= 16 { token = wanted }
        #endif
        let home = Self.home
        try? FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: home.appendingPathComponent("host-error.txt"))
        try? FileManager.default.removeItem(at: home.appendingPathComponent("python-ended.txt"))
        Task.detached(priority: .utility) { AXYONInbox.prune() }
        let listening = port
        Self.log.info("Starting Hermes on 127.0.0.1:\(listening, privacy: .public)")
        setenv("AXYON_HERMES_HOME", home.path, 1)
        setenv("AXYON_HOST_PORT", String(port), 1)
        setenv("AXYON_HOST_TOKEN", token, 1)
        setenv("AXYON_PORTFOLIO", HermesPortfolio.fileURL.path, 1)
        setenv("AXYON_POWERS", HermesPowers.folder.path, 1)
        setenv("AXYON_LANGUAGE", Locale.preferredLanguages.first ?? "pt-BR", 1)
        #if HERMES
        let resources = Bundle.main.resourcePath ?? ""
        let thread = Thread {
            let size = 2048
            var error = [CChar](repeating: 0, count: size)
            _ = axyon_python_run(resources, &error, size)
            let said = error.withUnsafeBufferPointer { String(cString: $0.baseAddress!) }
            Task { @MainActor in HermesEngine.shared.pythonEnded(said) }
        }
        thread.name = "Hermes"
        thread.stackSize = 16 << 20
        thread.qualityOfService = .userInitiated
        thread.start()
        #endif
        Task { await waitUntilReady() }
    }

    /// Ready, or why not. A start slower than four minutes (an older iPhone, its first launch) is said to be late, and
    /// still becomes ready if it does within a quarter of an hour: the app keeps asking, more slowly.
    private func waitUntilReady() async {
        let started = Date.now
        var late = false
        while Date.now.timeIntervalSince(started) < 900, !pythonStopped {
            if case .failed = state, !late { return }
            if let health = try? await call("GET", "/health", timeout: 3) {
                if health["ready"] as? Bool == true {
                    Self.log.info("Hermes is ready")
                    state = .ready
                    await pushKeys()
                    await refreshSettings()
                    listenToBridge()
                    // The documents read while AXYON Core started, digested now, and a meeting due held.
                    Task { await AXYONInvest.shared.think() }
                    return
                }
                if let problem = health["problem"] as? String, !problem.isEmpty {
                    state = .failed(Self.hostError() ?? problem)
                    return
                }
            }
            if !late, Date.now.timeIntervalSince(started) > 240 {
                Self.log.error("Hermes did not answer on its port in time")
                state = .failed(Self.hostError() ?? String(localized: "Hermes did not start in time."))
                late = true
            }
            try? await Task.sleep(for: late ? .seconds(5) : .milliseconds(400))
        }
    }

    private func pythonEnded(_ said: String) {
        pythonStopped = true
        Self.log.error("Python ended: \(said, privacy: .public)")
        // Beside the host's own traceback, for a test or a look at the app's folder.
        try? said.write(to: Self.home.appendingPathComponent("python-ended.txt"), atomically: true, encoding: .utf8)
        state = .failed(Self.hostError() ?? said)
    }

    /// The host's own traceback, when it stopped on one.
    private static func hostError() -> String? {
        guard let text = try? String(contentsOf: home.appendingPathComponent("host-error.txt"), encoding: .utf8),
              !text.isEmpty else { return nil }
        let lines = text.split(separator: "\n").suffix(6).joined(separator: "\n")
        return String(lines.prefix(800))
    }

    // MARK: The app on screen or away

    func appBecameActive() {
        guard Self.available else { return }
        portfolioDate = HermesPortfolio.receivedAt()
        // The reader looks for the assets' new documents, at most every half hour (AXYONInvest.swift), once the app
        // has settled.
        Task {
            try? await Task.sleep(for: .seconds(15))
            await AXYONInvest.shared.wake()
        }
        if state == .off {
            // A moment after the launch, so the first screen draws first.
            Task {
                try? await Task.sleep(for: .milliseconds(600))
                start()
            }
            return
        }
        #if HERMES
        // iOS may have taken the server's socket while the app was suspended: it listens again if so.
        if state == .ready { _ = axyon_python_exec("import axyon_hermes.host as h; h.revive()") }
        #endif
        if state == .ready, bridgeTask == nil { listenToBridge() }
    }

    /// The app left the screen: an answer being written gets the time iOS grants to end.
    func appWentAway() {
        guard answering != nil, backgroundTask == .invalid else { return }
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Hermes answer") { [weak self] in
            MainActor.assumeIsolated { self?.endBackgroundTime() }
        }
    }

    private func endBackgroundTime() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }

    // MARK: Chatting

    /// A message, with the photos and files the user sent with it (in Hermes' inbox: AXYONInbox).
    func send(_ text: String, attachments: [HermesAttachment] = []) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty || !attachments.isEmpty, state == .ready, answering == nil else { return }
        lastError = nil
        let answer = conversation.ask(text, attachments: attachments)
        answering = answer
        save()
        runTask = Task { await run(text, attachments: attachments, answer: answer) }
    }

    private func run(_ text: String, attachments: [HermesAttachment], answer: String) async {
        do {
            var body: [String: Any] = ["session": conversation.session, "message": text]
            if !attachments.isEmpty { body["attachments"] = attachments.map { $0.wire(inbox: AXYONInbox.folder) } }
            let started = try await call("POST", "/v1/runs", body)
            guard let run = started["run_id"] as? String else { throw HermesError.broken }
            runID = run
            var request = URLRequest(url: url("/v1/runs/\(run)/events"), timeoutInterval: 120)
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
            try await Self.stream(request, session: session) { line in
                guard let event = HermesEvent.parse(line) else { return true }
                if case .tool = event { self.acting = true }
                if case .tool(let name, _) = event, name == "cronjob_manage" || name == "iphone_alerts" {
                    // Their answers come as notifications, also with AXYON closed: iOS asks now, with the user here.
                    Task { _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) }
                }
                if case .toolDone = event { self.acting = false }
                self.conversation.apply(event, to: answer, label: Self.label)
                switch event {
                case .done, .error: return false
                default: return true
                }
            }
        } catch is CancellationError {
        } catch {
            conversation.interrupt(answer, reason: Self.describe(error))
        }
        conversation.interrupt(answer, reason: String(localized: "The answer stopped there: AXYON closed before it ended."))
        acting = false
        answering = nil
        runID = nil
        save()
        endBackgroundTime()
        if let entry = conversation.entries.last(where: { $0.id == answer }), !entry.failed,
           UIApplication.shared.applicationState != .active {
            HermesArms.notifyAnswer(entry.text)
        }
    }

    func stop() {
        guard let runID else {
            runTask?.cancel()
            return
        }
        Task { _ = try? await call("POST", "/v1/runs/\(runID)/stop") }
    }

    func newChat() {
        guard answering == nil else { return }
        conversation = .fresh()
        save()
    }

    private func save() {
        HermesChatStore.save(conversation)
    }

    /// A tool's name as the steps show it.
    nonisolated static func label(_ tool: String) -> String {
        switch tool {
        case "coucou_portfolio", "wealth_portfolio": String(localized: "Reading the portfolio")
        case "coucou_quote": String(localized: "Reading prices")
        case "coucou_web_search": String(localized: "Searching the web")
        case "coucou_web_read": String(localized: "Reading a page")
        case "coucou_home", "home_devices": String(localized: "Looking at the home")
        case "coucou_home_action", "home_control": String(localized: "Acting in the home")
        case "iphone_notify": String(localized: "Scheduling a notification")
        case "iphone_reminders": String(localized: "Reminders")
        case "iphone_calendar": String(localized: "Calendar")
        case "iphone_contacts": String(localized: "Contacts")
        case "iphone_open": String(localized: "Opening on the iPhone")
        case "iphone_shortcut": String(localized: "Running a shortcut")
        case "iphone_clipboard": String(localized: "Clipboard")
        case "iphone_status": String(localized: "Looking at the iPhone")
        case "iphone_location": String(localized: "Location")
        case "iphone_torch": String(localized: "Flashlight")
        case "iphone_music": String(localized: "Music")
        case "iphone_speak": String(localized: "Speaking")
        case "memory": String(localized: "Memory")
        case "skills_list", "skill_view", "skill_manage": String(localized: "Skills")
        case "session_search": String(localized: "Searching past chats")
        case "todo_list": String(localized: "To-do list")
        case "vision_analyze": String(localized: "Looking at the image")
        case "clarify": String(localized: "A question for you")
        case "iphone_files": String(localized: "Reading your file")
        case "iphone_alerts": String(localized: "Portfolio alerts")
        case "google_account": String(localized: "Google account")
        case "gmail": "Gmail"
        case "google_sheets": String(localized: "Google Sheets")
        case "google_drive": "Google Drive"
        case "google_calendar": String(localized: "Google Calendar")
        case "cronjob_manage": String(localized: "Scheduled tasks")
        case "delegate_task": String(localized: "A helper at work")
        case "image_generate": String(localized: "Making an image")
        case "wealth_committee": String(localized: "The committee is meeting")
        case "wealth_documents": String(localized: "Reading your assets' documents")
        case "wealth_learn": String(localized: "Learning with your portfolio")
        default:
            tool.hasPrefix("wealth_") ? String(localized: "Superpowers: money")
                : tool.hasPrefix("life_") ? String(localized: "Superpowers: your day") : tool
        }
    }

    // MARK: The brain

    func refreshSettings() async {
        guard state == .ready, let object = try? await call("GET", "/v1/settings") else { return }
        settings = HermesSettings(json: object)
    }

    /// The brain Hermes thinks with: its key goes into this iPhone's Keychain and to Hermes in memory only, then
    /// Hermes' config takes the brain and the model.
    @discardableResult
    func setBrain(_ brain: HermesBrain, model: String, address: String, key: String?) async -> Bool {
        var body: [String: Any] = ["provider": brain.id,
                                   "model": model.trimmingCharacters(in: .whitespacesAndNewlines),
                                   "baseUrl": address.trimmingCharacters(in: .whitespacesAndNewlines)]
        if let name = brain.keyName, let key = key?.trimmingCharacters(in: .whitespacesAndNewlines), !key.isEmpty {
            HermesKeychain.set(name, key)
            body["keys"] = [name: key]
        }
        if brain.needsAddress { UserDefaults.standard.set(address, forKey: Self.addressKey) }
        do {
            let object = try await call("POST", "/v1/settings", body)
            settings = HermesSettings(json: object)
            lastError = nil
            return true
        } catch {
            lastError = Self.describe(error)
            return false
        }
    }

    /// Forgets a brain's key, here and in Hermes.
    func forgetKey(of brain: HermesBrain) async {
        guard let name = brain.keyName else { return }
        HermesKeychain.set(name, nil)
        _ = try? await call("POST", "/v1/settings", ["keys": [name: ""]])
        await refreshSettings()
    }

    /// The keys kept in the Keychain, to Hermes after each start (it keeps none on disk), with the address of an
    /// OpenAI-compatible brain the key belongs to.
    private func pushKeys() async {
        var keys: [String: String] = [:]
        for name in HermesBrain.keyNames {
            if let value = HermesKeychain.get(name) { keys[name] = value }
        }
        var body: [String: Any] = [:]
        if !keys.isEmpty { body["keys"] = keys }
        if let address = UserDefaults.standard.string(forKey: Self.addressKey), !address.isEmpty {
            body["openaiBaseUrl"] = address
        }
        guard !body.isEmpty else { return }
        _ = try? await call("POST", "/v1/settings", body)
    }

    private static let addressKey = "hermesCustomAddress"

    // MARK: ChatGPT

    func startChatGPTSignIn() async {
        lastError = nil
        do {
            let object = try await call("POST", "/v1/auth/chatgpt/start", [:], timeout: 45)
            signIn = HermesSignIn(json: object)
            followSignIn()
        } catch {
            signIn = HermesSignIn(state: .failed, message: Self.describe(error))
        }
    }

    private func followSignIn() {
        signInTask?.cancel()
        signInTask = Task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                guard let object = try? await call("GET", "/v1/auth/chatgpt") else { continue }
                let now = HermesSignIn(json: object)
                signIn = now
                if now.state == .done {
                    await refreshSettings()
                    Haptics.success()
                    return
                }
                if now.state != .waiting { return }
            }
        }
    }

    // MARK: An e-mail waiting for the user's tap

    /// Shows the e-mail in the chat and waits for the user's tap: Send (true) or Cancel; nothing after four minutes,
    /// or when another one comes.
    func askToSend(_ ask: HermesMailAsk) async -> Bool {
        mailWaiting?.resume(returning: false)
        mailWaiting = nil
        mailAsk = ask
        let timeout = Task {
            try? await Task.sleep(for: .seconds(240))
            if !Task.isCancelled, mailAsk?.id == ask.id { answerMail(false) }
        }
        defer { timeout.cancel() }
        return await withCheckedContinuation { mailWaiting = $0 }
    }

    func answerMail(_ send: Bool) {
        mailAsk = nil
        mailWaiting?.resume(returning: send)
        mailWaiting = nil
    }

    // MARK: Google

    func refreshGoogle() async {
        guard state == .ready, let object = try? await call("GET", "/v1/google") else { return }
        google = HermesGoogle(json: object)
    }

    /// The user's OAuth client: the JSON they picked (copied into the inbox), or its ID and secret.
    func googleClient(path: String?, id: String, secret: String) async throws {
        var body: [String: Any] = ["clientId": id, "clientSecret": secret]
        if let path { body["path"] = path }
        try await call("POST", "/v1/google/client", body)
        await refreshGoogle()
    }

    /// Google's sign-in page, to open inside the app: Google sends the browser back to Hermes, which finishes it.
    func googleConnect() async throws -> URL {
        let object = try await call("POST", "/v1/google/connect", [:])
        guard let link = object["url"] as? String, let url = URL(string: link) else { throw HermesError.broken }
        return url
    }

    func googleFinish(_ pasted: String) async throws {
        try await call("POST", "/v1/google/finish", ["pasted": pasted])
        await refreshGoogle()
    }

    func googleDisconnect() async {
        _ = try? await call("POST", "/v1/google/disconnect", [:])
        await refreshGoogle()
    }

    // MARK: Scheduled tasks

    func cronTasks() async -> [HermesTask] {
        guard state == .ready, let object = try? await call("GET", "/v1/cron") else { return [] }
        return HermesTask.list(object)
    }

    /// Hermes' due tasks run now (when iOS woke AXYON): how many ran.
    func cronTick(timeout: TimeInterval) async -> Int {
        guard state == .ready, let object = try? await call("POST", "/v1/cron/tick", [:], timeout: timeout) else { return 0 }
        return object["ran"] as? Int ?? 0
    }

    // MARK: The portfolio

    func refreshPortfolio() async {
        guard state == .ready else { return }
        portfolioLoading = true
        defer { portfolioLoading = false }
        portfolio = try? await call("GET", "/v1/portfolio", timeout: 60)
    }

    func portfolioChanged() {
        portfolioDate = HermesPortfolio.receivedAt()
        if state == .ready { Task { await refreshPortfolio() } }
    }

    // MARK: Investing (AXYONInvest.swift)

    /// The committee's meeting on a proposal (axyon_hermes/invest.py): the engine's brief, the brain's five minds, the
    /// verdict the engine keeps. One long answer of the brain: up to five minutes.
    func investCommittee(_ body: [String: Any]) async throws -> [String: Any] {
        guard state == .ready else { throw HermesError.refused(String(localized: "AXYON Core is not running yet.")) }
        return try await call("POST", "/v1/invest/committee", body, timeout: 300)
    }

    /// The brain's digest of a document the reader read, kept with it.
    func investDigest(_ key: String, language: String) async throws -> [String: Any] {
        guard state == .ready else { throw HermesError.refused(String(localized: "AXYON Core is not running yet.")) }
        return try await call("POST", "/v1/invest/digest", ["key": key, "language": language], timeout: 240)
    }

    // MARK: The iPhone's own home

    func refreshHouse(quick: Bool = false) async {
        guard state == .ready else { return }
        houseLoading = true
        defer { houseLoading = false }
        if let object = try? await call("GET", quick ? "/v1/home?quick=1" : "/v1/home", timeout: 30) {
            house = HermesHouse(json: object)
        }
    }

    /// An action from the Home tab: the owner's own tap. nil once done, else why not, in words.
    func houseAction(_ device: HermesDevice, _ action: String, value: Any? = nil) async -> String? {
        var body: [String: Any] = ["action": action]
        if let value { body["value"] = value }
        do {
            let object = try await call("POST", "/v1/home/devices/\(Self.segment(device.id))/action", body, timeout: 40)
            if object["ok"] as? Bool == false {
                return object["message"] as? String ?? object["error"] as? String ?? String(localized: "Not done.")
            }
            return nil
        } catch {
            return Self.describe(error)
        }
    }

    /// Adds a TV or a box at `host`, then pairs it: the device asks on its own screen (up to a minute).
    func addDevice(driver: String, host: String, name: String, room: String) async throws -> String {
        let added = try await call("POST", "/v1/home/devices",
                                   ["driver": driver, "host": host, "name": name, "room": room], timeout: 30)
        guard let id = added["id"] as? String else { throw HermesError.broken }
        await refreshHouse(quick: true)
        let paired = try await call("POST", "/v1/home/devices/\(Self.segment(id))/pair", [:], timeout: 100)
        await refreshHouse(quick: true)
        return paired["message"] as? String ?? ""
    }

    func pairDevice(_ id: String) async throws -> String {
        let paired = try await call("POST", "/v1/home/devices/\(Self.segment(id))/pair", [:], timeout: 100)
        await refreshHouse(quick: true)
        return paired["message"] as? String ?? ""
    }

    func removeDevice(_ id: String) async {
        _ = try? await call("POST", "/v1/home/devices/\(Self.segment(id))/remove", [:])
        await refreshHouse(quick: true)
    }

    /// The TVs, boxes and Home Assistants on this iPhone's Wi-Fi.
    func scanHouse() async throws -> (found: [HermesFound], problem: String?) {
        guard let (address, prefix) = Self.wifiAddress() else {
            return ([], String(localized: "Connect the iPhone to your home's Wi-Fi to look for devices."))
        }
        let object = try await call("POST", "/v1/home/scan", ["address": address, "prefix": prefix], timeout: 60)
        return (HermesFound.list(object), object["error"] as? String)
    }

    /// The user's Home Assistant: its token into the Keychain first (Hermes reads it from there), then its address.
    @discardableResult
    func setAssistant(url: String, token: String) async throws -> Int {
        let token = token.trimmingCharacters(in: .whitespacesAndNewlines)
        if !token.isEmpty { HermesKeychain.set("home.ha.token", token, service: HermesKeychain.homeService) }
        let object = try await call("POST", "/v1/home/assistant", ["url": url], timeout: 30)
        await refreshHouse()
        return object["entities"] as? Int ?? 0
    }

    func removeAssistant() async {
        _ = try? await call("POST", "/v1/home/assistant/remove", [:])
        await refreshHouse(quick: true)
    }

    nonisolated private static func segment(_ id: String) -> String {
        var allowed = CharacterSet.urlPathAllowed
        allowed.remove(charactersIn: "/?#")
        return id.addingPercentEncoding(withAllowedCharacters: allowed) ?? id
    }

    /// This iPhone's address and prefix on its Wi-Fi (en0): a scan looks there, never on the cellular network.
    nonisolated static func wifiAddress() -> (String, Int)? {
        var list: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&list) == 0, let first = list else { return nil }
        defer { freeifaddrs(list) }
        for pointer in sequence(first: first, next: { $0.pointee.ifa_next }) {
            let entry = pointer.pointee
            guard String(cString: entry.ifa_name) == "en0", let address = entry.ifa_addr, let mask = entry.ifa_netmask,
                  address.pointee.sa_family == UInt8(AF_INET) else { continue }
            let size = Int(NI_MAXHOST)
            var host = [CChar](repeating: 0, count: size)
            guard getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(size), nil, 0,
                              NI_NUMERICHOST) == 0 else { continue }
            let bits = mask.withMemoryRebound(to: sockaddr_in.self, capacity: 1) {
                UInt32(bigEndian: $0.pointee.sin_addr.s_addr).nonzeroBitCount
            }
            return (String(decoding: host.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self), bits)
        }
        return nil
    }

    // MARK: The bridge: what Hermes asks of the iPhone

    private func listenToBridge() {
        bridgeTask?.cancel()
        bridgeTask = Task {
            while !Task.isCancelled {
                do {
                    var request = URLRequest(url: url("/v1/bridge"), timeoutInterval: 120)
                    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
                    try await Self.stream(request, session: session) { line in
                        guard line.hasPrefix("data: "),
                              let object = (try? JSONSerialization.jsonObject(with: Data(line.dropFirst(6).utf8))) as? [String: Any],
                              let id = object["id"] as? String, let kind = object["kind"] as? String else { return true }
                        let asked = (try? JSONSerialization.data(withJSONObject: object["request"] ?? [String: Any]()))
                            ?? Data("{}".utf8)
                        Task {
                            let answer = await HermesArms.answer(kind, asked)
                            var body = Data(#"{"answer":"#.utf8)
                            body.append(answer)
                            body.append(Data("}".utf8))
                            _ = try? await self.send(body, to: "/v1/bridge/\(id)")
                        }
                        return !Task.isCancelled
                    }
                } catch {}
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    // MARK: HTTP to the host

    private func url(_ path: String) -> URL {
        URL(string: "http://127.0.0.1:\(port)\(path)")!
    }

    @discardableResult
    private func call(_ method: String, _ path: String, _ body: [String: Any]? = nil,
                      timeout: TimeInterval = 30) async throws -> [String: Any] {
        var request = URLRequest(url: url(path), timeoutInterval: timeout)
        request.httpMethod = method
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, status) = try await Self.exchange(request, session: session)
        let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard (200..<300).contains(status) else {
            throw HermesError.refused(object["error"] as? String ?? "HTTP \(status)")
        }
        return object
    }

    /// A body already written as JSON (the bridge's answers).
    private func send(_ body: Data, to path: String) async throws {
        var request = URLRequest(url: url(path), timeoutInterval: 30)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = body
        _ = try await Self.exchange(request, session: session)
    }

    // The network is read off the main actor; only what is Sendable (bytes, a status, each line) comes back to it.

    nonisolated private static func exchange(_ request: URLRequest, session: URLSession) async throws -> (Data, Int) {
        let (data, response) = try await session.data(for: request)
        return (data, (response as? HTTPURLResponse)?.statusCode ?? 0)
    }

    /// The lines of a server-sent stream, each to `each` on the main actor, until it says to stop or the stream ends.
    nonisolated private static func stream(_ request: URLRequest, session: URLSession,
                                           each: @escaping @MainActor @Sendable (String) -> Bool) async throws {
        let (bytes, response) = try await session.bytes(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else { throw HermesError.refused("HTTP \(status)") }
        for try await line in bytes.lines {
            guard await each(line) else { return }
        }
    }

    nonisolated static func describe(_ error: Error) -> String {
        if case HermesError.refused(let message) = error { return message }
        if error is URLError { return String(localized: "Hermes did not answer inside the app.") }
        return error.localizedDescription
    }

    // MARK: Where things are

    /// Application Support/Hermes: what Hermes keeps (config, sign-ins, memories, skills, sessions).
    nonisolated static var home: URL {
        let base = (try? FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                 appropriateFor: nil, create: true))
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("Hermes", isDirectory: true)
    }

    nonisolated private static func randomToken() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// A port nobody uses on 127.0.0.1, for the host to listen on.
    nonisolated private static func freePort() -> UInt16? {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let bound = withUnsafeMutablePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, length) }
        }
        guard bound == 0 else { return nil }
        let named = withUnsafeMutablePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(fd, $0, &length) }
        }
        guard named == 0 else { return nil }
        return UInt16(bigEndian: address.sin_port)
    }
}

enum HermesError: Error {
    case broken
    case refused(String)
}

/// The brains' keys, in this iPhone's Keychain only (not in backups, not in iCloud Keychain). The home's tokens and
/// key (`home.`…, home.py) are in a service of their own, the only one the bridge reaches.
enum HermesKeychain {
    static let service = "fr.louisraille.Coucou.hermes"
    static let homeService = "fr.louisraille.Coucou.hermes.home"

    static func get(_ name: String, service: String = HermesKeychain.service) -> String? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service, kSecAttrAccount as String: name,
                                    kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    static func set(_ name: String, _ value: String?, service: String = HermesKeychain.service) {
        let lookup: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                     kSecAttrService as String: service, kSecAttrAccount as String: name]
        SecItemDelete(lookup as CFDictionary)
        guard let value, let data = value.data(using: .utf8) else { return }
        var item = lookup
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(item as CFDictionary, nil)
    }
}

/// The conversation with Hermes as this iPhone keeps it between launches (Application Support/hermes-chat.json): the
/// messages and steps as shown, and the Hermes session they belong to. Hermes keeps its own history in its sessions.
enum HermesChatStore {
    private static var fileURL: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("hermes-chat.json")
    }

    static func load() -> HermesConversation? {
        guard let data = try? Data(contentsOf: fileURL),
              let saved = try? JSONDecoder().decode(HermesConversation.self, from: data),
              saved.version <= HermesConversation.currentVersion else { return nil }
        return saved
    }

    static func save(_ conversation: HermesConversation) {
        guard let data = try? JSONEncoder().encode(conversation) else { return }
        try? data.write(to: fileURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
}

/// The portfolio the paired computer gives (PCIncoming.portfolio), kept for Hermes on this iPhone in Application
/// Support/AXYON/portfolio.json, with when it came. Forgotten when the computer stops giving it.
enum HermesPortfolio {
    static var fileURL: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/portfolio.json")
    }

    @MainActor
    static func keep(_ line: Data?) {
        let file = fileURL
        guard let line else {
            try? FileManager.default.removeItem(at: file)
            HermesEngine.shared.portfolioChanged()
            return
        }
        guard var object = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else { return }
        object["receivedAt"] = Date.now.timeIntervalSince1970
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
        try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        HermesEngine.shared.portfolioChanged()
    }

    static func receivedAt() -> Date? {
        guard let data = try? Data(contentsOf: fileURL),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let at = object["receivedAt"] as? Double else { return nil }
        return Date(timeIntervalSince1970: at)
    }
}
