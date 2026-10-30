import AVFoundation
import Foundation
import Observation
import PDFKit
import SwiftUI
import UIKit
import UserNotifications

// AXYON's investing on this iPhone (the build with Hermes inside): the portfolio and its dividends as rings, the
// committee of five minds, the reader of the assets' documents, the user's theses, the week in three minutes and the
// learning (AXYONInvestViews.swift draws them). Every number is the engine's (windows/powers, coucou_powers::invest,
// through HermesPowers); the brain only reads and votes (axyon_hermes/invest.py), citing where each point comes from.
// Nothing is bought or sold here, and the decision is always the user's.

// MARK: - What the engine says

/// The engine's JSON, read without failing on a missing field.
private enum InvestJSON {
    static func num(_ value: Any?) -> Double? {
        (value as? NSNumber)?.doubleValue
    }

    static func int(_ value: Any?) -> Int { Int(num(value) ?? 0) }
    static func str(_ value: Any?) -> String { value as? String ?? "" }
    static func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }
    static func dict(_ value: Any?) -> [String: Any] { value as? [String: Any] ?? [:] }
    static func strings(_ value: Any?) -> [String] { value as? [String] ?? [] }
}

/// A slice of a ring: a class of the portfolio, an asset, a month's dividends of an asset.
struct InvestSlice: Identifiable, Equatable {
    let id: String
    let label: String
    let value: Double
    var weight: Double = 0
    var target: Double? = nil
    var detail: String = ""
}

/// The dividends paid in a month, asset by asset.
struct InvestMonth: Identifiable, Equatable {
    let id: String
    let total: Double
    let items: [InvestSlice]

    private var date: Date? {
        let parts = id.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 2 else { return nil }
        return Calendar.current.date(from: DateComponents(year: parts[0], month: parts[1], day: 1))
    }

    /// "Out", as the carousel shows it.
    var short: String { date?.formatted(.dateTime.month(.abbreviated)).capitalized ?? id }
    /// "Outubro de 2026".
    var long: String { date?.formatted(.dateTime.month(.wide).year()).capitalized ?? id }
}

/// A document the reader read, with the brain's digest when there is one.
struct InvestDoc: Identifiable, Equatable {
    struct Figure: Identifiable, Equatable {
        let id: String
        let shown: String
        let page: Int
    }

    struct Point: Identifiable, Equatable {
        let id: Int
        let text: String
        let page: Int?
    }

    let id: String
    let ticker: String
    let kind: String
    let title: String
    let headline: String
    let delivered: String
    let url: String
    let pages: Int
    let figures: [Figure]
    let changes: [String]
    let points: [Point]
    /// better, worse, same or mixed; "" before the brain read it.
    let tone: String
    let thesisRisk: Bool
    let problem: String

    init(_ o: [String: Any]) {
        id = InvestJSON.str(o["key"])
        ticker = InvestJSON.str(o["ticker"])
        kind = InvestJSON.str(o["kind"])
        title = InvestJSON.str(o["title"])
        headline = InvestJSON.str(o["headline"])
        delivered = InvestJSON.str(o["delivered"])
        url = InvestJSON.str(o["url"])
        pages = InvestJSON.int(o["pages"])
        figures = InvestJSON.list(o["figures"]).map {
            Figure(id: InvestJSON.str($0["label"]), shown: InvestJSON.str($0["shown"]), page: InvestJSON.int($0["page"]))
        }
        changes = InvestJSON.strings(o["changes"])
        let digest = InvestJSON.dict(o["digest"])
        points = InvestJSON.list(digest["points"]).enumerated().map { index, p in
            Point(id: index, text: InvestJSON.str(p["text"]), page: InvestJSON.num(p["page"]).map { Int($0) })
        }
        tone = InvestJSON.str(digest["tone"])
        thesisRisk = digest["thesisRisk"] as? Bool ?? false
        problem = InvestJSON.str(o["problem"])
    }
}

