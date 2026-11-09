import AppIntents
import CoreLocation
import MapKit
import SwiftUI
import UIKit
import UserNotifications

// AXYON's routines on the iPhone (Hermes/app/axyon_hermes/automations.py keeps and runs them): a name, what starts it
// (a tap, Siri and the Shortcuts app through AXYONIntents.swift, a time of day, arriving at or leaving a place, which
// iOS' own region monitoring watches with AXYON closed) and its steps. The Home tab shows them first, as cards, with
// ideas to start from; the editor sets everything, the map included.

// MARK: - What a routine is

struct AXYONTrigger: Equatable, Hashable {
    /// manual, time, arrive or leave.
    var kind = "manual"
    var at = "07:00"
    /// 1 = Monday … 7 = Sunday; empty: every day.
    var days: [Int] = []
    var placeName = ""
    var latitude: Double?
    var longitude: Double?
    var radius: Double = 150

    init(kind: String = "manual") { self.kind = kind }

    init(json o: [String: Any]) {
        kind = o["kind"] as? String ?? "manual"
        at = o["at"] as? String ?? "07:00"
        days = (o["days"] as? [Any] ?? []).compactMap { ($0 as? NSNumber)?.intValue }
        if let place = o["place"] as? [String: Any] {
            placeName = place["name"] as? String ?? ""
            latitude = (place["lat"] as? NSNumber)?.doubleValue
            longitude = (place["lon"] as? NSNumber)?.doubleValue
            radius = (place["radius"] as? NSNumber)?.doubleValue ?? 150
        }
    }

    var wire: [String: Any] {
        switch kind {
        case "time": return ["kind": "time", "at": at, "days": days]
        case "arrive", "leave":
            return ["kind": kind, "place": ["name": placeName.isEmpty ? String(localized: "Home") : placeName,
                                            "lat": latitude ?? 0, "lon": longitude ?? 0, "radius": radius]]
        default: return ["kind": "manual"]
        }
    }

    var isPlace: Bool { kind == "arrive" || kind == "leave" }
    var hasPlace: Bool { latitude != nil && longitude != nil }
    var coordinate: CLLocationCoordinate2D? {
        guard let latitude, let longitude else { return nil }
        return CLLocationCoordinate2D(latitude: latitude, longitude: longitude)
    }

    var icon: String {
        switch kind {
        case "time": "clock.fill"
        case "arrive": "location.fill"
        case "leave": "figure.walk.departure"
        default: "hand.tap.fill"
        }
    }

    var summary: String {
        let place = placeName.isEmpty ? String(localized: "Home") : placeName
        switch kind {
        case "time": return String(localized: "At \(at) · \(AXYONTrigger.daysText(days))")
        case "arrive": return String(localized: "On arriving at \(place)")
        case "leave": return String(localized: "On leaving \(place)")
        default: return String(localized: "A tap, Siri or Shortcuts")
        }
    }

    static let weekdays: [String] = [String(localized: "Mon"), String(localized: "Tue"), String(localized: "Wed"),
                                     String(localized: "Thu"), String(localized: "Fri"), String(localized: "Sat"),
                                     String(localized: "Sun")]

    static func daysText(_ days: [Int]) -> String {
        let set = Set(days)
        if set.isEmpty || set.count == 7 { return String(localized: "every day") }
        if set == [1, 2, 3, 4, 5] { return String(localized: "weekdays") }
        if set == [6, 7] { return String(localized: "weekends") }
        return days.sorted().map { weekdays[max(0, min(6, $0 - 1))] }.joined(separator: ", ")
    }
}

struct AXYONStep: Identifiable, Equatable, Hashable {
    var id = UUID()
    /// device, every, iphone, pause or ask.
    var kind = "device"
    var device = ""
    var name = ""
    var action = "turn_on"
    var value = ""
    /// every: light, tv, fan or switch.
    var everyKind = "light"
    /// iphone: notify, reminder, speak, torch, music, open or shortcut.
    var phone = "notify"
    var title = ""
    var text = ""
    var input = ""
    var on = true
    var seconds = 5
    var minutes = 0

    init(kind: String, phone: String = "notify") {
        self.kind = kind
        self.phone = phone
        if kind == "every" { action = "turn_off" }
        if kind == "iphone", phone == "music" { value = "play" }
    }

    init(json o: [String: Any]) {
        kind = o["do"] as? String ?? "device"
        switch kind {
        case "device":
            device = o["device"] as? String ?? ""
            name = o["name"] as? String ?? device
            action = o["action"] as? String ?? "turn_on"
            if let v = o["value"] {
                value = (v as? NSNumber).map { $0.stringValue } ?? (v as? String ?? "")
            }
        case "every":
            everyKind = o["kind"] as? String ?? "light"
            action = o["action"] as? String ?? "turn_off"
        case "iphone":
            phone = o["action"] as? String ?? "notify"
            title = o["title"] as? String ?? ""
            switch phone {
            case "notify": text = o["body"] as? String ?? ""
            case "reminder":
                text = o["title"] as? String ?? ""
                minutes = (o["inMinutes"] as? NSNumber)?.intValue ?? 0
            case "open": text = o["url"] as? String ?? ""
            case "shortcut":
                text = o["name"] as? String ?? ""
                input = o["input"] as? String ?? ""
            case "torch": on = o["on"] as? Bool ?? true
            case "music": value = o["what"] as? String ?? "play"
            default: text = o["text"] as? String ?? ""
            }
        case "pause": seconds = (o["seconds"] as? NSNumber)?.intValue ?? 5
        default: text = o["prompt"] as? String ?? ""
        }
    }

    var wire: [String: Any] {
        switch kind {
        case "device":
            var out: [String: Any] = ["do": "device", "device": device, "action": action, "name": name]
            let v = value.trimmingCharacters(in: .whitespaces)
            if let number = Double(v.replacingOccurrences(of: ",", with: ".")) {
                out["value"] = number
            } else if !v.isEmpty {
                out["value"] = v
            }
            return out
        case "every": return ["do": "every", "kind": everyKind, "action": action]
        case "pause": return ["do": "pause", "seconds": seconds]
        case "ask": return ["do": "ask", "prompt": text]
        default:
            switch phone {
            case "notify": return ["do": "iphone", "action": "notify", "title": title, "body": text]
            case "reminder":
                var out: [String: Any] = ["do": "iphone", "action": "reminder", "title": text]
                if minutes > 0 { out["inMinutes"] = minutes }
                return out
            case "torch": return ["do": "iphone", "action": "torch", "on": on]
            case "music": return ["do": "iphone", "action": "music", "what": value.isEmpty ? "play" : value]
            case "open": return ["do": "iphone", "action": "open", "url": text]
            case "shortcut": return ["do": "iphone", "action": "shortcut", "name": text, "input": input]
            default: return ["do": "iphone", "action": "speak", "text": text]
            }
        }
    }

