import Charts
import SwiftUI

// Brute force, on the phone's own cores (windows/powers/src/radar.rs and simulate.rs): the market's radar, every
// traded real-estate fund read from the CVM and B3 and ranked from the official figures; and the portfolio's futures,
// tens of thousands of month-by-month paths with the contribution and the proventos bought back in, as a spread
// (a bad future, the middle one, a good one), in today's money.

private enum RadarJSON {
    static func num(_ value: Any?) -> Double? { (value as? NSNumber).map { $0.doubleValue + 0.0 } }
    static func int(_ value: Any?) -> Int { Int(num(value) ?? 0) }
    static func str(_ value: Any?) -> String { value as? String ?? "" }
    static func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }
    static func dict(_ value: Any?) -> [String: Any] { value as? [String: Any] ?? [:] }
}

// MARK: - The market's radar

struct RadarFund: Identifiable, Equatable {
    let id: String
    let name: String
    let kind: String
    let price: Double
    let vp: Double
    let pvp: Double
    let dy12: Double
    let vpChange: Double?
    let holders: Double?
    let holdersChange: Double?
    let volume: Double
    let paidMonths: Int
    let score: Double
    let flags: [String]
    let rank: Int?
    let held: Bool
    let of: Int
    let kindRank: Int?
    let kindOf: Int

    init(_ o: [String: Any]) {
        id = RadarJSON.str(o["ticker"])
        name = RadarJSON.str(o["name"])
        kind = RadarJSON.str(o["kind"])
        price = RadarJSON.num(o["price"]) ?? 0
        vp = RadarJSON.num(o["vp"]) ?? 0
        pvp = RadarJSON.num(o["pvp"]) ?? 0
        dy12 = RadarJSON.num(o["dy12"]) ?? 0
        vpChange = RadarJSON.num(o["vpChange"])
        holders = RadarJSON.num(o["holders"])
        holdersChange = RadarJSON.num(o["holdersChange"])
        volume = RadarJSON.num(o["volume"]) ?? 0
        paidMonths = RadarJSON.int(o["paidMonths"])
        score = RadarJSON.num(o["score"]) ?? 0
        flags = o["flags"] as? [String] ?? []
        rank = RadarJSON.num(o["rank"]).map { Int($0) }
        held = o["held"] as? Bool ?? false
        of = RadarJSON.int(o["of"])
        kindRank = RadarJSON.num(o["kindRank"]).map { Int($0) }
        kindOf = RadarJSON.int(o["kindOf"])
    }

    static func kindTitle(_ kind: String) -> String {
        switch kind {
        case "tijolo": String(localized: "Brick")
        case "papel": String(localized: "Paper")
        case "fof": String(localized: "Fund of funds")
        case "hibrido": String(localized: "Hybrid")
        default: String(localized: "Other")
        }
    }

    static func flagTitle(_ flag: String) -> String {
        switch flag {
        case "trap": String(localized: "yield trap?")
        case "deep": String(localized: "very deep discount")
        default: String(localized: "holders leaving")
        }
    }
}

@MainActor
@Observable
final class AXYONRadar {
    static let shared = AXYONRadar()

    private(set) var funds: [RadarFund] = []
    private(set) var mine: [RadarFund] = []
    private(set) var kinds: [(String, Int)] = []
    private(set) var read = ""
    private(set) var quotesDay = ""
    private(set) var ranked = 0
    private(set) var scannedFunds = 0
    private(set) var scannedMonths = 0
    private(set) var scannedQuotes = 0
    private(set) var tookMs = 0
    private(set) var scanning = false
    private(set) var problem: String?
    var kind = ""

    func load() async {
        await ask(scan: false)
    }

    /// The whole market read again (two years of every fund's reports and the day's quotes), then measured.
    func scan() async {
        guard !scanning else { return }
        scanning = true
        defer { scanning = false }
        await ask(scan: true)
        if problem == nil { Haptics.success() } else { Haptics.error() }
    }