/// A condition of a thesis, now.
struct InvestCheck: Identifiable, Equatable {
    let id: Int
    let label: String
    let now: Double?
    /// nil: not enough data to tell yet.
    let holds: Bool?
    let source: String
}

struct InvestThesis: Identifiable, Equatable {
    let id: String
    let text: String
    let written: String
    let ok: Bool
    let broken: Int
    let checks: [InvestCheck]
    let risks: [String]

    init(_ o: [String: Any]) {
        id = InvestJSON.str(o["ticker"])
        text = InvestJSON.str(o["text"])
        written = InvestJSON.str(o["written"])
        ok = o["ok"] as? Bool ?? true
        broken = InvestJSON.int(o["broken"])
        checks = InvestJSON.list(o["checks"]).enumerated().map { index, c in
            InvestCheck(id: index, label: InvestJSON.str(c["label"]), now: InvestJSON.num(c["now"]),
                        holds: c["holds"] as? Bool, source: InvestJSON.str(c["source"]))
        }
        risks = InvestJSON.strings(o["risks"])
    }
}

/// One of the committee's five minds, and how it voted.
struct InvestMind: Identifiable, Equatable {
    struct Point: Identifiable, Equatable {
        let id: Int
        let text: String
        let source: String
    }

    let id: String
    let vote: String
    let confidence: Int
    let veto: Bool
    let points: [Point]

    /// The archetype, in the user's language (never a real person).
    var name: String { InvestMind.name(id) }

    static func name(_ id: String) -> String {
        switch id {
        case "valuation": String(localized: "Valuation analyst")
        case "legal": String(localized: "Legal and governance")
        case "forensic": String(localized: "Forensic accountant")
        case "risk": String(localized: "Risk manager")
        case "macro": String(localized: "Macro strategist")
        default: id
        }
    }

    var symbol: String {
        switch id {
        case "valuation": "chart.bar.xaxis"
        case "legal": "building.columns"
        case "forensic": "magnifyingglass"
        case "risk": "shield.lefthalf.filled"
        default: "globe.americas"
        }
    }

    /// What it watches, in a line.
    var role: String {
        switch id {
        case "valuation": String(localized: "Price against value: P/VP, yield, the ceilings")
        case "legal": String(localized: "Rules, material facts, lawsuits, the manager's conflicts, dilution")
        case "forensic": String(localized: "Real profit, payouts above earnings, the auditor's remarks")
        case "risk": String(localized: "Concentration, vacancy, credit, what could go wrong")
        default: String(localized: "Rates, inflation, against the Tesouro IPCA+")
        }
    }
}

/// The committee's verdict on a proposal: the score, the vetoes, the minds and the chair's synthesis.
struct InvestVerdict: Identifiable, Equatable {
    let id: String
    let ticker: String
    let proposal: String
    let date: String
    let favor: Int
    let against: Int
    let neutral: Int
    let vetoedBy: [String]
    let minds: [InvestMind]
    let question: String
    let dissent: String
    let wouldChange: String
    let fit: String
    /// asked, document or thesis: why the committee met.
    let reason: String

    init(_ o: [String: Any]) {
        id = InvestJSON.str(o["id"])
        ticker = InvestJSON.str(o["ticker"])
        proposal = InvestJSON.str(o["proposal"])
        date = InvestJSON.str(o["date"])
        let score = InvestJSON.dict(o["score"])
        favor = InvestJSON.int(score["for"])
        against = InvestJSON.int(score["against"])
        neutral = InvestJSON.int(score["neutral"])
        vetoedBy = InvestJSON.strings(o["vetoedBy"])
        minds = InvestJSON.list(o["minds"]).map { m in
            InvestMind(id: InvestJSON.str(m["id"]), vote: InvestJSON.str(m["vote"]),
                       confidence: InvestJSON.int(m["confidence"]), veto: m["veto"] as? Bool ?? false,
                       points: InvestJSON.list(m["points"]).enumerated().map { index, p in
                           InvestMind.Point(id: index, text: InvestJSON.str(p["text"]), source: InvestJSON.str(p["source"]))
                       })
        }
        question = InvestJSON.str(o["question"])
        dissent = InvestJSON.str(o["dissent"])
        wouldChange = InvestJSON.str(o["wouldChange"])
        fit = InvestJSON.str(o["fit"])
        reason = InvestJSON.str(o["reason"])
    }

