import Charts
import SwiftUI
import UIKit

/// What helps decide, from what AXYON reads (windows/powers/src/insight.rs): an asset's sheet in its folder, the month's
/// contribution share by share, the year's income tax, and the portfolio's own return against the CDI.

private enum InsightJSON {
    static func num(_ value: Any?) -> Double? { (value as? NSNumber).map { $0.doubleValue + 0.0 } }
    static func str(_ value: Any?) -> String { value as? String ?? "" }
    static func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }
    static func dict(_ value: Any?) -> [String: Any] { value as? [String: Any] ?? [:] }
    static func strings(_ value: Any?) -> [String] { value as? [String] ?? [] }
}

@MainActor
enum AXYONInsight {
    static func call(_ tool: String, _ input: [String: Any]) async throws -> [String: Any] {
        let body = (try? JSONSerialization.data(withJSONObject: input)) ?? Data("{}".utf8)
        let answer = await HermesPowers.call(tool, input: body)
        let object = (try? JSONSerialization.jsonObject(with: answer)) as? [String: Any] ?? [:]
        guard object["ok"] as? Bool == true else {
            throw AXYONInvestError.said(object["text"] as? String ?? String(localized: "The superpowers did not answer."))
        }
        return object["data"] as? [String: Any] ?? [:]
    }
}

// MARK: - An asset's sheet

struct InvestFicha: Equatable {
    struct Indicator: Identifiable, Equatable {
        let id: Int
        let label: String
        let value: String
        let note: String
    }

    struct Signal: Identifiable, Equatable {
        let id: Int
        /// good, watch or bad.
        let tone: String
        let text: String
    }

    struct Month: Identifiable, Equatable {
        let id: String
        let vp: Double?
        let dy: Double?
        let holders: Double?
    }

    struct Dividend: Identifiable, Equatable {
        let id: Int
        let record: String
        let value: Double
    }

    struct Ceiling: Identifiable, Equatable {
        let id: String
        let value: Double
        let below: Double?
    }

    struct Reported: Identifiable, Equatable {
        let id: String
        let label: String
        let points: [String]
        let trend: String
    }

    var indicators: [Indicator] = []
    var signals: [Signal] = []
    var history: [Month] = []
    var dividends: [Dividend] = []
    var ceilings: [Ceiling] = []
    var reports: [Reported] = []
    var monthly: Double?
    var magic: Double?
    var price: Double?
    var quantity = 0.0

    init() {}

    init(_ o: [String: Any]) {
        indicators = InsightJSON.list(o["indicators"]).enumerated().map {
            Indicator(id: $0.offset, label: InsightJSON.str($0.element["label"]), value: InsightJSON.str($0.element["value"]),
                      note: InsightJSON.str($0.element["note"]))
        }
        signals = InsightJSON.list(o["signals"]).enumerated().map {
            Signal(id: $0.offset, tone: InsightJSON.str($0.element["tone"]), text: InsightJSON.str($0.element["text"]))
        }
        history = InsightJSON.list(o["history"]).map {
            Month(id: InsightJSON.str($0["month"]), vp: InsightJSON.num($0["vp"]), dy: InsightJSON.num($0["dy"]), holders: InsightJSON.num($0["holders"]))
        }
        dividends = InsightJSON.list(o["dividends"]).enumerated().map {
            Dividend(id: $0.offset, record: InsightJSON.str($0.element["record"]), value: InsightJSON.num($0.element["value"]) ?? 0)
        }
        ceilings = InsightJSON.list(o["ceilings"]).map {
            Ceiling(id: InsightJSON.str($0["method"]), value: InsightJSON.num($0["value"]) ?? 0, below: InsightJSON.num($0["below"]))
        }
        reports = InsightJSON.list(o["reports"]).map { r in
            Reported(id: InsightJSON.str(r["key"]), label: InsightJSON.str(r["label"]),
                     points: InsightJSON.list(r["points"]).map { InsightJSON.str($0["shown"]) }, trend: InsightJSON.str(r["trend"]))
        }
        let income = InsightJSON.dict(o["income"])
        monthly = InsightJSON.num(income["monthly"])
        magic = InsightJSON.num(income["magic"])
        price = InsightJSON.num(o["price"])
        quantity = InsightJSON.num(o["quantity"]) ?? 0
    }

