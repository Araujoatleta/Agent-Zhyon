import Foundation
import Observation

// The consolidator, the Carteira tab, kept the way Investidor10 keeps one. The user's lançamentos make each asset's
// position: quantity, average price with the costs, and the result realized and still open at the quote of now. Each
// asset's proventos are counted on the shares held at their data-com: received, provisioned and announced. Every number
// is the engine's (coucou_powers::wallet, through HermesPowers). The quotes are Google Finance's during the session,
// about 15 minutes late, and B3's close after it; they are read again by themselves while the tab is open
// (AXYONWalletViews.swift).

// MARK: - What the engine says

/// The engine's JSON, read without failing on a missing field.
private enum WalletJSON {
    /// The number, -0 read as 0 (a screen would show "-R$ 0,00").
    static func num(_ value: Any?) -> Double? { (value as? NSNumber).map { $0.doubleValue + 0.0 } }
    static func dbl(_ value: Any?) -> Double { num(value) ?? 0 }
    static func int(_ value: Any?) -> Int { Int(num(value) ?? 0) }
    static func str(_ value: Any?) -> String { value as? String ?? "" }
    static func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }
    static func dict(_ value: Any?) -> [String: Any] { value as? [String: Any] ?? [:] }
}

/// One asset as the consolidator shows it.
struct WalletAsset: Identifiable, Equatable, Hashable {
    let id: String
    let classKey: String
    let quantity: Double
    let average: Double
    let price: Double?
    /// The price is the session's (Google Finance), from `priceAt`; else B3's close of `closeDay`.
    let live: Bool
    let priceAt: Date?
    let closeDay: String
    let dayChange: Double?
    let dayPct: Double?
    let value: Double
    let cost: Double
    let gain: Double
    let gainPct: Double?
    let weight: Double
    let target: Double?
    let dividends12: Double
    let yieldOnCost: Double?
    let provisioned: Double
    let trades: Int
    /// Its lançamentos were made here: its position is theirs.
    let managed: Bool
    let realized: Double
    let unpriced: Bool

    init(_ o: [String: Any]) {
        id = WalletJSON.str(o["ticker"])
        classKey = WalletJSON.str(o["class"])
        quantity = WalletJSON.dbl(o["quantity"])
        average = WalletJSON.dbl(o["average"])
        price = WalletJSON.num(o["price"])
        live = o["live"] as? Bool ?? false
        priceAt = WalletJSON.num(o["priceAt"]).map { Date(timeIntervalSince1970: $0) }
        closeDay = WalletJSON.str(o["closeDay"])
        dayChange = WalletJSON.num(o["dayChange"])
        dayPct = WalletJSON.num(o["dayPct"])
        value = WalletJSON.dbl(o["value"])
        cost = WalletJSON.dbl(o["cost"])
        gain = WalletJSON.dbl(o["gain"])
        gainPct = WalletJSON.num(o["gainPct"])
        weight = WalletJSON.dbl(o["weight"])
        target = WalletJSON.num(o["target"])
        dividends12 = WalletJSON.dbl(o["dividends12"])
        yieldOnCost = WalletJSON.num(o["yieldOnCost"])
        provisioned = WalletJSON.dbl(o["provisioned"])
        trades = WalletJSON.int(o["trades"])
        managed = o["managed"] as? Bool ?? false
        realized = WalletJSON.dbl(o["realized"])
        unpriced = o["unpriced"] as? Bool ?? false
    }

    var ticker: String { id }
}

/// A class of the portfolio and its assets.
struct WalletClass: Identifiable, Equatable {
    let id: String
    let label: String
    let value: Double
    let cost: Double
    let gain: Double
    let gainPct: Double?
    let weight: Double
    let target: Double?
    let assets: [WalletAsset]

    init(_ o: [String: Any]) {
        id = WalletJSON.str(o["key"])
        label = AXYONInvest.className(id, WalletJSON.str(o["label"]))
        value = WalletJSON.dbl(o["value"])
        cost = WalletJSON.dbl(o["cost"])
        gain = WalletJSON.dbl(o["gain"])
        gainPct = WalletJSON.num(o["gainPct"])
        weight = WalletJSON.dbl(o["weight"])
        target = WalletJSON.num(o["target"])
        assets = WalletJSON.list(o["assets"]).map(WalletAsset.init)
    }
}

/// One lançamento: a buy, a sale, or what was held before the first ("saldo inicial").
struct WalletTrade: Identifiable, Equatable {
    let id: Int
    let date: String
    let ticker: String
    /// buy, sell or opening.
    let kind: String
    let quantity: Double
    let price: Double
    let fees: Double
    let total: Double
    /// A sale's result, against the average price that day.
    let result: Double?