    /// Ready to be kept: what each kind needs is there.
    var complete: Bool {
        switch kind {
        case "device": return !device.isEmpty && !action.isEmpty
        case "every", "pause": return true
        case "iphone": return phone == "torch" || phone == "music" || !text.trimmingCharacters(in: .whitespaces).isEmpty
        default: return !text.trimmingCharacters(in: .whitespaces).isEmpty
        }
    }

    var icon: String {
        switch kind {
        case "device": "house.fill"
        case "every": AXYONStep.everyIcon(everyKind)
        case "pause": "hourglass"
        case "ask": "sparkles"
        default:
            switch phone {
            case "notify": "bell.badge.fill"
            case "reminder": "checklist"
            case "speak": "speaker.wave.2.fill"
            case "torch": "flashlight.on.fill"
            case "music": "music.note"
            case "open": "arrow.up.forward.app.fill"
            default: "square.stack.3d.up.fill"
            }
        }
    }

    var summary: String {
        switch kind {
        case "device":
            let what = AXYONStep.actionTitle(action)
            let shown = name.isEmpty ? device : name
            return value.isEmpty ? "\(what) · \(shown)" : "\(what) \(AXYONStep.valueText(action, value)) · \(shown)"
        case "every": return "\(AXYONStep.everyTitle(everyKind)): \(AXYONStep.actionTitle(action).lowercased())"
        case "pause": return String(localized: "Wait \(seconds) s")
        case "ask": return String(localized: "Ask AXYON: \(text)")
        default:
            switch phone {
            case "notify": return String(localized: "Notification: \(text)")
            case "reminder":
                return minutes > 0 ? String(localized: "Reminder in \(minutes) min: \(text)") : String(localized: "Reminder: \(text)")
            case "speak": return String(localized: "Say: \(text)")
            case "torch": return on ? String(localized: "Flashlight on") : String(localized: "Flashlight off")
            case "music": return String(localized: "Music: \(AXYONStep.musicTitle(value))")
            case "open": return String(localized: "Open \(text)")
            default: return String(localized: "Shortcut: \(text)")
            }
        }
    }

    static func actionTitle(_ action: String) -> String {
        switch action {
        case "turn_on": String(localized: "Turn on")
        case "turn_off": String(localized: "Turn off")
        case "toggle": String(localized: "Switch")
        case "set_brightness": String(localized: "Brightness")
        case "set_color": String(localized: "Colour")
        case "set_volume": String(localized: "Volume")
        case "volume_up": String(localized: "Volume +")
        case "volume_down": String(localized: "Volume −")
        case "mute": String(localized: "Mute")
        case "unmute": String(localized: "Unmute")
        case "channel_up": String(localized: "Channel +")
        case "channel_down": String(localized: "Channel −")
        case "open_app": String(localized: "Open an app")
        case "key": String(localized: "Press a key")
        case "play_pause": String(localized: "Play / pause")
        case "next": String(localized: "Next")
        case "previous": String(localized: "Previous")
        case "open": String(localized: "Open")
        case "close": String(localized: "Close")
        case "stop": String(localized: "Stop")
        case "run": String(localized: "Run")
        case "preset": String(localized: "Preset")
        case "set_temperature": String(localized: "Temperature")
        default: action
        }
    }

    static func valueText(_ action: String, _ value: String) -> String {
        switch action {
        case "set_brightness", "set_volume": "\(value)%"
        case "set_temperature": "\(value)°"
        case "volume_up", "volume_down", "channel_up", "channel_down": "×\(value)"
        default: value
        }
    }

    static func everyTitle(_ kind: String) -> String {
        switch kind {
        case "tv": String(localized: "All the TVs")
        case "fan": String(localized: "All the fans")
        case "switch": String(localized: "All the plugs")
        default: String(localized: "All the lights")
        }
    }

    static func everyIcon(_ kind: String) -> String {
        switch kind {
        case "tv": "tv.fill"
        case "fan": "fan.fill"
        case "switch": "poweroutlet.type.b.fill"
        default: "lightbulb.2.fill"
        }
    }

    static func musicTitle(_ value: String) -> String {
        switch value {
        case "pause": String(localized: "pause")
        case "next": String(localized: "next")
        case "previous": String(localized: "previous")
        default: String(localized: "play")
        }
    }
}

struct AXYONRoutine: Identifiable, Equatable, Hashable {
    /// Empty: not kept yet.
    var id = ""
    var name = ""
    var icon = "sparkles"
    var enabled = true
    var trigger = AXYONTrigger()
    var steps: [AXYONStep] = []
    /// app or axyon (made by AXYON in the chat, with the owner's Face ID).
    var by = "app"
    var lastRun: String?
    var lastOK: Bool?

    init(name: String = "", icon: String = "sparkles", trigger: AXYONTrigger = AXYONTrigger(), steps: [AXYONStep] = []) {
        self.name = name
        self.icon = icon
        self.trigger = trigger
        self.steps = steps
    }

    init(json o: [String: Any]) {
        id = o["id"] as? String ?? ""
        name = o["name"] as? String ?? ""
        icon = o["icon"] as? String ?? "sparkles"
        enabled = o["enabled"] as? Bool ?? true
        trigger = AXYONTrigger(json: o["trigger"] as? [String: Any] ?? [:])
        steps = (o["steps"] as? [[String: Any]] ?? []).map(AXYONStep.init(json:))
        by = o["by"] as? String ?? "app"
        if let last = o["lastRun"] as? [String: Any] {
            lastRun = last["at"] as? String
            lastOK = last["ok"] as? Bool
        }
    }

    var wire: [String: Any] {
        var out: [String: Any] = ["name": name, "icon": icon, "enabled": enabled, "trigger": trigger.wire,
                                  "steps": steps.map(\.wire)]
        if !id.isEmpty { out["id"] = id }
        return out
    }

    static let icons = ["sparkles", "house.fill", "figure.walk.departure", "sun.max.fill", "moon.stars.fill",
                        "film.fill", "bed.double.fill", "brain.head.profile", "briefcase.fill", "car.fill",
                        "dumbbell.fill", "fork.knife", "cup.and.saucer.fill", "gamecontroller.fill", "book.fill",
                        "music.note", "party.popper.fill", "leaf.fill", "drop.fill", "pawprint.fill"]
}

/// What a run did, as the Home tab and Siri say it.
struct AXYONRoutineOutcome: Identifiable, Equatable {
    let id = UUID()
    let name: String
    let total: Int
    let done: Int
    let running: Bool
    let failures: [String]

    init(name: String, json o: [String: Any]) {
        self.name = name
        let steps = o["steps"] as? [[String: Any]] ?? []
        total = steps.count
        done = steps.filter { $0["ok"] as? Bool == true }.count
        running = o["running"] as? Bool ?? false
        failures = steps.filter { $0["ok"] as? Bool != true }.map { step in
            let what = step["what"] as? String ?? ""
            let said = step["said"] as? String ?? ""
            return what.isEmpty ? said : "\(what): \(said)"
        }
    }

