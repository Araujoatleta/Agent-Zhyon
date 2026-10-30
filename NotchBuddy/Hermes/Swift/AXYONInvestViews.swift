import SwiftUI

// The investing screens of AXYON on this iPhone (AXYONInvest.swift holds their data): charcoal, a ring of thick
// rounded segments in soft colours that slide from one view to the next, the total in its middle, a legend in two
// columns, pills to choose what it shows and a carousel of months; then the committee, the reader, the theses, the
// week aloud and the learning, each in its own glass.

// MARK: - The look

/// The charcoal behind the investing screens, a little lighter at the top.
struct InvestBackdrop: View {
    var body: some View {
        LinearGradient(colors: [Color(hex: "#202023"), Color(hex: "#141416")], startPoint: .top, endPoint: .bottom)
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

/// The ring's soft colours, in the order slices take them.
enum InvestPalette {
    static let colors: [Color] = ["#F2F5C4", "#C5E8B7", "#E5CCFF", "#E8836B", "#9B9B9B", "#DDF6FA", "#B3B3F2", "#FBE2B6"]
        .map { Color(hex: $0) }
    static let favor = Color(hex: "#C5E8B7")
    static let against = Color(hex: "#E8836B")
    static let neutral = Color(hex: "#9B9B9B")
    static let mixed = Color(hex: "#FBE2B6")

    static func color(_ index: Int) -> Color { colors[index % colors.count] }

    static func vote(_ vote: String) -> Color {
        switch vote {
        case "for": favor
        case "against": against
        default: neutral
        }
    }

    static func tone(_ tone: String) -> Color {
        switch tone {
        case "better": favor
        case "worse": against
        case "mixed": mixed
        default: neutral
        }
    }
}

extension AXYONMoney {
    /// "R$ 12,8 mil": a large amount in few characters, for the ring's middle.
    static func compact(_ value: Double) -> String {
        let size = abs(value)
        guard size >= 10_000 else { return text(value) }
        let millions = size >= 1_000_000
        let scaled = value / (millions ? 1_000_000 : 1_000)
        let suffix = millions ? String(localized: "M", comment: "millions, after an amount") : String(localized: "K", comment: "thousands, after an amount")
        return "R$ " + scaled.formatted(.number.precision(.fractionLength(0...1))) + " " + suffix
    }
}

/// A segment of the ring, where it starts and ends on the circle (0 to 1, from the top, clockwise).
private struct RingSlot: Identifiable, Equatable {
    let id: Int
    let from: Double
    let to: Double
    let color: Color
    let shown: Bool
}

/// The ring of the video's design: thick segments with round ends and a gap between each, a faint track under them.
/// Segments keep their place (`capacity` of them), so a new set of values slides and resizes them rather than drawing
/// a new ring.
struct InvestRing: View {
    var values: [Double]
    var colors: [Color]
    var lineWidth: CGFloat = 28
    var gap: Double = 0.014
    var capacity = 8

    var body: some View {
        GeometryReader { geo in
            let size = min(geo.size.width, geo.size.height)
            let diameter = max(size - lineWidth, 1)
            let cap = Double(lineWidth / 2) / (Double.pi * Double(diameter))
            let slots = Self.slots(values, colors: colors, capacity: max(capacity, values.count), gap: gap, cap: cap)
            ZStack {
                Circle()
                    .stroke(Color.white.opacity(0.06), lineWidth: lineWidth)
                    .frame(width: diameter, height: diameter)
                ForEach(slots) { slot in
                    Circle()
                        .trim(from: slot.from, to: slot.to)
                        .stroke(slot.color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                        .frame(width: diameter, height: diameter)
                        .opacity(slot.shown ? 1 : 0)
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
            .animation(.spring(response: 0.75, dampingFraction: 0.82), value: slots)
        }
        .accessibilityHidden(true)
    }

    private static func slots(_ values: [Double], colors: [Color], capacity: Int, gap: Double, cap: Double) -> [RingSlot] {
        let total = values.reduce(0) { $0 + max($1, 0) }
        var out: [RingSlot] = []
        var start = 0.0
        for index in 0..<capacity {
            let value = index < values.count ? max(values[index], 0) : 0
            let color = index < colors.count ? colors[index] : InvestPalette.color(index)
            let share = total > 0 ? value / total : 0
            guard share > 0 else {
                out.append(RingSlot(id: index, from: start, to: start, color: color, shown: false))
                continue
            }
            let pad = gap / 2 + cap
            var from = start + pad
            var to = start + share - pad
            if to <= from {
                // A slice too small for its round ends: a dot in its middle.
                from = start + share / 2
                to = from + 0.0005
            }
            out.append(RingSlot(id: index, from: from, to: min(to, 1), color: color, shown: true))
            start += share
        }
        return out
    }
}

/// A choice in a capsule: the chosen one a white pill with dark text, sliding to the next.
struct InvestSegments<Value: Hashable>: View {
    struct Option: Identifiable {
        let value: Value
        let title: String
        var id: Value { value }
    }

    let options: [Option]
    @Binding var selection: Value
    @Namespace private var pill

    var body: some View {
        HStack(spacing: 2) {
            ForEach(options) { option in
                let chosen = option.value == selection
                Button {
                    withAnimation(.spring(duration: 0.35, bounce: 0.2)) { selection = option.value }
                } label: {
                    Text(option.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(chosen ? Color.black : Color.white.opacity(0.65))
                        .lineLimit(1)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                        .background {
                            if chosen {
                                Capsule().fill(Color.white).matchedGeometryEffect(id: "pill", in: pill)
                            }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chosen ? .isSelected : [])
            }
        }
        .padding(3)
        .background(Capsule().fill(Color.white.opacity(0.08)))
    }
}

/// A pill with a chevron that opens a menu.
private struct InvestPill: View {
    let title: String

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
            Image(systemName: "chevron.down").font(.caption.weight(.bold))
        }
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .overlay(Capsule().strokeBorder(Color.white.opacity(0.28), lineWidth: 1))
        .contentShape(Capsule())
    }
}

/// The outlined capsule under the ring ("History").
private struct InvestOutlineLabel: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 24)
            .padding(.vertical, 11)
            .overlay(Capsule().strokeBorder(Color.white.opacity(0.35), lineWidth: 1))
            .contentShape(Capsule())
    }
}

/// A small tag: where a point comes from ("F3", "D1 p.4"), a document's kind, a vote.
struct InvestTag: View {
    let text: String
    var color: Color = .white

    var body: some View {
        Text(text)
            .font(.caption2.weight(.semibold).monospaced())
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(Capsule().fill(color.opacity(0.14)))
    }
}

/// A section's title and the line under it.
private struct InvestTitle: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.title3.weight(.bold)).foregroundStyle(.white)
            Text(subtitle).font(.footnote).foregroundStyle(AXYONPalette.dim)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// "27/10/2026" from the engine's "2026-10-27".
func investDay(_ iso: String) -> String {
    let parts = iso.split(separator: "-")
    guard parts.count == 3 else { return iso }
    return "\(parts[2])/\(parts[1])/\(parts[0])"
}

// MARK: - The investing screen

/// What the ring shows.
enum InvestShown: Hashable {
    case allocation, dividends
}

/// How the portfolio's ring is cut.
enum InvestCut: Hashable {
    case classes, assets
}

struct AXYONInvestView: View {
    private let invest = AXYONInvest.shared
    @State private var shown = InvestShown.allocation
    @State private var cut = InvestCut.classes
    @State private var month = 11
    @State private var appeared = false
    @State private var writing = false
    @State private var targeting = false

    private var overview: InvestOverview { invest.overview ?? InvestOverview() }

