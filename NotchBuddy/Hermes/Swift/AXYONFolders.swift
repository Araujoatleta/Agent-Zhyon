import SwiftUI

// The folders: every asset AXYON keeps something about (held, its lançamentos, a meeting of the committee, a document
// the reader read, a thesis, a decision), kept as a folder the user opens. A folder holds the asset's every analysis,
// newest first: the committee's verdicts (its last twelve), the documents read with their digests, the thesis checked
// now and the decisions of the journal. The engine keeps them (coucou_powers::invest::folders, folder).

// MARK: - What the engine says

private enum FolderJSON {
    static func num(_ value: Any?) -> Double? { (value as? NSNumber).map { $0.doubleValue + 0.0 } }
    static func int(_ value: Any?) -> Int { Int(num(value) ?? 0) }
    static func str(_ value: Any?) -> String { value as? String ?? "" }
    static func list(_ value: Any?) -> [[String: Any]] { value as? [[String: Any]] ?? [] }
    static func dict(_ value: Any?) -> [String: Any]? { value as? [String: Any] }
}

/// A folder in the list: the asset, what it holds, when it last changed, its last verdict.
struct InvestFolderSummary: Identifiable, Equatable, Hashable {
    let id: String
    let classKey: String
    let held: Bool
    let verdicts: Int
    let documents: Int
    let thesis: Bool
    let decisions: Int
    let trades: Int
    let last: String
    /// The last verdict: "2 × 1", opinion A and B, whether a mind vetoed.
    let score: String?
    let assetView: String
    let portfolioView: String
    let vetoed: Bool

    init(_ o: [String: Any]) {
        id = FolderJSON.str(o["ticker"])
        classKey = FolderJSON.str(o["class"])
        held = o["held"] as? Bool ?? false
        verdicts = FolderJSON.int(o["verdicts"])
        documents = FolderJSON.int(o["documents"])
        thesis = o["thesis"] as? Bool ?? false
        decisions = FolderJSON.int(o["decisions"])
        trades = FolderJSON.int(o["trades"])
        last = FolderJSON.str(o["last"])
        let latest = FolderJSON.dict(o["latest"])
        score = latest.map { "\(FolderJSON.int($0["for"])) × \(FolderJSON.int($0["against"]))" }
        assetView = FolderJSON.str(latest?["asset"])
        portfolioView = FolderJSON.str(latest?["portfolio"])
        vetoed = latest?["vetoed"] as? Bool ?? false
    }

    var ticker: String { id }
    /// Something to read in it beyond the position.
    var analyses: Int { verdicts + documents + decisions + (thesis ? 1 : 0) }
}

/// A decision written in the journal.
struct InvestDecisionEntry: Identifiable, Equatable {
    let id: String
    let date: String
    let action: String
    let quantity: Double?
    let price: Double?
    let reasons: String
    let expectation: String
    let confidence: Int?

    init(_ o: [String: Any]) {
        id = FolderJSON.str(o["id"])
        date = FolderJSON.str(o["date"])
        action = FolderJSON.str(o["action"])
        quantity = FolderJSON.num(o["quantity"])
        price = FolderJSON.num(o["price"])
        reasons = FolderJSON.str(o["reasons"])
        expectation = FolderJSON.str(o["expectation"])
        confidence = FolderJSON.num(o["confidence"]).map { Int($0) }
    }

    var actionName: String {
        switch action {
        case "buy": String(localized: "Buy")
        case "sell": String(localized: "Sell")
        case "hold": String(localized: "Hold")
        case "avoid": String(localized: "Avoid")
        default: action
        }
    }
}

/// An asset's folder, opened.
struct InvestFolder: Equatable {
    struct Position: Equatable {
        let quantity: Double
        let average: Double
        let price: Double?
        let value: Double
        let gain: Double
        let gainPct: Double?
        let weight: Double
    }

    let ticker: String
    let classKey: String
    let position: Position?
    let verdicts: [InvestVerdict]
    let documents: [InvestDoc]
    let thesis: InvestThesis?
    let decisions: [InvestDecisionEntry]
    /// The asset's sheet: indicators, a year of dividends, the VP month by month, what its reports said, signals.
    let ficha: InvestFicha