    var isEmpty: Bool { indicators.isEmpty && signals.isEmpty && dividends.isEmpty }
}

/// The asset's sheet in its folder: what to look at, the figures, a year of dividends, the VP month by month, what
/// its reports said, its ceilings.
struct InvestFichaSection: View {
    let ficha: InvestFicha

    var body: some View {
        if !ficha.isEmpty {
            VStack(alignment: .leading, spacing: 14) {
                AXYONHeader(title: String(localized: "The asset's sheet"))
                if !ficha.signals.isEmpty { signals }
                if ficha.monthly != nil || ficha.magic != nil { income }
                if !ficha.indicators.isEmpty { indicators }
                if ficha.dividends.count > 1 { dividendChart }
                if ficha.history.compactMap(\.vp).count > 2 { vpChart }
                if !ficha.reports.isEmpty { reports }
                if !ficha.ceilings.isEmpty { ceilings }
            }
        }
    }

    private func color(_ tone: String) -> Color {
        switch tone {
        case "good": InvestPalette.favor
        case "bad": InvestPalette.against
        default: InvestPalette.mixed
        }
    }

    private var signals: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(ficha.signals) { s in
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Image(systemName: s.tone == "good" ? "arrow.up.circle.fill" : s.tone == "bad" ? "exclamationmark.triangle.fill" : "eye.fill")
                        .foregroundStyle(color(s.tone))
                    Text(s.text).font(.callout).foregroundStyle(Color.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22)
    }

    private var income: some View {
        HStack(spacing: 12) {
            if let m = ficha.monthly {
                tile(String(localized: "Pays you a month"), AXYONMoney.text(m), "banknote.fill")
            }
            if let magic = ficha.magic {
                tile(String(localized: "Magic number"), String(localized: "\(Int(magic)) shares"), "sparkles",
                     progress: magic > 0 ? min(1, ficha.quantity / magic) : nil)
            }
        }
    }

    private func tile(_ title: String, _ value: String, _ icon: String, progress: Double? = nil) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(title, systemImage: icon).font(.caption).foregroundStyle(AXYONPalette.dim)
            Text(value).font(.headline).monospacedDigit().foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.7)
            if let progress {
                ProgressView(value: progress).tint(AXYONTheme.current.accent)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }

    private var indicators: some View {
        LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
            ForEach(ficha.indicators) { i in
                VStack(alignment: .leading, spacing: 3) {
                    Text(i.label).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
                    Text(i.value).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.7)
                    if !i.note.isEmpty {
                        Text(i.note).font(.caption2).foregroundStyle(AXYONPalette.faint).lineLimit(1).minimumScaleFactor(0.8)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .axyonCard(cornerRadius: 16)
            }
        }
    }

    private var dividendChart: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Dividend a share, data-com by data-com").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            Chart(ficha.dividends) { d in
                BarMark(x: .value("Data-com", "\(d.id) " + investDay(d.record)), y: .value("R$", d.value))
                    .foregroundStyle(LinearGradient(colors: [AXYONTheme.current.accent, AXYONTheme.current.accentDeep.opacity(0.5)],
                                                    startPoint: .top, endPoint: .bottom))
                    .cornerRadius(4)
            }
            .chartXAxis(.hidden)
            .chartYAxis {
                AxisMarks(position: .leading) { _ in
                    AxisGridLine().foregroundStyle(Color.white.opacity(0.08))
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .frame(height: 130)
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private var vpChart: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("VP a share, month by month (CVM)").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            Chart(ficha.history.filter { $0.vp != nil }) { m in
                LineMark(x: .value("Month", m.id), y: .value("VP", m.vp ?? 0))
                    .foregroundStyle(AXYONTheme.current.accent)
                    .interpolationMethod(.catmullRom)
                PointMark(x: .value("Month", m.id), y: .value("VP", m.vp ?? 0))
                    .foregroundStyle(AXYONTheme.current.accent)
                    .symbolSize(18)
                if let p = ficha.price {
                    RuleMark(y: .value("Price", p))
                        .foregroundStyle(Color.white.opacity(0.35))
                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 3]))
                }
            }
            .chartXAxis(.hidden)
            .chartYScale(domain: .automatic(includesZero: false))
            .chartYAxis {
                AxisMarks(position: .leading) { _ in
                    AxisGridLine().foregroundStyle(Color.white.opacity(0.08))
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .frame(height: 130)
            Text("The dashed line is the price: under the VP, a discount.").font(.caption2).foregroundStyle(AXYONPalette.faint)
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private var reports: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("What its reports said, report by report").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            ForEach(ficha.reports) { r in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: r.trend == "up" ? "arrow.up.right" : r.trend == "down" ? "arrow.down.right" : "arrow.right")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(AXYONPalette.dim)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(r.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                        Text(r.points.joined(separator: " → ")).font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22)
    }

    private var ceilings: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Ceiling prices").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            ForEach(ficha.ceilings) { c in
                HStack {
                    Text(c.id).font(.footnote).foregroundStyle(AXYONPalette.dim)
                    Spacer()
                    Text(WalletNumber.price(c.value)).font(.footnote.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                    if let b = c.below {
                        InvestTag(text: b >= 0 ? String(localized: "\(WalletNumber.percent(b)) below") : String(localized: "\(WalletNumber.percent(-b)) above"),
                                  color: b >= 0 ? InvestPalette.favor : InvestPalette.against)
                    }
                }
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }
}