    var body: some View {
        ScrollView {
            VStack(spacing: 26) {
                controls
                if shown == .dividends { InvestMonthsBar(months: overview.dividends, index: $month) }
                ringBlock
                InvestCommitteeCard(invest: invest)
                InvestReaderCard(invest: invest)
                InvestThesesCard(invest: invest, writing: $writing)
                InvestWeeklyCard(invest: invest)
                InvestLearnCard(invest: invest)
                Text("AXYON computes every number from the official sources, and the committee and the reader say where each point comes from. Nothing is bought or sold here: this is not an investment recommendation, the decision is yours.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 8)
            }
            .padding(.horizontal, 20)
            .padding(.top, 8)
            .padding(.bottom, 28)
        }
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Investments")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button { Task { await invest.refresh() } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    Button { Task { await invest.read(force: true); await invest.think() } } label: {
                        Label("Read the documents now", systemImage: "doc.text.magnifyingglass")
                    }
                    Button { writing = true } label: { Label("Write a thesis", systemImage: "square.and.pencil") }
                    Button { targeting = true } label: { Label("Targets per asset", systemImage: "scope") }
                    Toggle(isOn: Binding(get: { overview.apprentice }, set: { on in Task { await invest.setApprentice(on) } })) {
                        Label("Apprentice mode", systemImage: "graduationcap")
                    }
                } label: {
                    Image(systemName: "ellipsis").font(.body.weight(.bold))
                }
                .accessibilityLabel(Text("More"))
            }
        }
        .refreshable { await invest.refresh() }
        .task {
            await invest.load()
            if let last = invest.overview?.dividends.indices.last { month = last }
            withAnimation(.spring(response: 0.9, dampingFraction: 0.8)) { appeared = true }
            await invest.refresh()
        }
        .sheet(isPresented: $writing) {
            InvestThesisSheet(invest: invest)
        }
        .sheet(isPresented: $targeting) {
            InvestTargetsSheet(invest: invest)
        }
    }

    // MARK: The ring

    private var controls: some View {
        HStack(spacing: 10) {
            Menu {
                Picker(selection: $shown.animation(.spring(duration: 0.5))) {
                    Text("Portfolio").tag(InvestShown.allocation)
                    Text("Dividends").tag(InvestShown.dividends)
                } label: { EmptyView() }
            } label: {
                InvestPill(title: shown == .allocation ? String(localized: "Portfolio") : String(localized: "Dividends"))
            }
            Spacer(minLength: 4)
            if shown == .allocation {
                InvestSegments<InvestCut>(options: [.init(value: .classes, title: String(localized: "Class")),
                                                     .init(value: .assets, title: String(localized: "Asset"))],
                                           selection: $cut)
            } else {
                Text(overview.dividends.indices.contains(month) ? overview.dividends[month].long : "")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(AXYONPalette.dim)
            }
        }
    }

    /// The slices shown: at most seven, the rest as one.
    private var slices: [InvestSlice] {
        let all: [InvestSlice]
        switch shown {
        case .allocation: all = cut == .classes ? overview.byClass : overview.byAsset
        case .dividends: all = overview.dividends.indices.contains(month) ? overview.dividends[month].items : []
        }
        guard all.count > 8 else { return all }
        let rest = all.dropFirst(7)
        let value = rest.reduce(0) { $0 + $1.value }
        let weight = rest.reduce(0) { $0 + $1.weight }
        return Array(all.prefix(7)) + [InvestSlice(id: "others", label: String(localized: "Others"), value: value, weight: weight)]
    }

    private var middle: (caption: String, amount: Double, under: String) {
        switch shown {
        case .allocation:
            let gain = overview.gainPct.map { String(localized: "\(AXYONMoney.percent($0)) since bought") } ?? ""
            return (String(localized: "Total"), overview.total, gain)
        case .dividends:
            guard overview.dividends.indices.contains(month) else { return (String(localized: "Paid"), 0, "") }
            let m = overview.dividends[month]
            let year = overview.dividends.suffix(12).reduce(0) { $0 + $1.total }
            return (String(localized: "Paid in \(m.short)"), m.total, String(localized: "\(AXYONMoney.compact(year)) in 12 months"))
        }
    }

    private var ringBlock: some View {
        let shownSlices = appeared ? slices : []
        let center = middle
        return VStack(spacing: 22) {
            ZStack {
                InvestRing(values: shownSlices.map(\.value), colors: shownSlices.indices.map(InvestPalette.color))
                VStack(spacing: 4) {
                    Text(center.caption)
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(AXYONPalette.dim)
                    Text(AXYONMoney.compact(center.amount))
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .minimumScaleFactor(0.5)
                        .contentTransition(.numericText())
                        .animation(.spring(duration: 0.5), value: center.amount)
                    if !center.under.isEmpty {
                        Text(center.under)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(AXYONPalette.dim)
                            .lineLimit(1)
                            .minimumScaleFactor(0.7)
                    }
                }
                .padding(.horizontal, 44)
            }
            .frame(height: 270)
            .contentShape(Rectangle())
            .simultaneousGesture(DragGesture(minimumDistance: 24).onEnded { drag in
                guard shown == .dividends, abs(drag.translation.width) > abs(drag.translation.height) else { return }
                let next = drag.translation.width < 0 ? month + 1 : month - 1
                if overview.dividends.indices.contains(next) {
                    withAnimation(.spring(duration: 0.45)) { month = next }
                }
            })
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text("\(center.caption): \(AXYONMoney.text(center.amount))"))

            if slices.isEmpty {
                Text(shown == .allocation
                     ? String(localized: "Your portfolio shows here once AXYON knows it: from your computer, or told in the chat.")
                     : String(localized: "No dividend paid this month."))
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
            } else {
                InvestLegend(slices: slices)
            }
            if shown == .dividends { dividendsSource }
            NavigationLink {
                if shown == .allocation { AXYONWalletView() } else { InvestDividendsHistory(months: overview.dividends) }
            } label: {
                InvestOutlineLabel(title: String(localized: "History"))
            }
            .buttonStyle(.plain)
        }
    }
}

extension AXYONInvestView {
    /// Where the month's dividends come from: the user's sheet (and the day it was read), or the funds' announcements
    /// for what was held; a month the two disagree on says both.
    @ViewBuilder var dividendsSource: some View {
        if overview.dividends.indices.contains(month) {
            let shownMonth = overview.dividends[month]
            VStack(spacing: 4) {
                if shownMonth.source == "planilha" {
                    Label(sheetLine, systemImage: "tablecells").labelStyle(.titleAndIcon)
                    if !shownMonth.covered {
                        Text("Your sheet has no amount for this month.")
                    } else if shownMonth.diverges {
                        Text("The funds announced \(AXYONMoney.text(shownMonth.announced)) for what you held: worth checking.")
                            .foregroundStyle(InvestPalette.mixed)
                    }
                } else {
                    Label(String(localized: "From the funds' announcements, for what you held"), systemImage: "megaphone")
                        .labelStyle(.titleAndIcon)
                }
            }
            .font(.caption)
            .foregroundStyle(AXYONPalette.dim)
            .multilineTextAlignment(.center)
        }
    }

    private var sheetLine: String {
        let name = overview.dividendsSource.isEmpty ? String(localized: "Your sheet") : overview.dividendsSource
        guard !overview.dividendsRead.isEmpty else { return name }
        return name + " · " + String(localized: "read on \(investDay(overview.dividendsRead))")
    }
}

/// The share of the portfolio wanted in each asset, percent: the sheet's "% Ideal" when the computer sends it, or set
/// here. The committee weighs each asset against its target before any opinion on adding to it.
struct InvestTargetsSheet: View {
    let invest: AXYONInvest
    @Environment(\.dismiss) private var dismiss
    @State private var typed: [String: String] = [:]
    @State private var saving = false
    @State private var problem: String?