    var ok: Bool { !running && failures.isEmpty }

    var sentence: String {
        if running { return String(localized: "\(name) is running: \(done) steps done so far.") }
        if failures.isEmpty { return String(localized: "\(name): done, \(total) steps.") }
        return String(localized: "\(name): \(done) of \(total) steps. \(failures.first ?? "")")
    }
}

// MARK: - The routines kept

/// A routine as Siri, the Shortcuts app and the places read it with AXYON closed (UserDefaults).
struct AXYONRoutineMirror: Codable, Sendable, Equatable {
    var id: String
    var name: String
    var icon: String
    var kind: String
    var at: String
    var days: [Int]
    var enabled: Bool
}

@MainActor
@Observable
final class AXYONRoutineStore {
    static let shared = AXYONRoutineStore()

    private(set) var routines: [AXYONRoutine] = []
    private(set) var loaded = false
    private(set) var problem: String?
    private(set) var running: Set<String> = []
    var outcome: AXYONRoutineOutcome?

    func load() async {
        let engine = HermesEngine.shared
        guard engine.hostUp else { return }
        do {
            let object = try await engine.routines()
            routines = (object["routines"] as? [[String: Any]] ?? []).map(AXYONRoutine.init(json:))
            loaded = true
            problem = nil
            Self.mirror(routines)
            AXYONPlaces.shared.sync(routines)
            AXYONShortcuts.updateAppShortcutParameters()
        } catch {
            problem = HermesEngine.describe(error)
        }
    }

    @discardableResult
    func save(_ routine: AXYONRoutine) async throws -> AXYONRoutine {
        let object = try await HermesEngine.shared.saveRoutine(routine.wire)
        let saved = AXYONRoutine(json: object)
        await load()
        if saved.trigger.isPlace { AXYONPlaces.shared.askAlways() }
        return saved
    }

    func remove(_ routine: AXYONRoutine) async {
        try? await HermesEngine.shared.removeRoutine(routine.id)
        await load()
    }

    func setEnabled(_ routine: AXYONRoutine, _ on: Bool) async {
        var changed = routine
        changed.enabled = on
        _ = try? await save(changed)
    }

    func run(_ routine: AXYONRoutine) async {
        guard !running.contains(routine.id) else { return }
        running.insert(routine.id)
        defer { running.remove(routine.id) }
        do {
            let object = try await HermesEngine.shared.runRoutine(routine.id, origin: "app", wait: 60)
            let outcome = AXYONRoutineOutcome(name: routine.name, json: object)
            show(outcome)
            if outcome.ok { Haptics.success() } else { Haptics.error() }
        } catch {
            let failed: [[String: Any]] = [["ok": false, "said": HermesEngine.describe(error)]]
            show(AXYONRoutineOutcome(name: routine.name, json: ["steps": failed]))
            Haptics.error()
        }
        await load()
    }

    /// The run's outcome above the routines, for eight seconds.
    private func show(_ shown: AXYONRoutineOutcome) {
        withAnimation(.spring(duration: 0.4)) { outcome = shown }
        Task {
            try? await Task.sleep(for: .seconds(8))
            if outcome?.id == shown.id {
                withAnimation(.spring(duration: 0.3)) { outcome = nil }
            }
        }
    }

    // MARK: Away from the screen (Siri, a place, iOS' background refresh)

    /// A routine run with AXYON maybe closed: the host started if needed, then the routine, all within `budget`
    /// seconds (what iOS grants); what it did, in a sentence.
    static func runAway(_ id: String, origin: String, budget: TimeInterval) async -> String {
        let engine = HermesEngine.shared
        let started = Date.now
        let name = mirrored.first { $0.id == id }?.name ?? String(localized: "The routine")
        guard await engine.waitForHost(budget: max(4, budget - 6)) else {
            return String(localized: "AXYON could not start in time: open it and run \(name) again.")
        }
        let left = max(3, budget - Date.now.timeIntervalSince(started) - 3)
        do {
            let object = try await engine.runRoutine(id, origin: origin, wait: left)
            return AXYONRoutineOutcome(name: name, json: object).sentence
        } catch {
            return HermesEngine.describe(error)
        }
    }

    /// The time routines that came due, when iOS wakes AXYON in the background.
    static func backgroundTick(budget: TimeInterval) async {
        let engine = HermesEngine.shared
        let started = Date.now
        guard await engine.waitForHost(budget: max(3, budget - 4)) else { return }
        _ = await engine.routinesTick(wait: max(2, budget - Date.now.timeIntervalSince(started) - 1))
    }

    // MARK: A copy for Siri, the Shortcuts app and the places, read with AXYON closed

    nonisolated static let mirrorKey = "axyon.routines.mirror"

    static func mirror(_ routines: [AXYONRoutine]) {
        let list = routines.map { AXYONRoutineMirror(id: $0.id, name: $0.name, icon: $0.icon, kind: $0.trigger.kind,
                                           at: $0.trigger.at, days: $0.trigger.days, enabled: $0.enabled) }
        if let data = try? JSONEncoder().encode(list) { UserDefaults.standard.set(data, forKey: mirrorKey) }
    }

    nonisolated static var mirrored: [AXYONRoutineMirror] {
        guard let data = UserDefaults.standard.data(forKey: mirrorKey),
              let list = try? JSONDecoder().decode([AXYONRoutineMirror].self, from: data) else { return [] }
        return list
    }

    /// When the time routines run next (for iOS' background refresh).
    nonisolated static func nextTimes(from now: Date = .now) -> [Date] {
        let calendar = Calendar.current
        return mirrored.filter { $0.enabled && $0.kind == "time" }.compactMap { (r: AXYONRoutineMirror) -> Date? in
            let parts = r.at.split(separator: ":").compactMap { Int($0) }
            guard parts.count == 2 else { return nil }
            for offset in 0..<8 {
                guard let day = calendar.date(byAdding: .day, value: offset, to: now),
                      let at = calendar.date(bySettingHour: parts[0], minute: parts[1], second: 0, of: day) else { continue }
                let weekday = (calendar.component(.weekday, from: at) + 5) % 7 + 1  // Monday = 1
                if at > now, r.days.isEmpty || r.days.contains(weekday) { return at }
            }
            return nil
        }
    }

    /// A time routine came due in the last two hours (the host runs each once).
    nonisolated static func due(at now: Date = .now) -> Bool {
        let calendar = Calendar.current
        return mirrored.contains { r in
            guard r.enabled, r.kind == "time" else { return false }
            let parts = r.at.split(separator: ":").compactMap { Int($0) }
            guard parts.count == 2, let at = calendar.date(bySettingHour: parts[0], minute: parts[1], second: 0, of: now)
            else { return false }
            let weekday = (calendar.component(.weekday, from: at) + 5) % 7 + 1
            return at <= now && now.timeIntervalSince(at) < 2 * 3600 && (r.days.isEmpty || r.days.contains(weekday))
        }
    }
}