    /// "3 × 2".
    var score: String { "\(favor) × \(against)" }
    var vetoed: Bool { !vetoedBy.isEmpty }
}

/// A meeting the committee should hold by itself: a new document that matters, or a thesis that broke.
struct InvestMeeting: Identifiable, Equatable {
    let id: String
    let reason: String
    let document: String
    let about: String
}

/// The training portfolio, with pretend money.
struct InvestPaper: Equatable {
    struct Position: Identifiable, Equatable {
        let id: String
        let quantity: Double
        let average: Double
        let value: Double
    }

    var started = ""
    var start = 0.0
    var cash = 0.0
    var value = 0.0
    var returnPct = 0.0
    var realPct: Double?
    var cdiPct: Double?
    var positions: [Position] = []

    init() {}

    init(_ o: [String: Any]) {
        started = InvestJSON.str(o["started"])
        start = InvestJSON.num(o["start"]) ?? 0
        cash = InvestJSON.num(o["cash"]) ?? 0
        value = InvestJSON.num(o["value"]) ?? 0
        returnPct = InvestJSON.num(o["returnPct"]) ?? 0
        realPct = InvestJSON.num(o["realPct"])
        cdiPct = InvestJSON.num(o["cdiPct"])
        positions = InvestJSON.list(o["positions"]).map {
            Position(id: InvestJSON.str($0["ticker"]), quantity: InvestJSON.num($0["quantity"]) ?? 0,
                     average: InvestJSON.num($0["average"]) ?? 0, value: InvestJSON.num($0["value"]) ?? 0)
        }
    }
}

/// The investing screen, as the engine computes it.
struct InvestOverview: Equatable {
    var total = 0.0
    var cost = 0.0
    var gainPct: Double?
    var pricedOn = ""
    var byClass: [InvestSlice] = []
    var byAsset: [InvestSlice] = []
    var dividends: [InvestMonth] = []
    var feed: [InvestDoc] = []
    var pending = 0
    var undigested = 0
    var theses: [InvestThesis] = []
    var verdicts: [InvestVerdict] = []
    var meetings: [InvestMeeting] = []
    var apprentice = false
    var cardsDue = 0
    var judged = 0
    var paper = InvestPaper()
    var weeklyTold = false

    init() {}

