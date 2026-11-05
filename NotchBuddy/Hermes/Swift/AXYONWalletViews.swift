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
    @State private var recording = false
    @State private var backingUp = false
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
        #if targetEnvironment(simulator)
        .defaultScrollAnchor(AXYONTestScreen.anchor)
        #endif
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Wallet")
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button { backingUp = true } label: {
                    Image(systemName: "externaldrive.fill.badge.checkmark").font(.body.weight(.semibold))
                }
                .accessibilityLabel(Text("Backup"))
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button { adding = true } label: {
                        Label("Purchase or sale", systemImage: "arrow.left.arrow.right")
                    }
                    Button { recording = true } label: {
                        Label("Provento received", systemImage: "banknote")
                    }
                } label: {
                    Image(systemName: "plus").font(.body.weight(.bold))
                }
                .accessibilityLabel(Text("New lançamento"))
            }
        }
        .refreshable { await wallet.load() }
        .task {
            #if targetEnvironment(simulator)
            await AXYONTestScreen.seed(wallet)
            #endif
            await wallet.load()
            #if targetEnvironment(simulator)
            open(AXYONTestScreen.base)
            #endif
            // Like the sheet's GOOGLEFINANCE: the quotes again by themselves while the tab is open.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(120))
                guard !Task.isCancelled else { break }
                await wallet.quotes()
            }
        }
        .sheet(isPresented: $adding) { WalletTradeSheet(wallet: wallet) }
        .sheet(isPresented: $recording) { WalletProventoSheet() }
        .sheet(isPresented: $backingUp) { WalletBackupSheet() }
        .navigationDestination(item: $asset) { picked in
            WalletAssetView(wallet: wallet, ticker: picked.ticker)
        }
    }

    #if targetEnvironment(simulator)
    /// CI's screenshots: the part of the tab AXYON_TEST_TAB asks for (AXYONTestScreens.swift).
    private func open(_ screen: String) {
        switch screen {
        case "wallet-trades": section = .trades
        case "wallet-income": section = .income
        case "wallet-asset": asset = overview.asset("BBAS3") ?? overview.assets.first
        case "wallet-new": adding = true
        case "wallet-provento": recording = true
        case "wallet-backup": backingUp = true
        default: break
        }
    }
    #endif
}

// MARK: - The summary

struct WalletSummaryCard: View {
    let overview: WalletOverview
    let reading: Bool
    let updated: Date?