    private var assets: [InvestSlice] { invest.overview?.byAsset ?? [] }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if assets.isEmpty {
                        Text("Your portfolio shows here once AXYON knows it: from your computer, or told in the chat.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(assets) { asset in
                        HStack(spacing: 10) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(asset.label).font(.body.weight(.semibold))
                                Text("Now \(Self.share(asset.weight))").font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 8)
                            TextField(String(localized: "none"), text: field(asset))
                                .keyboardType(.decimalPad)
                                .multilineTextAlignment(.trailing)
                                .frame(width: 72)
                            Text(verbatim: "%").foregroundStyle(.secondary)
                        }
                    }
                } footer: {
                    Text("Your sheet's \"Percentual Ideal\" column, read by the computer, wins over a target set here. Empty: no target.")
                }
                if let problem {
                    Text(problem).foregroundStyle(InvestPalette.against)
                }
            }
            .scrollContentBackground(.hidden)
            .background { InvestBackdrop() }
            .navigationTitle("Targets per asset")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }.disabled(saving || typed.isEmpty)
                }
            }
        }
        .presentationDetents([.large])
    }

    /// "19,5%".
    static func share(_ value: Double) -> String {
        value.formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "pt_BR"))) + "%"
    }

    private func field(_ asset: InvestSlice) -> Binding<String> {
        Binding(get: {
            typed[asset.id] ?? asset.target.map { $0.formatted(.number.precision(.fractionLength(0...2)).locale(Locale(identifier: "pt_BR"))) } ?? ""
        }, set: { typed[asset.id] = $0 })
    }

    private func save() {
        saving = true
        problem = nil
        Task {
            defer { saving = false }
            for (ticker, text) in typed {
                let cleaned = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
                let value = cleaned.isEmpty ? nil : Double(cleaned)
                if !cleaned.isEmpty, value == nil {
                    problem = String(localized: "\(ticker): not a number.")
                    return
                }
                do {
                    try await invest.setTarget(ticker, value.flatMap { $0 > 0 ? $0 : nil })
                } catch {
                    problem = AXYONInvest.describe(error)
                    return
                }
            }
            dismiss()
        }
    }
}

/// The months under the pills: the one shown bold in the middle, its neighbours dimmer the farther they are.
struct InvestMonthsBar: View {
    let months: [InvestMonth]
    @Binding var index: Int

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(-2...2), id: \.self) { offset in
                let at = index + offset
                if months.indices.contains(at) {
                    Button {
                        withAnimation(.spring(duration: 0.45)) { index = at }
                    } label: {
                        Text(months[at].short)
                            .font(offset == 0 ? Font.title3.weight(.bold) : Font.callout.weight(.medium))
                            .foregroundStyle(Color.white.opacity(offset == 0 ? 1 : (abs(offset) == 1 ? 0.42 : 0.2)))
                            .frame(maxWidth: .infinity)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                } else {
                    Color.clear.frame(maxWidth: .infinity, maxHeight: 1)
                }
            }
        }
        .frame(height: 30)
    }
}

/// The legend in two columns: a dot, the name, the amount in bold.
struct InvestLegend: View {
    let slices: [InvestSlice]

