import Charts
import SwiftUI

// The Carteira tab: the consolidator (AXYONWallet.swift holds its data). Net worth, the day and the result on top;
// then the assets class by class, the lançamentos month by month, and the proventos (received, to be received,
// announced) with a year of them in bars. In the investing screens' charcoal and glass. The quotes come again by
// themselves every two minutes while the tab is open and B3 trades.

enum WalletSection: Hashable {
    case assets, trades, income
}

struct AXYONWalletHome: View {
    private let wallet = AXYONWallet.shared
    @State private var section = WalletSection.assets
    @State private var adding = false
    @State private var asset: WalletAsset?

    private var overview: WalletOverview { wallet.overview ?? WalletOverview() }

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                WalletSummaryCard(overview: overview, reading: wallet.loading || wallet.quoting, updated: wallet.updated)
                InvestSegments<WalletSection>(options: [.init(value: .assets, title: String(localized: "Assets")),
                                                        .init(value: .trades, title: String(localized: "Lançamentos")),
                                                        .init(value: .income, title: String(localized: "Proventos"))],
                                              selection: $section)
                if let problem = wallet.problem {
                    Text(problem)
                        .font(.footnote)
                        .foregroundStyle(InvestPalette.against)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                switch section {
                case .assets:
                    WalletAssetsList(overview: overview, add: { adding = true }) { asset = $0 }
                case .trades:
                    WalletTradesList(wallet: wallet, trades: overview.trades, add: { adding = true })
                case .income:
                    WalletIncomeView(overview: overview)
                }
                Text("Quotes from Google Finance during B3's session (about 15 minutes late) and B3's close after it. Proventos from FundosNet (funds) and B3 (companies), counted on the shares held at each data-com. Nothing is bought or sold here.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 8)
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
            .padding(.bottom, 28)
        }
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Wallet")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { adding = true } label: {
                    Image(systemName: "plus").font(.body.weight(.bold))
                }
                .accessibilityLabel(Text("New lançamento"))
            }
        }
        .refreshable { await wallet.load() }
        .task {
            await wallet.load()
            // Like the sheet's GOOGLEFINANCE: the quotes again by themselves while the tab is open.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(120))
                guard !Task.isCancelled else { break }
                await wallet.quotes()
            }
        }
        .sheet(isPresented: $adding) { WalletTradeSheet(wallet: wallet) }
        .navigationDestination(item: $asset) { picked in
            WalletAssetView(wallet: wallet, ticker: picked.ticker)
        }
    }
}

// MARK: - The summary

struct WalletSummaryCard: View {
    let overview: WalletOverview
    let reading: Bool
    let updated: Date?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Net worth")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(AXYONPalette.dim)
                Text(AXYONMoney.text(overview.total))
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                    .contentTransition(.numericText())
                    .animation(.snappy, value: overview.total)
                if let day = overview.day {
                    Label {
                        Text("\(WalletNumber.signed(day)) (\(WalletNumber.percent(overview.dayPct ?? 0, places: 2))) today")
                    } icon: {
                        Image(systemName: day >= 0 ? "arrowtriangle.up.fill" : "arrowtriangle.down.fill")
                    }
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
            Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 14) {
                GridRow {
                    metric(String(localized: "Invested"), AXYONMoney.text(overview.cost))
                    metric(String(localized: "Result"), WalletNumber.result(overview.gain, overview.gainPct),
                           color: overview.gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
                GridRow {
                    metric(String(localized: "Proventos in 12 months"), AXYONMoney.text(overview.twelve))
                    metric(String(localized: "To receive"), AXYONMoney.text(overview.provisioned),
                           color: overview.provisioned > 0 ? InvestPalette.favor : .white)
                }
                if overview.realized != 0 {
                    GridRow {
                        metric(String(localized: "Realized in sales"), WalletNumber.signed(overview.realized),
                               color: overview.realized >= 0 ? AXYONPalette.up : AXYONPalette.down)
                        metric(String(localized: "Yield on cost"), overview.yieldOnCost.map { WalletNumber.percent($0, places: 2) } ?? "—")
                    }
                }
            }
            Divider().overlay(Color.white.opacity(0.08))
            quotes
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 26)
    }