    init(_ o: [String: Any]) {
        ticker = FolderJSON.str(o["ticker"])
        classKey = FolderJSON.str(o["class"])
        position = FolderJSON.dict(o["position"]).map {
            Position(quantity: FolderJSON.num($0["quantity"]) ?? 0, average: FolderJSON.num($0["average"]) ?? 0,
                     price: FolderJSON.num($0["price"]), value: FolderJSON.num($0["value"]) ?? 0,
                     gain: FolderJSON.num($0["gain"]) ?? 0, gainPct: FolderJSON.num($0["gainPct"]),
                     weight: FolderJSON.num($0["weight"]) ?? 0)
        }
        verdicts = FolderJSON.list(o["verdicts"]).map(InvestVerdict.init)
        documents = FolderJSON.list(o["documents"]).map(InvestDoc.init)
        thesis = FolderJSON.dict(o["thesis"]).map(InvestThesis.init)
        decisions = FolderJSON.list(o["decisions"]).map(InvestDecisionEntry.init)
        ficha = FolderJSON.dict(o["ficha"]).map(InvestFicha.init) ?? InvestFicha()
    }

    var isEmpty: Bool { verdicts.isEmpty && documents.isEmpty && thesis == nil && decisions.isEmpty }
}

// MARK: - The store

@MainActor
@Observable
final class AXYONFolders {
    static let shared = AXYONFolders()

    private(set) var list: [InvestFolderSummary] = []
    private(set) var loading = false

    private func engine(_ tool: String, _ input: [String: Any] = [:]) async throws -> Any {
        let body = (try? JSONSerialization.data(withJSONObject: input)) ?? Data("{}".utf8)
        let answer = await HermesPowers.call(tool, input: body)
        let object = (try? JSONSerialization.jsonObject(with: answer)) as? [String: Any] ?? [:]
        guard object["ok"] as? Bool == true else {
            throw AXYONInvestError.said(object["text"] as? String ?? String(localized: "The superpowers did not answer."))
        }
        return object["data"] ?? NSNull()
    }

    func load() async {
        guard !loading else { return }
        loading = true
        defer { loading = false }
        if let data = try? await engine("invest_folders") as? [[String: Any]] {
            list = data.map(InvestFolderSummary.init)
        }
    }

    func open(_ ticker: String) async throws -> InvestFolder {
        let data = try await engine("invest_folder", ["ticker": ticker])
        return InvestFolder(data as? [String: Any] ?? [:])
    }
}

// MARK: - The folder's shape

/// A folder: a rounded body with a tab at its top left.
struct FolderShape: InsettableShape {
    var radius: CGFloat = 22
    /// The tab's share of the width, and its height.
    var tab: CGFloat = 0.44
    var tabHeight: CGFloat = 14
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let corner = min(radius, r.height / 3, r.width / 4)
        let top = r.minY + tabHeight
        let tabEnd = r.minX + r.width * tab
        var p = Path()
        p.move(to: CGPoint(x: r.minX, y: r.minY + corner))
        p.addQuadCurve(to: CGPoint(x: r.minX + corner, y: r.minY), control: CGPoint(x: r.minX, y: r.minY))
        p.addLine(to: CGPoint(x: tabEnd - 12, y: r.minY))
        // The tab's edge, sloping down to the body.
        p.addCurve(to: CGPoint(x: tabEnd + 12, y: top), control1: CGPoint(x: tabEnd, y: r.minY), control2: CGPoint(x: tabEnd, y: top))
        p.addLine(to: CGPoint(x: r.maxX - corner, y: top))
        p.addQuadCurve(to: CGPoint(x: r.maxX, y: top + corner), control: CGPoint(x: r.maxX, y: top))
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY - corner))
        p.addQuadCurve(to: CGPoint(x: r.maxX - corner, y: r.maxY), control: CGPoint(x: r.maxX, y: r.maxY))
        p.addLine(to: CGPoint(x: r.minX + corner, y: r.maxY))
        p.addQuadCurve(to: CGPoint(x: r.minX, y: r.maxY - corner), control: CGPoint(x: r.minX, y: r.maxY))
        p.closeSubpath()
        return p
    }

    func inset(by amount: CGFloat) -> FolderShape {
        var shape = self
        shape.inset += amount
        return shape
    }
}

/// Frosted glass in a folder's shape, its colour's light inside it.
private struct FolderGlass: ViewModifier {
    let color: Color

    func body(content: Content) -> some View {
        let theme = AXYONTheme.current
        let shape = FolderShape()
        content
            .background {
                ZStack {
                    shape.fill(.ultraThinMaterial)
                    shape.fill(LinearGradient(colors: [theme.glass.opacity(0.40), theme.glass.opacity(0.62)], startPoint: .top, endPoint: .bottom))
                    shape.fill(RadialGradient(colors: [color.opacity(0.38), color.opacity(0)], center: UnitPoint(x: 0.85, y: 0.15),
                                              startRadius: 0, endRadius: 140))
                    shape.fill(LinearGradient(colors: [.white.opacity(0.10), .white.opacity(0)], startPoint: .top, endPoint: .center))
                }
                .compositingGroup()
                .shadow(color: .black.opacity(0.42), radius: 18, y: 10)
            }
            .overlay(shape.strokeBorder(AXYONPalette.rim(color), lineWidth: 1).allowsHitTesting(false))
    }
}

