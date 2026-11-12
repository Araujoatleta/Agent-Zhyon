import Foundation
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The Carteira outside the app, and back (windows/powers/src/backup.rs): an Excel workbook that restores it all (the
/// lançamentos, the proventos recorded, the committee's analyses in each asset's folder), a CSV of its records, a PDF
/// report to read, print or send; and a file brought back, which replaces the Carteira of this iPhone (a copy of the
/// one here kept first) or, for B3's own files, adds their lançamentos and proventos.
@MainActor
enum AXYONBackup {
    enum Kind: String, Identifiable, CaseIterable {
        case excel, csv, pdf
        var id: String { rawValue }
    }

    private static func engine(_ tool: String, _ input: [String: Any]) async throws -> [String: Any] {
        let body = (try? JSONSerialization.data(withJSONObject: input)) ?? Data("{}".utf8)
        let answer = await HermesPowers.call(tool, input: body)
        let object = (try? JSONSerialization.jsonObject(with: answer)) as? [String: Any] ?? [:]
        guard object["ok"] as? Bool == true else {
            throw AXYONInvestError.said(object["text"] as? String ?? String(localized: "The superpowers did not answer."))
        }
        return object["data"] as? [String: Any] ?? [:]
    }

    /// The file of `kind`, in the app's temporary folder, ready to share or save.
    static func export(_ kind: Kind) async throws -> URL {
        if kind == .pdf {
            return try pdf(AXYONWallet.shared.overview ?? WalletOverview())
        }
        let answer = try await engine("invest_export", ["format": kind == .csv ? "csv" : "xlsx"])
        guard let name = answer["name"] as? String, let data = Data(base64Encoded: answer["base64"] as? String ?? "") else {
            throw AXYONInvestError.said(String(localized: "The file could not be made."))
        }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
        try data.write(to: url, options: .atomic)
        return url
    }

    /// Where the copies kept before an import go: Application Support/AXYON/backups.
    private static var kept: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("AXYON/backups", isDirectory: true)
    }

    /// Brings `url` back: the Carteira of this iPhone is replaced by the file's (a copy of it kept first, the last ten
    /// of them); B3's files add their lançamentos and proventos. What was done, in words.
    static func restore(_ url: URL) async throws -> String {
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        let data = try Data(contentsOf: url)
        guard data.count < 40_000_000 else { throw AXYONInvestError.said(String(localized: "This file is too big to be a Carteira.")) }
        // What is here now, kept first.
        if let book = try? await engine("invest_export", ["format": "xlsx"]),
           let bytes = Data(base64Encoded: book["base64"] as? String ?? "") {
            try? FileManager.default.createDirectory(at: kept, withIntermediateDirectories: true)
            let stamp = Date.now.formatted(.iso8601.year().month().day().time(includingFractionalSeconds: false))
                .replacingOccurrences(of: ":", with: "-")
            try? bytes.write(to: kept.appendingPathComponent("antes-de-importar-\(stamp).xlsx"), options: .atomic)
            let old = (try? FileManager.default.contentsOfDirectory(at: kept, includingPropertiesForKeys: nil)) ?? []
            for file in old.sorted(by: { $0.lastPathComponent > $1.lastPathComponent }).dropFirst(10) {
                try? FileManager.default.removeItem(at: file)
            }
        }
        let answer = try await engine("invest_import", ["name": url.lastPathComponent, "base64": data.base64EncodedString()])
        await AXYONWallet.shared.load()
        await AXYONFolders.shared.load()
        HermesEngine.shared.portfolioChanged()
        return answer["note"] as? String ?? String(localized: "Carteira restored.")
    }

    // MARK: The PDF report

    /// A report of the Carteira on A4 pages: the summary, the positions, the proventos and the lançamentos.
    static func pdf(_ o: WalletOverview) throws -> URL {
        let stamp = Date.now.formatted(.iso8601.year().month().day())
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("AXYON-carteira-\(stamp).pdf")
        let page = CGRect(x: 0, y: 0, width: 595, height: 842)
        let format = UIGraphicsPDFRendererFormat()
        format.documentInfo = [kCGPDFContextTitle as String: "AXYON · Carteira", kCGPDFContextCreator as String: "AXYON"]
        let renderer = UIGraphicsPDFRenderer(bounds: page, format: format)
        let accent = UIColor(AXYONTheme.current.accentDeep)
        try renderer.writePDF(to: url) { context in
            var report = PDFReport(context: context, page: page, accent: accent)
            report.start()
            report.title(String(localized: "Your Carteira"), Date.now.formatted(date: .long, time: .shortened))
            report.figures([
                (String(localized: "Net worth"), AXYONMoney.text(o.total)),
                (String(localized: "Invested"), AXYONMoney.text(o.cost)),
                (String(localized: "Result"), AXYONMoney.signed(o.gain) + (o.gainPct.map { " (" + WalletNumber.percent($0) + ")" } ?? "")),
                (String(localized: "Realized in sales"), AXYONMoney.signed(o.realized)),
                (String(localized: "Proventos in 12 months"), AXYONMoney.text(o.twelve)),
                (String(localized: "To receive"), AXYONMoney.text(o.provisioned)),
                (String(localized: "This year"), AXYONMoney.text(o.year)),
                (String(localized: "Yield on cost"), o.yieldOnCost.map { WalletNumber.percent($0, places: 2) } ?? "—"),
            ])
            report.heading(String(localized: "Positions"))
            report.table(
                [String(localized: "Asset"), String(localized: "Qty"), String(localized: "Avg price"), String(localized: "Price"),
                 String(localized: "Value"), String(localized: "Result"), String(localized: "Weight")],
                widths: [0.15, 0.11, 0.15, 0.14, 0.17, 0.17, 0.11],
                rows: o.classes.flatMap { group in
                    group.assets.map { a in
                        [a.ticker, WalletNumber.quantity(a.quantity), WalletNumber.price(a.average), a.price.map(WalletNumber.price) ?? "—",
                         AXYONMoney.text(a.value), AXYONMoney.signed(a.gain), WalletNumber.percent(a.weight)]
                    }
                })
            report.heading(String(localized: "Proventos received"))
            report.table(
                [String(localized: "Asset"), String(localized: "Kind"), String(localized: "Data-com"), String(localized: "Paid on"),
                 String(localized: "Shares"), String(localized: "Net")],
                widths: [0.15, 0.2, 0.16, 0.16, 0.13, 0.2],
                rows: (o.pending + o.received).map { p in
                    [p.ticker, p.kind + (p.recorded ? " *" : ""), p.record.isEmpty ? "—" : investDay(p.record),
                     p.payment.map { investDay($0) } ?? "—", p.quantity > 0 ? WalletNumber.quantity(p.quantity) : "—", AXYONMoney.text(p.net)]
                })
            report.note(String(localized: "* recorded by you, as your statement shows it."))
            report.heading(String(localized: "Lançamentos"))
            report.table(
                [String(localized: "Date"), String(localized: "Asset"), String(localized: "Operation"), String(localized: "Qty"),
                 String(localized: "Price"), String(localized: "Total")],
                widths: [0.16, 0.14, 0.18, 0.14, 0.17, 0.21],
                rows: o.trades.map { t in
                    [investDay(t.date), t.ticker,
                     t.kind == "sell" ? String(localized: "Sale") : t.kind == "opening" ? String(localized: "Opening balance") : String(localized: "Purchase"),
                     WalletNumber.quantity(t.quantity), WalletNumber.price(t.price), AXYONMoney.text(t.total)]
                })
            report.note(String(localized: "To restore the Carteira, import the Excel or CSV file in AXYON → Carteira → Backup. This report is for reading."))
            report.finish()
        }
        return url
    }
}