// MARK: - Ideas to start from

enum AXYONRoutineIdeas {
    static var all: [AXYONRoutine] {
        let morning = AXYONTrigger(kind: "time")
        var night = AXYONTrigger(kind: "time")
        night.at = "23:00"
        var weekdays = morning
        weekdays.days = [1, 2, 3, 4, 5]
        var lightsOn = AXYONStep(kind: "every")
        lightsOn.action = "turn_on"
        let lightsOff = AXYONStep(kind: "every")
        var tvsOff = AXYONStep(kind: "every")
        tvsOff.everyKind = "tv"
        var pause = AXYONStep(kind: "iphone", phone: "music")
        pause.value = "pause"
        return [
            AXYONRoutine(name: String(localized: "Home again"), icon: "house.fill", trigger: AXYONTrigger(kind: "arrive"),
                         steps: [lightsOn, note(String(localized: "Welcome home!"))]),
            AXYONRoutine(name: String(localized: "Leaving home"), icon: "figure.walk.departure",
                         trigger: AXYONTrigger(kind: "leave"),
                         steps: [lightsOff, tvsOff, note(String(localized: "Everything is off at home."))]),
            AXYONRoutine(name: String(localized: "Good morning"), icon: "sun.max.fill", trigger: weekdays,
                         steps: [ask(String(localized: "Good morning! My day in a few lines: today's agenda, the reminders due today, the weather where I am and how my portfolio opened."))]),
            AXYONRoutine(name: String(localized: "Good night"), icon: "moon.stars.fill", trigger: night,
                         steps: [lightsOff, tvsOff, ask(String(localized: "What do I have tomorrow? Appointments and reminders, briefly."))]),
            AXYONRoutine(name: String(localized: "Film time"), icon: "film.fill",
                         steps: [lightsOff, pause, note(String(localized: "Enjoy the film! 🍿"))]),
            AXYONRoutine(name: String(localized: "Focus"), icon: "brain.head.profile",
                         steps: [pause, reminder(String(localized: "Time for a break"), 50),
                                 note(String(localized: "50 minutes of focus. I will tell you when to stop."))]),
        ]
    }

    private static func note(_ text: String) -> AXYONStep {
        var step = AXYONStep(kind: "iphone", phone: "notify")
        step.text = text
        return step
    }

    private static func ask(_ text: String) -> AXYONStep {
        var step = AXYONStep(kind: "ask")
        step.text = text
        return step
    }

    private static func reminder(_ text: String, _ minutes: Int) -> AXYONStep {
        var step = AXYONStep(kind: "iphone", phone: "reminder")
        step.text = text
        step.minutes = minutes
        return step
    }
}

// MARK: - Places, watched by iOS with AXYON closed

/// Arriving at or leaving a routine's place: iOS' region monitoring (up to 20 regions an app) wakes AXYON, which runs
/// the routine in the half minute iOS grants and says what it did in a notification. Needs location "Always".
final class AXYONPlaces: NSObject, CLLocationManagerDelegate, @unchecked Sendable {
    /// Made at launch, on the main thread (CoucouPhoneApp), which then has the location manager's callbacks.
    static let shared = AXYONPlaces()
    private let manager = CLLocationManager()
    private static let prefix = "axyon-place-"

    /// At launch, so a region iOS woke AXYON for is heard.
    @MainActor
    func wake() {
        manager.delegate = self
    }

    @MainActor
    var always: Bool { manager.authorizationStatus == .authorizedAlways }

    @MainActor
    func askAlways() {
        guard manager.authorizationStatus != .authorizedAlways else { return }
        manager.requestAlwaysAuthorization()
    }

    @MainActor
    func sync(_ routines: [AXYONRoutine]) {
        guard CLLocationManager.isMonitoringAvailable(for: CLCircularRegion.self) else { return }
        let wanted = routines.filter { $0.enabled && $0.trigger.isPlace && $0.trigger.hasPlace }.prefix(18)
        let ids = Set(wanted.map { Self.prefix + $0.id })
        for region in manager.monitoredRegions where region.identifier.hasPrefix(Self.prefix) && !ids.contains(region.identifier) {
            manager.stopMonitoring(for: region)
        }
        guard manager.authorizationStatus == .authorizedAlways else { return }
        for routine in wanted {
            guard let center = routine.trigger.coordinate else { continue }
            let region = CLCircularRegion(center: center,
                                          radius: min(max(100, routine.trigger.radius), manager.maximumRegionMonitoringDistance),
                                          identifier: Self.prefix + routine.id)
            region.notifyOnEntry = routine.trigger.kind == "arrive"
            region.notifyOnExit = routine.trigger.kind == "leave"
            manager.startMonitoring(for: region)
        }
    }

    func locationManager(_ manager: CLLocationManager, didEnterRegion region: CLRegion) {
        let id = region.identifier
        Task { @MainActor in await AXYONPlaces.shared.fire(id, arriving: true) }
    }

    func locationManager(_ manager: CLLocationManager, didExitRegion region: CLRegion) {
        let id = region.identifier
        Task { @MainActor in await AXYONPlaces.shared.fire(id, arriving: false) }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        Task { @MainActor in AXYONPlaces.shared.sync(AXYONRoutineStore.shared.routines) }
    }

    @MainActor
    private func fire(_ identifier: String, arriving: Bool) async {
        guard identifier.hasPrefix(Self.prefix) else { return }
        let id = String(identifier.dropFirst(Self.prefix.count))
        guard let routine = AXYONRoutineStore.mirrored.first(where: { $0.id == id }), routine.enabled,
              routine.kind == (arriving ? "arrive" : "leave") else { return }
        let time = AXYONBackgroundTime("AXYON routine")
        defer { time.end() }
        let said = await AXYONRoutineStore.runAway(id, origin: "place", budget: 26)
        await AXYONRoutineNotice.post(title: routine.name, body: said)
    }
}

/// The time iOS grants an app to end what it does after leaving the screen (or being woken).
@MainActor
final class AXYONBackgroundTime {
    private var task: UIBackgroundTaskIdentifier = .invalid

    init(_ name: String) {
        task = UIApplication.shared.beginBackgroundTask(withName: name) { [weak self] in
            MainActor.assumeIsolated { self?.end() }
        }
    }

    func end() {
        guard task != .invalid else { return }
        UIApplication.shared.endBackgroundTask(task)
        task = .invalid
    }
}