    init(_ o: [String: Any]) {
        id = WalletJSON.int(o["id"])
        date = WalletJSON.str(o["date"])
        ticker = WalletJSON.str(o["ticker"])
        kind = WalletJSON.str(o["kind"])
        quantity = WalletJSON.dbl(o["quantity"])
        price = WalletJSON.dbl(o["price"])
        fees = WalletJSON.dbl(o["fees"])
        total = WalletJSON.dbl(o["total"])
        result = WalletJSON.num(o["result"])
    }

    /// "2026-10" of its date.
    var month: String { String(date.prefix(7)) }
}

/// One provento: what an asset paid, or will pay, for the shares held at its data-com.
struct WalletPaid: Identifiable, Equatable {
    let id: String
    let ticker: String
    /// Rendimento, Dividendo, Juros sobre capital próprio, Amortização.
    let kind: String
    let record: String
    let payment: String?
    let perShare: Double
    let quantity: Double
    let gross: Double
    /// Net of the income tax held at the source (JCP).
    let net: Double
    /// Counted on the shares held now: no lançamento tells what was held at the data-com.
    let estimated: Bool

    init(_ o: [String: Any]) {
        ticker = WalletJSON.str(o["ticker"])
        kind = WalletJSON.str(o["kind"])
        record = WalletJSON.str(o["record"])
        payment = o["payment"] as? String
        perShare = WalletJSON.dbl(o["perShare"])
        quantity = WalletJSON.dbl(o["quantity"])
        gross = WalletJSON.dbl(o["gross"])
        net = WalletJSON.dbl(o["net"])
        estimated = o["estimated"] as? Bool ?? false
        id = "\(ticker)-\(kind)-\(record)-\(payment ?? "")-\(perShare)"
    }

    var isJCP: Bool { kind.localizedCaseInsensitiveContains("juros") }
}

/// The proventos received in one month, asset by asset.
struct WalletMonth: Identifiable, Equatable {
    struct Item: Identifiable, Equatable {
        let id: String
        let value: Double
    }

    let id: String
    let total: Double
    let items: [Item]

    private var date: Date? {
        let parts = id.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 2 else { return nil }
        return Calendar.current.date(from: DateComponents(year: parts[0], month: parts[1], day: 1))
    }

    /// "Out".
    var short: String { date?.formatted(.dateTime.month(.abbreviated)).capitalized ?? id }
    /// "Outubro de 2026".
    var long: String { date?.formatted(.dateTime.month(.wide).year()).capitalized ?? id }
}

/// The consolidator's screen, as the engine computes it.
struct WalletOverview: Equatable {
    var total = 0.0
    var cost = 0.0
    var gain = 0.0
    var gainPct: Double?
    var realized = 0.0
    var realizedYear = 0.0
    var day: Double?
    var dayPct: Double?
    var liveAt: Date?
    var pricedOn = ""
    var twelve = 0.0
    var year = 0.0
    var provisioned = 0.0
    var announced = 0.0
    var yieldOnCost: Double?
    var months: [WalletMonth] = []
    var received: [WalletPaid] = []
    var pending: [WalletPaid] = []
    var ahead: [WalletPaid] = []
    var classes: [WalletClass] = []
    var trades: [WalletTrade] = []
    var soldOut = 0

    init() {}

    init(_ o: [String: Any]) {
        total = WalletJSON.dbl(o["total"])
        cost = WalletJSON.dbl(o["cost"])
        gain = WalletJSON.dbl(o["gain"])
        gainPct = WalletJSON.num(o["gainPct"])
        realized = WalletJSON.dbl(o["realized"])
        realizedYear = WalletJSON.dbl(o["realizedYear"])
        day = WalletJSON.num(o["day"])
        dayPct = WalletJSON.num(o["dayPct"])
        liveAt = WalletJSON.num(o["liveAt"]).map { Date(timeIntervalSince1970: $0) }
        pricedOn = WalletJSON.str(o["pricedOn"])
        let income = WalletJSON.dict(o["income"])
        twelve = WalletJSON.dbl(income["twelve"])
        year = WalletJSON.dbl(income["year"])
        provisioned = WalletJSON.dbl(income["provisioned"])
        announced = WalletJSON.dbl(income["announced"])
        yieldOnCost = WalletJSON.num(income["yieldOnCost"])
        months = WalletJSON.list(income["months"]).map { m in
            WalletMonth(id: WalletJSON.str(m["month"]), total: WalletJSON.dbl(m["total"]),
                        items: WalletJSON.list(m["items"]).map { WalletMonth.Item(id: WalletJSON.str($0["ticker"]), value: WalletJSON.dbl($0["value"])) })
        }
        received = WalletJSON.list(income["received"]).map(WalletPaid.init)
        pending = WalletJSON.list(income["pending"]).map(WalletPaid.init)
        ahead = WalletJSON.list(income["ahead"]).map(WalletPaid.init)
        classes = WalletJSON.list(o["classes"]).map(WalletClass.init)
        trades = WalletJSON.list(o["trades"]).map(WalletTrade.init)
        soldOut = (o["soldOut"] as? [Any])?.count ?? 0
    }