    init(_ o: [String: Any]) {
        total = InvestJSON.num(o["total"]) ?? 0
        cost = InvestJSON.num(o["cost"]) ?? 0
        gainPct = InvestJSON.num(o["gainPct"])
        pricedOn = InvestJSON.str(o["pricedOn"])
        byClass = InvestJSON.list(o["byClass"]).map {
            let key = InvestJSON.str($0["key"])
            return InvestSlice(id: key, label: AXYONInvest.className(key, InvestJSON.str($0["label"])),
                               value: InvestJSON.num($0["value"]) ?? 0, weight: InvestJSON.num($0["weight"]) ?? 0,
                               target: InvestJSON.num($0["target"]))
        }
        byAsset = InvestJSON.list(o["byAsset"]).map {
            let quantity = InvestJSON.num($0["quantity"]) ?? 0
            let gain = InvestJSON.num($0["gainPct"]).map { " · " + AXYONMoney.percent($0) } ?? ""
            return InvestSlice(id: InvestJSON.str($0["key"]), label: InvestJSON.str($0["label"]),
                               value: InvestJSON.num($0["value"]) ?? 0, weight: InvestJSON.num($0["weight"]) ?? 0,
                               detail: quantity.formatted() + gain)
        }
        dividends = InvestJSON.list(o["dividends"]).map { m in
            let items = InvestJSON.list(m["items"]).map {
                InvestSlice(id: InvestJSON.str($0["key"]), label: InvestJSON.str($0["label"]),
                            value: InvestJSON.num($0["value"]) ?? 0)
            }
            let total = InvestJSON.num(m["total"]) ?? 0
            let weighted = items.map { item in
                var slice = item
                slice.weight = total > 0 ? item.value / total * 100 : 0
                return slice
            }
            return InvestMonth(id: InvestJSON.str(m["month"]), total: total, items: weighted)
        }
        feed = InvestJSON.list(o["feed"]).map(InvestDoc.init)
        pending = InvestJSON.int(o["pending"])
        undigested = InvestJSON.int(o["undigested"])
        theses = InvestJSON.list(o["theses"]).map(InvestThesis.init)
        verdicts = InvestJSON.list(o["verdicts"]).map(InvestVerdict.init)
        meetings = InvestJSON.list(o["meetings"]).map {
            InvestMeeting(id: InvestJSON.str($0["ticker"]), reason: InvestJSON.str($0["reason"]),
                          document: InvestJSON.str($0["document"]), about: InvestJSON.str($0["about"]))
        }
        let learn = InvestJSON.dict(o["learn"])
        apprentice = learn["apprentice"] as? Bool ?? false
        cardsDue = InvestJSON.int(learn["due"])
        judged = InvestJSON.int(learn["judged"])
        paper = InvestPaper(InvestJSON.dict(learn["paper"]))
        weeklyTold = InvestJSON.dict(o["weekly"])["told"] as? Bool ?? false
    }
}

/// A term of the glossary, with its value in the user's own assets.
struct InvestTerm: Identifiable, Equatable {
    let id: String
    let term: String
    let simple: String
    let why: String
    let mine: String?
}

struct InvestCard: Identifiable, Equatable {
    let id: String
    let front: String
    let back: String
}

/// The personal score: by how sure the user said they were, how often they were right.
struct InvestScore: Equatable {
    struct Band: Identifiable, Equatable {
        let id: String
        let judged: Int
        let rightPct: Double
    }

    var decisions = 0
    var judged = 0
    var right = 0
    var bands: [Band] = []
    var averageReturn: Double?
    var averageCdi: Double?
}

/// The learning screen.
struct InvestLearn: Equatable {
    var apprentice = false
    var glossary: [InvestTerm] = []
    var due: [InvestCard] = []
    var cards = 0
    var paper = InvestPaper()
    var score = InvestScore()

    init(_ o: [String: Any]) {
        apprentice = o["apprentice"] as? Bool ?? false
        glossary = InvestJSON.list(o["glossary"]).map {
            InvestTerm(id: InvestJSON.str($0["key"]), term: InvestJSON.str($0["term"]), simple: InvestJSON.str($0["simple"]),
                       why: InvestJSON.str($0["why"]), mine: $0["mine"] as? String)
        }
        let cardsObject = InvestJSON.dict(o["cards"])
        due = InvestJSON.list(cardsObject["due"]).map {
            InvestCard(id: InvestJSON.str($0["id"]), front: InvestJSON.str($0["front"]), back: InvestJSON.str($0["back"]))
        }
        cards = InvestJSON.int(cardsObject["total"])
        paper = InvestPaper(InvestJSON.dict(o["paper"]))
        let s = InvestJSON.dict(o["score"])
        score = InvestScore(decisions: InvestJSON.int(s["decisions"]), judged: InvestJSON.int(s["judged"]),
                            right: InvestJSON.int(s["right"]),
                            bands: InvestJSON.list(s["bands"]).map {
                                InvestScore.Band(id: InvestJSON.str($0["label"]), judged: InvestJSON.int($0["judged"]),
                                                 rightPct: InvestJSON.num($0["rightPct"]) ?? 0)
                            },
                            averageReturn: InvestJSON.num(s["averageReturn"]), averageCdi: InvestJSON.num(s["averageCdi"]))
    }
}