    private func metric(_ title: String, _ value: String, color: Color = .white) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
            Text(value)
                .font(.subheadline.weight(.semibold))
                .monospacedDigit()
                .foregroundStyle(color)
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Where the quotes are from, and when.
    private var quotes: some View {
        HStack(spacing: 8) {
            if reading {
                ProgressView().controlSize(.mini).tint(.white)
            } else {
                Circle()
                    .fill(overview.liveAt != nil ? AXYONPalette.up : AXYONPalette.faint)
                    .frame(width: 7, height: 7)
            }
            Group {
                if let at = overview.liveAt {
                    Text("Google Finance · \(at.formatted(date: .omitted, time: .shortened)) · about 15 min late")
                } else if !overview.pricedOn.isEmpty {
                    Text("B3's close of \(investDay(overview.pricedOn))")
                } else {
                    Text("No quote read yet")
                }
            }
            .font(.caption)
            .foregroundStyle(AXYONPalette.dim)
            .lineLimit(1)
            .minimumScaleFactor(0.8)
            Spacer(minLength: 0)
        }
    }
}

// MARK: - The assets

struct WalletAssetsList: View {
    let overview: WalletOverview
    let add: () -> Void
    let open: (WalletAsset) -> Void

    var body: some View {
        VStack(spacing: 22) {
            if overview.classes.isEmpty {
                WalletEmpty(add: add)
            }
            ForEach(overview.classes) { group in
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(group.label).font(.title3.weight(.bold)).foregroundStyle(.white)
                        Spacer(minLength: 8)
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(AXYONMoney.text(group.value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                            Text(classLine(group)).font(.caption2).foregroundStyle(AXYONPalette.faint)
                        }
                    }
                    VStack(spacing: 0) {
                        ForEach(Array(group.assets.enumerated()), id: \.element.id) { index, item in
                            if index > 0 { Divider().overlay(Color.white.opacity(0.07)).padding(.leading, 14) }
                            Button { open(item) } label: { WalletAssetRow(asset: item) }
                                .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 2)
                    .axyonCard(cornerRadius: 22)
                }
            }
        }
    }

    private func classLine(_ group: WalletClass) -> String {
        let weight = WalletNumber.percent(group.weight)
        let share = String(localized: "\(weight) of the portfolio")
        guard let target = group.target else { return share }
        let goal = WalletNumber.percent(target)
        return share + " · " + String(localized: "target \(goal)")
    }
}