    private func ask(scan: Bool) async {
        var input: [String: Any] = ["most": 50]
        if scan { input["do"] = "scan" }
        if !kind.isEmpty { input["kind"] = kind }
        do {
            let o = try await AXYONInsight.call("invest_radar", input)
            funds = RadarJSON.list(o["funds"]).map(RadarFund.init)
            mine = RadarJSON.list(o["mine"]).map(RadarFund.init)
            kinds = RadarJSON.list(o["kinds"]).map { (RadarJSON.str($0["kind"]), RadarJSON.int($0["count"])) }
            read = RadarJSON.str(o["read"])
            quotesDay = RadarJSON.str(o["quotesDay"])
            ranked = RadarJSON.int(o["ranked"])
            scannedFunds = RadarJSON.int(o["scannedFunds"])
            scannedMonths = RadarJSON.int(o["scannedMonths"])
            scannedQuotes = RadarJSON.int(o["scannedQuotes"])
            tookMs = RadarJSON.int(o["tookMs"])
            problem = nil
        } catch {
            problem = AXYONInvest.describe(error)
        }
    }
}

/// A radar's screen drawn in code: rings, a sweeping beam while the market is read, a blip for each fund found.
struct RadarScope: View {
    let sweeping: Bool
    let blips: [Double]
    var size: CGFloat = 150

    var body: some View {
        let accent = AXYONTheme.current.accent
        TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: !sweeping)) { context in
            let turn = sweeping ? context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 2.4) / 2.4 : 0.12
            Canvas { canvas, area in
                let center = CGPoint(x: area.width / 2, y: area.height / 2)
                let radius = min(area.width, area.height) / 2 - 2
                for ring in 1...4 {
                    let r = radius * CGFloat(ring) / 4
                    canvas.stroke(Path(ellipseIn: CGRect(x: center.x - r, y: center.y - r, width: r * 2, height: r * 2)),
                                  with: .color(accent.opacity(ring == 4 ? 0.55 : 0.22)), lineWidth: ring == 4 ? 1.4 : 0.8)
                }
                for axis in 0..<4 {
                    let angle = CGFloat(axis) * .pi / 4
                    var line = Path()
                    line.move(to: CGPoint(x: center.x - cos(angle) * radius, y: center.y - sin(angle) * radius))
                    line.addLine(to: CGPoint(x: center.x + cos(angle) * radius, y: center.y + sin(angle) * radius))
                    canvas.stroke(line, with: .color(accent.opacity(0.12)), lineWidth: 0.6)
                }
                // The beam: a wedge that fades behind its edge.
                let head = CGFloat(turn) * 2 * .pi
                for step in 0..<28 {
                    let from = head - CGFloat(step + 1) * 0.045
                    var wedge = Path()
                    wedge.move(to: center)
                    wedge.addArc(center: center, radius: radius, startAngle: .radians(from), endAngle: .radians(from + 0.05), clockwise: false)
                    wedge.closeSubpath()
                    canvas.fill(wedge, with: .color(accent.opacity(0.42 * (1 - Double(step) / 28))))
                }
                // Each fund found: brighter just after the beam passed.
                for (index, score) in blips.enumerated() {
                    let angle = CGFloat(index) * 2.399963 + 0.7
                    let distance = radius * CGFloat(0.22 + 0.72 * (1 - min(max(score, 0), 100) / 100))
                    let point = CGPoint(x: center.x + cos(angle) * distance, y: center.y + sin(angle) * distance)
                    var behind = (head - angle).truncatingRemainder(dividingBy: 2 * .pi)
                    if behind < 0 { behind += 2 * .pi }
                    let glow = sweeping ? max(0.25, 1 - Double(behind) / (2 * .pi)) : 0.8
                    let dot = CGRect(x: point.x - 2.6, y: point.y - 2.6, width: 5.2, height: 5.2)
                    canvas.fill(Path(ellipseIn: dot.insetBy(dx: -3, dy: -3)), with: .color(accent.opacity(0.18 * glow)))
                    canvas.fill(Path(ellipseIn: dot), with: .color(Color.white.opacity(glow)))
                }
            }
        }
        .frame(width: size, height: size)
        .background(Circle().fill(RadialGradient(colors: [accent.opacity(0.22), Color.black.opacity(0.35)],
                                                 center: .center, startRadius: 2, endRadius: size / 2)))
        .clipShape(Circle())
        .shadow(color: accent.opacity(0.45), radius: 16)
        .accessibilityHidden(true)
    }
}