/// The week in three minutes: the script the code made, read aloud here.
struct InvestWeekly: Equatable {
    let week: String
    let title: String
    let text: String
    let seconds: Int
}

enum AXYONInvestError: LocalizedError {
    case said(String)

    var errorDescription: String? {
        switch self {
        case .said(let text): text
        }
    }
}

// MARK: - The store

/// What the investing screens show, and what they do, through the engine (HermesPowers) and, for the brain's part,
/// AXYON Core (HermesEngine).
@MainActor
@Observable
final class AXYONInvest {
    static let shared = AXYONInvest()

    private(set) var overview: InvestOverview?
    private(set) var learn: InvestLearn?
    private(set) var weekly: InvestWeekly?
    private(set) var loading = false
    /// What the reader is doing now, said to the user ("Reading GGRC11's documents…"); nil when it rests.
    private(set) var reading: String?
    /// The asset the committee is meeting on now.
    private(set) var meeting: String?
    private(set) var problem: String?

    @ObservationIgnored private var readerRan: Date?
    @ObservationIgnored private var thinking = false

    /// The brain's language for what it writes.
    nonisolated static var language: String {
        let code = Locale.preferredLanguages.first ?? "pt-BR"
        if code.hasPrefix("pt") { return "Brazilian Portuguese" }
        if code.hasPrefix("es") { return "Spanish" }
        if code.hasPrefix("fr") { return "French" }
        return "English"
    }

    /// A class of the portfolio in the user's language (the engine names them in Portuguese).
    nonisolated static func className(_ key: String, _ given: String) -> String {
        switch key {
        case "acao": String(localized: "Stocks")
        case "fii": "FIIs"
        case "etf": "ETFs"
        case "bdr": "BDRs"
        case "renda_fixa": String(localized: "Fixed income")
        case "tesouro": "Tesouro Direto"
        case "cripto": String(localized: "Crypto")
        case "outro": String(localized: "Other")
        default: given
        }
    }

    /// A document's kind in the user's language.
    nonisolated static func kindName(_ kind: String) -> String {
        switch kind {
        case "material_fact": String(localized: "material fact")
        case "report": String(localized: "manager's report")
        case "release": String(localized: "results release")
        case "assembly": String(localized: "assembly")
        case "offering": String(localized: "share offering")
        case "notice": String(localized: "notice to holders")
        default: String(localized: "document")
        }
    }

    // MARK: The engine

    /// One of the app's own calls of the engine (coucou_powers::invest `app`, windows/powers-ffi): its data, or why not.
    private func engine(_ tool: String, _ input: [String: Any] = [:]) async throws -> Any {
        let body = (try? JSONSerialization.data(withJSONObject: input)) ?? Data("{}".utf8)
        let answer = await HermesPowers.call(tool, input: body)
        let object = (try? JSONSerialization.jsonObject(with: answer)) as? [String: Any] ?? [:]
        guard object["ok"] as? Bool == true else {
            throw AXYONInvestError.said(object["text"] as? String ?? String(localized: "The superpowers did not answer."))
        }
        return object["data"] ?? NSNull()
    }

    nonisolated static func describe(_ error: Error) -> String {
        if let said = error as? AXYONInvestError, case .said(let text) = said { return text }
        return HermesEngine.describe(error)
    }

    // MARK: The screen

    /// The screen from what the engine keeps: at once, nothing read.
    func load() async {
        if let object = try? await engine("invest_view") as? [String: Any] {
            overview = InvestOverview(object)
        }
    }