struct WalletAssetRow: View {
    let asset: WalletAsset

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(asset.ticker).font(.headline).foregroundStyle(.white).lineLimit(1).fixedSize()
                Text("Qty \(WalletNumber.quantity(asset.quantity)) · avg \(WalletNumber.price(asset.average))")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .lineLimit(1)
                HStack(spacing: 6) {
                    Text(asset.price.map(WalletNumber.price) ?? "—").foregroundStyle(Color.white.opacity(0.8))
                    if let day = asset.dayPct {
                        Text((day >= 0 ? "▲ " : "▼ ") + WalletNumber.percent(abs(day), places: 2))
                            .foregroundStyle(day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                    }
                }
                .font(.caption.weight(.medium))
                .monospacedDigit()
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 4) {
                Text(AXYONMoney.text(asset.value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                Text(WalletNumber.result(asset.gain, asset.gainPct))
                    .font(.caption.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(asset.gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                Text(WalletNumber.share(asset.weight, target: asset.target))
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
                if asset.provisioned > 0 {
                    InvestTag(text: String(localized: "\(AXYONMoney.text(asset.provisioned)) to receive"), color: InvestPalette.favor)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}

/// Nothing held yet: how to begin.
struct WalletEmpty: View {
    let add: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Image(systemName: "briefcase.fill").font(.title2).foregroundStyle(.white)
            Text("Your consolidated portfolio").font(.headline).foregroundStyle(.white)
            Text("Enter your buys and sales: AXYON works out the average price, the result and the proventos of each asset, with the quotes of now. A portfolio from your computer's sheet shows here too.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
            Button(action: add) {
                Label("New lançamento", systemImage: "plus")
            }
            .coucouProminentButton()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .axyonCard(cornerRadius: 22)
    }
}

// MARK: - The lançamentos

struct WalletTradesList: View {
    let wallet: AXYONWallet
    let trades: [WalletTrade]
    let add: () -> Void
    @State private var removing: WalletTrade?
    @State private var problem: String?

    /// A month's lançamentos.
    struct Month: Identifiable {
        let id: String
        let trades: [WalletTrade]
    }

    /// The months, newest first, each with its lançamentos.
    private var months: [Month] {
        var order: [String] = []
        var by: [String: [WalletTrade]] = [:]
        for t in trades {
            if by[t.month] == nil { order.append(t.month) }
            by[t.month, default: []].append(t)
        }
        return order.map { Month(id: $0, trades: by[$0] ?? []) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Button(action: add) {
                Label("New lançamento", systemImage: "plus.circle.fill")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                    .foregroundStyle(.white)
                    .axyonCard(cornerRadius: 18)
            }
            .buttonStyle(.plain)
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
            }
            if trades.isEmpty {
                Text("No lançamento yet. Each buy or sale you enter moves its asset's position and average price, the way a broker's statement does.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
            ForEach(months) { month in
                let list = month.trades
                VStack(alignment: .leading, spacing: 8) {
                    Text(WalletMonth(id: month.id, total: 0, items: []).long)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(AXYONPalette.dim)
                    VStack(spacing: 0) {
                        ForEach(Array(list.enumerated()), id: \.element.id) { index, trade in
                            if index > 0 { Divider().overlay(Color.white.opacity(0.07)).padding(.leading, 14) }
                            WalletTradeRow(trade: trade)
                                .contextMenu {
                                    Button(role: .destructive) { removing = trade } label: {
                                        Label("Delete lançamento", systemImage: "trash")
                                    }
                                }
                        }
                    }
                    .axyonCard(cornerRadius: 20)
                }
            }
            if !trades.isEmpty {
                Text("Touch and hold a lançamento to delete it.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
        }
        .confirmationDialog(String(localized: "Delete this lançamento?"), isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
                            titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                guard let trade = removing else { return }
                Task {
                    do {
                        try await wallet.remove(trade)
                        problem = nil
                    } catch {
                        problem = AXYONInvest.describe(error)
                    }
                }
            }
        } message: {
            if let trade = removing {
                Text("\(trade.ticker) · \(investDay(trade.date)) · \(WalletNumber.quantity(trade.quantity)) × \(WalletNumber.price(trade.price))")
            }
        }
    }
}

struct WalletTradeRow: View {
    let trade: WalletTrade

    private var tag: (String, Color) {
        switch trade.kind {
        case "sell": (String(localized: "Sale"), InvestPalette.against)
        case "opening": (String(localized: "Opening balance"), InvestPalette.neutral)
        default: (String(localized: "Purchase"), InvestPalette.favor)
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(trade.ticker).font(.headline).foregroundStyle(.white).lineLimit(1).fixedSize()
                    InvestTag(text: tag.0, color: tag.1)
                }
                Text("\(investDay(trade.date)) · \(WalletNumber.quantity(trade.quantity)) × \(WalletNumber.price(trade.price))")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .monospacedDigit()
                if trade.fees > 0 {
                    Text("Costs \(AXYONMoney.text(trade.fees))").font(.caption2).foregroundStyle(AXYONPalette.faint)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 4) {
                Text(AXYONMoney.text(trade.total)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                if let result = trade.result {
                    Text(String(localized: "Result \(WalletNumber.signed(result))"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(result >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}

// MARK: - The proventos

struct WalletIncomeView: View {
    let overview: WalletOverview
    var ticker: String?

    private func only(_ list: [WalletPaid]) -> [WalletPaid] {
        guard let ticker else { return list }
        return list.filter { $0.ticker == ticker }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            if ticker == nil {
                summary
                chart
            }
            section(String(localized: "To receive"), icon: "clock.fill", color: InvestPalette.favor, only(overview.pending),
                    empty: String(localized: "Nothing provisioned: no data-com passed with the payment still ahead."))
            section(String(localized: "Announced, data-com ahead"), icon: "calendar.badge.clock", color: InvestPalette.mixed, only(overview.ahead),
                    empty: nil)
            section(String(localized: "Received"), icon: "checkmark.circle.fill", color: .white, Array(only(overview.received).prefix(24)),
                    empty: String(localized: "No provento received in what AXYON has read yet."))
        }
    }

    private var summary: some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 14) {
            GridRow {
                figure(String(localized: "Received in 12 months"), AXYONMoney.text(overview.twelve))
                figure(String(localized: "This year"), AXYONMoney.text(overview.year))
            }
            GridRow {
                figure(String(localized: "To receive"), AXYONMoney.text(overview.provisioned), color: InvestPalette.favor)
                figure(String(localized: "Yield on cost"), overview.yieldOnCost.map { WalletNumber.percent($0, places: 2) } ?? "—")
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private func figure(_ title: String, _ value: String, color: Color = .white) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
            Text(value).font(.headline).monospacedDigit().foregroundStyle(color).lineLimit(1).minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// A year of proventos received, month by month.
    private var chart: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Month by month").font(.headline).foregroundStyle(.white)
            Chart(overview.months) { month in
                BarMark(x: .value("Month", month.short), y: .value("R$", month.total))
                    .foregroundStyle(InvestPalette.favor.gradient)
                    .cornerRadius(5)
            }
            .chartYAxis {
                AxisMarks(position: .leading) { _ in
                    AxisGridLine().foregroundStyle(Color.white.opacity(0.08))
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .chartXAxis {
                AxisMarks { _ in
                    AxisValueLabel().foregroundStyle(AXYONPalette.faint)
                }
            }
            .frame(height: 170)
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    @ViewBuilder
    private func section(_ title: String, icon: String, color: Color, _ list: [WalletPaid], empty: String?) -> some View {
        if !list.isEmpty || empty != nil {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Label(title, systemImage: icon).font(.headline).foregroundStyle(color)
                    Spacer()
                    if !list.isEmpty {
                        Text(AXYONMoney.text(list.reduce(0) { $0 + $1.net }))
                            .font(.subheadline.weight(.semibold))
                            .monospacedDigit()
                            .foregroundStyle(color)
                    }
                }
                if list.isEmpty, let empty {
                    Text(empty).font(.footnote).foregroundStyle(AXYONPalette.dim)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(list.enumerated()), id: \.element.id) { index, paid in
                            if index > 0 { Divider().overlay(Color.white.opacity(0.07)).padding(.leading, 14) }
                            WalletPaidRow(paid: paid)
                        }
                    }
                    .axyonCard(cornerRadius: 20)
                }
            }
        }
    }
}

struct WalletPaidRow: View {
    let paid: WalletPaid

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(paid.ticker).font(.subheadline.weight(.bold)).foregroundStyle(.white).lineLimit(1).fixedSize()
                    InvestTag(text: kind, color: paid.isJCP ? InvestPalette.mixed : AXYONPalette.dim)
                }
                Text(dates).font(.caption).foregroundStyle(AXYONPalette.dim)
                Text(amount)
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
                    .monospacedDigit()
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 3) {
                Text(AXYONMoney.text(paid.net)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                if paid.isJCP {
                    Text("net of income tax").font(.caption2).foregroundStyle(AXYONPalette.faint)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }

    private var kind: String {
        switch paid.kind {
        case "Juros sobre capital próprio": "JCP"
        case "Rendimento": String(localized: "Income")
        case "Amortização": String(localized: "Amortization")
        default: String(localized: "Dividend")
        }
    }

    /// "204 × R$ 0,0831", and whether it was counted on the shares held now.
    private var amount: String {
        let line = WalletNumber.quantity(paid.quantity) + " × " + WalletNumber.price(paid.perShare)
        guard paid.estimated else { return line }
        return line + " · " + String(localized: "estimated")
    }

    private var dates: String {
        let day = investDay(paid.record)
        let record = String(localized: "data-com \(day)")
        guard let payment = paid.payment else { return record + " · " + String(localized: "payment to be set") }
        let paidOn = investDay(payment)
        return record + " · " + String(localized: "paid on \(paidOn)")
    }
}

// MARK: - One asset

struct WalletAssetView: View {
    let wallet: AXYONWallet
    let ticker: String
    @State private var adding = false
    @State private var selling = false

    private var overview: WalletOverview { wallet.overview ?? WalletOverview() }
    private var asset: WalletAsset? { overview.asset(ticker) }
    private var trades: [WalletTrade] { overview.trades.filter { $0.ticker == ticker } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let asset {
                    header(asset)
                    figures(asset)
                }
                HStack(spacing: 12) {
                    action(String(localized: "Buy"), "plus") { selling = false; adding = true }
                    action(String(localized: "Sell"), "minus") { selling = true; adding = true }
                }
                WalletIncomeView(overview: overview, ticker: ticker)
                if !trades.isEmpty {
                    WalletTradesList(wallet: wallet, trades: trades, add: { selling = false; adding = true })
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
        }
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle(ticker)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $adding) { WalletTradeSheet(wallet: wallet, ticker: ticker, sell: selling) }
    }

    private func header(_ a: WalletAsset) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(AXYONInvest.className(a.classKey, a.classKey)).font(.footnote.weight(.semibold)).foregroundStyle(AXYONPalette.dim)
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(a.price.map(WalletNumber.price) ?? "—")
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.white)
                if let day = a.dayPct {
                    Text((day >= 0 ? "▲ " : "▼ ") + WalletNumber.percent(abs(day), places: 2))
                        .font(.headline)
                        .foregroundStyle(day >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
            }
            Text(quoteLine(a)).font(.caption).foregroundStyle(AXYONPalette.faint)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .axyonCard(cornerRadius: 24)
    }

    private func quoteLine(_ a: WalletAsset) -> String {
        if a.live, let at = a.priceAt {
            return String(localized: "Google Finance · \(at.formatted(date: .omitted, time: .shortened)) · about 15 min late")
        }
        if !a.closeDay.isEmpty {
            return String(localized: "B3's close of \(investDay(a.closeDay))")
        }
        return String(localized: "No quote read yet")
    }

    private func figures(_ a: WalletAsset) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 14) {
            GridRow {
                cell(String(localized: "Quantity"), WalletNumber.quantity(a.quantity))
                cell(String(localized: "Average price"), WalletNumber.price(a.average))
            }
            GridRow {
                cell(String(localized: "Invested"), AXYONMoney.text(a.cost))
                cell(String(localized: "Value now"), AXYONMoney.text(a.value))
            }
            GridRow {
                cell(String(localized: "Result"), WalletNumber.result(a.gain, a.gainPct),
                     color: a.gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                cell(String(localized: "Share of the portfolio"), WalletNumber.share(a.weight, target: a.target))
            }
            GridRow {
                cell(String(localized: "Proventos in 12 months"), AXYONMoney.text(a.dividends12))
                cell(String(localized: "Yield on cost"), a.yieldOnCost.map { WalletNumber.percent($0, places: 2) } ?? "—")
            }
            if a.realized != 0 || a.provisioned > 0 {
                GridRow {
                    cell(String(localized: "Realized in sales"), WalletNumber.signed(a.realized), color: a.realized >= 0 ? AXYONPalette.up : AXYONPalette.down)
                    cell(String(localized: "To receive"), AXYONMoney.text(a.provisioned), color: InvestPalette.favor)
                }
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private func cell(_ title: String, _ value: String, color: Color = .white) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
            Text(value).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(color).lineLimit(1).minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func action(_ title: String, _ icon: String, _ run: @escaping () -> Void) -> some View {
        Button(action: run) {
            Label(title, systemImage: icon)
                .font(.subheadline.weight(.semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .foregroundStyle(.white)
                .axyonCard(cornerRadius: 18)
        }
        .buttonStyle(.plain)
    }
}

// MARK: - A new lançamento

struct WalletTradeSheet: View {
    let wallet: AXYONWallet
    @Environment(\.dismiss) private var dismiss
    @State private var sell: Bool
    @State private var ticker: String
    @State private var date = Date.now
    @State private var quantity = ""
    @State private var price = ""
    @State private var fees = ""
    @State private var opening = true
    @State private var saving = false
    @State private var problem: String?

    init(wallet: AXYONWallet, ticker: String = "", sell: Bool = false) {
        self.wallet = wallet
        _ticker = State(initialValue: ticker)
        _sell = State(initialValue: sell)
    }

    private var code: String { ticker.trimmingCharacters(in: .whitespaces).uppercased() }
    private var held: WalletAsset? { wallet.overview?.asset(code) }
    /// Held from the sheet, with no lançamento yet: is this one added to those shares, or part of their history?
    private var asksOpening: Bool { !sell && held.map { !$0.managed && $0.trades == 0 && $0.quantity > 0 } ?? false }

    private var total: Double? {
        guard let q = WalletNumber.parse(quantity), let p = WalletNumber.parse(price), q > 0, p > 0 else { return nil }
        let f = WalletNumber.parse(fees) ?? 0
        return sell ? q * p - f : q * p + f
    }

    private var ready: Bool { !code.isEmpty && total != nil && !saving }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(selection: $sell) {
                        Text("Purchase").tag(false)
                        Text("Sale").tag(true)
                    } label: { EmptyView() }
                    .pickerStyle(.segmented)
                }
                .listRowBackground(Color.clear)
                Section {
                    TextField(String(localized: "Ticker (GARE11, BBAS3…)"), text: $ticker)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                    DatePicker(String(localized: "Date"), selection: $date, in: ...Date.now, displayedComponents: .date)
                    TextField(String(localized: "Quantity"), text: $quantity)
                        .keyboardType(.decimalPad)
                    HStack {
                        TextField(String(localized: "Price per share"), text: $price)
                            .keyboardType(.decimalPad)
                        if let now = held?.price {
                            Button(String(localized: "Use \(WalletNumber.price(now))")) {
                                price = now.formatted(.number.precision(.fractionLength(2...4)).locale(Locale(identifier: "pt_BR")))
                            }
                            .font(.footnote.weight(.semibold))
                            .buttonStyle(.borderless)
                        }
                    }
                    TextField(String(localized: "Costs (brokerage, fees) — optional"), text: $fees)
                        .keyboardType(.decimalPad)
                }
                if asksOpening, let held {
                    Section {
                        Picker(selection: $opening) {
                            Text("A new buy, added to the \(WalletNumber.quantity(held.quantity)) you hold").tag(true)
                            Text("Part of the history of those \(WalletNumber.quantity(held.quantity))").tag(false)
                        } label: { EmptyView() }
                        .pickerStyle(.inline)
                    } header: {
                        Text("You hold \(WalletNumber.quantity(held.quantity)) \(code) from your sheet")
                    } footer: {
                        Text(opening
                             ? String(localized: "The shares you hold come first, as an opening balance at their average price; this buy is added to them.")
                             : String(localized: "Enter every buy and sale of those shares: the position becomes what they make."))
                    }
                }
                if let total {
                    Section {
                        HStack {
                            Text(sell ? String(localized: "You receive") : String(localized: "Total"))
                            Spacer()
                            Text(AXYONMoney.text(total)).font(.headline).monospacedDigit()
                        }
                    }
                }
                if let problem {
                    Section {
                        Text(problem).foregroundStyle(InvestPalette.against)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background { InvestBackdrop() }
            .navigationTitle(sell ? String(localized: "New sale") : String(localized: "New buy"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if saving {
                        ProgressView()
                    } else {
                        Button("Save") { save() }.disabled(!ready)
                    }
                }
            }
        }
        .presentationDetents([.large])
    }

    private func save() {
        saving = true
        problem = nil
        Task {
            defer { saving = false }
            do {
                try await wallet.add(ticker: code, sell: sell, date: date, quantity: quantity, price: price, fees: fees,
                                     opening: sell || !asksOpening || opening)
                dismiss()
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
    }
}