struct AXYONRadarView: View {
    private let radar = AXYONRadar.shared
    @State private var opened: RadarFund?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                scope
                if let problem = radar.problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.down)
                }
                if !radar.mine.isEmpty {
                    AXYONHeader(title: String(localized: "Where your funds stand"))
                    ForEach(radar.mine) { fund in
                        Button { opened = fund } label: { mineRow(fund) }
                            .buttonStyle(PressableButtonStyle())
                    }
                }
                if radar.ranked > 0 {
                    AXYONHeader(title: String(localized: "The market, ranked"))
                    kindPicker
                    VStack(spacing: 0) {
                        ForEach(radar.funds) { fund in
                            Button { opened = fund } label: { row(fund) }
                                .buttonStyle(.plain)
                            if fund.id != radar.funds.last?.id {
                                Divider().overlay(Color.white.opacity(0.08)).padding(.leading, 52)
                            }
                        }
                    }
                    .axyonCard(cornerRadius: 22)
                    Text("A ranking from official figures (the CVM's monthly reports and B3's quotes), to start a study from: never a recommendation. The yield on price is estimated from the yield each fund reported on its net asset value.")
                        .font(.caption2)
                        .foregroundStyle(AXYONPalette.faint)
                }
            }
            .padding(20)
        }
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("Market radar")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $opened) { fund in
            NavigationStack { RadarFundView(fund: fund, ranked: radar.ranked) }
                .presentationDetents([.medium, .large])
        }
        .task {
            await radar.load()
            #if targetEnvironment(simulator)
            if radar.ranked == 0, AXYONTestScreen.base == "invest-radar" { await radar.scan() }
            #endif
        }
    }

    private var scope: some View {
        VStack(spacing: 14) {
            RadarScope(sweeping: radar.scanning, blips: radar.funds.prefix(40).map(\.score))
            if radar.scanning {
                Text("Reading every fund on B3: two years of the CVM's reports and the day's quotes…")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
            } else if radar.ranked > 0 {
                Text(String(localized: "\(radar.ranked) funds ranked"))
                    .font(.title3.weight(.bold))
                    .foregroundStyle(.white)
                Text(String(localized: "\(radar.scannedFunds) funds, \(radar.scannedMonths) months of reports and \(radar.scannedQuotes) quotes gone through, measured in \(radar.tookMs) ms on this iPhone. Read on \(investDay(radar.read)), quotes of \(investDay(radar.quotesDay))."))
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
            } else {
                Text("The whole market on your iPhone").font(.title3.weight(.bold)).foregroundStyle(.white)
                Text("AXYON reads every real-estate fund traded on B3, not only yours, from the CVM's official reports, and ranks them by discount, income, stability, holders and liquidity. Best on Wi-Fi: it is tens of megabytes.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
            }
            Button {
                Task { await radar.scan() }
            } label: {
                Label(radar.ranked > 0 ? String(localized: "Sweep again") : String(localized: "Sweep the market"),
                      systemImage: "dot.radiowaves.left.and.right")
                    .font(.headline)
            }
            .coucouGlassButton(prominent: true)
            .disabled(radar.scanning)
        }
        .padding(20)
        .frame(maxWidth: .infinity)
        .axyonCard(cornerRadius: 28)
    }

    private var kindPicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                chip(String(localized: "All"), "", radar.ranked)
                ForEach(radar.kinds, id: \.0) { kind, count in
                    chip(RadarFund.kindTitle(kind), kind, count)
                }
            }
        }
    }

    private func chip(_ title: String, _ kind: String, _ count: Int) -> some View {
        let chosen = radar.kind == kind
        return Button {
            radar.kind = kind
            Task { await radar.load() }
        } label: {
            Text(verbatim: "\(title) · \(count)")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(chosen ? Color.black : Color.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Capsule().fill(chosen ? Color.white : Color.white.opacity(0.1)))
        }
        .buttonStyle(.plain)
    }

    private func mineRow(_ fund: RadarFund) -> some View {
        HStack(spacing: 12) {
            RadarScore(score: fund.score, size: 46)
            VStack(alignment: .leading, spacing: 3) {
                Text(fund.id).font(.headline).foregroundStyle(.white)
                Text(fund.rank.map { String(localized: "#\($0) of \(fund.of) overall") } ?? String(localized: "Not ranked: it barely trades or reports"))
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                if let kindRank = fund.kindRank {
                    Text(String(localized: "#\(kindRank) of \(fund.kindOf) among \(RadarFund.kindTitle(fund.kind).lowercased())"))
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 3) {
                Text(verbatim: "P/VP \(fund.pvp.formatted(.number.precision(.fractionLength(2)).locale(Locale(identifier: "pt_BR"))))")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                Text(verbatim: "DY \(WalletNumber.percent(fund.dy12))")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(14)
        .axyonCard(cornerRadius: 20)
    }

    private func row(_ fund: RadarFund) -> some View {
        HStack(spacing: 10) {
            Text(verbatim: fund.rank.map { String($0) } ?? "–")
                .font(.caption.weight(.bold))
                .monospacedDigit()
                .foregroundStyle(AXYONPalette.dim)
                .frame(width: 28, alignment: .trailing)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(fund.id).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    if fund.held { InvestTag(text: String(localized: "yours"), color: AXYONTheme.current.accent) }
                    ForEach(fund.flags, id: \.self) { flag in
                        InvestTag(text: RadarFund.flagTitle(flag), color: flag == "shrinking" ? InvestPalette.mixed : InvestPalette.against)
                    }
                }
                Text(verbatim: "P/VP \(fund.pvp.formatted(.number.precision(.fractionLength(2)).locale(Locale(identifier: "pt_BR")))) · DY \(WalletNumber.percent(fund.dy12)) · \(RadarFund.kindTitle(fund.kind))")
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(AXYONPalette.dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            RadarScore(score: fund.score, size: 38)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }
}

/// A score in a ring: the fuller and greener, the higher.
struct RadarScore: View {
    let score: Double
    var size: CGFloat = 42

    private var color: Color {
        score >= 65 ? InvestPalette.favor : score >= 45 ? InvestPalette.mixed : InvestPalette.against
    }

    var body: some View {
        ZStack {
            Circle().stroke(Color.white.opacity(0.12), lineWidth: size * 0.1)
            Circle()
                .trim(from: 0, to: max(0.02, min(1, score / 100)))
                .stroke(color, style: StrokeStyle(lineWidth: size * 0.1, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text(verbatim: score > 0 ? String(Int(score.rounded())) : "–")
                .font(.system(size: size * 0.34, weight: .bold, design: .rounded))
                .foregroundStyle(.white)
        }
        .frame(width: size, height: size)
    }
}

/// One fund of the radar, with what each figure means.
struct RadarFundView: View {
    let fund: RadarFund
    let ranked: Int
    @Environment(\.dismiss) private var dismiss

    private func number(_ value: Double, _ places: Int = 2) -> String {
        value.formatted(.number.precision(.fractionLength(places)).locale(Locale(identifier: "pt_BR")))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 14) {
                    RadarScore(score: fund.score, size: 64)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(fund.id).font(.title2.weight(.bold)).foregroundStyle(.white)
                        Text(fund.name).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(2)
                        if let rank = fund.rank {
                            Text(String(localized: "#\(rank) of \(ranked) · \(RadarFund.kindTitle(fund.kind))"))
                                .font(.footnote.weight(.semibold))
                                .foregroundStyle(AXYONTheme.current.accent)
                        }
                    }
                }
                ForEach(fund.flags, id: \.self) { flag in
                    Label(flagText(flag), systemImage: "exclamationmark.triangle.fill")
                        .font(.footnote)
                        .foregroundStyle(flag == "shrinking" ? InvestPalette.mixed : InvestPalette.against)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .axyonCard(cornerRadius: 16, tint: InvestPalette.against)
                }
                line(String(localized: "Price over net asset value (P/VP)"), number(fund.pvp),
                     String(localized: "Each R$ 1,00 of the fund's assets costs R$ \(number(fund.pvp)) on the exchange. Price \(WalletNumber.price(fund.price)), net asset value a share \(WalletNumber.price(fund.vp))."))
                line(String(localized: "A year of yield, on today's price"), WalletNumber.percent(fund.dy12),
                     String(localized: "Estimated from the yields the fund reported to the CVM each month. It paid in \(fund.paidMonths) of the last 12 months."))
                if let change = fund.vpChange {
                    line(String(localized: "Net asset value a share, in 12 months"), AXYONMoney.percent(change),
                         String(localized: "A net asset value that falls while the yield stays high may mean the fund pays out its own capital."))
                }
                if let holders = fund.holders {
                    line(String(localized: "Holders"), WalletNumber.quantity(holders.rounded()),
                         fund.holdersChange.map { String(localized: "\(AXYONMoney.percent($0)) in 12 months.") } ?? "")
                }
                line(String(localized: "Traded on the day"), AXYONMoney.text(fund.volume),
                     String(localized: "Little trading makes a price that says less, and selling harder."))
                Text("The score weighs the discount (25 %), the income against the market (25 %), the stability of the net asset value (20 %), the holders (10 %), the liquidity (10 %) and the regularity of payments (10 %).")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
            .padding(20)
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } }
        }
    }

    private func flagText(_ flag: String) -> String {
        switch flag {
        case "trap": String(localized: "A yield well above the market's on a net asset value that fell more than 8 % in a year: it may be a trap.")
        case "deep": String(localized: "A discount past 35 %: the market sees a problem here. Look into why before anything.")
        default: String(localized: "More than a tenth of its holders left in a year.")
        }
    }

    private func line(_ title: String, _ value: String, _ note: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title).font(.subheadline).foregroundStyle(AXYONPalette.dim)
                Spacer()
                Text(value).font(.headline).monospacedDigit().foregroundStyle(.white)
            }
            if !note.isEmpty {
                Text(note).font(.caption).foregroundStyle(AXYONPalette.faint).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(14)
        .axyonCard(cornerRadius: 18)
    }
}