// MARK: - The folders

/// A folder in a row or a grid: the asset's colour, its letters, what it holds.
struct InvestFolderCard: View {
    let folder: InvestFolderSummary
    let color: Color
    var wide = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                WalletAvatar(ticker: folder.ticker, color: color, size: 40)
                Spacer(minLength: 4)
                if folder.held {
                    Circle().fill(AXYONPalette.up).frame(width: 8, height: 8).shadow(color: AXYONPalette.up, radius: 4)
                        .accessibilityLabel(Text("In your portfolio"))
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(folder.ticker).font(.headline).foregroundStyle(.white).lineLimit(1)
                Text(AXYONInvest.className(folder.classKey, folder.classKey)).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(1)
            }
            InvestChips(spacing: 5) {
                if folder.verdicts > 0 {
                    InvestTag(text: String(localized: "\(folder.verdicts) analyses"), color: AXYONTheme.current.accent)
                }
                if folder.documents > 0 {
                    InvestTag(text: String(localized: "\(folder.documents) documents"), color: AXYONPalette.dim)
                }
                if folder.thesis {
                    InvestTag(text: String(localized: "thesis"), color: InvestPalette.favor)
                }
                if folder.analyses == 0 {
                    InvestTag(text: String(localized: "no analysis yet"), color: AXYONPalette.faint)
                }
            }
            Spacer(minLength: 0)
            if !folder.last.isEmpty {
                Text("Last on \(investDay(folder.last))").font(.caption2).foregroundStyle(AXYONPalette.faint)
            }
        }
        .padding(.top, 24)
        .padding([.horizontal, .bottom], 14)
        .frame(width: wide ? nil : 170, height: 212, alignment: .topLeading)
        .frame(maxWidth: wide ? .infinity : nil, alignment: .topLeading)
        .modifier(FolderGlass(color: color))
        .contentShape(FolderShape())
    }
}

/// The folders on the investing screen: a row of them, the held first, and all of them a tap away.
struct InvestFoldersSection: View {
    private let folders = AXYONFolders.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                AXYONHeader(title: String(localized: "Folders"))
                if !folders.list.isEmpty {
                    NavigationLink {
                        InvestFoldersView()
                    } label: {
                        Text("All").font(.subheadline.weight(.semibold)).foregroundStyle(AXYONTheme.current.accent)
                    }
                }
            }
            if folders.list.isEmpty {
                Text("Each asset gets its folder: the committee's analyses, the documents read, its thesis and your decisions, kept to open again.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .axyonCard(cornerRadius: 22)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 12) {
                        ForEach(Array(folders.list.prefix(12).enumerated()), id: \.element.id) { index, folder in
                            NavigationLink {
                                InvestFolderView(ticker: folder.ticker)
                            } label: {
                                InvestFolderCard(folder: folder, color: AXYONTheme.current.series[index % AXYONTheme.current.series.count])
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 2)
                }
                .scrollClipDisabled()
            }
        }
        .task { await folders.load() }
    }
}

/// Every folder, two by two.
struct InvestFoldersView: View {
    private let folders = AXYONFolders.shared

    var body: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)], spacing: 14) {
                ForEach(Array(folders.list.enumerated()), id: \.element.id) { index, folder in
                    NavigationLink {
                        InvestFolderView(ticker: folder.ticker)
                    } label: {
                        InvestFolderCard(folder: folder, color: AXYONTheme.current.series[index % AXYONTheme.current.series.count], wide: true)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
        }
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Folders")
        .navigationBarTitleDisplayMode(.large)
        .refreshable { await folders.load() }
        .task { await folders.load() }
    }
}

// MARK: - One folder