    var assets: [WalletAsset] { classes.flatMap(\.assets) }

    func asset(_ ticker: String) -> WalletAsset? { assets.first { $0.ticker == ticker } }
}

// MARK: - The store

/// What the consolidator shows, and what it does, through the engine.
@MainActor
@Observable
final class AXYONWallet {
    static let shared = AXYONWallet()

    private(set) var overview: WalletOverview?
    /// What its numbers rest on is being read (closes, the funds' and companies' dividends).
    private(set) var loading = false
    /// The session's quotes are being read.
    private(set) var quoting = false
    private(set) var problem: String?
    /// When the screen last came back from the engine.
    private(set) var updated: Date?

    private func engine(_ tool: String, _ input: [String: Any] = [:]) async throws -> [String: Any] {
        let body = (try? JSONSerialization.data(withJSONObject: input)) ?? Data("{}".utf8)
        let answer = await HermesPowers.call(tool, input: body)
        let object = (try? JSONSerialization.jsonObject(with: answer)) as? [String: Any] ?? [:]
        guard object["ok"] as? Bool == true else {
            throw AXYONInvestError.said(object["text"] as? String ?? String(localized: "The superpowers did not answer."))
        }
        return object["data"] as? [String: Any] ?? [:]
    }

    private func take(_ object: [String: Any]) {
        overview = WalletOverview(object)
        updated = .now
        problem = nil
    }

    /// The screen, after what is due is read.
    func load() async {
        guard !loading else { return }
        loading = true
        defer { loading = false }
        do {
            take(try await engine("invest_wallet"))
        } catch {
            problem = AXYONInvest.describe(error)
        }
    }

    /// The session's quotes again (the engine reads them at most every two minutes, only while B3 trades).
    func quotes() async {
        guard !loading, !quoting else { return }
        quoting = true
        defer { quoting = false }
        if let object = try? await engine("invest_quotes") {
            take(object)
        }
    }

    /// A lançamento added; the engine's reason in words when it refuses.
    func add(ticker: String, sell: Bool, date: Date, quantity: String, price: String, fees: String, opening: Bool) async throws {
        var day = Calendar(identifier: .gregorian)
        day.timeZone = .current
        let parts = day.dateComponents([.year, .month, .day], from: date)
        let iso = String(format: "%04d-%02d-%02d", parts.year ?? 2000, parts.month ?? 1, parts.day ?? 1)
        var input: [String: Any] = ["do": "add", "ticker": ticker, "kind": sell ? "sell" : "buy", "date": iso,
                                    "quantity": quantity, "price": price, "opening": opening]
        if !fees.trimmingCharacters(in: .whitespaces).isEmpty { input["fees"] = fees }
        take(try await engine("invest_trade", input))
    }

    func remove(_ trade: WalletTrade) async throws {
        take(try await engine("invest_trade", ["do": "remove", "id": trade.id]))
    }
}

// MARK: - Numbers as written in Brazil

enum WalletNumber {
    /// "1.234,56", "9,20" or "9.20": a comma makes the comma the decimal mark.
    static func parse(_ text: String) -> Double? {
        var t = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "R$", with: "").replacingOccurrences(of: " ", with: "")
        if t.contains(",") {
            t = t.replacingOccurrences(of: ".", with: "").replacingOccurrences(of: ",", with: ".")
        }
        return Double(t)
    }

    /// "204", "0,5": a quantity, its fraction only when it has one.
    static func quantity(_ value: Double) -> String {
        value.formatted(.number.precision(.fractionLength(0...6)).locale(Locale(identifier: "pt_BR")))
    }

    /// "12,3%".
    static func percent(_ value: Double, places: Int = 1) -> String {
        value.formatted(.number.precision(.fractionLength(places)).locale(Locale(identifier: "pt_BR"))) + "%"
    }

    /// "+R$ 12,30" / "−R$ 4,00".
    static func signed(_ value: Double) -> String { AXYONMoney.signed(value) }

    /// A unit price as precise as it was given ("R$ 0,0831").
    static func price(_ value: Double) -> String {
        "R$ " + value.formatted(.number.precision(.fractionLength(2...4)).locale(Locale(identifier: "pt_BR")))
    }

    /// "+R$ 120,00 · 8,4%": a result and, when there is a cost to measure it on, its percentage.
    static func result(_ gain: Double, _ pct: Double?) -> String {
        guard let pct else { return signed(gain) }
        return signed(gain) + " · " + percent(pct)
    }

    /// "12,3% · meta 15,0%": a share of the portfolio and the user's target for it.
    static func share(_ weight: Double, target: Double?) -> String {
        guard let target else { return percent(weight) }
        let goal = percent(target)
        return percent(weight) + " · " + String(localized: "target \(goal)")
    }
}