/// The way to the radar, on the Investments screen.
struct InvestRadarCard: View {
    private let radar = AXYONRadar.shared

    var body: some View {
        NavigationLink {
            AXYONRadarView()
        } label: {
            HStack(spacing: 14) {
                RadarScope(sweeping: false, blips: radar.funds.prefix(18).map(\.score), size: 64)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Market radar").font(.headline).foregroundStyle(.white)
                    Text(radar.ranked > 0 ? String(localized: "\(radar.ranked) funds ranked on this iPhone")
                                          : String(localized: "Every fund on B3, read and ranked on this iPhone"))
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
            }
            .padding(16)
            .axyonCard(cornerRadius: 24)
        }
        .buttonStyle(PressableButtonStyle())
        .task { await radar.load() }
    }
}

// MARK: - The portfolio's futures

struct FutureYear: Identifiable, Equatable {
    let id: Int
    let valueLow: Double
    let valueMid: Double
    let valueHigh: Double
    let incomeLow: Double
    let incomeMid: Double
    let incomeHigh: Double
    let chance: Double?
}

struct FutureShock: Identifiable, Equatable {
    let id: String
    let ticker: String
    let value: Double
    let income: Double
}

struct AXYONFuturesView: View {
    @State private var monthly = "1000"
    @State private var years = 15
    @State private var goal = ""
    @State private var paths = 100_000
    @State private var yearly: [FutureYear] = []
    @State private var shocks: [FutureShock] = []
    @State private var todayValue = 0.0
    @State private var todayIncome = 0.0
    @State private var goalYear: Int?
    @State private var ran = 0
    @State private var cores = 0
    @State private var tookMs = 0
    @State private var steps = 0.0
    @State private var working = false
    @State private var problem: String?
    @State private var income = true

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                inputs
                if let problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill").font(.footnote).foregroundStyle(AXYONPalette.down)
                }
                if !yearly.isEmpty {
                    headline
                    chart
                    if !shocks.isEmpty {
                        AXYONHeader(title: String(localized: "If it happened today"))
                        ForEach(shocks) { shock in shockRow(shock) }
                    }
                    Text("Not a forecast: a spread of futures in today's money (inflation taken out). Assumed: funds' prices keep their purchasing power and swing 14 % a year; shares gain 4 % a year above inflation and swing 26 %; dividends keep their purchasing power and swing 8 %; each contribution and every provento buys more of the portfolio as it is split today. A bad future is one in ten; a good one too.")
                        .font(.caption2)
                        .foregroundStyle(AXYONPalette.faint)
                }
            }
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("The portfolio's futures")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            #if targetEnvironment(simulator)
            if AXYONTestScreen.base == "wallet-futures" {
                goal = "3000"
                await run()
            }
            #endif
        }
    }

    private var inputs: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Each month I put in").font(.subheadline).foregroundStyle(.white)
                Spacer()
                TextField(String(localized: "R$ 1.000"), text: $monthly)
                    .keyboardType(.decimalPad)
                    .multilineTextAlignment(.trailing)
                    .font(.headline)
                    .frame(width: 120)
            }
            HStack {
                Text("The monthly income I want").font(.subheadline).foregroundStyle(.white)
                Spacer()
                TextField(String(localized: "optional"), text: $goal)
                    .keyboardType(.decimalPad)
                    .multilineTextAlignment(.trailing)
                    .font(.headline)
                    .frame(width: 120)
            }
            Stepper(String(localized: "For \(years) years"), value: $years, in: 1...40)
                .font(.subheadline)
            Picker("Futures", selection: $paths) {
                Text(verbatim: "20.000").tag(20_000)
                Text(verbatim: "100.000").tag(100_000)
                Text(verbatim: "200.000").tag(200_000)
            }
            .pickerStyle(.segmented)
            Button {
                Task { await run() }
            } label: {
                HStack {
                    if working { ProgressView().tint(.white) }
                    Label(working ? String(localized: "Running the futures…") : String(localized: "Run the futures"), systemImage: "cpu")
                        .font(.headline)
                }
                .frame(maxWidth: .infinity)
            }
            .coucouGlassButton(prominent: true)
            .disabled(working)
        }
        .padding(16)
        .axyonCard(cornerRadius: 24)
    }

    private var headline: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(String(localized: "\(ran) futures in \(seconds) s, on \(cores) cores"), systemImage: "bolt.fill")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(AXYONTheme.current.accent)
            Text(String(localized: "\(Int(steps / 1_000_000)) million steps of prices and dividends, computed on this iPhone."))
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
            if let last = yearly.last {
                Text(String(localized: "In year \(last.id), in the middle future: \(AXYONMoney.text(last.incomeMid)) a month and \(AXYONMoney.text(last.valueMid)) of patrimony. Today: \(AXYONMoney.text(todayIncome)) a month, \(AXYONMoney.text(todayValue))."))
                    .font(.callout)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let wanted = WalletNumber.parse(goal), wanted > 0 {
                Text(goalYear.map { String(localized: "The income reaches \(AXYONMoney.text(wanted)) a month in year \($0), in the middle future.") }
                     ?? String(localized: "The income does not reach \(AXYONMoney.text(wanted)) a month by year \(years), in the middle future."))
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(goalYear == nil ? InvestPalette.mixed : InvestPalette.favor)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22)
    }

    private var seconds: String {
        (Double(tookMs) / 1000).formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: "pt_BR")))
    }

    private var chart: some View {
        VStack(alignment: .leading, spacing: 10) {
            Picker("Shown", selection: $income) {
                Text("Monthly income").tag(true)
                Text("Patrimony").tag(false)
            }
            .pickerStyle(.segmented)
            Chart {
                ForEach(yearly) { y in
                    AreaMark(x: .value("Year", y.id), yStart: .value("Bad", income ? y.incomeLow : y.valueLow),
                             yEnd: .value("Good", income ? y.incomeHigh : y.valueHigh))
                        .foregroundStyle(AXYONTheme.current.accent.opacity(0.22))
                        .interpolationMethod(.monotone)
                    LineMark(x: .value("Year", y.id), y: .value("Middle", income ? y.incomeMid : y.valueMid))
                        .foregroundStyle(AXYONTheme.current.accent)
                        .lineStyle(StrokeStyle(lineWidth: 2.5, lineCap: .round))
                        .interpolationMethod(.monotone)
                }
                if income, let wanted = WalletNumber.parse(goal), wanted > 0 {
                    RuleMark(y: .value("Goal", wanted))
                        .foregroundStyle(InvestPalette.favor.opacity(0.8))
                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [5, 4]))
                }
            }
            .chartXAxis {
                AxisMarks(values: .automatic(desiredCount: 6)) { _ in
                    AxisGridLine().foregroundStyle(Color.white.opacity(0.06))
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .chartYAxis {
                AxisMarks(position: .leading) { _ in
                    AxisGridLine().foregroundStyle(Color.white.opacity(0.08))
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .frame(height: 220)
            .animation(.spring(duration: 0.5), value: income)
            Text("The band goes from a bad future to a good one; the line is the middle. The years run along the bottom.")
                .font(.caption2)
                .foregroundStyle(AXYONPalette.faint)
            if yearly.contains(where: { $0.chance != nil }) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(yearly.filter { $0.id == 1 || $0.id % 5 == 0 || $0.id == yearly.count }) { y in
                            VStack(spacing: 2) {
                                Text(String(localized: "Year \(y.id)")).font(.caption2).foregroundStyle(AXYONPalette.dim)
                                Text(verbatim: WalletNumber.percent(y.chance ?? 0, places: 0))
                                    .font(.subheadline.weight(.bold))
                                    .foregroundStyle(.white)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.08)))
                        }
                    }
                }
                Text("The chance the income is at or above your goal, by year.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private func shockRow(_ shock: FutureShock) -> some View {
        let title: String = switch shock.id {
        case "prices": String(localized: "Prices fall 30 %")
        case "dividends": String(localized: "Every dividend is cut by 20 %")
        default: String(localized: "\(shock.ticker), your biggest payer, stops paying")
        }
        return VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            Text(String(localized: "Patrimony \(AXYONMoney.text(shock.value)) · income \(AXYONMoney.text(shock.income)) a month"))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(AXYONPalette.dim)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 18)
    }

    private func run() async {
        working = true
        problem = nil
        defer { working = false }
        var input: [String: Any] = ["monthly": WalletNumber.parse(monthly) ?? 0, "years": years, "paths": paths]
        if let wanted = WalletNumber.parse(goal), wanted > 0 { input["goal"] = wanted }
        do {
            let o = try await AXYONInsight.call("invest_simulate", input)
            let list = RadarJSON.list(o["yearly"])
            guard !list.isEmpty, (RadarJSON.num(RadarJSON.dict(o["today"])["value"]) ?? 0) > 0 else {
                yearly = []
                problem = String(localized: "There is no priced position to simulate yet: add your assets in the Wallet.")
                return
            }
            let parsed = list.map { y in
                let value = RadarJSON.dict(y["value"])
                let earned = RadarJSON.dict(y["income"])
                return FutureYear(id: RadarJSON.int(y["year"]),
                                  valueLow: RadarJSON.num(value["low"]) ?? 0, valueMid: RadarJSON.num(value["mid"]) ?? 0,
                                  valueHigh: RadarJSON.num(value["high"]) ?? 0, incomeLow: RadarJSON.num(earned["low"]) ?? 0,
                                  incomeMid: RadarJSON.num(earned["mid"]) ?? 0, incomeHigh: RadarJSON.num(earned["high"]) ?? 0,
                                  chance: RadarJSON.num(y["chance"]))
            }
            let today = RadarJSON.dict(o["today"])
            todayValue = RadarJSON.num(today["value"]) ?? 0
            todayIncome = RadarJSON.num(today["income"]) ?? 0
            goalYear = RadarJSON.num(o["goalYear"]).map { Int($0) }
            ran = RadarJSON.int(o["paths"])
            cores = RadarJSON.int(o["cores"])
            tookMs = RadarJSON.int(o["tookMs"])
            steps = RadarJSON.num(o["steps"]) ?? 0
            shocks = RadarJSON.list(o["shocks"]).map {
                FutureShock(id: RadarJSON.str($0["id"]), ticker: RadarJSON.str($0["ticker"]),
                            value: RadarJSON.num($0["value"]) ?? 0, income: RadarJSON.num($0["income"]) ?? 0)
            }
            withAnimation(.spring(duration: 0.6)) { yearly = parsed }
            Haptics.success()
        } catch {
            problem = AXYONInvest.describe(error)
            Haptics.error()
        }
    }
}