    /// What is due read first (prices, the funds' documents, the CVM's lists); then, without waiting for them, the
    /// reader's round and the brain's.
    func refresh() async {
        guard !loading else { return }
        loading = true
        defer { loading = false }
        do {
            if let object = try await engine("invest_refresh") as? [String: Any] {
                overview = InvestOverview(object)
                problem = nil
            }
        } catch {
            problem = Self.describe(error)
        }
        Task { await wake() }
    }

    /// The app came back: the reader looks for new documents (at most every half hour), and the brain reads them.
    func wake() async {
        await read()
        await think()
    }

    // MARK: The reader

    /// The reader's round: the documents due (a material fact first, then the reports) downloaded by the engine, a
    /// PDF read here (PDFKit; a scanned page by the iPhone's text recognition when `ocr`), and kept with what changed.
    /// At most `most` documents, each download `seconds` at most, and once every half hour unless `force`.
    func read(most: Int = 4, ocr: Bool = true, force: Bool = false, seconds: Int = 25) async {
        guard reading == nil else { return }
        if !force, let last = readerRan, Date.now.timeIntervalSince(last) < 30 * 60 { return }
        readerRan = .now
        reading = String(localized: "Looking for new documents…")
        defer { reading = nil }
        let due = (try? await engine("invest_docs", ["do": "due", "most": most])) as? [[String: Any]] ?? []
        for pending in due {
            let ticker = pending["ticker"] as? String ?? ""
            reading = String(localized: "Reading \(ticker)'s documents…")
            do {
                let fetched = try await engine("invest_docs", ["do": "fetch", "pending": pending, "seconds": seconds])
                    as? [String: Any] ?? [:]
                var pages = fetched["pages"] as? [String] ?? []
                if let path = fetched["pdf"] as? String {
                    let file = URL(fileURLWithPath: path)
                    pages = await AXYONReader.pages(of: file, ocr: ocr)
                    try? FileManager.default.removeItem(at: file)
                }
                guard pages.contains(where: { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) else {
                    throw AXYONInvestError.said(String(localized: "No text could be read from it."))
                }
                _ = try await engine("invest_docs", ["do": "ingest", "pending": pending, "pages": pages])
            } catch {
                _ = try? await engine("invest_docs", ["do": "failed", "pending": pending, "problem": Self.describe(error)])
            }
        }
        if !due.isEmpty { await load() }
    }

    /// The brain's part, when AXYON Core is up: the digests of the documents read, then a meeting the committee is
    /// due (a new document that matters, a thesis that broke), told as a notification.
    func think() async {
        let hermes = HermesEngine.shared
        guard hermes.canChat, !thinking else { return }
        thinking = true
        defer { thinking = false }
        let waiting = (try? await engine("invest_docs", ["do": "undigested", "most": 3])) as? [[String: Any]] ?? []
        for document in waiting {
            guard let key = document["key"] as? String else { continue }
            let ticker = document["ticker"] as? String ?? ""
            reading = String(localized: "AXYON Core is reading \(ticker)…")
            _ = try? await hermes.investDigest(key, language: Self.language)
        }
        reading = nil
        let due = (try? await engine("invest_committee", ["do": "meetings", "most": 1])) as? [[String: Any]] ?? []
        for planned in due {
            let ticker = planned["ticker"] as? String ?? ""
            meeting = ticker
            let body: [String: Any] = ["proposal": planned["about"] as? String ?? ticker, "ticker": ticker,
                                       "reason": planned["reason"] as? String ?? "document",
                                       "document": planned["document"] as? String ?? "", "language": Self.language]
            if let made = try? await hermes.investCommittee(body) {
                await AXYONAlerts.tell(verdict: InvestVerdict(made))
            }
            meeting = nil
        }
        if !waiting.isEmpty || !due.isEmpty { await load() }
    }

    /// A document read, with its digest and figures.
    func document(_ key: String) async -> InvestDoc? {
        guard let object = try? await engine("invest_docs", ["do": "document", "key": key]) as? [String: Any],
              object["key"] != nil else { return nil }
        return InvestDoc(object)
    }

    // MARK: The committee

    /// The committee on the user's proposal ("comprar 100 cotas de GGRC11"): the verdict, kept.
    func ask(_ proposal: String) async throws -> InvestVerdict {
        let hermes = HermesEngine.shared
        guard hermes.canChat else {
            throw AXYONInvestError.said(String(localized: "The committee needs AXYON Core running with a brain: see Settings."))
        }
        let text = proposal.trimmingCharacters(in: .whitespacesAndNewlines)
        meeting = text
        defer { meeting = nil }
        do {
            let made = try await hermes.investCommittee(["proposal": text, "reason": "asked", "language": Self.language])
            await load()
            return InvestVerdict(made)
        } catch {
            throw AXYONInvestError.said(Self.describe(error))
        }
    }

    // MARK: The theses

    func writeThesis(_ ticker: String, _ text: String) async throws {
        _ = try await engine("invest_thesis", ["do": "write", "ticker": ticker, "text": text])
        await load()
    }

    func removeThesis(_ ticker: String) async {
        _ = try? await engine("invest_thesis", ["do": "remove", "ticker": ticker])
        await load()
    }

    // MARK: Learning

    func loadLearn() async {
        if let object = try? await engine("invest_learn", ["do": "view"]) as? [String: Any] {
            learn = InvestLearn(object)
        }
    }

    private func learning(_ input: [String: Any]) async throws {
        if let object = try await engine("invest_learn", input) as? [String: Any] {
            learn = InvestLearn(object)
        }
        await load()
    }

    func setApprentice(_ on: Bool) async {
        _ = try? await learning(["do": "apprentice", "on": on])
    }

    func makeCards() async {
        _ = try? await learning(["do": "cards"])
    }

    /// A card graded 0 (forgot) to 5 (easy).
    func grade(_ card: InvestCard, _ grade: Int) async {
        _ = try? await learning(["do": "grade", "id": card.id, "grade": grade])
    }

    /// A pretend trade at the real price.
    func paper(_ ticker: String, quantity: Double, sell: Bool) async throws {
        try await learning(["do": "paper", "ticker": ticker, "quantity": quantity, "action": sell ? "sell" : "buy"])
    }

    func resetPaper(_ amount: Double?) async {
        var input: [String: Any] = ["do": "reset"]
        if let amount { input["amount"] = amount }
        _ = try? await learning(input)
    }

    // MARK: The week

    func loadWeekly() async {
        guard let object = try? await engine("invest_weekly", ["language": Locale.preferredLanguages.first ?? "pt-BR"])
                as? [String: Any] else { return }
        weekly = InvestWeekly(week: InvestJSON.str(object["week"]), title: InvestJSON.str(object["title"]),
                              text: InvestJSON.str(object["text"]), seconds: InvestJSON.int(object["seconds"]))
    }

    /// The week's summary was heard or sent.
    func weeklyTold() async {
        _ = try? await engine("invest_weekly", ["do": "told"])
        await load()
    }
}

// MARK: - Reading a PDF

enum AXYONReader {
    /// A PDF's pages as text (PDFKit), at most 60; a scanned page read by the iPhone's text recognition when `ocr` (the
    /// first 20 such pages). Nothing leaves the iPhone.
    static func pages(of file: URL, ocr: Bool) async -> [String] {
        await Task.detached(priority: .utility) {
            guard let document = PDFDocument(url: file) else { return [String]() }
            var pages: [String] = []
            var scans = 0
            for index in 0..<min(document.pageCount, 60) {
                guard let page = document.page(at: index) else {
                    pages.append("")
                    continue
                }
                var text = page.string ?? ""
                if ocr, text.trimmingCharacters(in: .whitespacesAndNewlines).count < 20, scans < 20 {
                    scans += 1
                    text = AXYONInbox.recognize(page)
                }
                pages.append(text)
            }
            return pages
        }.value
    }
}

// MARK: - The week aloud

/// The week's summary as a sound file in the iPhone's own voice (AAC, .m4a), to send: WhatsApp takes it as audio.
enum AXYONWeeklyAudio {
    static let remindKey = "axyon.weekly.remind"