/// The report's pages, drawn top to bottom: a band of the accent at the top, a page number at the bottom.
private struct PDFReport {
    let context: UIGraphicsPDFRendererContext
    let page: CGRect
    let accent: UIColor
    var y: CGFloat = 0
    var number = 0
    let margin: CGFloat = 40

    init(context: UIGraphicsPDFRendererContext, page: CGRect, accent: UIColor) {
        self.context = context
        self.page = page
        self.accent = accent
    }

    private var width: CGFloat { page.width - margin * 2 }

    private func font(_ size: CGFloat, _ weight: UIFont.Weight = .regular) -> UIFont { .systemFont(ofSize: size, weight: weight) }

    private func draw(_ text: String, at rect: CGRect, size: CGFloat, weight: UIFont.Weight = .regular, color: UIColor = .black,
                      align: NSTextAlignment = .left) {
        let style = NSMutableParagraphStyle()
        style.alignment = align
        style.lineBreakMode = .byTruncatingTail
        (text as NSString).draw(in: rect, withAttributes: [.font: font(size, weight), .foregroundColor: color, .paragraphStyle: style])
    }

    mutating func start() {
        context.beginPage()
        number += 1
        let band = CGRect(x: 0, y: 0, width: page.width, height: 54)
        accent.setFill()
        UIRectFill(band)
        draw("AXYON", at: CGRect(x: margin, y: 17, width: 200, height: 22), size: 16, weight: .heavy, color: .white)
        draw(String(localized: "Carteira"), at: CGRect(x: page.width - margin - 200, y: 19, width: 200, height: 20), size: 12,
             weight: .semibold, color: .white, align: .right)
        draw("\(number)", at: CGRect(x: margin, y: page.height - 30, width: width, height: 14), size: 9, color: .gray, align: .center)
        y = 74
    }