enum AXYONRoutineNotice {
    static func post(title: String, body: String) async {
        let center = UNUserNotificationCenter.current()
        let content = UNMutableNotificationContent()
        content.title = String(title.prefix(80))
        content.body = String(body.prefix(400))
        content.sound = .default
        let request = UNNotificationRequest(identifier: "axyon-routine-" + UUID().uuidString, content: content,
                                            trigger: UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false))
        try? await center.add(request)
    }
}

// MARK: - The routines on the Home tab

struct AXYONRoutinesSection: View {
    private let store = AXYONRoutineStore.shared
    @State private var editing: Editing?

    struct Editing: Identifiable {
        let id = UUID()
        var routine: AXYONRoutine
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                AXYONHeader(title: String(localized: "Routines"))
                Button {
                    editing = Editing(routine: AXYONRoutine(steps: []))
                } label: {
                    Image(systemName: "plus")
                        .font(.callout.weight(.bold))
                        .foregroundStyle(.white)
                        .frame(width: 34, height: 34)
                        .coucouGlass(.regular, in: Circle())
                }
                .buttonStyle(PressableButtonStyle())
                .accessibilityLabel(Text("New routine"))
            }
            if let outcome = store.outcome {
                AXYONOutcomeBanner(outcome: outcome) {
                    withAnimation(.spring(duration: 0.3)) { store.outcome = nil }
                }
                .transition(.move(edge: .top).combined(with: .opacity))
            }
            if store.routines.isEmpty {
                intro
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 12) {
                        ForEach(store.routines) { routine in
                            AXYONRoutineCard(routine: routine, running: store.running.contains(routine.id)) {
                                Task { await store.run(routine) }
                            } edit: {
                                editing = Editing(routine: routine)
                            } toggle: {
                                Task { await store.setEnabled(routine, !routine.enabled) }
                            } remove: {
                                Task { await store.remove(routine) }
                            }
                        }
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 2)
                }
                .scrollClipDisabled()
                ideas(title: String(localized: "More ideas"))
            }
            AXYONSiriCard()
        }
        .sheet(item: $editing) { item in
            NavigationStack { AXYONRoutineEditor(routine: item.routine) }
        }
        .task {
            await store.load()
            #if targetEnvironment(simulator)
            await seedForScreens()
            #endif
        }
    }

    private var intro: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 14) {
                AXYONRoutineGlyph(icon: "sparkles", running: false, size: 52)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Your day on autopilot").font(.headline).foregroundStyle(.white)
                    Text("A routine does several things at once: when you arrive home, at a time, or when you tell Siri. Start from an idea, or ask AXYON in the chat: “every night at 11, turn everything off”.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            ideas(title: nil)
        }
        .padding(16)
        .axyonCard(cornerRadius: 24)
    }

    private func ideas(title: String?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if let title {
                Text(title).font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.dim)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(AXYONRoutineIdeas.all.filter { idea in !store.routines.contains { $0.name == idea.name } },
                            id: \.name) { idea in
                        Button {
                            editing = Editing(routine: idea)
                        } label: {
                            Label(idea.name, systemImage: idea.icon)
                                .font(.footnote.weight(.semibold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 8)
                                .coucouGlass(.regular, in: Capsule())
                        }
                        .buttonStyle(PressableButtonStyle())
                    }
                }
                .padding(.vertical, 2)
            }
        }
    }

    #if targetEnvironment(simulator)
    /// CI's screenshots (Hermes/tests/simulator.sh): three routines from the ideas, and the editor of one opened.
    private func seedForScreens() async {
        guard AXYONTestScreen.name.hasPrefix("house") else { return }
        if store.routines.isEmpty {
            for idea in AXYONRoutineIdeas.all where ["sun.max.fill", "moon.stars.fill", "film.fill"].contains(idea.icon) {
                _ = try? await store.save(idea)
            }
        }
        if AXYONTestScreen.base == "house-routine", let first = store.routines.first(where: { $0.trigger.kind == "time" }) {
            editing = Editing(routine: first)
        }
    }
    #endif
}

/// A routine's symbol in a sphere of the accent's light; while it runs, a ring of light turns around it.
struct AXYONRoutineGlyph: View {
    let icon: String
    let running: Bool
    var size: CGFloat = 46
    var dimmed = false

    var body: some View {
        let theme = AXYONTheme.current
        ZStack {
            Circle()
                .fill(RadialGradient(colors: dimmed ? [Color.white.opacity(0.18), Color.white.opacity(0.05)]
                                                   : [theme.accent, theme.accentDeep],
                                     center: UnitPoint(x: 0.35, y: 0.3), startRadius: 1, endRadius: size * 0.7))
                .overlay(Circle().strokeBorder(.white.opacity(0.35), lineWidth: 0.8))
                .shadow(color: dimmed ? .clear : theme.accent.opacity(0.55), radius: running ? 14 : 8)
            Image(systemName: icon)
                .font(.system(size: size * 0.42, weight: .semibold))
                .foregroundStyle(.white)
                .symbolEffect(.bounce, value: running)
            if running {
                TimelineView(.animation) { context in
                    let turn = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.2) / 1.2
                    Circle()
                        .trim(from: 0, to: 0.32)
                        .stroke(AngularGradient(colors: [.white.opacity(0), .white], center: .center),
                                style: StrokeStyle(lineWidth: 3, lineCap: .round))
                        .rotationEffect(.degrees(turn * 360))
                        .padding(-5)
                }
            }
        }
        .frame(width: size, height: size)
    }
}

struct AXYONRoutineCard: View {
    let routine: AXYONRoutine
    let running: Bool
    let run: () -> Void
    let edit: () -> Void
    let toggle: () -> Void
    let remove: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                AXYONRoutineGlyph(icon: routine.icon, running: running, dimmed: !routine.enabled)
                Spacer()
                Button(action: run) {
                    Image(systemName: running ? "ellipsis" : "play.fill")
                        .font(.callout.weight(.bold))
                        .foregroundStyle(.white)
                        .frame(width: 36, height: 36)
                        .background(Color.white.opacity(running ? 0.28 : 0.14), in: Circle())
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(PressableButtonStyle())
                .disabled(running)
                .accessibilityLabel(Text("Run"))
            }
            Spacer(minLength: 0)
            Text(routine.name)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(2)
            Label(routine.trigger.summary, systemImage: routine.trigger.icon)
                .font(.caption2)
                .foregroundStyle(AXYONPalette.dim)
                .lineLimit(2)
            HStack(spacing: 6) {
                Text(String(localized: "\(routine.steps.count) steps"))
                if let ok = routine.lastOK {
                    Image(systemName: ok ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                        .foregroundStyle(ok ? AXYONPalette.up : AXYONPalette.down)
                }
                if routine.by == "axyon" {
                    Image(systemName: "sparkles").foregroundStyle(AXYONTheme.current.accent)
                }
                if !routine.enabled { Text("Off") }
            }
            .font(.caption2.weight(.semibold))
            .foregroundStyle(AXYONPalette.faint)
        }
        .padding(14)
        .frame(width: 168, height: 178, alignment: .topLeading)
        .axyonCard(cornerRadius: 24, tint: running ? AXYONTheme.current.accent : nil)
        .opacity(routine.enabled ? 1 : 0.62)
        .scaleEffect(running ? 1.03 : 1)
        .animation(.spring(duration: 0.4, bounce: 0.35), value: running)
        .contentShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
        .onTapGesture(perform: edit)
        .contextMenu {
            Button(action: run) { Label("Run now", systemImage: "play.fill") }
            Button(action: edit) { Label("Edit", systemImage: "slider.horizontal.3") }
            Button(action: toggle) {
                Label(routine.enabled ? String(localized: "Turn off") : String(localized: "Turn on"),
                      systemImage: routine.enabled ? "pause.circle" : "play.circle")
            }
            Button(role: .destructive, action: remove) { Label("Remove", systemImage: "trash") }
        }
    }
}