// MARK: - The month's contribution

struct AporteBuy: Identifiable, Equatable {
    let id: String
    let shares: Double
    let price: Double
    let total: Double
    let income: Double?
    let why: [String]
}

/// How much to put in this month, and where: share by share, with the reasons; what the court told to avoid, apart.
struct WalletAporteSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var amount: String
    @State private var buys: [AporteBuy] = []
    @State private var leftOut: [(String, String)] = []
    @State private var left = 0.0
    @State private var income = 0.0
    @State private var basis = ""
    @State private var working = false
    @State private var problem: String?
    @State private var buying: AporteBuy?
    @FocusState private var focused: Bool
    private let start: String

    init(start: String = "") {
        self.start = start
        _amount = State(initialValue: start)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("How much will you invest this month?").font(.headline).foregroundStyle(.white)
                        HStack(spacing: 10) {
                            TextField(String(localized: "R$ 1.000,00"), text: $amount)
                                .keyboardType(.decimalPad)
                                .focused($focused)
                                .font(.title2.weight(.semibold))
                                .padding(.horizontal, 14)
                                .padding(.vertical, 10)
                                .background(Capsule().fill(Color.white.opacity(0.08)))
                            Button {
                                compute()
                            } label: {
                                if working { ProgressView() } else { Text("See").font(.headline) }
                            }
                            .buttonStyle(WhiteCapsuleButtonStyle())
                            .disabled(working || WalletNumber.parse(amount) == nil)
                        }
                        Text("AXYON spreads it share by share toward your targets, cheap ones a little first, and leaves out what the court told to avoid. Nothing is bought here.")
                            .font(.caption)
                            .foregroundStyle(AXYONPalette.dim)
                    }
                    .padding(16)
                    .axyonCard(cornerRadius: 24)
                    if let problem {
                        Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
                    }
                    if !buys.isEmpty {
                        HStack(spacing: 12) {
                            summary(String(localized: "Buys"), AXYONMoney.text((WalletNumber.parse(amount) ?? 0) - left))
                            summary(String(localized: "Left over"), AXYONMoney.text(left))
                            summary(String(localized: "Income more a month"), "+" + AXYONMoney.text(income))
                        }
                        ForEach(buys) { b in
                            VStack(alignment: .leading, spacing: 8) {
                                HStack(alignment: .firstTextBaseline) {
                                    WalletAvatar(ticker: b.id, color: AXYONTheme.current.accent, size: 36)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(verbatim: "\(WalletNumber.quantity(b.shares)) \(b.id)").font(.headline).foregroundStyle(.white)
                                        Text("at \(WalletNumber.price(b.price))").font(.caption).foregroundStyle(AXYONPalette.dim)
                                    }
                                    Spacer()
                                    Text(AXYONMoney.text(b.total)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                                }
                                ForEach(b.why, id: \.self) { w in
                                    Label(w, systemImage: "checkmark.circle").font(.caption).foregroundStyle(AXYONPalette.dim)
                                }
                                Button { buying = b } label: {
                                    Label("Record this purchase", systemImage: "plus.circle").font(.footnote.weight(.semibold))
                                }
                                .buttonStyle(.borderless)
                            }
                            .padding(16)
                            .axyonCard(cornerRadius: 22)
                        }
                        if !basis.isEmpty {
                            Text("Targets used: \(basis).").font(.caption2).foregroundStyle(AXYONPalette.faint)
                        }
                    }
                    if !leftOut.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Left out").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                            ForEach(leftOut, id: \.0) { item in
                                Label(item.0 + ": " + item.1, systemImage: "hand.raised.fill").font(.caption).foregroundStyle(InvestPalette.mixed)
                            }
                        }
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .axyonCard(cornerRadius: 22, tint: InvestPalette.mixed)
                    }
                }
                .padding(20)
            }
            .scrollDismissesKeyboard(.interactively)
            .background { InvestBackdrop() }
            .navigationTitle(String(localized: "Where to invest"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(item: $buying) { b in
                WalletTradeSheet(wallet: AXYONWallet.shared, ticker: b.id)
            }
        }
        .onAppear {
            if start.isEmpty { focused = true } else { compute() }
        }
    }

    private func summary(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.caption2).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
            Text(value).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.7)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 16)
    }

    private func compute() {
        guard let value = WalletNumber.parse(amount) else { return }
        focused = false
        working = true
        problem = nil
        Task {
            defer { working = false }
            do {
                let o = try await AXYONInsight.call("invest_aporte", ["amount": value])
                buys = InsightJSON.list(o["buys"]).map {
                    AporteBuy(id: InsightJSON.str($0["ticker"]), shares: InsightJSON.num($0["shares"]) ?? 0, price: InsightJSON.num($0["price"]) ?? 0,
                              total: InsightJSON.num($0["total"]) ?? 0, income: InsightJSON.num($0["income"]), why: InsightJSON.strings($0["why"]))
                }
                leftOut = InsightJSON.list(o["leftOut"]).map { (InsightJSON.str($0["ticker"]), InsightJSON.str($0["reason"])) }
                left = InsightJSON.num(o["left"]) ?? 0
                income = InsightJSON.num(o["income"]) ?? 0
                basis = InsightJSON.str(o["basis"])
                if buys.isEmpty { problem = String(localized: "The amount does not buy a whole share of any asset.") }
                Haptics.success()
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
    }
}

// MARK: - The year's income tax

struct IRPFGood: Identifiable, Equatable {
    let id: String
    let group: String
    let code: String
    let cnpj: String
    let before: Double
    let after: Double
    let text: String
    let fromLancamentos: Bool
}

struct IRPFIncome: Identifiable, Equatable {
    let id: String
    let cnpj: String
    let name: String
    let value: Double
}

/// The income tax of a year: each line of Bens e Direitos ready to copy, the exempt income by fund, the dividends, the
/// JCP and the sales month by month.
struct WalletIRPFView: View {
    @State private var year = Calendar.current.component(.year, from: .now) - 1
    @State private var goods: [IRPFGood] = []
    @State private var exempt: [IRPFIncome] = []
    @State private var dividends: [IRPFIncome] = []
    @State private var jcp: [IRPFIncome] = []
    @State private var sales: [(String, Double, Double, Double)] = []
    @State private var loading = false
    @State private var problem: String?
    @State private var copied: String?

    private var thisYear: Int { Calendar.current.component(.year, from: .now) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                InvestSegments<Int>(options: [.init(value: thisYear - 1, title: String(thisYear - 1)), .init(value: thisYear, title: String(thisYear))],
                                    selection: $year)
                if loading { ProgressView().frame(maxWidth: .infinity) }
                if let problem { Text(problem).font(.footnote).foregroundStyle(InvestPalette.against) }
                AXYONHeader(title: String(localized: "Bens e Direitos"))
                if goods.isEmpty && !loading {
                    Text("No asset held at the end of \(String(year)).").font(.footnote).foregroundStyle(AXYONPalette.dim)
                }
                ForEach(goods) { g in
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text(g.id).font(.headline).foregroundStyle(.white)
                            if !g.group.isEmpty {
                                InvestTag(text: String(localized: "Group \(g.group) · Code \(g.code)"), color: AXYONTheme.current.accent)
                            }
                            Spacer()
                        }
                        if !g.cnpj.isEmpty {
                            Text("CNPJ \(g.cnpj)").font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                        }
                        HStack {
                            figure(String(localized: "On 31/12/\(String(year - 1))"), g.before)
                            figure(String(localized: "On 31/12/\(String(year))"), g.after)
                        }
                        Text(g.text).font(.footnote).foregroundStyle(Color.white.opacity(0.85)).textSelection(.enabled)
                        HStack {
                            Button { copy(g.text, g.id) } label: {
                                Label(copied == g.id ? String(localized: "Copied") : String(localized: "Copy the text"), systemImage: copied == g.id ? "checkmark" : "doc.on.doc")
                                    .font(.footnote.weight(.semibold))
                            }
                            .buttonStyle(.borderless)
                            if !g.fromLancamentos {
                                Spacer()
                                InvestTag(text: String(localized: "from your sheet: check the year"), color: InvestPalette.mixed)
                            }
                        }
                    }
                    .padding(16)
                    .axyonCard(cornerRadius: 22)
                }
                incomeSection(String(localized: "Exempt income · FII"), exempt)
                incomeSection(String(localized: "Exempt income · dividends"), dividends)
                incomeSection(String(localized: "Exclusive taxation · JCP"), jcp)
                if !sales.isEmpty {
                    AXYONHeader(title: String(localized: "Sales and DARF"))
                    VStack(spacing: 0) {
                        ForEach(sales, id: \.0) { s in
                            HStack {
                                Text(investMonth(s.0)).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                                Spacer()
                                VStack(alignment: .trailing, spacing: 2) {
                                    Text("Result \(AXYONMoney.signed(s.2 + s.3))").font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                                    Text("DARF \(AXYONMoney.text(s.1))").font(.footnote.weight(.semibold)).monospacedDigit()
                                        .foregroundStyle(s.1 > 0 ? InvestPalette.against : InvestPalette.favor)
                                }
                            }
                            .padding(.vertical, 10)
                            .padding(.horizontal, 14)
                        }
                    }
                    .axyonCard(cornerRadius: 20)
                }
                Text("Bens e Direitos at the purchase cost (fees included), not the market value. FII income and dividends are exempt; JCP is taxed at the source. Check against the broker's and B3's reports before declaring.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle(String(localized: "Income tax"))
        .navigationBarTitleDisplayMode(.inline)
        .task(id: year) { await load() }
    }

    private func figure(_ title: String, _ value: Double) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(AXYONPalette.dim)
            Text(AXYONMoney.text(value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func incomeSection(_ title: String, _ list: [IRPFIncome]) -> some View {
        if !list.isEmpty {
            AXYONHeader(title: title)
            VStack(spacing: 0) {
                ForEach(list) { i in
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(i.id).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                            if !i.cnpj.isEmpty {
                                Text("CNPJ \(i.cnpj)").font(.caption2).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                            }
                        }
                        Spacer()
                        Text(AXYONMoney.text(i.value)).font(.footnote.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                        Button { copy("\(i.cnpj) \(i.name) \(AXYONMoney.text(i.value))", "\(title)\(i.id)") } label: {
                            Image(systemName: copied == "\(title)\(i.id)" ? "checkmark" : "doc.on.doc").font(.caption)
                        }
                        .buttonStyle(.borderless)
                    }
                    .padding(.vertical, 10)
                    .padding(.horizontal, 14)
                }
            }
            .axyonCard(cornerRadius: 20)
        }
    }

    private func copy(_ text: String, _ key: String) {
        UIPasteboard.general.string = text
        copied = key
        Haptics.success()
    }

    private func investMonth(_ key: String) -> String {
        let parts = key.split(separator: "-")
        return parts.count == 2 ? "\(parts[1])/\(parts[0])" : key
    }

    private func load() async {
        loading = true
        problem = nil
        defer { loading = false }
        do {
            let o = try await AXYONInsight.call("invest_irpf", ["year": year])
            goods = InsightJSON.list(o["goods"]).map {
                IRPFGood(id: InsightJSON.str($0["ticker"]), group: InsightJSON.str($0["group"]), code: InsightJSON.str($0["code"]),
                         cnpj: InsightJSON.str($0["cnpj"]), before: InsightJSON.num($0["before"]) ?? 0, after: InsightJSON.num($0["after"]) ?? 0,
                         text: InsightJSON.str($0["text"]), fromLancamentos: $0["fromLancamentos"] as? Bool ?? false)
            }
            let income: (Any?) -> [IRPFIncome] = { value in
                InsightJSON.list(value).map {
                    IRPFIncome(id: InsightJSON.str($0["ticker"]), cnpj: InsightJSON.str($0["cnpj"]), name: InsightJSON.str($0["name"]),
                               value: InsightJSON.num($0["value"]) ?? 0)
                }
            }
            exempt = income(o["exempt"])
            dividends = income(o["dividends"])
            jcp = income(o["jcp"])
            sales = InsightJSON.list(o["sales"]).map {
                (InsightJSON.str($0["month"]), InsightJSON.num($0["darf"]) ?? 0, InsightJSON.num($0["sharesResult"]) ?? 0, InsightJSON.num($0["fundsResult"]) ?? 0)
            }
        } catch {
            problem = AXYONInvest.describe(error)
        }
    }
}

// MARK: - On the Carteira

/// Under the Carteira's summary: the return its money made a year against the CDI, where to invest this month, and
/// the year's income tax.
struct WalletToolsRow: View {
    let overview: WalletOverview
    let invest: () -> Void
    let taxes: () -> Void

    var body: some View {
        VStack(spacing: 10) {
            if let rate = overview.returnRate {
                HStack(spacing: 12) {
                    Image(systemName: "chart.line.uptrend.xyaxis")
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(rate >= (overview.cdi ?? 0) ? InvestPalette.favor : InvestPalette.mixed)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Your money made \(WalletNumber.percent(rate)) a year").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                        Text(overview.cdi.map { String(localized: "CDI now \(WalletNumber.percent($0)) a year · buys, sales, proventos and the value now") }
                             ?? String(localized: "Buys, sales, proventos and the value now"))
                            .font(.caption)
                            .foregroundStyle(AXYONPalette.dim)
                    }
                    Spacer(minLength: 0)
                }
                .padding(14)
                .axyonCard(cornerRadius: 20)
            }
            HStack(spacing: 10) {
                Button(action: invest) { tool(String(localized: "Where to invest"), "target") }
                    .buttonStyle(PressableButtonStyle())
                Button(action: taxes) { tool(String(localized: "Income tax"), "doc.text.fill") }
                    .buttonStyle(PressableButtonStyle())
            }
        }
    }

    private func tool(_ title: String, _ icon: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon).foregroundStyle(AXYONTheme.current.accent)
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.8)
            Spacer(minLength: 0)
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
        }
        .padding(14)
        .frame(maxWidth: .infinity)
        .axyonCard(cornerRadius: 20)
    }
}