    mutating func finish() {}

    mutating func room(_ height: CGFloat) {
        if y + height > page.height - 44 { start() }
    }

    mutating func title(_ text: String, _ subtitle: String) {
        draw(text, at: CGRect(x: margin, y: y, width: width, height: 30), size: 22, weight: .bold)
        draw(subtitle, at: CGRect(x: margin, y: y + 30, width: width, height: 16), size: 10, color: .darkGray)
        y += 58
    }

    /// Figures two by row, each a label over its value.
    mutating func figures(_ items: [(String, String)]) {
        let column = width / 2
        for pair in stride(from: 0, to: items.count, by: 2) {
            room(40)
            for (i, item) in items[pair..<min(pair + 2, items.count)].enumerated() {
                let x = margin + CGFloat(i) * column
                draw(item.0, at: CGRect(x: x, y: y, width: column - 10, height: 13), size: 9, color: .darkGray)
                draw(item.1, at: CGRect(x: x, y: y + 13, width: column - 10, height: 20), size: 14, weight: .semibold)
            }
            y += 40
        }
        y += 6
    }

    mutating func heading(_ text: String) {
        room(60)
        y += 8
        accent.setFill()
        UIRectFill(CGRect(x: margin, y: y + 3, width: 4, height: 16))
        draw(text, at: CGRect(x: margin + 10, y: y, width: width - 10, height: 22), size: 14, weight: .bold)
        y += 28
    }

    mutating func note(_ text: String) {
        room(30)
        let rect = CGRect(x: margin, y: y, width: width, height: 28)
        draw(text, at: rect, size: 8.5, color: .darkGray)
        y += 26
    }

    mutating func table(_ titles: [String], widths: [CGFloat], rows: [[String]]) {
        let height: CGFloat = 18
        func drawTitles(_ me: inout PDFReport) {
            me.room(height * 2)
            me.accent.withAlphaComponent(0.12).setFill()
            UIRectFill(CGRect(x: me.margin, y: me.y, width: me.width, height: height))
            var x = me.margin
            for (i, title) in titles.enumerated() where i < widths.count {
                let w = me.width * widths[i]
                me.draw(title, at: CGRect(x: x + 4, y: me.y + 4, width: w - 8, height: height - 4), size: 8.5, weight: .bold,
                        color: me.accent, align: i == 0 ? .left : .right)
                x += w
            }
            me.y += height
        }
        drawTitles(&self)
        if rows.isEmpty {
            draw(String(localized: "Nothing yet."), at: CGRect(x: margin + 4, y: y + 4, width: width, height: height), size: 9, color: .gray)
            y += height + 6
            return
        }
        for (r, row) in rows.enumerated() {
            if y + height > page.height - 44 {
                start()
                drawTitles(&self)
            }
            if r % 2 == 1 {
                UIColor(white: 0.96, alpha: 1).setFill()
                UIRectFill(CGRect(x: margin, y: y, width: width, height: height))
            }
            var x = margin
            for (i, cell) in row.enumerated() where i < widths.count {
                let w = width * widths[i]
                draw(cell, at: CGRect(x: x + 4, y: y + 4, width: w - 8, height: height - 4), size: 8.5,
                     weight: i == 0 ? .semibold : .regular, align: i == 0 ? .left : .right)
                x += w
            }
            y += height
        }
        y += 8
    }
}