struct AXYONOutcomeBanner: View {
    let outcome: AXYONRoutineOutcome
    let close: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: outcome.running ? "hourglass" : outcome.ok ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                .font(.title3)
                .foregroundStyle(outcome.ok || outcome.running ? AXYONPalette.up : AXYONPalette.down)
                .symbolEffect(.bounce, value: outcome.id)
            Text(outcome.sentence)
                .font(.footnote)
                .foregroundStyle(.white)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            Button(action: close) {
                Image(systemName: "xmark").font(.caption.weight(.bold)).foregroundStyle(AXYONPalette.dim)
            }
            .buttonStyle(.plain)
        }
        .padding(14)
        .axyonCard(cornerRadius: 20, tint: outcome.ok || outcome.running ? AXYONPalette.up : AXYONPalette.down)
    }
}

/// Siri and the Shortcuts app: the phrase, and the way to an automation that runs a routine with AXYON closed.
struct AXYONSiriCard: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Siri and Shortcuts", systemImage: "waveform.circle.fill")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
            Text("Say “Run Good night in AXYON” or “Ask AXYON”. In the Shortcuts app, an automation (a time, arriving home, an NFC tag, CarPlay, an alarm stopping…) with the action “Run AXYON routine” runs it to the minute, even with AXYON closed.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
                .fixedSize(horizontal: false, vertical: true)
            ShortcutsLink()
                .shortcutsLinkStyle(.automaticOutline)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22)
    }
}

// MARK: - The editor

struct AXYONRoutineEditor: View {
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    @State private var routine: AXYONRoutine
    @State private var stepEditing: StepEditing?
    @State private var saving = false
    @State private var problem: String?
    @State private var camera: MapCameraPosition = .automatic
    @State private var locating = false
    private let store = AXYONRoutineStore.shared

    struct StepEditing: Identifiable {
        let id = UUID()
        var step: AXYONStep
        var index: Int?
    }

    init(routine: AXYONRoutine) {
        _routine = State(initialValue: routine)
        if let c = routine.trigger.coordinate {
            _camera = State(initialValue: .region(MKCoordinateRegion(center: c, latitudinalMeters: routine.trigger.radius * 6,
                                                                     longitudinalMeters: routine.trigger.radius * 6)))
        }
    }

    private var canSave: Bool {
        !routine.name.trimmingCharacters(in: .whitespaces).isEmpty && !routine.steps.isEmpty
            && (!routine.trigger.isPlace || routine.trigger.hasPlace)
    }

