import BackgroundTasks
import Foundation
import UserNotifications

/// The superpowers' alerts with AXYON closed, as the computer's island tells them (Settings → Superpowers, off until
/// the user turns them on). iOS wakes the app now and then for a background refresh, at its own pace (a few times a
/// day); the engine reads what is due and says what is worth telling (HermesPowers.watch), and each thing comes as a
/// notification, once: an asset gone under the user's own ceiling or back above it, a strong day for an asset or for
/// the whole portfolio, an asset under one of its ceilings; and in the morning, once, the day (its record dates and
/// payments, a DARF, the decisions to look at again, the studies, the training, the English lesson). Only the
/// official sources the superpowers read are contacted, and nothing about the user is sent.
enum AXYONAlerts {
    /// The background refresh's name (BGTaskSchedulerPermittedIdentifiers, project-hermes.yml).
    static let task = "fr.louisraille.Coucou.powers-watch"
    static let enabledKey = "axyon.alerts"
    /// iOS gives a background refresh about half a minute: the reading stops well before.
    static let backgroundBudget: Double = 20

    static var enabled: Bool { UserDefaults.standard.bool(forKey: enabledKey) }

    /// The next look while the alerts are on or Hermes has scheduled tasks (HermesCron): iOS decides when, not before
    /// half an hour from now, or a quarter of an hour when a task comes due sooner.
    static func schedule() {
        let next = HermesCron.nextTimes().min()
        guard enabled || next != nil else {
            BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: task)
            return
        }
        let request = BGAppRefreshTaskRequest(identifier: task)
        let wait = next.map { min(30 * 60, max(15 * 60, $0.timeIntervalSinceNow)) } ?? 30 * 60
        request.earliestBeginDate = Date(timeIntervalSinceNow: enabled ? min(wait, 30 * 60) : wait)
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            HermesEngine.log.error("No background look scheduled: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// iOS woke AXYON for a look: the next one first (iOS may end this one early), then Hermes' tasks that came due
    /// (iOS gives about half a minute in all), then the alerts with the time left.
    static func backgroundLook() async {
        schedule()
        let started = Date.now
        if HermesCron.due() {
            await HermesCron.run(budget: enabled ? 18 : 26)
        }
        let left = (enabled && HermesCron.hasTasks ? 26 : backgroundBudget) - Date.now.timeIntervalSince(started)
        if enabled, left > 4 { await look(budget: min(left, backgroundBudget)) }
    }

    /// One look: what is worth telling, as notifications, and the day's briefing between 7 and 11. How many
    /// notifications there were.
    @discardableResult
    static func look(budget: Double = backgroundBudget) async -> Int {
        guard enabled else { return 0 }
        let morning = (7..<11).contains(Calendar.current.component(.hour, from: .now))
        let found = await HermesPowers.watch(budget: budget, morning: morning)
        let alerts = found["alerts"] as? [[String: Any]] ?? []
        for alert in alerts {
            await post(alert)
        }
        let day = (found["day"] as? [[String: Any]] ?? []).map(line).filter { !$0.isEmpty }
        if !day.isEmpty {
            await brief(day)
        }
        return alerts.count + (day.isEmpty ? 0 : 1)
    }

    /// The morning's briefing, as the computer's island says it.
    private static func brief(_ lines: [String]) async {
        let content = UNMutableNotificationContent()
        content.title = String(localized: "Good morning! Today:")
        content.body = lines.prefix(8).map { "• " + $0 }.joined(separator: "\n")
        content.sound = .default
        content.threadIdentifier = "axyon-day"
        let request = UNNotificationRequest(identifier: "axyon-day", content: content, trigger: nil)
        try? await UNUserNotificationCenter.current().add(request)
    }

    /// A day's item in the user's language (src-tauri/src/powers/mod.rs, `line`).
    private static func line(_ item: [String: Any]) -> String {
        let vars = item["vars"] as? [String: String] ?? [:]
        let v = { (key: String) in vars[key] ?? "" }
        let amount = vars["amount"].flatMap(Double.init).map(AXYONMoney.text)
        let code = item["code"] as? String ?? ""
        switch code {
        case "record", "payment", "amortization", "assembly", "offering", "material_fact", "notice", "report":
            let what: String
            switch code {
            case "record": what = String(localized: "data-com")
            case "payment": what = String(localized: "dividend paid")
            case "amortization": what = String(localized: "amortization")
            case "assembly": what = String(localized: "assembly: your vote")
            case "offering": what = String(localized: "offering: your decision")
            case "material_fact": what = String(localized: "material fact")
            case "notice": what = String(localized: "notice to holders")
            default: what = String(localized: "manager's report")
            }
            let when: String
            switch v("when") {
            case "today": when = String(localized: "today")
            case "tomorrow": when = String(localized: "tomorrow")
            default: when = String(localized: "soon")
            }
            return ([v("ticker"), what, when] + (amount.map { [$0] } ?? [])).joined(separator: " · ")
        case "darf":
            let due = v("due").split(separator: "-")
            let date = due.count == 3 ? "\(due[2])/\(due[1])/\(due[0])" : v("due")
            return String(localized: "DARF of \(amount ?? "") due \(date)")
        case "decision":
            let parts = v("what").split(separator: " ", maxSplits: 1).map(String.init)
            let ticker = parts.count == 2 ? parts[1] : v("what")
            let verb: String
            switch parts.count == 2 ? parts[0] : "" {
            case "buy": verb = String(localized: "buy")
            case "sell": verb = String(localized: "sell")
            case "hold": verb = String(localized: "hold")
            default: verb = String(localized: "avoid")
            }
            return String(localized: "Look again at your decision: \(verb) \(ticker)")
        case "study":
            return String(localized: "Study: \(v("subjects"))")
        case "study_review":
            return String(localized: "Review: \(v("subjects"))")
        case "training":
            return String(localized: "Training: \(v("session"))")
        case "training_light":
            return String(localized: "Training: \(v("session")), light")
        case "rest":
            return String(localized: "Training: rest or a walk today")
        case "english":
            let cards = Int(v("cards")) ?? 0
            return cards == 0 ? String(localized: "English: \(v("company"))'s results")
                : String(localized: "English: \(v("company"))'s results and \(String(cards)) cards")
        default:
            return item["title"] as? String ?? ""
        }
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