    var body: some View {
        let theme = AXYONTheme.current
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Net worth")
                        .font(.caption.weight(.semibold))
                        .textCase(.uppercase)
                        .tracking(1.4)
                        .foregroundStyle(AXYONPalette.dim)
                    Text(AXYONMoney.text(overview.total))
                        .font(.system(size: 40, weight: .bold))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .minimumScaleFactor(0.6)
                        .contentTransition(.numericText())
                        .animation(.snappy, value: overview.total)
                }
                Spacer(minLength: 4)
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 14, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 36)
                    .background(Circle().fill(.white.opacity(0.08)))
                    .overlay(Circle().strokeBorder(.white.opacity(0.18), lineWidth: 0.8))
                    .accessibilityHidden(true)
            }
            if let day = overview.day {
                dayPill(day)
            }
            if !overview.classes.isEmpty {
                allocation
            }
            Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 10) {
                GridRow {
                    tile(String(localized: "Invested"), AXYONMoney.text(overview.cost), icon: "banknote.fill")
                    tile(String(localized: "Result"), WalletNumber.result(overview.gain, overview.gainPct),
                         icon: overview.gain >= 0 ? "chart.line.uptrend.xyaxis" : "chart.line.downtrend.xyaxis",
                         color: overview.gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                }
                GridRow {
                    tile(String(localized: "Proventos in 12 months"), AXYONMoney.text(overview.twelve), icon: "arrow.down.circle.fill")
                    tile(String(localized: "To receive"), AXYONMoney.text(overview.provisioned), icon: "clock.fill",
                         color: overview.provisioned > 0 ? InvestPalette.favor : .white)
                }
                if overview.realized != 0 {
                    GridRow {
                        tile(String(localized: "Realized in sales"), WalletNumber.signed(overview.realized), icon: "checkmark.seal.fill",
                             color: overview.realized >= 0 ? AXYONPalette.up : AXYONPalette.down)
                        tile(String(localized: "Yield on cost"), overview.yieldOnCost.map { WalletNumber.percent($0, places: 2) } ?? "—",
                             icon: "percent")
                    }
                }
            }
            quotes
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(alignment: .topTrailing) {
            // The accent's light inside the glass, as the designs light a card from within.
            Circle()
                .fill(RadialGradient(colors: [theme.accent.opacity(0.42), theme.accent.opacity(0)],
                                     center: .center, startRadius: 0, endRadius: 150))
                .frame(width: 300, height: 300)
                .offset(x: 110, y: -120)
                .allowsHitTesting(false)
        }
        .clipShape(RoundedRectangle(cornerRadius: 30, style: .continuous))
        .axyonCard(cornerRadius: 30)
    }

    private func dayPill(_ day: Double) -> some View {
        let color = day >= 0 ? AXYONPalette.up : AXYONPalette.down
        return Label {
            Text("\(WalletNumber.signed(day)) (\(WalletNumber.percent(overview.dayPct ?? 0, places: 2))) today")
        } icon: {
            Image(systemName: day >= 0 ? "arrow.up.right" : "arrow.down.right")
        }
        .font(.subheadline.weight(.semibold))
        .monospacedDigit()
        .foregroundStyle(color)
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(Capsule().fill(color.opacity(0.14)))
        .overlay(Capsule().strokeBorder(color.opacity(0.32), lineWidth: 0.8))
    }

    /// The share of each class, as a bar of light and a legend.
    private var allocation: some View {
        let classes = overview.classes.filter { $0.value > 0 }
        let series = AXYONTheme.current.series
        return VStack(alignment: .leading, spacing: 10) {
            GeometryReader { geo in
                let room = max(geo.size.width - CGFloat(max(classes.count - 1, 0)) * 4, 0)
                HStack(spacing: 4) {
                    ForEach(Array(classes.enumerated()), id: \.element.id) { index, group in
                        let color = series[index % series.count]
                        Capsule()
                            .fill(LinearGradient(colors: [color, color.opacity(0.65)], startPoint: .top, endPoint: .bottom))
                            .frame(width: max(8, room * group.weight / 100))
                            .shadow(color: color.opacity(0.55), radius: 6)
                    }
                }
            }
            .frame(height: 10)
            InvestChips(spacing: 12) {
                ForEach(Array(classes.enumerated()), id: \.element.id) { index, group in
                    HStack(spacing: 5) {
                        Circle().fill(series[index % series.count]).frame(width: 7, height: 7)
                        Text(group.label).foregroundStyle(AXYONPalette.dim)
                        Text(WalletNumber.percent(group.weight, places: 0)).foregroundStyle(.white).monospacedDigit()
                    }
                    .font(.caption.weight(.semibold))
                }
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func tile(_ title: String, _ value: String, icon: String, color: Color = .white) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Image(systemName: icon)
                .font(.caption.weight(.bold))
                .foregroundStyle(AXYONTheme.current.accent)
                .frame(width: 26, height: 26)
                .background(Circle().fill(AXYONTheme.current.accent.opacity(0.16)))
            Text(value)
                .font(.headline)
                .monospacedDigit()
                .foregroundStyle(color)
                .lineLimit(1)
                .minimumScaleFactor(0.65)
            Text(title)
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(.white.opacity(0.055)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(.white.opacity(0.09), lineWidth: 0.8))
    }

    /// Where the quotes are from, and when: a light that glows while they are the session's.
    private var quotes: some View {
        HStack(spacing: 8) {
            if reading {
                ProgressView().controlSize(.mini).tint(.white)
            } else {
                Circle()
                    .fill(overview.liveAt != nil ? AXYONPalette.up : AXYONPalette.faint)
                    .frame(width: 7, height: 7)
                    .shadow(color: overview.liveAt != nil ? AXYONPalette.up : .clear, radius: 4)
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

/// An asset as the designs draw a person: a circle of its colour with its letters.
struct WalletAvatar: View {
    let ticker: String
    var color: Color = AXYONTheme.current.accent
    var size: CGFloat = 42

    var body: some View {
        Text(String(ticker.prefix(4)))
            .font(.system(size: size * 0.26, weight: .heavy, design: .rounded))
            .foregroundStyle(.white)
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .frame(width: size, height: size)
            .background(Circle().fill(LinearGradient(colors: [color.opacity(0.95), color.opacity(0.35)],
                                                     startPoint: .topLeading, endPoint: .bottomTrailing)))
            .overlay(Circle().strokeBorder(.white.opacity(0.32), lineWidth: 0.8))
            .shadow(color: color.opacity(0.5), radius: 8)
            .accessibilityHidden(true)
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
            ForEach(Array(overview.classes.enumerated()), id: \.element.id) { position, group in
                let color = AXYONTheme.current.series[position % AXYONTheme.current.series.count]
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .firstTextBaseline) {
                        HStack(spacing: 8) {
                            Circle().fill(color).frame(width: 9, height: 9).shadow(color: color, radius: 5)
                            Text(group.label).font(.title3.weight(.bold)).foregroundStyle(.white)
                        }
                        Spacer(minLength: 8)
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(AXYONMoney.text(group.value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                            Text(classLine(group)).font(.caption2).foregroundStyle(AXYONPalette.faint)
                        }
                    }
                    VStack(spacing: 0) {
                        ForEach(Array(group.assets.enumerated()), id: \.element.id) { index, item in
                            if index > 0 { Divider().overlay(Color.white.opacity(0.07)).padding(.leading, 14) }
                            Button { open(item) } label: { WalletAssetRow(asset: item, color: color) }
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
    var color: Color = AXYONTheme.current.accent

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            WalletAvatar(ticker: asset.ticker, color: color)
            VStack(alignment: .leading, spacing: 4) {
                Text(asset.ticker).font(.headline).foregroundStyle(.white).lineLimit(1).fixedSize()
                Text("Qty \(WalletNumber.quantity(asset.quantity)) · avg \(WalletNumber.price(asset.average))")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .layoutPriority(1)
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
                if asset.needsDate {
                    InvestTag(text: String(localized: "no purchase date"), color: InvestPalette.mixed)
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
    @State private var recording = false
    @State private var problem: String?

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
            recordButton
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
            }
            section(String(localized: "To receive"), icon: "clock.fill", color: InvestPalette.favor, only(overview.pending),
                    empty: String(localized: "Nothing provisioned: no data-com passed with the payment still ahead."))
            section(String(localized: "Announced, data-com ahead"), icon: "calendar.badge.clock", color: InvestPalette.mixed, only(overview.ahead),
                    empty: nil)
            section(String(localized: "Received"), icon: "checkmark.circle.fill", color: .white, Array(only(overview.received).prefix(24)),
                    empty: String(localized: "No provento received in what AXYON has read yet."))
        }
    }

    /// A provento received elsewhere (the broker's statement, Investidor10): it counts as the user gives it.
    private var recordButton: some View {
        Button {
            recording = true
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "plus.circle.fill")
                    .font(.title3)
                    .foregroundStyle(.white)
                    .frame(width: 40, height: 40)
                    .background(Circle().fill(AXYONTheme.current.accentGradient))
                    .axyonGlow(radius: 8, strength: 0.5)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Record a provento received").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    Text("As your broker or Investidor10 shows it: it counts in place of the one AXYON found.")
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                        .multilineTextAlignment(.leading)
                }
                Spacer(minLength: 4)
                Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
            }
            .padding(14)
            .axyonCard(cornerRadius: 20)
        }
        .buttonStyle(PressableButtonStyle())
        .sheet(isPresented: $recording) { WalletProventoSheet(ticker: ticker ?? "") }
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
                BarMark(x: .value("Month", month.short), y: .value("R$", month.total), width: .ratio(0.55))
                    .foregroundStyle(LinearGradient(colors: [AXYONTheme.current.accent, AXYONTheme.current.accentDeep.opacity(0.45)],
                                                    startPoint: .top, endPoint: .bottom))
                    .cornerRadius(6)
            }
            .shadow(color: AXYONTheme.current.accent.opacity(0.45), radius: 8)
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
                                .contentShape(Rectangle())
                                .contextMenu {
                                    if paid.recorded {
                                        Button(role: .destructive) {
                                            Task {
                                                do {
                                                    try await AXYONWallet.shared.removeProvento(paid)
                                                    problem = nil
                                                } catch {
                                                    problem = AXYONInvest.describe(error)
                                                }
                                            }
                                        } label: {
                                            Label("Remove this provento", systemImage: "trash")
                                        }
                                    }
                                }
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
                    if paid.recorded {
                        InvestTag(text: String(localized: "recorded by you"), color: AXYONTheme.current.accent)
                    }
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
        if paid.recorded && paid.quantity <= 0 { return String(localized: "as your statement shows it") }
        let line = WalletNumber.quantity(paid.quantity) + " × " + WalletNumber.price(paid.perShare)
        if paid.estimated { return line + " · " + String(localized: "estimated") }
        if paid.basis == "planilha" { return line + " · " + String(localized: "checked with your sheet") }
        return line
    }

    private var dates: String {
        let day = investDay(paid.record)
        let record = String(localized: "data-com \(day)")
        guard let payment = paid.payment else { return record + " · " + String(localized: "payment to be set") }
        let paidOn = investDay(payment)
        if paid.record.isEmpty { return String(localized: "paid on \(paidOn)") }
        return record + " · " + String(localized: "paid on \(paidOn)")
    }
}

/// A provento received, entered as the user's statement shows it (their broker's, Investidor10's): the asset, when it
/// was paid, its data-com when known, and what was received, in all or per share.
struct WalletProventoSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var ticker: String
    @State private var kind = "rendimento"
    @State private var payment = Date.now
    @State private var knowsRecord = false
    @State private var record = Date.now
    @State private var perShareWay = false
    @State private var value = ""
    @State private var perShare = ""
    @State private var quantity = ""
    @State private var saving = false
    @State private var problem: String?

    init(ticker: String = "") {
        _ticker = State(initialValue: ticker)
        _kind = State(initialValue: ticker.hasSuffix("11") || ticker.isEmpty ? "rendimento" : "dividendo")
    }

    private var code: String { ticker.trimmingCharacters(in: .whitespaces).uppercased() }

    private var total: Double? {
        if perShareWay {
            guard let p = WalletNumber.parse(perShare), let q = WalletNumber.parse(quantity), p > 0, q > 0 else { return nil }
            return p * q
        }
        return WalletNumber.parse(value).flatMap { $0 > 0 ? $0 : nil }
    }

    private var ready: Bool { !code.isEmpty && total != nil && !saving }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Ticker (BTCI11, BBAS3…)"), text: $ticker)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                    Picker(String(localized: "Kind"), selection: $kind) {
                        Text("Income").tag("rendimento")
                        Text("Dividend").tag("dividendo")
                        Text(verbatim: "JCP").tag("jcp")
                        Text("Amortization").tag("amortizacao")
                    }
                    DatePicker(String(localized: "Paid on"), selection: $payment, displayedComponents: .date)
                    Toggle(String(localized: "I know the data-com"), isOn: $knowsRecord.animation())
                    if knowsRecord {
                        DatePicker(String(localized: "Data-com"), selection: $record, in: ...payment, displayedComponents: .date)
                    }
                }
                .listRowBackground(GlassRowBackground())
                Section {
                    InvestSegments<Bool>(options: [.init(value: false, title: String(localized: "Amount received")),
                                                   .init(value: true, title: String(localized: "Per share"))],
                                         selection: $perShareWay)
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
                Section {
                    if perShareWay {
                        TextField(String(localized: "R$ per share"), text: $perShare).keyboardType(.decimalPad)
                        TextField(String(localized: "Shares"), text: $quantity).keyboardType(.decimalPad)
                    } else {
                        TextField(String(localized: "Amount received, R$"), text: $value).keyboardType(.decimalPad)
                    }
                } footer: {
                    Text("It counts as received on the day it was paid, in place of the one AXYON's sources give for that asset and day. Hold it down in the list to remove it.")
                }
                .listRowBackground(GlassRowBackground())
                if let total {
                    Section {
                        HStack {
                            Text("You received")
                            Spacer()
                            Text(AXYONMoney.text(total)).font(.headline).monospacedDigit()
                        }
                    }
                    .listRowBackground(GlassRowBackground(tint: AXYONTheme.current.accent))
                }
                if let problem {
                    Section {
                        Text(problem).foregroundStyle(InvestPalette.against)
                    }
                    .listRowBackground(GlassRowBackground())
                }
            }
            .scrollContentBackground(.hidden)
            .background { InvestBackdrop() }
            .navigationTitle(String(localized: "Provento received"))
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
                try await AXYONWallet.shared.addProvento(ticker: code, kind: kind, payment: payment, record: knowsRecord ? record : nil,
                                                         value: perShareWay ? "" : value, perShare: perShareWay ? perShare : "",
                                                         quantity: perShareWay ? quantity : "")
                Haptics.success()
                dismiss()
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
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
                    if asset.trades == 0 {
                        WalletSinceCard(wallet: wallet, asset: asset)
                    }
                    figures(asset)
                }
                HStack(spacing: 12) {
                    action(String(localized: "Buy"), "plus") { selling = false; adding = true }
                    action(String(localized: "Sell"), "minus") { selling = true; adding = true }
                }
                NavigationLink {
                    InvestFolderView(ticker: ticker)
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: "folder.fill")
                            .font(.title3)
                            .foregroundStyle(AXYONTheme.current.accentGradient)
                            .axyonGlow(radius: 8)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Open \(ticker)'s folder").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                            Text("The committee's analyses, the documents read, the thesis").font(.caption).foregroundStyle(AXYONPalette.dim)
                        }
                        Spacer(minLength: 4)
                        Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(AXYONPalette.faint)
                    }
                    .padding(16)
                    .axyonCard(cornerRadius: 20)
                }
                .buttonStyle(.plain)
                WalletIncomeView(overview: overview, ticker: ticker)
                if !trades.isEmpty {
                    WalletTradesList(wallet: wallet, trades: trades, add: { selling = false; adding = true })
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
        }
        #if targetEnvironment(simulator)
        .defaultScrollAnchor(AXYONTestScreen.anchor)
        #endif
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
                    InvestSegments<Bool>(options: [.init(value: false, title: String(localized: "Purchase")),
                                                   .init(value: true, title: String(localized: "Sale"))],
                                         selection: $sell)
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
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
                .listRowBackground(GlassRowBackground())
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
                    .listRowBackground(GlassRowBackground())
                }
                if let total {
                    Section {
                        HStack {
                            Text(sell ? String(localized: "You receive") : String(localized: "Total"))
                            Spacer()
                            Text(AXYONMoney.text(total)).font(.headline).monospacedDigit()
                        }
                    }
                    .listRowBackground(GlassRowBackground(tint: AXYONTheme.current.accent))
                }
                if let problem {
                    Section {
                        Text(problem).foregroundStyle(InvestPalette.against)
                    }
                    .listRowBackground(GlassRowBackground())
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

// MARK: - Since when an asset of the sheet is held

/// An asset from the sheet, with no lançamento: since when it is held, so that no provento from before the purchase
/// counts. The proventos of a month the sheet's "Proventos pagos" records come from it either way.
struct WalletSinceCard: View {
    let wallet: AXYONWallet
    let asset: WalletAsset
    @State private var date = Date.now
    @State private var editing = false
    @State private var saving = false
    @State private var problem: String?

    private var asking: Bool { asset.needsDate || editing }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if asking {
                Label(String(localized: "Since when do you hold \(asset.ticker)?"), systemImage: "calendar.badge.clock")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("Only the proventos whose data-com came after your purchase are yours. Tell the day of your first purchase (or enter your buys as lançamentos).")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                DatePicker(String(localized: "First purchase"), selection: $date, in: ...Date.now, displayedComponents: .date)
                    .tint(InvestPalette.favor)
                HStack {
                    if editing {
                        Button(String(localized: "Cancel")) { editing = false }
                            .buttonStyle(.borderless)
                    }
                    Spacer()
                    Button {
                        save(date)
                    } label: {
                        if saving { ProgressView() } else { Text("Save") }
                    }
                    .coucouProminentButton()
                    .disabled(saving)
                }
            } else {
                HStack(spacing: 10) {
                    Image(systemName: "calendar").foregroundStyle(InvestPalette.favor)
                    Text("Proventos counted since \(investDay(asset.heldFrom))")
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    Spacer(minLength: 4)
                    Button(String(localized: "Change")) { editing = true }
                        .font(.footnote.weight(.semibold))
                        .buttonStyle(.borderless)
                }
            }
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22, tint: asset.needsDate ? InvestPalette.mixed : nil)
    }

    private func save(_ day: Date) {
        saving = true
        problem = nil
        Task {
            defer { saving = false }
            do {
                try await wallet.since(asset.ticker, date: day)
                editing = false
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
    }
}