    var body: some View {
        List {
            Section {
                TextField("Name (what you tell Siri)", text: $routine.name)
                    .font(.headline)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(AXYONRoutine.icons, id: \.self) { icon in
                            Button {
                                withAnimation(.spring(duration: 0.3)) { routine.icon = icon }
                            } label: {
                                AXYONRoutineGlyph(icon: icon, running: false, size: 40, dimmed: routine.icon != icon)
                                    .scaleEffect(routine.icon == icon ? 1.1 : 1)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 4)
                }
                Toggle("On", isOn: $routine.enabled)
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Picker("Starts", selection: $routine.trigger.kind) {
                    Text("By hand").tag("manual")
                    Text("At a time").tag("time")
                    Text("On arriving").tag("arrive")
                    Text("On leaving").tag("leave")
                }
                .pickerStyle(.segmented)
                switch routine.trigger.kind {
                case "time": timeFields
                case "arrive", "leave": placeFields
                default:
                    Text("Run it with the play button, Siri (“Run \(routine.name.isEmpty ? String(localized: "Good night") : routine.name) in AXYON”) or an automation of the Shortcuts app.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
            } header: {
                Text("When")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                ForEach(Array(routine.steps.enumerated()), id: \.element.id) { index, step in
                    Button {
                        stepEditing = StepEditing(step: step, index: index)
                    } label: {
                        HStack(spacing: 12) {
                            Text("\(index + 1)")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.black)
                                .frame(width: 22, height: 22)
                                .background(Circle().fill(.white))
                            Image(systemName: icon(step))
                                .foregroundStyle(AXYONTheme.current.accent)
                                .frame(width: 24)
                            Text(step.summary)
                                .font(.callout)
                                .foregroundStyle(.white)
                                .lineLimit(2)
                            Spacer(minLength: 0)
                        }
                    }
                    .contextMenu {
                        if index > 0 {
                            Button { routine.steps.swapAt(index, index - 1) } label: { Label("Move up", systemImage: "arrow.up") }
                        }
                        if index < routine.steps.count - 1 {
                            Button { routine.steps.swapAt(index, index + 1) } label: { Label("Move down", systemImage: "arrow.down") }
                        }
                        Button(role: .destructive) { routine.steps.remove(at: index) } label: { Label("Remove", systemImage: "trash") }
                    }
                }
                .onDelete { routine.steps.remove(atOffsets: $0) }
                .onMove { routine.steps.move(fromOffsets: $0, toOffset: $1) }
                addStepMenu
            } header: {
                Text("Steps, in order")
            } footer: {
                Text("Hold a step to move or remove it. A step that fails does not stop the next ones.")
            }
            .listRowBackground(GlassRowBackground())

            if !routine.id.isEmpty {
                Section {
                    Button {
                        Task {
                            await store.run(routine)
                            dismiss()
                        }
                    } label: {
                        Label("Run now", systemImage: "play.fill")
                    }
                    Button(role: .destructive) {
                        Task {
                            await store.remove(routine)
                            dismiss()
                        }
                    } label: {
                        Label("Remove the routine", systemImage: "trash")
                    }
                }
                .listRowBackground(GlassRowBackground())
            }

            if let problem {
                Section {
                    Label(problem, systemImage: "exclamationmark.triangle.fill").foregroundStyle(AXYONPalette.down)
                }
                .listRowBackground(GlassRowBackground())
            }
        }
        .scrollContentBackground(.hidden)
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle(routine.id.isEmpty ? String(localized: "New routine") : routine.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button {
                    Task { await save() }
                } label: {
                    if saving { ProgressView() } else { Text("Save").bold() }
                }
                .disabled(!canSave || saving)
            }
        }
        .sheet(item: $stepEditing) { item in
            NavigationStack {
                AXYONStepEditor(step: item.step) { done in
                    if let index = item.index, routine.steps.indices.contains(index) {
                        routine.steps[index] = done
                    } else {
                        routine.steps.append(done)
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
    }

    private func icon(_ step: AXYONStep) -> String {
        if step.kind == "device", let device = engine.house?.devices.first(where: { $0.id == step.device }) {
            return AXYONDeviceTile.icon(device.kind)
        }
        return step.icon
    }

    private var addStepMenu: some View {
        Menu {
            Section("The home") {
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "device")) } label: { Label("A device", systemImage: "house.fill") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "every")) } label: { Label("Every light, TV or plug", systemImage: "lightbulb.2.fill") }
            }
            Section("The iPhone") {
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "notify")) } label: { Label("A notification", systemImage: "bell.badge.fill") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "reminder")) } label: { Label("A reminder", systemImage: "checklist") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "speak")) } label: { Label("Say something", systemImage: "speaker.wave.2.fill") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "music")) } label: { Label("Music", systemImage: "music.note") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "torch")) } label: { Label("The flashlight", systemImage: "flashlight.on.fill") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "shortcut")) } label: { Label("One of your Shortcuts", systemImage: "square.stack.3d.up.fill") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "iphone", phone: "open")) } label: { Label("Open a link or app", systemImage: "arrow.up.forward.app.fill") }
            }
            Section("AXYON") {
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "ask")) } label: { Label("Ask AXYON Core", systemImage: "sparkles") }
                Button { stepEditing = StepEditing(step: AXYONStep(kind: "pause")) } label: { Label("Wait", systemImage: "hourglass") }
            }
        } label: {
            Label("Add a step", systemImage: "plus.circle.fill")
                .font(.callout.weight(.semibold))
        }
    }

    private var timeBinding: Binding<Date> {
        Binding {
            let parts = routine.trigger.at.split(separator: ":").compactMap { Int($0) }
            return Calendar.current.date(bySettingHour: parts.first ?? 7, minute: parts.count > 1 ? parts[1] : 0,
                                         second: 0, of: .now) ?? .now
        } set: { date in
            let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
            routine.trigger.at = String(format: "%02d:%02d", parts.hour ?? 7, parts.minute ?? 0)
        }
    }

    @ViewBuilder private var timeFields: some View {
        DatePicker("Time", selection: timeBinding, displayedComponents: .hourAndMinute)
        HStack(spacing: 6) {
            ForEach(1...7, id: \.self) { day in
                let on = routine.trigger.days.contains(day)
                Button {
                    withAnimation(.spring(duration: 0.25)) {
                        if on { routine.trigger.days.removeAll { $0 == day } } else { routine.trigger.days.append(day) }
                        routine.trigger.days.sort()
                    }
                } label: {
                    Text(AXYONTrigger.weekdays[day - 1])
                        .font(.caption.weight(.bold))
                        .foregroundStyle(on ? Color.black : Color.white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 9)
                        .background(Capsule().fill(on ? Color.white : Color.white.opacity(0.1)))
                }
                .buttonStyle(.plain)
            }
        }
        Text(String(localized: "Runs \(AXYONTrigger.daysText(routine.trigger.days)) at \(routine.trigger.at) while AXYON is open; with AXYON closed, iOS wakes it now and then. For the exact minute, make a Shortcuts automation at that time with “Run AXYON routine”."))
            .font(.footnote)
            .foregroundStyle(AXYONPalette.dim)
    }

    @ViewBuilder private var placeFields: some View {
        TextField("Place's name (Home, Work…)", text: $routine.trigger.placeName)
        MapReader { proxy in
            Map(position: $camera) {
                if let c = routine.trigger.coordinate {
                    MapCircle(center: c, radius: routine.trigger.radius)
                        .foregroundStyle(AXYONTheme.current.accent.opacity(0.25))
                        .stroke(AXYONTheme.current.accent, lineWidth: 2)
                    Marker(routine.trigger.placeName.isEmpty ? String(localized: "Home") : routine.trigger.placeName,
                           systemImage: routine.trigger.kind == "arrive" ? "house.fill" : "figure.walk.departure",
                           coordinate: c)
                        .tint(AXYONTheme.current.accent)
                }
            }
            .mapStyle(.standard(elevation: .flat, pointsOfInterest: .excludingAll))
            .onTapGesture { point in
                if let c = proxy.convert(point, from: .local) { place(c) }
            }
        }
        .frame(height: 210)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
        Button {
            Task { await locate() }
        } label: {
            HStack {
                Label("Where I am now", systemImage: "location.fill")
                Spacer()
                if locating { ProgressView() }
            }
        }
        .disabled(locating)
        VStack(alignment: .leading, spacing: 4) {
            Text(String(localized: "Radius: \(Int(routine.trigger.radius)) m"))
                .font(.footnote.weight(.semibold))
            Slider(value: $routine.trigger.radius, in: 100...1000, step: 50)
                .tint(AXYONTheme.current.accent)
        }
        if !AXYONPlaces.shared.always {
            Button {
                AXYONPlaces.shared.askAlways()
            } label: {
                Label("Allow location “Always”", systemImage: "location.circle.fill")
            }
            Text("iOS wakes AXYON when you arrive or leave only with location “Always” (iPhone Settings → AXYON → Location). AXYON keeps no record of where you go.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
        } else {
            Text("Tap the map to move the place. iOS wakes AXYON on arriving or leaving, even closed, and AXYON tells you what it did.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
        }
    }

    private func place(_ c: CLLocationCoordinate2D) {
        routine.trigger.latitude = c.latitude
        routine.trigger.longitude = c.longitude
        withAnimation {
            camera = .region(MKCoordinateRegion(center: c, latitudinalMeters: routine.trigger.radius * 6,
                                                longitudinalMeters: routine.trigger.radius * 6))
        }
    }

    private func locate() async {
        locating = true
        defer { locating = false }
        let answer = await HermesArms.answer("iphone", HermesArms.data(["action": "location"]))
        let object = HermesArms.object(answer)
        if let lat = (object["latitude"] as? NSNumber)?.doubleValue, let lon = (object["longitude"] as? NSNumber)?.doubleValue {
            place(CLLocationCoordinate2D(latitude: lat, longitude: lon))
            problem = nil
        } else {
            problem = object["error"] as? String ?? String(localized: "AXYON could not tell where the iPhone is.")
        }
    }

    private func save() async {
        saving = true
        defer { saving = false }
        routine.name = routine.name.trimmingCharacters(in: .whitespaces)
        do {
            try await store.save(routine)
            Haptics.success()
            dismiss()
        } catch {
            problem = HermesEngine.describe(error)
            Haptics.error()
        }
    }
}

// MARK: - One step

struct AXYONStepEditor: View {
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    @State private var step: AXYONStep
    let done: (AXYONStep) -> Void

    init(step: AXYONStep, done: @escaping (AXYONStep) -> Void) {
        _step = State(initialValue: step)
        self.done = done
    }

    private var devices: [HermesDevice] {
        (engine.house?.devices ?? []).filter { $0.paired && !$0.can.filter { $0 != "state" }.isEmpty }
    }

    private var device: HermesDevice? { devices.first { $0.id == step.device } }

    var body: some View {
        List {
            Section {
                switch step.kind {
                case "device": deviceFields
                case "every": everyFields
                case "pause":
                    Stepper(String(localized: "Wait \(step.seconds) s"), value: $step.seconds, in: 1...120, step: 5)
                case "ask":
                    TextField("What AXYON Core looks into when the routine runs", text: $step.text, axis: .vertical)
                        .lineLimit(3...8)
                    Text("The answer comes as a notification. AXYON Core can read your agenda, reminders, e-mails, portfolio, the web and the home.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                default: phoneFields
                }
            }
            .listRowBackground(GlassRowBackground())
        }
        .scrollContentBackground(.hidden)
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle(Text(verbatim: step.summary.isEmpty ? "" : String(step.summary.prefix(40))))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button("Done") {
                    done(step)
                    dismiss()
                }
                .bold()
                .disabled(!step.complete)
            }
        }
    }

    @ViewBuilder private var deviceFields: some View {
        if devices.isEmpty {
            Text("No device in AXYON's home yet: add one in the Home tab first.")
                .foregroundStyle(AXYONPalette.dim)
        } else {
            Picker("Device", selection: Binding(get: { step.device }, set: { id in
                step.device = id
                step.name = devices.first { $0.id == id }?.name ?? id
                let can = (devices.first { $0.id == id }?.can ?? []).filter { $0 != "state" }
                if !can.contains(step.action) { step.action = can.first ?? "turn_on" }
                step.value = ""
            })) {
                Text("Choose").tag("")
                ForEach(devices) { d in
                    Text(d.room.isEmpty ? d.name : "\(d.name) · \(d.room)").tag(d.id)
                }
            }
            if let device {
                Picker("Action", selection: $step.action) {
                    ForEach(device.can.filter { $0 != "state" }, id: \.self) { action in
                        Text(AXYONStep.actionTitle(action)).tag(action)
                    }
                }
                valueField(device)
            }
        }
    }

    @ViewBuilder private func valueField(_ device: HermesDevice) -> some View {
        switch step.action {
        case "set_brightness", "set_volume":
            let level = Binding<Double>(get: { Double(step.value) ?? 50 }, set: { step.value = String(Int($0)) })
            VStack(alignment: .leading) {
                Text("\(Int(level.wrappedValue))%").font(.footnote.weight(.semibold))
                Slider(value: level, in: 0...100, step: 5).tint(AXYONTheme.current.accent)
            }
        case "set_color":
            Picker("Colour", selection: $step.value) {
                ForEach(["vermelho", "laranja", "amarelo", "verde", "ciano", "azul", "roxo", "rosa", "quente", "frio"],
                        id: \.self) { Text(verbatim: $0.capitalized).tag($0) }
            }
        case "open_app":
            if device.apps.isEmpty {
                TextField("The app's name", text: $step.value)
            } else {
                Picker("App", selection: $step.value) {
                    ForEach(device.apps, id: \.self) { Text(verbatim: $0).tag($0) }
                }
            }
        case "key":
            Picker("Key", selection: $step.value) {
                ForEach(["home", "back", "ok", "up", "down", "left", "right", "play_pause", "menu", "source"], id: \.self) {
                    Text(verbatim: $0).tag($0)
                }
            }
        case "volume_up", "volume_down", "channel_up", "channel_down", "preset":
            let count = Binding<Int>(get: { Int(step.value) ?? 1 }, set: { step.value = String($0) })
            Stepper(step.action == "preset" ? String(localized: "Preset \(count.wrappedValue)")
                                            : String(localized: "\(count.wrappedValue) times"),
                    value: count, in: 1...15)
        case "set_temperature":
            let degrees = Binding<Int>(get: { Int(step.value) ?? 22 }, set: { step.value = String($0) })
            Stepper("\(degrees.wrappedValue)°", value: degrees, in: 16...30)
        default:
            EmptyView()
        }
    }

    @ViewBuilder private var everyFields: some View {
        Picker("Which", selection: Binding(get: { step.everyKind }, set: { kind in
            step.everyKind = kind
            if kind == "tv" || kind == "switch" { step.action = "turn_off" }
        })) {
            Text("All the lights").tag("light")
            Text("All the TVs").tag("tv")
            Text("All the fans").tag("fan")
            Text("All the plugs").tag("switch")
        }
        Picker("Action", selection: $step.action) {
            if step.everyKind == "light" || step.everyKind == "fan" {
                Text("Turn on").tag("turn_on")
            }
            Text("Turn off").tag("turn_off")
        }
        .pickerStyle(.segmented)
        Text("Every one AXYON reaches: the iPhone's own, the Hue bridge's and Home Assistant's. TVs and plugs only off: turning every plug on would power what you did not choose.")
            .font(.footnote)
            .foregroundStyle(AXYONPalette.dim)
    }

    @ViewBuilder private var phoneFields: some View {
        switch step.phone {
        case "notify":
            TextField("Title (optional: the routine's name)", text: $step.title)
            TextField("What it says", text: $step.text, axis: .vertical).lineLimit(2...5)
        case "reminder":
            TextField("The reminder", text: $step.text)
            Stepper(step.minutes == 0 ? String(localized: "No time") : String(localized: "In \(step.minutes) min"),
                    value: $step.minutes, in: 0...720, step: 5)
        case "speak":
            TextField("What AXYON says aloud", text: $step.text, axis: .vertical).lineLimit(2...4)
        case "torch":
            Toggle("Flashlight on", isOn: $step.on)
        case "music":
            Picker("Music", selection: $step.value) {
                Text("Play").tag("play")
                Text("Pause").tag("pause")
                Text("Next").tag("next")
                Text("Previous").tag("previous")
            }
            .pickerStyle(.segmented)
        case "open":
            TextField("https://…, spotify:, whatsapp://…", text: $step.text)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Text("Opens only with the iPhone unlocked.").font(.footnote).foregroundStyle(AXYONPalette.dim)
        default:
            TextField("The shortcut's exact name", text: $step.text)
            TextField("Its input (optional)", text: $step.input)
            Text("Your Shortcuts reach what AXYON cannot by itself: HomeKit scenes, Focus, alarms, Wi-Fi. Opens the Shortcuts app, with the iPhone unlocked.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
        }
    }
}