    var body: some View {
        LazyVGrid(columns: [GridItem(.flexible(), spacing: 18), GridItem(.flexible(), spacing: 18)], spacing: 14) {
            ForEach(Array(slices.enumerated()), id: \.element.id) { index, slice in
                HStack(spacing: 8) {
                    Circle().fill(InvestPalette.color(index)).frame(width: 9, height: 9)
                    Text(slice.label)
                        .font(.footnote)
                        .foregroundStyle(Color.white.opacity(0.72))
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    Text(AXYONMoney.compact(slice.value))
                        .font(.footnote.weight(.bold))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                }
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// The dividends month by month, newest first.
struct InvestDividendsHistory: View {
    let months: [InvestMonth]

    var body: some View {
        List {
            ForEach(Array(months.reversed())) { month in
                Section {
                    if month.items.isEmpty {
                        Text("No dividend paid this month.").foregroundStyle(AXYONPalette.dim)
                    }
                    ForEach(month.items) { item in
                        HStack {
                            Text(item.label).foregroundStyle(.white)
                            Spacer()
                            Text(AXYONMoney.text(item.value)).monospacedDigit().foregroundStyle(.white)
                        }
                    }
                } header: {
                    HStack {
                        Text(month.long)
                        Spacer()
                        Text(AXYONMoney.text(month.total)).monospacedDigit()
                    }
                }
                .listRowBackground(Color.white.opacity(0.06))
            }
        }
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Dividends")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: - The committee

/// Five equal segments, each the colour of its mind's vote, the score in the middle.
struct InvestVoteRing: View {
    let minds: [InvestMind]
    let score: String
    var size: CGFloat = 52
    var lineWidth: CGFloat = 7

    var body: some View {
        ZStack {
            InvestRing(values: Array(repeating: 1.0, count: minds.count), colors: minds.map { InvestPalette.vote($0.vote) },
                       lineWidth: lineWidth, gap: 0.03, capacity: 5)
            Text(score)
                .font(.system(size: size * 0.24, weight: .bold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(.white)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .padding(lineWidth + 2)
        }
        .frame(width: size, height: size)
    }
}

struct InvestCommitteeCard: View {
    @Environment(HermesEngine.self) private var engine
    let invest: AXYONInvest
    @State private var proposal = ""
    @State private var problem: String?
    @State private var made: InvestVerdict?

    private var verdicts: [InvestVerdict] { invest.overview?.verdicts ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            InvestTitle(title: String(localized: "The committee"),
                        subtitle: String(localized: "Five minds that disagree in front of you, then the chair's score"))
            HStack(spacing: 10) {
                TextField(String(localized: "Buy 100 shares of GGRC11?"), text: $proposal, axis: .vertical)
                    .lineLimit(1...3)
                    .textInputAutocapitalization(.sentences)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    .background(Capsule().fill(Color.white.opacity(0.07)))
                    .disabled(invest.meeting != nil)
                Button {
                    convene()
                } label: {
                    Image(systemName: "arrow.up")
                        .font(.body.weight(.bold))
                        .foregroundStyle(.black)
                        .frame(width: 42, height: 42)
                        .background(Circle().fill(Color.white.opacity(canAsk ? 1 : 0.3)))
                }
                .buttonStyle(.plain)
                .disabled(!canAsk)
                .accessibilityLabel(Text("Convene the committee"))
            }
            if let meeting = invest.meeting {
                HStack(spacing: 10) {
                    ProgressView().tint(.white)
                    Text("The committee is meeting on \(meeting)…")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .lineLimit(2)
                }
            } else if !engine.canChat {
                Text("The committee needs AXYON Core running with a brain: see Settings.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
            }
            if let meetings = invest.overview?.meetings, !meetings.isEmpty, invest.meeting == nil {
                ForEach(meetings) { planned in
                    Label(String(localized: "Meeting due: \(planned.about)"), systemImage: "bell.badge")
                        .font(.footnote)
                        .foregroundStyle(InvestPalette.mixed)
                        .lineLimit(3)
                }
            }
            ForEach(verdicts.prefix(3)) { verdict in
                NavigationLink {
                    InvestVerdictView(verdict: verdict)
                } label: {
                    InvestVerdictRow(verdict: verdict)
                }
                .buttonStyle(.plain)
            }
            if verdicts.count > 3 {
                NavigationLink {
                    InvestVerdictsList(verdicts: verdicts)
                } label: {
                    Text("All the meetings").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
        .navigationDestination(item: $made) { verdict in
            InvestVerdictView(verdict: verdict)
        }
    }

    private var canAsk: Bool {
        engine.canChat && invest.meeting == nil && !proposal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func convene() {
        let text = proposal
        problem = nil
        Task {
            do {
                made = try await invest.ask(text)
                proposal = ""
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
    }
}

extension InvestVerdict: Hashable {
    func hash(into hasher: inout Hasher) { hasher.combine(id) }
}

/// A verdict in a line: its score ring, the asset, the chair's question, why it met.
struct InvestVerdictRow: View {
    let verdict: InvestVerdict

    var body: some View {
        HStack(spacing: 14) {
            InvestVoteRing(minds: verdict.minds, score: verdict.score)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(verdict.ticker).font(.headline).foregroundStyle(.white)
                    if verdict.vetoed {
                        InvestTag(text: String(localized: "veto"), color: InvestPalette.against)
                    }
                    if verdict.reason == "document" || verdict.reason == "thesis" {
                        InvestTag(text: String(localized: "by itself"), color: InvestPalette.mixed)
                    }
                    if !verdict.portfolio.view.isEmpty {
                        InvestTag(text: verdict.portfolio.viewName, color: InvestPalette.vote(verdict.portfolio.tone))
                    }
                    Spacer(minLength: 0)
                    Text(investDay(verdict.date)).font(.caption2).foregroundStyle(AXYONPalette.faint)
                }
                Text(verdict.question.isEmpty ? verdict.proposal : verdict.question)
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
            }
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
        }
        .contentShape(Rectangle())
    }
}

struct InvestVerdictsList: View {
    let verdicts: [InvestVerdict]

    var body: some View {
        ScrollView {
            VStack(spacing: 14) {
                ForEach(verdicts) { verdict in
                    NavigationLink {
                        InvestVerdictView(verdict: verdict)
                    } label: {
                        InvestVerdictRow(verdict: verdict).padding(14).axyonCard(cornerRadius: 22)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle("The committee")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// A meeting of the committee: the score; the two opinions, the asset itself and what to do with it in the
/// portfolio; what the audit found, the critical apart from the notes; what to verify; the chair's synthesis; each
/// mind's vote, why, its points with their kind and sources and what it lacked; then the facts and documents cited,
/// each with its dates. A source opens where it comes from.
struct InvestVerdictView: View {
    let verdict: InvestVerdict
    @State private var appeared = false
    @State private var opened: InvestCitation?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                header
                if !verdict.asset.isEmpty || !verdict.portfolio.isEmpty {
                    VStack(spacing: 12) {
                        opinion("A", String(localized: "Opinion A · The asset"), verdict.asset)
                        opinion("B", String(localized: "Opinion B · In your portfolio"), verdict.portfolio)
                    }
                }
                findings(String(localized: "Critical alerts"), "exclamationmark.octagon.fill", InvestPalette.against, verdict.critical)
                findings(String(localized: "Material points"), "exclamationmark.triangle.fill", InvestPalette.mixed, verdict.material)
                if !verdict.toVerify.isEmpty { toVerify }
                chair
                ForEach(verdict.minds) { mind in
                    InvestMindCard(mind: mind) { opened = $0 }
                }
                findings(String(localized: "Notes"), "info.circle.fill", InvestPalette.neutral, verdict.notes)
                if !verdict.facts.isEmpty || !verdict.documents.isEmpty { sources }
                Text("The committee never orders: its five minds are archetypes, every number is the code's and every point cites its source. Nothing is bought or sold; this is not an investment recommendation.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle(verdict.ticker)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { withAnimation(.spring(response: 0.9, dampingFraction: 0.8)) { appeared = true } }
        .sheet(item: $opened) { citation in
            InvestSourceSheet(citation: citation, verdict: verdict)
        }
    }

    private var vetoes: String { verdict.vetoedBy.map { InvestMind.name($0) }.joined(separator: ", ") }

    private var header: some View {
        VStack(spacing: 14) {
            ZStack {
                InvestRing(values: appeared ? Array(repeating: 1.0, count: verdict.minds.count) : [],
                           colors: verdict.minds.map { InvestPalette.vote($0.vote) }, lineWidth: 22, gap: 0.03, capacity: 5)
                VStack(spacing: 2) {
                    Text(verdict.score)
                        .font(.system(size: 44, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                    Text("for × against").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.dim)
                    if verdict.neutral > 0 {
                        Text("\(verdict.neutral) neutral").font(.caption2).foregroundStyle(AXYONPalette.faint)
                    }
                }
            }
            .frame(width: 210, height: 210)
            if verdict.vetoed {
                Label(String(localized: "Vetoed by \(vetoes)"), systemImage: "hand.raised.fill")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(InvestPalette.against)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(Capsule().fill(InvestPalette.against.opacity(0.14)))
            }
            Text("“\(verdict.proposal)”")
                .font(.callout)
                .foregroundStyle(AXYONPalette.dim)
                .multilineTextAlignment(.center)
            if !verdict.date.isEmpty {
                Text("Committee of \(investDay(verdict.date))").font(.caption2).foregroundStyle(AXYONPalette.faint)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// One of the two opinions, each on its own: a vote against buying more is not a call to sell.
    @ViewBuilder
    private func opinion(_ letter: String, _ title: String, _ given: InvestOpinion) -> some View {
        if !given.isEmpty {
            let color = InvestPalette.vote(given.tone)
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    Text(verbatim: letter)
                        .font(.caption.weight(.heavy))
                        .foregroundStyle(.black)
                        .frame(width: 24, height: 24)
                        .background(Circle().fill(color))
                    Text(title).font(.headline).foregroundStyle(.white)
                    Spacer(minLength: 4)
                    InvestTag(text: given.viewName, color: color)
                }
                if !given.why.isEmpty {
                    Text(given.why).font(.callout).foregroundStyle(Color.white.opacity(0.88))
                }
                HStack(spacing: 6) {
                    if let confidence = given.confidence {
                        Text("Confidence \(confidence)%").font(.caption2.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                    }
                    InvestCitations(text: given.sources.joined(separator: ", ")) { opened = $0 }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .axyonCard(cornerRadius: 22)
        }
    }

    /// What the audit found at one severity, by the auditor or by AXYON's code.
    @ViewBuilder
    private func findings(_ title: String, _ icon: String, _ color: Color, _ items: [InvestAuditItem]) -> some View {
        if !items.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                Label(title, systemImage: icon).font(.headline).foregroundStyle(color)
                ForEach(items) { item in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(item.text).font(.callout).foregroundStyle(Color.white.opacity(0.9))
                        HStack(spacing: 6) {
                            InvestTag(text: item.by == "auditor" ? String(localized: "auditor") : String(localized: "AXYON's code"),
                                      color: AXYONPalette.dim)
                            if !item.minds.isEmpty {
                                Text(item.minds.map { InvestMind.name($0) }.joined(separator: ", "))
                                    .font(.caption2)
                                    .foregroundStyle(AXYONPalette.faint)
                                    .lineLimit(1)
                            }
                            InvestCitations(text: item.sources.joined(separator: ", ")) { opened = $0 }
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .axyonCard(cornerRadius: 22, tint: color == InvestPalette.neutral ? nil : color)
        }
    }

    private var toVerify: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("To verify", systemImage: "checklist").font(.headline).foregroundStyle(.white)
            ForEach(Array(verdict.toVerify.enumerated()), id: \.offset) { _, line in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: "circle").font(.caption2).foregroundStyle(AXYONPalette.faint)
                    Text(line).font(.callout).foregroundStyle(Color.white.opacity(0.88))
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    private var chair: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("The chair", systemImage: "person.3.fill").font(.headline).foregroundStyle(.white)
            if !verdict.question.isEmpty {
                Text(verdict.question).font(.title3.weight(.semibold)).foregroundStyle(.white)
            }
            line(String(localized: "The losing side's best argument"), verdict.dissent)
            line(String(localized: "What would change the picture"), verdict.wouldChange)
            line(String(localized: "Against your own targets"), verdict.fit)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .axyonCard(cornerRadius: 24)
    }

    @ViewBuilder
    private func line(_ title: String, _ text: String) -> some View {
        if !text.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                Text(text).font(.callout).foregroundStyle(Color.white.opacity(0.85))
            }
        }
    }

    /// The facts the committee was given and the documents it read, each with its dates; a tap opens one.
    private var sources: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Sources", systemImage: "books.vertical.fill").font(.headline).foregroundStyle(.white)
            ForEach(verdict.documents) { doc in
                Button { opened = InvestCitation(ref: doc.id, page: nil) } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        InvestTag(text: doc.id)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(doc.title).font(.callout).foregroundStyle(.white).multilineTextAlignment(.leading)
                            Text([AXYONInvest.kindName(doc.kind), investDay(doc.delivered)].filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.caption2)
                                .foregroundStyle(AXYONPalette.faint)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            ForEach(verdict.facts) { fact in
                Button { opened = InvestCitation(ref: fact.id, page: nil) } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        InvestTag(text: fact.id, color: InvestPalette.status(fact.status))
                        VStack(alignment: .leading, spacing: 2) {
                            Text(fact.text).font(.footnote).foregroundStyle(Color.white.opacity(0.88)).multilineTextAlignment(.leading)
                            Text(InvestSourceSheet.dates(fact)).font(.caption2).foregroundStyle(AXYONPalette.faint).multilineTextAlignment(.leading)
                        }
                        Spacer(minLength: 0)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }
}

/// The sources a text names ("F3, D1 p.4"), each a tag that opens it.
struct InvestCitations: View {
    let text: String
    let open: (InvestCitation) -> Void

    var body: some View {
        let found = InvestCitation.parse(text)
        if !found.isEmpty {
            HStack(spacing: 4) {
                ForEach(found) { citation in
                    Button { open(citation) } label: { InvestTag(text: citation.label) }
                        .buttonStyle(.plain)
                        .accessibilityLabel(Text("Open the source \(citation.label)"))
                }
            }
        }
    }
}

extension InvestPalette {
    /// A fact's status: confirmed and computed plain, an estimate soft, missing grey, divergent red.
    static func status(_ status: String) -> Color {
        switch status {
        case "divergente": against
        case "estimativa": mixed
        case "ausente": neutral
        default: .white
        }
    }
}

/// One mind: who it is and what it watches, its vote and how sure it is, why, its veto's criterion, each point with
/// its kind, whether AXYON's code found it in its source, and the source to open; what it lacked; and when the audit
/// revised it, its vote before.
struct InvestMindCard: View {
    let mind: InvestMind
    var open: (InvestCitation) -> Void = { _ in }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Image(systemName: mind.symbol)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.black)
                    .frame(width: 38, height: 38)
                    .background(Circle().fill(InvestPalette.vote(mind.vote)))
                VStack(alignment: .leading, spacing: 2) {
                    Text(mind.name).font(.headline).foregroundStyle(.white)
                    Text(mind.role).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(2)
                }
                Spacer(minLength: 4)
                VStack(alignment: .trailing, spacing: 4) {
                    InvestTag(text: InvestMind.voteName(mind.vote), color: InvestPalette.vote(mind.vote))
                    if mind.veto {
                        InvestTag(text: String(localized: "veto"), color: InvestPalette.against)
                    }
                    if mind.revised {
                        InvestTag(text: String(localized: "revised"), color: InvestPalette.mixed)
                    }
                }
            }
            // How sure it is.
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.08))
                    Capsule().fill(InvestPalette.vote(mind.vote)).frame(width: geo.size.width * CGFloat(min(max(mind.confidence, 0), 100)) / 100)
                }
            }
            .frame(height: 5)
            .accessibilityLabel(Text("Sure at \(mind.confidence)%"))
            if mind.revised, !mind.was.isEmpty {
                Text("The audit changed its vote: it was \(InvestMind.voteName(mind.was)).")
                    .font(.caption)
                    .foregroundStyle(InvestPalette.mixed)
            }
            if !mind.why.isEmpty {
                Text(mind.why).font(.callout).foregroundStyle(.white)
            }
            if mind.veto, !mind.vetoReason.isEmpty {
                Label(String(localized: "Veto: \(InvestMind.vetoName(mind.vetoReason))"), systemImage: "hand.raised.fill")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(InvestPalette.against)
            }
            ForEach(mind.points) { point in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        mark(point)
                        Text(point.text).font(.callout).foregroundStyle(Color.white.opacity(0.88))
                        Spacer(minLength: 0)
                    }
                    HStack(spacing: 4) {
                        if !point.kind.isEmpty {
                            InvestTag(text: InvestMind.kindName(point.kind), color: AXYONPalette.dim)
                        }
                        InvestCitations(text: point.source, open: open)
                    }
                    .padding(.leading, 18)
                    if point.verified == false, !point.check.isEmpty {
                        Text(point.check).font(.caption2).foregroundStyle(InvestPalette.mixed).padding(.leading, 18)
                    }
                }
            }
            if mind.points.isEmpty {
                Text("Said nothing: short of data.").font(.footnote).foregroundStyle(AXYONPalette.faint)
            }
            if !mind.missing.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Missing data that could change its vote").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                    ForEach(Array(mind.missing.enumerated()), id: \.offset) { _, missing in
                        Text("· \(missing)").font(.footnote).foregroundStyle(AXYONPalette.dim)
                    }
                }
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }

    /// Whether AXYON's code found the point in its source: a seal when it did, a triangle when it did not.
    @ViewBuilder
    private func mark(_ point: InvestMind.Point) -> some View {
        switch point.verified {
        case true?:
            Image(systemName: "checkmark.seal.fill").font(.caption).foregroundStyle(InvestPalette.favor)
                .accessibilityLabel(Text("Found in its source"))
        case false?:
            Image(systemName: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(InvestPalette.mixed)
                .accessibilityLabel(Text("Not found in its source"))
        case nil:
            Text("•").foregroundStyle(AXYONPalette.faint)
        }
    }
}

/// A source opened: a fact with its source, dates, formula and status; or a document, the page cited read here, and
/// the link to open it where it was published.
struct InvestSourceSheet: View {
    let citation: InvestCitation
    let verdict: InvestVerdict
    @Environment(\.dismiss) private var dismiss
    @State private var page: String?
    @State private var loading = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if citation.isFact {
                        if let fact = verdict.fact(citation.ref) { factView(fact) } else { gone }
                    } else {
                        if let doc = verdict.document(citation.ref) { docView(doc) } else { gone }
                    }
                }
                .padding(20)
            }
            .background { InvestBackdrop() }
            .navigationTitle(citation.label)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .task { await loadPage() }
    }

    /// "Fonte · ref. 09/2026 · consultado em 29/10/2026".
    static func dates(_ fact: InvestFact) -> String {
        var parts = [fact.source]
        if !fact.reference.isEmpty { parts.append(String(localized: "ref. \(fact.reference)")) }
        if !fact.consulted.isEmpty { parts.append(String(localized: "read on \(investDay(fact.consulted))")) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private var gone: some View {
        Text("This source is not in the meeting's record.").font(.callout).foregroundStyle(AXYONPalette.dim)
    }

    private func factView(_ fact: InvestFact) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            InvestTag(text: fact.statusName, color: InvestPalette.status(fact.status))
            Text(fact.text).font(.title3.weight(.semibold)).foregroundStyle(.white)
            row(String(localized: "Source"), fact.source)
            row(String(localized: "Reference date"), fact.reference)
            row(String(localized: "Read on"), investDay(fact.consulted))
            row(String(localized: "Calculation"), fact.formula)
            if !fact.warning.isEmpty {
                Label(fact.warning, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(InvestPalette.mixed)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func docView(_ doc: InvestDocRef) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 6) {
                InvestTag(text: AXYONInvest.kindName(doc.kind))
                if !doc.delivered.isEmpty {
                    Text(investDay(doc.delivered)).font(.caption).foregroundStyle(AXYONPalette.faint)
                }
            }
            Text(doc.title).font(.title3.weight(.semibold)).foregroundStyle(.white)
            if let url = URL(string: doc.url), !doc.url.isEmpty {
                Link(destination: url) {
                    Label("Open the document", systemImage: "arrow.up.right.square").font(.callout.weight(.semibold))
                }
                .tint(.white)
            }
            if let number = citation.page {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Page \(number) of \(doc.pages)").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                    if loading {
                        ProgressView().tint(.white)
                    } else if let page, !page.isEmpty {
                        Text(page)
                            .font(.footnote)
                            .foregroundStyle(Color.white.opacity(0.85))
                            .textSelection(.enabled)
                    } else {
                        Text("The page's text is no longer kept on this iPhone: open the document.")
                            .font(.footnote)
                            .foregroundStyle(AXYONPalette.dim)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(14)
                .axyonCard(cornerRadius: 18)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func row(_ title: String, _ text: String) -> some View {
        if !text.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                Text(text).font(.callout).foregroundStyle(Color.white.opacity(0.88)).textSelection(.enabled)
            }
        }
    }

    private func loadPage() async {
        guard !citation.isFact, let number = citation.page, let doc = verdict.document(citation.ref) else { return }
        loading = true
        page = await AXYONInvest.shared.page(doc.key, number)
        loading = false
    }
}

// MARK: - The reader

struct InvestReaderCard: View {
    let invest: AXYONInvest

    private var feed: [InvestDoc] { invest.overview?.feed ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            InvestTitle(title: String(localized: "The intern who reads everything"),
                        subtitle: String(localized: "Your assets' reports, material facts and releases: only what changed"))
            if let reading = invest.reading {
                HStack(spacing: 10) {
                    ProgressView().tint(.white)
                    Text(reading).font(.footnote).foregroundStyle(AXYONPalette.dim)
                }
            } else if let pending = invest.overview?.pending, pending > 0 {
                HStack {
                    Text("\(pending) documents to read").font(.footnote).foregroundStyle(AXYONPalette.dim)
                    Spacer()
                    Button("Read now") {
                        Task { await invest.read(force: true); await invest.think() }
                    }
                    .font(.footnote.weight(.semibold))
                    .buttonStyle(WhiteCapsuleButtonStyle())
                    .controlSize(.small)
                }
            }
            if feed.isEmpty {
                Text("Nothing read yet: once your portfolio is in AXYON, the reader fetches its funds' and companies' documents from FundosNet and the CVM, and tells you what changed.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
            ForEach(feed.prefix(4)) { doc in
                NavigationLink {
                    InvestDocumentView(invest: invest, doc: doc)
                } label: {
                    InvestDocRow(doc: doc)
                }
                .buttonStyle(.plain)
            }
            if feed.count > 4 {
                NavigationLink {
                    InvestFeedView(invest: invest)
                } label: {
                    Text("Everything it read").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
    }
}

/// A document in a line: its tone, the asset and kind, its headline and the first thing that changed.
struct InvestDocRow: View {
    let doc: InvestDoc

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Circle()
                .fill(InvestPalette.tone(doc.tone))
                .frame(width: 10, height: 10)
                .padding(.top, 5)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(doc.ticker).font(.subheadline.weight(.bold)).foregroundStyle(.white)
                    InvestTag(text: AXYONInvest.kindName(doc.kind))
                    if doc.thesisRisk {
                        InvestTag(text: String(localized: "thesis at risk"), color: InvestPalette.against)
                    }
                    Spacer(minLength: 0)
                    Text(investDay(doc.delivered)).font(.caption2).foregroundStyle(AXYONPalette.faint)
                }
                Text(doc.headline).font(.footnote.weight(.semibold)).foregroundStyle(.white).lineLimit(2)
                if let first = doc.changes.first ?? doc.points.first?.text {
                    Text(first).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(2)
                }
            }
        }
        .contentShape(Rectangle())
    }
}

struct InvestFeedView: View {
    let invest: AXYONInvest

    var body: some View {
        ScrollView {
            VStack(spacing: 14) {
                ForEach(invest.overview?.feed ?? []) { doc in
                    NavigationLink {
                        InvestDocumentView(invest: invest, doc: doc)
                    } label: {
                        InvestDocRow(doc: doc).padding(14).axyonCard(cornerRadius: 20)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle("What the reader read")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// A document: the brain's points with their pages, what changed since the previous one, the figures the code found.
struct InvestDocumentView: View {
    let invest: AXYONInvest
    @State var doc: InvestDoc

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        InvestTag(text: AXYONInvest.kindName(doc.kind))
                        Text(investDay(doc.delivered)).font(.caption).foregroundStyle(AXYONPalette.faint)
                    }
                    Text(doc.headline).font(.title2.weight(.bold)).foregroundStyle(.white)
                    if doc.headline != doc.title {
                        Text(doc.title).font(.footnote).foregroundStyle(AXYONPalette.dim)
                    }
                    if let link = URL(string: doc.url), !doc.url.isEmpty {
                        Link(destination: link) {
                            Label("Open the document", systemImage: "arrow.up.right.square")
                                .font(.footnote.weight(.semibold))
                        }
                        .tint(.white)
                    }
                }
                if !doc.points.isEmpty {
                    section(String(localized: "What matters")) {
                        ForEach(doc.points) { point in
                            HStack(alignment: .firstTextBaseline, spacing: 8) {
                                Text("•").foregroundStyle(AXYONPalette.faint)
                                Text(point.text).font(.callout).foregroundStyle(Color.white.opacity(0.88))
                                Spacer(minLength: 0)
                                if let page = point.page { InvestTag(text: String(localized: "p. \(page)")) }
                            }
                        }
                    }
                } else if doc.problem.isEmpty {
                    Text("AXYON Core summarizes it when it is running; what the code found is below.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
                if !doc.problem.isEmpty {
                    Text(doc.problem).font(.footnote).foregroundStyle(InvestPalette.against)
                }
                if !doc.changes.isEmpty {
                    section(String(localized: "What changed since the previous one")) {
                        ForEach(doc.changes, id: \.self) { change in
                            Label(change, systemImage: "arrow.left.arrow.right").font(.callout).foregroundStyle(.white)
                        }
                    }
                }
                if !doc.figures.isEmpty {
                    section(String(localized: "The figures, as the code found them")) {
                        ForEach(doc.figures) { figure in
                            HStack {
                                Text(figure.id).font(.callout).foregroundStyle(AXYONPalette.dim)
                                Spacer()
                                Text(figure.shown).font(.callout.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                                InvestTag(text: String(localized: "p. \(figure.page)"))
                            }
                        }
                    }
                }
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle(doc.ticker)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            if let fresh = await invest.document(doc.id) { doc = fresh }
        }
    }

    private func section<Content: View>(_ title: String, @ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline).foregroundStyle(.white)
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }
}

// MARK: - The theses

struct InvestThesesCard: View {
    let invest: AXYONInvest
    @Binding var writing: Bool

    private var theses: [InvestThesis] { invest.overview?.theses ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top) {
                InvestTitle(title: String(localized: "Living theses"),
                            subtitle: String(localized: "Why you hold each asset, checked against every new report"))
                Button {
                    writing = true
                } label: {
                    Image(systemName: "plus")
                        .font(.body.weight(.bold))
                        .foregroundStyle(.black)
                        .frame(width: 34, height: 34)
                        .background(Circle().fill(Color.white))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Write a thesis"))
            }
            if theses.isEmpty {
                Text("Write why you hold an asset in your own words (P/VP below 1.05; yield above 9%; vacancy up to 10%): AXYON checks it against each new report and the price, and tells you the day it breaks.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
            ForEach(theses) { thesis in
                InvestThesisRow(thesis: thesis)
                    .contextMenu {
                        Button(role: .destructive) {
                            Task { await invest.removeThesis(thesis.id) }
                        } label: {
                            Label("Remove the thesis", systemImage: "trash")
                        }
                    }
            }
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
    }
}

struct InvestThesisRow: View {
    let thesis: InvestThesis

    private var status: (String, Color) {
        if thesis.broken > 0 { return (String(localized: "broke"), InvestPalette.against) }
        if !thesis.risks.isEmpty { return (String(localized: "at risk"), InvestPalette.mixed) }
        return (String(localized: "holds"), InvestPalette.favor)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(thesis.id).font(.headline).foregroundStyle(.white)
                InvestTag(text: status.0, color: status.1)
                Spacer()
                Text(investDay(thesis.written)).font(.caption2).foregroundStyle(AXYONPalette.faint)
            }
            Text(thesis.text).font(.footnote).foregroundStyle(AXYONPalette.dim).lineLimit(3)
            ForEach(thesis.checks) { check in
                HStack(spacing: 8) {
                    Image(systemName: check.holds == true ? "checkmark.circle.fill" : (check.holds == false ? "xmark.circle.fill" : "circle.dashed"))
                        .foregroundStyle(check.holds == true ? InvestPalette.favor : (check.holds == false ? InvestPalette.against : InvestPalette.neutral))
                    Text(check.label).font(.caption).foregroundStyle(.white).lineLimit(2)
                    Spacer(minLength: 4)
                    if let now = check.now {
                        Text(now.formatted(.number.precision(.fractionLength(0...2))))
                            .font(.caption.weight(.semibold))
                            .monospacedDigit()
                            .foregroundStyle(.white)
                    }
                }
            }
            ForEach(thesis.risks, id: \.self) { risk in
                Label(risk, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(InvestPalette.mixed)
                    .lineLimit(2)
            }
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.white.opacity(0.05)))
    }
}

struct InvestThesisSheet: View {
    let invest: AXYONInvest
    @Environment(\.dismiss) private var dismiss
    @State private var ticker = ""
    @State private var text = ""
    @State private var problem: String?
    @State private var saving = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Ticker (GGRC11)"), text: $ticker)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                } header: {
                    Text("The asset")
                }
                Section {
                    TextField(String(localized: "P/VP below 1.05; yield above 9%; vacancy up to 10%"), text: $text, axis: .vertical)
                        .lineLimit(3...8)
                } header: {
                    Text("Why you hold it")
                } footer: {
                    Text("Each condition with a number is checked against the price and every new report; the rest stays as your words, for the committee to read.")
                }
                if let problem {
                    Section { Text(problem).foregroundStyle(InvestPalette.against) }
                }
            }
            .scrollContentBackground(.hidden)
            .background { InvestBackdrop() }
            .navigationTitle("A living thesis")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(saving || ticker.trimmingCharacters(in: .whitespaces).isEmpty
                                  || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private func save() {
        saving = true
        problem = nil
        Task {
            do {
                try await invest.writeThesis(ticker.uppercased(), text)
                dismiss()
            } catch {
                problem = AXYONInvest.describe(error)
            }
            saving = false
        }
    }
}

// MARK: - The week aloud

struct InvestWeeklyCard: View {
    let invest: AXYONInvest
    @Environment(\.openURL) private var openURL
    @State private var playing = false
    @State private var audio: URL?
    @State private var recording = false
    @State private var problem: String?
    @State private var remind = UserDefaults.standard.bool(forKey: AXYONWeeklyAudio.remindKey)

    private var language: String { AXYONInvest.speech }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            InvestTitle(title: String(localized: "Your week in three minutes"),
                        subtitle: String(localized: "The portfolio, the dividends, what the reader read and the committee, aloud"))
            HStack(spacing: 16) {
                Button {
                    toggle()
                } label: {
                    Image(systemName: playing ? "stop.fill" : "play.fill")
                        .font(.title2.weight(.bold))
                        .foregroundStyle(.black)
                        .frame(width: 60, height: 60)
                        .background(Circle().fill(Color.white))
                }
                .buttonStyle(.plain)
                .disabled(invest.weekly == nil)
                .accessibilityLabel(Text(playing ? "Stop" : "Listen"))
                VStack(alignment: .leading, spacing: 3) {
                    Text(invest.weekly?.title ?? String(localized: "Your week"))
                        .font(.headline)
                        .foregroundStyle(.white)
                        .lineLimit(2)
                    Text(String(localized: "About \(max(1, (invest.weekly?.seconds ?? 180) / 60)) min"))
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                }
                Spacer(minLength: 0)
            }
            HStack(spacing: 10) {
                if let audio {
                    ShareLink(item: audio, preview: SharePreview(invest.weekly?.title ?? "AXYON", image: Image(systemName: "waveform"))) {
                        Label("Send the audio", systemImage: "waveform")
                    }
                    .buttonStyle(WhiteCapsuleButtonStyle())
                    .controlSize(.small)
                } else {
                    Button {
                        record()
                    } label: {
                        if recording {
                            ProgressView().tint(.black)
                        } else {
                            Label("Make the audio", systemImage: "waveform")
                        }
                    }
                    .buttonStyle(WhiteCapsuleButtonStyle())
                    .controlSize(.small)
                    .disabled(recording || invest.weekly == nil)
                }
                Button {
                    sendText()
                } label: {
                    Label("WhatsApp", systemImage: "paperplane.fill")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 9)
                        .overlay(Capsule().strokeBorder(Color.white.opacity(0.35), lineWidth: 1))
                }
                .buttonStyle(.plain)
                .disabled(invest.weekly == nil)
            }
            Toggle(isOn: $remind) {
                Text("Remind me every Sunday").font(.subheadline).foregroundStyle(.white)
            }
            .onChange(of: remind) { _, on in
                Task { await AXYONWeeklyAudio.remind(on) }
            }
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
            }
            Text("Made by AXYON's code from your own records; your portfolio stays on this iPhone. Not an investment recommendation.")
                .font(.caption2)
                .foregroundStyle(AXYONPalette.faint)
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
        .task { await invest.loadWeekly() }
        .onDisappear {
            if playing { MochiVoice.shared.stop() }
            playing = false
        }
    }

    private func toggle() {
        if playing {
            MochiVoice.shared.stop()
            playing = false
            return
        }
        guard let weekly = invest.weekly else { return }
        MochiVoice.shared.say(weekly.text, language: language)
        playing = true
        Task {
            await invest.weeklyTold()
            // The button comes back once the voice is done.
            while playing {
                try? await Task.sleep(for: .milliseconds(600))
                if !MochiVoice.shared.isSpeaking { playing = false }
            }
        }
    }

    private func record() {
        guard let weekly = invest.weekly else { return }
        recording = true
        problem = nil
        Task {
            do {
                audio = try await AXYONWeeklyAudio.render(weekly.text, language: language)
                await invest.weeklyTold()
            } catch {
                problem = AXYONInvest.describe(error)
            }
            recording = false
        }
    }

    private func sendText() {
        guard let weekly = invest.weekly, let url = AXYONWeeklyAudio.whatsApp(weekly.text) else { return }
        openURL(url) { opened in
            if !opened { problem = String(localized: "WhatsApp is not on this iPhone: send the audio instead.") }
        }
        Task { await invest.weeklyTold() }
    }
}

// MARK: - Learning

struct InvestLearnCard: View {
    let invest: AXYONInvest

    private var overview: InvestOverview { invest.overview ?? InvestOverview() }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            InvestTitle(title: String(localized: "Learn with your own portfolio"),
                        subtitle: String(localized: "Plain words, cards, a training portfolio and your own score"))
            Toggle(isOn: Binding(get: { overview.apprentice }, set: { on in Task { await invest.setApprentice(on) } })) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Apprentice mode").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    Text("The committee, the reader and the week explain each term the first time").font(.caption).foregroundStyle(AXYONPalette.dim)
                }
            }
            NavigationLink {
                InvestGlossaryView(invest: invest)
            } label: {
                AXYONRow(icon: "book.closed", title: String(localized: "Glossary"),
                         detail: String(localized: "Each term with its value in your assets"))
            }
            .buttonStyle(.plain)
            NavigationLink {
                InvestCardsView(invest: invest)
            } label: {
                AXYONRow(icon: "rectangle.on.rectangle.angled", title: String(localized: "Flashcards"),
                         detail: overview.cardsDue > 0 ? String(localized: "\(overview.cardsDue) to review today")
                             : String(localized: "Your portfolio's terms and shape"))
            }
            .buttonStyle(.plain)
            NavigationLink {
                InvestPaperView(invest: invest)
            } label: {
                AXYONRow(icon: "gamecontroller", title: String(localized: "Training portfolio"),
                         detail: overview.paper.started.isEmpty ? String(localized: "Practise with pretend money first")
                             : String(localized: "\(AXYONMoney.compact(overview.paper.value)) · \(AXYONMoney.percent(overview.paper.returnPct))"))
            }
            .buttonStyle(.plain)
            NavigationLink {
                InvestScoreView(invest: invest)
            } label: {
                AXYONRow(icon: "target", title: String(localized: "Your score"),
                         detail: overview.judged > 0 ? String(localized: "\(overview.judged) decisions judged")
                             : String(localized: "When you are sure, how often you are right"))
            }
            .buttonStyle(.plain)
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
    }
}

struct InvestGlossaryView: View {
    let invest: AXYONInvest
    @State private var search = ""

    private var terms: [InvestTerm] {
        let all = invest.learn?.glossary ?? []
        let wanted = search.trimmingCharacters(in: .whitespaces)
        guard !wanted.isEmpty else { return all }
        return all.filter { $0.term.localizedCaseInsensitiveContains(wanted) || $0.simple.localizedCaseInsensitiveContains(wanted) }
    }

    var body: some View {
        List(terms) { term in
            VStack(alignment: .leading, spacing: 6) {
                Text(term.term).font(.headline).foregroundStyle(.white)
                Text(term.simple).font(.callout).foregroundStyle(Color.white.opacity(0.85))
                Text(term.why).font(.footnote).foregroundStyle(AXYONPalette.dim)
                if let mine = term.mine {
                    Label(mine, systemImage: "person.crop.circle")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(InvestPalette.favor)
                }
            }
            .padding(.vertical, 4)
            .listRowBackground(Color.white.opacity(0.06))
        }
        .searchable(text: $search)
        .scrollContentBackground(.hidden)
        .background { InvestBackdrop() }
        .navigationTitle("Glossary")
        .navigationBarTitleDisplayMode(.inline)
        .task { await invest.loadLearn() }
    }
}

/// The cards of the user's own assets, one at a time: the question, then the answer, then how well it was known.
struct InvestCardsView: View {
    let invest: AXYONInvest
    @State private var flipped = false

    private var card: InvestCard? { invest.learn?.due.first }

    var body: some View {
        VStack(spacing: 24) {
            if let card {
                Text("\(invest.learn?.due.count ?? 0) to review").font(.footnote).foregroundStyle(AXYONPalette.dim)
                Button {
                    withAnimation(.spring(duration: 0.45)) { flipped.toggle() }
                } label: {
                    VStack(spacing: 16) {
                        Text(card.front).font(.title3.weight(.semibold)).foregroundStyle(.white).multilineTextAlignment(.center)
                        if flipped {
                            Divider().overlay(Color.white.opacity(0.15))
                            Text(card.back).font(.body).foregroundStyle(Color.white.opacity(0.88)).multilineTextAlignment(.center)
                        } else {
                            Text("Tap to see the answer").font(.footnote).foregroundStyle(AXYONPalette.faint)
                        }
                    }
                    .padding(26)
                    .frame(maxWidth: .infinity, minHeight: 260)
                    .axyonCard(cornerRadius: 30)
                }
                .buttonStyle(.plain)
                if flipped {
                    HStack(spacing: 10) {
                        grade(String(localized: "Forgot"), 1, InvestPalette.against, card)
                        grade(String(localized: "Hard"), 3, InvestPalette.mixed, card)
                        grade(String(localized: "Good"), 4, InvestPalette.favor, card)
                        grade(String(localized: "Easy"), 5, Color(hex: "#DDF6FA"), card)
                    }
                }
            } else {
                Image(systemName: "rectangle.on.rectangle.angled").font(.system(size: 44, weight: .light)).foregroundStyle(.white)
                Text("No card to review now").font(.title3.weight(.semibold)).foregroundStyle(.white)
                Text("AXYON makes cards from your own assets: what P/VP or the yield is, with your funds' values, and your portfolio's shape.")
                    .font(.callout)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
                Button("Make my cards") { Task { await invest.makeCards() } }
                    .buttonStyle(WhiteCapsuleButtonStyle())
            }
            Spacer()
        }
        .padding(20)
        .background { InvestBackdrop() }
        .navigationTitle("Flashcards")
        .navigationBarTitleDisplayMode(.inline)
        .task { await invest.loadLearn() }
    }

    private func grade(_ title: String, _ value: Int, _ color: Color, _ card: InvestCard) -> some View {
        Button {
            flipped = false
            Task { await invest.grade(card, value) }
        } label: {
            Text(title)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.black)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 11)
                .background(Capsule().fill(color))
        }
        .buttonStyle(.plain)
    }
}

/// The training portfolio: pretend money at the real prices, against the real portfolio and the CDI.
struct InvestPaperView: View {
    let invest: AXYONInvest
    @State private var ticker = ""
    @State private var quantity = ""
    @State private var problem: String?

    private var paper: InvestPaper { invest.learn?.paper ?? InvestPaper() }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(AXYONMoney.text(paper.started.isEmpty ? 10_000 : paper.value))
                        .font(.system(size: 38, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                    HStack(spacing: 22) {
                        AXYONFigure(value: AXYONMoney.percent(paper.returnPct), label: String(localized: "Training"),
                                    color: paper.returnPct >= 0 ? InvestPalette.favor : InvestPalette.against)
                        if let real = paper.realPct {
                            AXYONFigure(value: AXYONMoney.percent(real), label: String(localized: "Your real one"))
                        }
                        if let cdi = paper.cdiPct {
                            AXYONFigure(value: AXYONMoney.percent(cdi), label: "CDI")
                        }
                    }
                    Text(paper.started.isEmpty ? String(localized: "Start with R$ 10.000 of pretend money: buy and sell at the real prices, nothing real moves.")
                         : String(localized: "Since \(investDay(paper.started)) · cash \(AXYONMoney.text(paper.cash))"))
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(18)
                .axyonCard(cornerRadius: 24)

                VStack(alignment: .leading, spacing: 12) {
                    Text("A pretend trade").font(.headline).foregroundStyle(.white)
                    HStack(spacing: 10) {
                        TextField(String(localized: "Ticker"), text: $ticker)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .padding(12)
                            .background(Capsule().fill(Color.white.opacity(0.07)))
                        TextField(String(localized: "Quantity"), text: $quantity)
                            .keyboardType(.numberPad)
                            .padding(12)
                            .frame(width: 110)
                            .background(Capsule().fill(Color.white.opacity(0.07)))
                    }
                    HStack(spacing: 10) {
                        Button("Buy") { trade(sell: false) }.buttonStyle(WhiteCapsuleButtonStyle())
                        Button("Sell") { trade(sell: true) }.buttonStyle(WhiteCapsuleButtonStyle())
                        Spacer()
                        Button("Start again") { Task { await invest.resetPaper(nil) } }
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(AXYONPalette.dim)
                    }
                    if let problem {
                        Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
                    }
                }
                .padding(18)
                .axyonCard(cornerRadius: 24)

                if !paper.positions.isEmpty {
                    VStack(spacing: 0) {
                        ForEach(paper.positions) { position in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(position.id).font(.headline).foregroundStyle(.white)
                                    Text(String(localized: "\(position.quantity.formatted()) at \(AXYONMoney.text(position.average))"))
                                        .font(.caption)
                                        .foregroundStyle(AXYONPalette.dim)
                                }
                                Spacer()
                                Text(AXYONMoney.text(position.value)).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(.white)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                        }
                    }
                    .axyonCard(cornerRadius: 22)
                }
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle("Training portfolio")
        .navigationBarTitleDisplayMode(.inline)
        .task { await invest.loadLearn() }
    }

    private func trade(sell: Bool) {
        guard let count = Double(quantity.trimmingCharacters(in: .whitespaces)), count > 0 else {
            problem = String(localized: "How many shares?")
            return
        }
        problem = nil
        Task {
            do {
                try await invest.paper(ticker.uppercased(), quantity: count, sell: sell)
                quantity = ""
            } catch {
                problem = AXYONInvest.describe(error)
            }
        }
    }
}

/// The personal score: by how sure the user said they were, how often they were right.
struct InvestScoreView: View {
    let invest: AXYONInvest

    private var score: InvestScore { invest.learn?.score ?? InvestScore() }

    private var rightPct: Int {
        Int((Double(score.right) / Double(max(score.judged, 1)) * 100).rounded())
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    if score.judged > 0 {
                        Text("\(rightPct)%")
                            .font(.system(size: 52, weight: .bold, design: .rounded))
                            .foregroundStyle(.white)
                        Text("of \(score.judged) decisions judged right").font(.callout).foregroundStyle(AXYONPalette.dim)
                    } else {
                        Text("Not judged yet").font(.title2.weight(.bold)).foregroundStyle(.white)
                        Text("Tell AXYON in the chat what you decide and how sure you are, from 0 to 100: the decision journal judges each one later against the price and the CDI.")
                            .font(.callout)
                            .foregroundStyle(AXYONPalette.dim)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(18)
                .axyonCard(cornerRadius: 24)

                ForEach(score.bands.filter { $0.judged > 0 }) { band in
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text(String(localized: "When you say \(band.id) sure")).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                            Spacer()
                            Text("\(Int(band.rightPct.rounded()))%").font(.subheadline.weight(.bold)).monospacedDigit().foregroundStyle(.white)
                        }
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                Capsule().fill(Color.white.opacity(0.08))
                                Capsule().fill(InvestPalette.favor).frame(width: geo.size.width * CGFloat(min(max(band.rightPct, 0), 100) / 100))
                            }
                        }
                        .frame(height: 8)
                        Text("\(band.judged) decisions").font(.caption).foregroundStyle(AXYONPalette.dim)
                    }
                    .padding(16)
                    .axyonCard(cornerRadius: 22)
                }
                if let mine = score.averageReturn, let cdi = score.averageCdi {
                    Text(String(localized: "Your decisions made \(AXYONMoney.percent(mine)) on average; the CDI \(AXYONMoney.percent(cdi)) over the same days."))
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
            }
            .padding(20)
        }
        .background { InvestBackdrop() }
        .navigationTitle("Your score")
        .navigationBarTitleDisplayMode(.inline)
        .task { await invest.loadLearn() }
    }
}