    static func render(_ text: String, language: String) async throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(String(localized: "AXYON-week") + ".m4a")
        try? FileManager.default.removeItem(at: url)
        let recording = WeeklyRecording(url: url)
        return try await withCheckedThrowingContinuation { continuation in
            recording.start(text: text, language: language, continuation: continuation)
        }
    }

    /// WhatsApp with the week's text written, for the user to pick who and send.
    static func whatsApp(_ text: String) -> URL? {
        var parts = URLComponents(string: "whatsapp://send")
        parts?.queryItems = [URLQueryItem(name: "text", value: text)]
        return parts?.url
    }

    /// A reminder every Sunday at 18:00 that the week's summary is ready (on, or off).
    static func remind(_ on: Bool) async {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: ["axyon-weekly"])
        UserDefaults.standard.set(on, forKey: remindKey)
        guard on else { return }
        _ = try? await center.requestAuthorization(options: [.alert, .sound])
        let content = UNMutableNotificationContent()
        content.title = String(localized: "Your week in three minutes")
        content.body = String(localized: "Your portfolio's summary is ready: open AXYON to hear it or send it on WhatsApp.")
        content.sound = .default
        var when = DateComponents()
        when.weekday = 1
        when.hour = 18
        when.minute = 0
        let trigger = UNCalendarNotificationTrigger(dateMatching: when, repeats: true)
        try? await center.add(UNNotificationRequest(identifier: "axyon-weekly", content: content, trigger: trigger))
    }
}

