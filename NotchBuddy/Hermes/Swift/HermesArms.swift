import AVFoundation
import Contacts
import CoreLocation
import EventKit
import Foundation
import MediaPlayer
import UIKit
import UserNotifications

/// What Hermes does with the iPhone itself, asked over the bridge (axyon_hermes/iphone.py): notifications, reminders
/// and calendar events, contacts, opening apps and links, the user's Shortcuts, the clipboard, where the iPhone is,
/// its state, the flashlight, music and speaking, and Mochi's home through a paired computer. Each request and answer
/// is a JSON object (Data, so nothing that is not Sendable crosses an actor); what iOS asks permission for is asked
/// the first time, by iOS.
///
/// What it never does by itself: send a message or an e-mail, or call someone. Those open the app concerned with
/// everything filled in, and the user sends.
@MainActor
enum HermesArms {
    static func answer(_ kind: String, _ request: Data) async -> Data {
        let r = object(request)
        switch kind {
        case "iphone": return await iphone(r)
        case "home": return data(home())
        case "homeAction": return data(await homeAction(r))
        default: return data(["error": "The app does not know \(kind)."])
        }
    }

    private static func iphone(_ r: [String: Any]) async -> Data {
        let action = r["action"] as? String ?? ""
        let request = data(r)
        switch action {
        case "notify": return await notify(request)
        case "reminders": return await reminders(request)
        case "calendar": return await calendar(request)
        case "contacts": return await contacts(request)
        case "location": return await location()
        case "open": return data(await open(r))
        case "shortcut": return data(await shortcut(r))
        case "clipboard": return data(clipboard(r))
        case "status": return data(status())
        case "torch": return data(torch(r))
        case "music": return data(music(r))
        case "speak":
            MochiVoice.shared.say(r["text"] as? String ?? "", language: Locale.preferredLanguages.first ?? "pt-BR")
            return data(["ok": true])
        default:
            return data(["error": "No such iPhone action: \(action)."])
        }
    }