/// The system's share sheet for a file: Save to Files, iCloud Drive, AirDrop, WhatsApp, Mail.
struct AXYONShareSheet: UIViewControllerRepresentable {
    let url: URL

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: [url], applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// A file to share, as a sheet presents it.
struct AXYONSharedFile: Identifiable {
    let url: URL
    var id: String { url.absoluteString }
}

/// The Carteira's backup: the three files to take it out, and the way to bring one back.
struct WalletBackupSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var making: AXYONBackup.Kind?
    @State private var shared: AXYONSharedFile?
    @State private var picking = false
    @State private var confirming: URL?
    @State private var restoring = false
    @State private var said: String?
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    hero
                    AXYONHeader(title: String(localized: "Take it out"))
                    exportRow(.excel, icon: "tablecells.fill", title: String(localized: "Excel, complete"),
                              detail: String(localized: "Positions, lançamentos and proventos in sheets, and a hidden sheet with everything AXYON keeps (the committee's analyses too): importing it brings it all back."))
                    exportRow(.csv, icon: "doc.plaintext.fill", title: String(localized: "CSV"),
                              detail: String(localized: "The positions, lançamentos, proventos recorded, purchase days and targets, one per line: Excel and Google Sheets open it, and importing it brings the Carteira back."))
                    exportRow(.pdf, icon: "doc.richtext.fill", title: String(localized: "PDF report"),
                              detail: String(localized: "The summary, the positions, the proventos and the lançamentos on A4 pages, to read, print or send."))
                    AXYONHeader(title: String(localized: "Bring it back"))
                    importRow
                    if let said {
                        Label(said, systemImage: "checkmark.seal.fill")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(InvestPalette.favor)
                            .padding(14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .axyonCard(cornerRadius: 18, tint: InvestPalette.favor)
                    }
                    if let problem {
                        Text(problem).font(.footnote).foregroundStyle(InvestPalette.against)
                    }
                }
                .padding(20)
            }
            .background { InvestBackdrop() }
            .navigationTitle(String(localized: "Backup"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(item: $shared) { file in
                AXYONShareSheet(url: file.url).presentationDetents([.medium, .large])
            }
            .fileImporter(isPresented: $picking, allowedContentTypes: Self.importable) { result in
                if case .success(let url) = result { confirming = url }
            }
            .confirmationDialog(String(localized: "Import this file?"), isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
                                titleVisibility: .visible, presenting: confirming) { url in
                Button(String(localized: "Import \(url.lastPathComponent)")) { restore(url) }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("An AXYON backup replaces the Carteira of this iPhone with the file's; a copy of the one here is kept first. A B3 file adds its lançamentos and proventos.")
            }
        }
        .presentationDetents([.large])
    }

    private static var importable: [UTType] {
        var types: [UTType] = [.commaSeparatedText, .plainText, .spreadsheet]
        if let xlsx = UTType(filenameExtension: "xlsx") { types.insert(xlsx, at: 0) }
        return types
    }

    private var hero: some View {
        HStack(spacing: 14) {
            Image(systemName: "archivebox.fill")
                .font(.system(size: 26, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 58, height: 58)
                .background(Circle().fill(AXYONTheme.current.accentGradient))
                .axyonGlow(radius: 14, strength: 0.6)
            VStack(alignment: .leading, spacing: 4) {
                Text("Your Carteira, safe outside the app").font(.headline).foregroundStyle(.white)
                Text("Save the file in Files, iCloud Drive or send it to yourself. If AXYON is ever reinstalled or something goes wrong, import it and everything comes back.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(16)
        .axyonCard(cornerRadius: 24)
    }

    private func exportRow(_ kind: AXYONBackup.Kind, icon: String, title: String, detail: String) -> some View {
        Button {
            Task {
                making = kind
                problem = nil
                defer { making = nil }
                do {
                    shared = AXYONSharedFile(url: try await AXYONBackup.export(kind))
                    Haptics.success()
                } catch {
                    problem = AXYONInvest.describe(error)
                }
            }
        } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: icon)
                    .font(.title3)
                    .foregroundStyle(AXYONTheme.current.accent)
                    .frame(width: 34)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    Text(detail).font(.caption).foregroundStyle(AXYONPalette.dim).multilineTextAlignment(.leading)
                }
                Spacer(minLength: 6)
                if making == kind {
                    ProgressView()
                } else {
                    Image(systemName: "square.and.arrow.up").font(.callout.weight(.semibold)).foregroundStyle(AXYONPalette.dim)
                }
            }
            .padding(14)
            .axyonCard(cornerRadius: 20)
        }
        .buttonStyle(PressableButtonStyle())
        .disabled(making != nil || restoring)
    }

    private var importRow: some View {
        Button {
            picking = true
        } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "square.and.arrow.down.fill")
                    .font(.title3)
                    .foregroundStyle(.white)
                    .frame(width: 34)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Import a file").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    Text("An AXYON backup (Excel or CSV) brings the Carteira back as it was. B3's Negociação and Movimentação files add their lançamentos and proventos.")
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                        .multilineTextAlignment(.leading)
                }
                Spacer(minLength: 6)
                if restoring { ProgressView() }
            }
            .padding(14)
            .axyonCard(cornerRadius: 20, tint: AXYONTheme.current.accent)
        }
        .buttonStyle(PressableButtonStyle())
        .disabled(making != nil || restoring)
    }

    private func restore(_ url: URL) {
        Task {
            restoring = true
            problem = nil
            said = nil
            defer { restoring = false }
            do {
                said = try await AXYONBackup.restore(url)
                Haptics.success()
            } catch {
                problem = AXYONInvest.describe(error)
                Haptics.error()
            }
        }
    }
}
