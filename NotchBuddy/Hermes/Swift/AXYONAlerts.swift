import BackgroundTasks
import Foundation
import UserNotifications

/// The superpowers' alerts with AXYON closed, as the computer's island tells them (Settings → Superpowers, off until
/// the user turns them on). iOS wakes the app now and then for a background refresh, at its own pace (a few times a
/// day); the engine reads what is due and says what is worth telling (HermesPowers.watch), and each thing comes as a
/// notification, once: an asset gone under the user's own ceiling or back above it, a strong day for an asset or for
/// the whole portfolio, an asset under one of its ceilings. Only the official sources the superpowers read are
/// contacted, and nothing about the user is sent.
enum AXYONAlerts {
    /// The background refresh's name (BGTaskSchedulerPermittedIdentifiers, project-hermes.yml).
    static let task = "fr.louisraille.Coucou.powers-watch"
    static let enabledKey = "axyon.alerts"
    /// iOS gives a background refresh about half a minute: the reading stops well before.
    static let backgroundBudget: Double = 20

    static var enabled: Bool { UserDefaults.standard.bool(forKey: enabledKey) }

    /// The next look while the alerts are on: iOS decides when, not before half an hour from now.
    static func schedule() {
        guard enabled else {
            BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: task)
            return
        }
        let request = BGAppRefreshTaskRequest(identifier: task)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 30 * 60)
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            HermesEngine.log.error("No background look scheduled: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// iOS woke AXYON for a look: the next one first (iOS may end this one early), then this one.
    static func backgroundLook() async {
        schedule()
        await look(budget: backgroundBudget)
    }

    /// One look: what is worth telling, as notifications. How many there were.
    @discardableResult
    static func look(budget: Double = backgroundBudget) async -> Int {
        guard enabled else { return 0 }
        let alerts = await HermesPowers.watch(budget: budget)
        for alert in alerts {
            await post(alert)
        }
        return alerts.count
    }

    /// One alert as the computer's island says it, in the user's language.
    private static func post(_ a: [String: Any]) async {
        let kind = a["kind"] as? String ?? ""
        let ticker = a["ticker"] as? String ?? ""
        let now = AXYONMoney.text(a["price"] as? Double ?? 0)
        let ceiling = AXYONMoney.text(a["ceiling"] as? Double ?? 0)
        let day = percent(a["day"] as? Double ?? 0)
        let money = (a["money"] as? Double).map(AXYONMoney.signed)
        let title: String
        let body: String
        switch kind {
        case "into_zone":
            title = String(localized: "\(ticker) went below your ceiling")
            body = String(localized: "\(now) now · your ceiling \(ceiling)")
        case "out_of_zone":
            title = String(localized: "\(ticker) went back above your ceiling")
            body = String(localized: "\(now) now · your ceiling \(ceiling)")
        case "up", "down":
            title = kind == "up" ? String(localized: "\(ticker) up \(day) today") : String(localized: "\(ticker) down \(day) today")
            body = money.map { String(localized: "\(now) now · your position \($0) today") } ?? String(localized: "\(now) now")
        case "portfolio_up", "portfolio_down":
            title = kind == "portfolio_up" ? String(localized: "Your portfolio is up \(day) today")
                : String(localized: "Your portfolio is down \(day) today")
            body = String(localized: "\(now) now · \(money ?? AXYONMoney.signed(0)) today")
        case "opportunity":
            let below = percent(a["below"] as? Double ?? 0)
            title = String(localized: "\(ticker) is \(below) below its ceiling")
            body = String(localized: "\(now) now · \(method(a["method"] as? String ?? "")): \(ceiling)")
        default:
            return
        }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        content.threadIdentifier = "axyon-alerts"
        // One per asset and kind: a newer one takes its place.
        let request = UNNotificationRequest(identifier: "axyon-alert-\(kind)-\(ticker)", content: content, trigger: nil)
        try? await UNUserNotificationCenter.current().add(request)
    }

    /// "3,2%", in the user's own way of writing it.
    private static func percent(_ value: Double) -> String {
        (abs(value) / 100).formatted(.percent.precision(.fractionLength(1)))
    }

    /// A ceiling method's name (coucou_powers::ceiling).
    private static func method(_ id: String) -> String {
        switch id {
        case "bazin": return "Bazin"
        case "bazin_projected": return String(localized: "projected Bazin")
        case "graham": return "Graham"
        case "lynch": return "Peter Lynch"
        case "pvp": return "P/VP"
        default: return String(localized: "your own ceiling")
        }
    }
}