    nonisolated static func object(_ data: Data) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }

    nonisolated static func data(_ object: [String: Any]) -> Data {
        (try? JSONSerialization.data(withJSONObject: object)) ?? Data(#"{"error":"The answer could not be written."}"#.utf8)
    }

    // MARK: Notifications

    /// A notification at a time (`at`, ISO 8601, or `inSeconds`): it comes even with AXYON closed.
    nonisolated private static func notify(_ request: Data) async -> Data {
        let r = object(request)
        let center = UNUserNotificationCenter.current()
        let granted = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        guard granted else { return data(["error": "Notifications are off for AXYON in the iPhone's Settings."]) }
        let content = UNMutableNotificationContent()
        content.title = String((r["title"] as? String ?? "AXYON").prefix(120))
        content.body = String((r["body"] as? String ?? "").prefix(600))
        content.sound = .default
        let when = date(r["at"]) ?? (r["inSeconds"] as? Double).map { Date.now.addingTimeInterval(max(1, $0)) }
        let trigger: UNNotificationTrigger
        if let when, when.timeIntervalSinceNow > 1 {
            let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute, .second], from: when)
            trigger = UNCalendarNotificationTrigger(dateMatching: parts, repeats: false)
        } else {
            trigger = UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false)
        }
        let id = "hermes-" + UUID().uuidString
        do {
            try await center.add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
            return data(["ok": true, "id": id, "when": iso(when ?? .now)])
        } catch {
            return data(["error": error.localizedDescription])
        }
    }

    /// Hermes' answer, when it ended with AXYON away from the screen.
    nonisolated static func notifyAnswer(_ text: String) {
        let content = UNMutableNotificationContent()
        content.title = "AXYON"
        content.body = String(text.prefix(300))
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "hermes-answer-" + UUID().uuidString,
                                                                     content: content, trigger: nil))
    }

    // MARK: Reminders and calendar

    nonisolated private static func reminders(_ request: Data) async -> Data {
        let r = object(request)
        let store = EKEventStore()
        guard (try? await store.requestFullAccessToReminders()) == true else {
            return data(["error": "AXYON may not use Reminders: iPhone Settings → Apps → AXYON → Reminders."])
        }
        switch r["do"] as? String ?? "list" {
        case "add":
            let reminder = EKReminder(eventStore: store)
            reminder.title = r["title"] as? String ?? ""
            reminder.notes = r["notes"] as? String
            reminder.calendar = store.defaultCalendarForNewReminders()
            if let due = date(r["due"]) {
                reminder.dueDateComponents = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: due)
                reminder.addAlarm(EKAlarm(absoluteDate: due))
            }
            do {
                try store.save(reminder, commit: true)
                return data(["ok": true, "id": reminder.calendarItemIdentifier])
            } catch {
                return data(["error": error.localizedDescription])
            }
        case "complete":
            guard let id = r["id"] as? String, let item = store.calendarItem(withIdentifier: id) as? EKReminder else {
                return data(["error": "No such reminder."])
            }
            item.isCompleted = true
            do {
                try store.save(item, commit: true)
                return data(["ok": true])
            } catch {
                return data(["error": error.localizedDescription])
            }
        default:
            let predicate = store.predicateForIncompleteReminders(withDueDateStarting: nil, ending: nil, calendars: nil)
            let rows: [[String: String]] = await withCheckedContinuation { continuation in
                store.fetchReminders(matching: predicate) { @Sendable found in
                    let rows = (found ?? []).prefix(60).map { reminder -> [String: String] in
                        var row = ["id": reminder.calendarItemIdentifier, "title": reminder.title ?? ""]
                        if let parts = reminder.dueDateComponents, let due = Calendar.current.date(from: parts) {
                            row["due"] = iso(due)
                        }
                        return row
                    }
                    continuation.resume(returning: Array(rows))
                }
            }
            return data(["reminders": rows])
        }
    }

    nonisolated private static func calendar(_ request: Data) async -> Data {
        let r = object(request)
        let store = EKEventStore()
        guard (try? await store.requestFullAccessToEvents()) == true else {
            return data(["error": "AXYON may not use the Calendar: iPhone Settings → Apps → AXYON → Calendars."])
        }
        switch r["do"] as? String ?? "list" {
        case "add":
            guard let start = date(r["start"]) else { return data(["error": "An event needs its start (ISO 8601)."]) }
            let event = EKEvent(eventStore: store)
            event.title = r["title"] as? String ?? ""
            event.startDate = start
            event.endDate = date(r["end"]) ?? start.addingTimeInterval(3600)
            event.location = r["location"] as? String
            event.notes = r["notes"] as? String
            event.calendar = store.defaultCalendarForNewEvents
            do {
                try store.save(event, span: .thisEvent)
                return data(["ok": true, "id": event.eventIdentifier ?? ""])
            } catch {
                return data(["error": error.localizedDescription])
            }
        default:
            let from = date(r["from"]) ?? Calendar.current.startOfDay(for: .now)
            let to = date(r["to"]) ?? from.addingTimeInterval(7 * 86_400)
            let predicate = store.predicateForEvents(withStart: from, end: to, calendars: nil)
            let rows = store.events(matching: predicate).prefix(80).map { event -> [String: Any] in
                var row: [String: Any] = ["title": event.title ?? "", "start": iso(event.startDate),
                                          "end": iso(event.endDate), "allDay": event.isAllDay]
                if let place = event.location, !place.isEmpty { row["location"] = place }
                return row
            }
            return data(["events": Array(rows)])
        }
    }

    // MARK: Contacts

    nonisolated private static func contacts(_ request: Data) async -> Data {
        let r = object(request)
        let store = CNContactStore()
        guard (try? await store.requestAccess(for: .contacts)) == true else {
            return data(["error": "AXYON may not read Contacts: iPhone Settings → Apps → AXYON → Contacts."])
        }
        let query = (r["query"] as? String ?? "").trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return data(["error": "Who? A name to look for."]) }
        let keys = [CNContactGivenNameKey, CNContactFamilyNameKey, CNContactPhoneNumbersKey, CNContactEmailAddressesKey,
                    CNContactOrganizationNameKey] as [CNKeyDescriptor]
        let people = (try? store.unifiedContacts(matching: CNContact.predicateForContacts(matchingName: query),
                                                 keysToFetch: keys)) ?? []
        let found = people.prefix(10).map { person -> [String: Any] in
            ["name": "\(person.givenName) \(person.familyName)".trimmingCharacters(in: .whitespaces),
             "company": person.organizationName,
             "phones": person.phoneNumbers.map(\.value.stringValue),
             "emails": person.emailAddresses.map { $0.value as String }]
        }
        return data(["contacts": Array(found)])
    }

    // MARK: Apps, links and shortcuts

    /// The schemes Hermes may open: the web, maps, and the apps that send or call only after the user taps.
    private static let schemes: Set<String> = [
        "https", "http", "tel", "facetime", "facetime-audio", "sms", "mailto", "maps", "comgooglemaps", "waze",
        "whatsapp", "tg", "spotify", "youtube", "music", "shortcuts", "instagram", "calshow", "app-settings",
    ]

    private static func open(_ r: [String: Any]) async -> [String: Any] {
        guard let text = r["url"] as? String, let url = URL(string: text), let scheme = url.scheme?.lowercased() else {
            return ["error": "Not a link."]
        }
        guard schemes.contains(scheme) else { return ["error": "AXYON does not open \(scheme): links."] }
        let target = scheme == "app-settings" ? URL(string: UIApplication.openSettingsURLString)! : url
        let opened = await UIApplication.shared.open(target)
        return opened ? ["ok": true, "note": "Opened on the iPhone: the user finishes there."]
            : ["error": "No app on this iPhone opens that."]
    }

    /// One of the user's Shortcuts, by its name, with an input: the Shortcuts app runs it, then comes back.
    private static func shortcut(_ r: [String: Any]) async -> [String: Any] {
        let name = r["name"] as? String ?? ""
        guard !name.isEmpty, var parts = URLComponents(string: "shortcuts://x-callback-url/run-shortcut") else {
            return ["error": "Which shortcut?"]
        }
        var items = [URLQueryItem(name: "name", value: name), URLQueryItem(name: "x-success", value: "coucou://hermes")]
        if let input = r["input"] as? String, !input.isEmpty {
            items += [URLQueryItem(name: "input", value: "text"), URLQueryItem(name: "text", value: input)]
        }
        parts.queryItems = items
        guard let url = parts.url, await UIApplication.shared.open(url) else {
            return ["error": "The Shortcuts app did not open."]
        }
        return ["ok": true, "note": "Shortcuts is running \"\(name)\"."]
    }

    private static func clipboard(_ r: [String: Any]) -> [String: Any] {
        if r["do"] as? String == "set" {
            UIPasteboard.general.string = r["text"] as? String ?? ""
            return ["ok": true]
        }
        return ["text": String((UIPasteboard.general.string ?? "").prefix(8000))]
    }

    // MARK: The iPhone itself

    private static func status() -> [String: Any] {
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        var out: [String: Any] = ["model": device.model, "system": "\(device.systemName) \(device.systemVersion)",
                                  "name": device.name, "lowPowerMode": ProcessInfo.processInfo.isLowPowerModeEnabled,
                                  "timeZone": TimeZone.current.identifier, "now": iso(.now)]
        if device.batteryLevel >= 0 { out["battery"] = Int(device.batteryLevel * 100) }
        out["charging"] = device.batteryState == .charging || device.batteryState == .full
        if let values = try? URL(fileURLWithPath: NSHomeDirectory())
            .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey, .volumeTotalCapacityKey]) {
            if let free = values.volumeAvailableCapacityForImportantUsage { out["freeGB"] = Double(free / 100_000_000) / 10 }
            if let total = values.volumeTotalCapacity { out["totalGB"] = Double(total / 100_000_000) / 10 }
        }
        return out
    }

    nonisolated private static func location() async -> Data {
        let session = CLServiceSession(authorization: .whenInUse)
        defer { session.invalidate() }
        do {
            for try await update in CLLocationUpdate.liveUpdates() {
                if update.authorizationDenied || update.authorizationDeniedGlobally {
                    return data(["error": "AXYON may not know where the iPhone is: iPhone Settings → Privacy → Location."])
                }
                guard let place = update.location else { continue }
                var out: [String: Any] = ["latitude": place.coordinate.latitude, "longitude": place.coordinate.longitude,
                                          "accuracyMeters": Int(place.horizontalAccuracy)]
                if let mark = try? await CLGeocoder().reverseGeocodeLocation(place).first {
                    out["city"] = mark.locality ?? ""
                    out["region"] = mark.administrativeArea ?? ""
                    out["country"] = mark.country ?? ""
                    out["street"] = [mark.thoroughfare, mark.subThoroughfare].compactMap { $0 }.joined(separator: ", ")
                }
                return data(out)
            }
        } catch {
            return data(["error": error.localizedDescription])
        }
        return data(["error": "No position yet."])
    }

    private static func torch(_ r: [String: Any]) -> [String: Any] {
        guard let device = AVCaptureDevice.default(for: .video), device.hasTorch else {
            return ["error": "This iPhone has no flashlight."]
        }
        let on = r["on"] as? Bool ?? (device.torchMode != .on)
        do {
            try device.lockForConfiguration()
            device.torchMode = on ? .on : .off
            device.unlockForConfiguration()
            return ["ok": true, "on": on]
        } catch {
            return ["error": error.localizedDescription]
        }
    }

    private static func music(_ r: [String: Any]) -> [String: Any] {
        let player = MPMusicPlayerController.systemMusicPlayer
        switch r["do"] as? String ?? "now" {
        case "play": player.play()
        case "pause": player.pause()
        case "next": player.skipToNextItem()
        case "previous": player.skipToPreviousItem()
        default: break
        }
        var out: [String: Any] = ["playing": player.playbackState == .playing]
        if let item = player.nowPlayingItem {
            out["title"] = item.title ?? ""
            out["artist"] = item.artist ?? ""
        }
        return out
    }

    // MARK: Mochi's home through the paired computer

    private static func unreachable() -> [String: Any]? {
        let pc = PCLink.shared
        guard pc.isPaired else {
            return ["error": "No computer is paired. The devices AXYON drives from this iPhone itself are in home_devices."]
        }
        guard pc.status == .connected else {
            return ["error": "\(pc.name) is not reachable now (at home on the Wi-Fi, or anywhere with Tailscale on both)."]
        }
        guard pc.offers("home"), let home = pc.model.home, home.enabled else {
            return ["error": "Mochi's home on \(pc.name) is not shared with this iPhone (Settings → Phone there)."]
        }
        return nil
    }

    private static func home() -> [String: Any] {
        if let problem = unreachable() { return problem }
        let pc = PCLink.shared
        pc.refreshHome()
        let home = pc.model.home ?? PCHome()
        return ["computer": pc.name,
                "devices": home.devices.map(describe),
                "routines": home.routines.map { ["id": $0.id, "name": $0.name, "steps": $0.steps] as [String: Any] }]
    }

    private static func describe(_ d: PCHome.Device) -> [String: Any] {
        var state: [String: Any] = ["online": d.state.online]
        if let on = d.state.on { state["on"] = on }
        if let volume = d.state.volume { state["volume"] = volume }
        if let muted = d.state.muted { state["muted"] = muted }
        if let brightness = d.state.brightness { state["brightness"] = brightness }
        if let app = d.state.app { state["app"] = app }
        if let channel = d.state.channel { state["channel"] = channel }
        return ["id": d.id, "name": d.name, "room": d.room, "kind": d.kind, "source": d.source, "can": d.caps,
                "apps": Array(d.apps.prefix(20)), "state": state, "asksFirst": d.ask]
    }

    private static func homeAction(_ r: [String: Any]) async -> [String: Any] {
        if let problem = unreachable() { return problem }
        let pc = PCLink.shared
        let wanted = r["device"] as? String ?? ""
        let devices = pc.model.home?.devices ?? []
        guard let device = devices.first(where: { $0.id == wanted })
                ?? devices.first(where: { $0.name.localizedCaseInsensitiveCompare(wanted) == .orderedSame }) else {
            return ["error": "No device \(wanted) in Mochi's home."]
        }
        let action = r["action"] as? String ?? ""
        var value: PCHomeValue?
        switch action {
        case "set_volume", "set_brightness":
            if let n = r["value"] as? Double { value = .number(Int(n)) }
            else if let t = r["value"] as? String, let n = Int(t) { value = .number(n) }
        case "open_app":
            let asked = "\(r["value"] ?? "")"
            value = .text(device.apps.first { $0.localizedCaseInsensitiveCompare(asked) == .orderedSame } ?? asked)
        case "set_channel":
            value = .text("\(r["value"] ?? "")")
        default:
            value = nil
        }
        let result = await pc.homeControl(device, action: action, value: value)
        var out: [String: Any] = ["ok": result.ok]
        if !result.reason.isEmpty { out["reason"] = result.reason }
        if !result.message.isEmpty { out["message"] = result.message }
        if !result.detail.isEmpty { out["detail"] = result.detail }
        return out
    }

    // MARK: Dates

    nonisolated private static func date(_ value: Any?) -> Date? {
        guard let text = value as? String, !text.isEmpty else { return nil }
        if let d = ISO8601DateFormatter().date(from: text) { return d }
        let local = DateFormatter()
        local.locale = Locale(identifier: "en_US_POSIX")
        local.timeZone = .current
        for format in ["yyyy-MM-dd'T'HH:mm:ss", "yyyy-MM-dd'T'HH:mm", "yyyy-MM-dd HH:mm", "yyyy-MM-dd"] {
            local.dateFormat = format
            if let d = local.date(from: text) { return d }
        }
        return nil
    }

    nonisolated private static func iso(_ date: Date) -> String {
        let f = ISO8601DateFormatter()
        f.timeZone = .current
        f.formatOptions = [.withInternetDateTime]
        return f.string(from: date)
    }
}