/// The voice written to a file, buffer after buffer, until it says it ended (an empty buffer), or four minutes.
private final class WeeklyRecording: @unchecked Sendable {
    private let lock = NSLock()
    private let url: URL
    private let synthesizer = AVSpeechSynthesizer()
    private var file: AVAudioFile?
    private var failure: Error?
    private var continuation: CheckedContinuation<URL, Error>?

    init(url: URL) {
        self.url = url
    }

    func start(text: String, language: String, continuation: CheckedContinuation<URL, Error>) {
        lock.lock()
        self.continuation = continuation
        lock.unlock()
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: language) ?? AVSpeechSynthesisVoice(language: "pt-BR")
        synthesizer.write(utterance) { [self] buffer in
            take(buffer)
        }
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 240) { [self] in
            finish()
        }
    }

    private func take(_ buffer: AVAudioBuffer) {
        guard let pcm = buffer as? AVAudioPCMBuffer else { return }
        guard pcm.frameLength > 0 else {
            finish()
            return
        }
        lock.lock()
        defer { lock.unlock() }
        guard continuation != nil, failure == nil else { return }
        do {
            if file == nil {
                let settings: [String: Any] = [AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: pcm.format.sampleRate,
                                               AVNumberOfChannelsKey: pcm.format.channelCount, AVEncoderBitRateKey: 48_000]
                file = try AVAudioFile(forWriting: url, settings: settings, commonFormat: pcm.format.commonFormat,
                                       interleaved: pcm.format.isInterleaved)
            }
            try file?.write(from: pcm)
        } catch {
            failure = error
        }
    }

    private func finish() {
        lock.lock()
        let waiting = continuation
        continuation = nil
        let written = file != nil
        // Let go of the file: it is closed and its sound written out.
        file = nil
        let error = failure
        lock.unlock()
        guard let waiting else { return }
        if let error {
            waiting.resume(throwing: error)
        } else if written {
            waiting.resume(returning: url)
        } else {
            waiting.resume(throwing: AXYONInvestError.said(String(localized: "The iPhone's voice could not record the summary.")))
        }
    }
}