/// An asset's folder, opened: its position, the committee's analyses, the documents read, the thesis, the decisions.
struct InvestFolderView: View {
    let ticker: String
    @State private var folder: InvestFolder?
    @State private var problem: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                header
                if let problem {
                    Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
                }
                if let folder {
                    InvestFichaSection(ficha: folder.ficha)
                    if folder.isEmpty {
                        Text("No trial yet. The court meets by itself when the intern brings a new document of \(ticker), and the trial is kept here, with the documents read.")
                            .font(.footnote)
                            .foregroundStyle(AXYONPalette.dim)
                            .padding(16)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .axyonCard(cornerRadius: 22)
                    }
                    if !folder.verdicts.isEmpty {
                        section(String(localized: "The court's trials"), count: folder.verdicts.count) {
                            ForEach(folder.verdicts) { verdict in
                                NavigationLink {
                                    InvestTrialView(verdict: verdict)
                                } label: {
                                    InvestVerdictRow(verdict: verdict).padding(14).axyonCard(cornerRadius: 22)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if let thesis = folder.thesis {
                        section(String(localized: "Thesis"), count: nil) {
                            InvestThesisRow(thesis: thesis).padding(16).axyonCard(cornerRadius: 22)
                        }
                    }
                    if !folder.documents.isEmpty {
                        section(String(localized: "Documents read"), count: folder.documents.count) {
                            ForEach(folder.documents) { doc in
                                NavigationLink {
                                    InvestDocumentView(invest: AXYONInvest.shared, doc: doc)
                                } label: {
                                    InvestDocRow(doc: doc).padding(14).axyonCard(cornerRadius: 20)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    if !folder.decisions.isEmpty {
                        section(String(localized: "Your decisions"), count: folder.decisions.count) {
                            ForEach(folder.decisions) { decision in
                                decisionRow(decision)
                            }
                        }
                    }
                } else if problem == nil {
                    ProgressView().tint(.white).frame(maxWidth: .infinity).padding(.top, 30)
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
        .refreshable { await load() }
        .task { await load() }
    }

    private func load() async {
        do {
            folder = try await AXYONFolders.shared.open(ticker)
            problem = nil
        } catch {
            problem = AXYONInvest.describe(error)
        }
    }

    /// The folder's cover: the asset, its class, its position and where to see it in the Wallet.
    private var header: some View {
        let color = AXYONTheme.current.accent
        return VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 14) {
                WalletAvatar(ticker: ticker, color: color, size: 56)
                VStack(alignment: .leading, spacing: 3) {
                    Text(ticker).font(.title2.weight(.bold)).foregroundStyle(.white)
                    Text(AXYONInvest.className(folder?.classKey ?? "", folder?.classKey ?? ""))
                        .font(.subheadline)
                        .foregroundStyle(AXYONPalette.dim)
                }
                Spacer(minLength: 0)
                Image(systemName: "folder.fill")
                    .font(.title2)
                    .foregroundStyle(AXYONTheme.current.accentGradient)
                    .axyonGlow(radius: 8)
            }
            if let p = folder?.position {
                Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 10) {
                    GridRow {
                        figure(String(localized: "Quantity"), WalletNumber.quantity(p.quantity))
                        figure(String(localized: "Average price"), WalletNumber.price(p.average))
                        figure(String(localized: "Quote"), p.price.map(WalletNumber.price) ?? "—")
                    }
                    GridRow {
                        figure(String(localized: "Value now"), AXYONMoney.text(p.value))
                        figure(String(localized: "Result"), WalletNumber.result(p.gain, p.gainPct), color: p.gain >= 0 ? AXYONPalette.up : AXYONPalette.down)
                        figure(String(localized: "Weight"), WalletNumber.percent(p.weight))
                    }
                }
                NavigationLink {
                    WalletAssetView(wallet: AXYONWallet.shared, ticker: ticker)
                } label: {
                    Label("Proventos and lançamentos", systemImage: "briefcase.fill")
                        .font(.subheadline.weight(.semibold))
                }
                .buttonStyle(WhiteCapsuleButtonStyle())
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(FolderGlass(color: color))
    }

    private func figure(_ title: String, _ value: String, color: Color = .white) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(AXYONPalette.dim).lineLimit(1).minimumScaleFactor(0.8)
            Text(value).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(color).lineLimit(1).minimumScaleFactor(0.65)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func section<Content: View>(_ title: String, count: Int?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                AXYONHeader(title: title)
                if let count {
                    Text(verbatim: "\(count)")
                        .font(.system(.caption, design: .rounded).weight(.bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(Capsule().fill(AXYONTheme.current.accent.opacity(0.35)))
                }
            }
            content()
        }
    }

    private func decisionRow(_ decision: InvestDecisionEntry) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                InvestTag(text: decision.actionName, color: AXYONTheme.current.accent)
                if let confidence = decision.confidence {
                    InvestTag(text: String(localized: "sure at \(confidence)%"), color: AXYONPalette.dim)
                }
                Spacer(minLength: 0)
                Text(investDay(decision.date)).font(.caption2).foregroundStyle(AXYONPalette.faint)
            }
            if !decision.reasons.isEmpty {
                Text(decision.reasons).font(.callout).foregroundStyle(.white)
            }
            if !decision.expectation.isEmpty {
                Text(decision.expectation).font(.footnote).foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }
}
