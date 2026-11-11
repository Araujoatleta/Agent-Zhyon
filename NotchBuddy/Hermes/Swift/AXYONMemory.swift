import Accelerate
import Foundation
import NaturalLanguage
import SwiftUI

// AXYON's whole memory, searched by meaning, on the phone: every page of every document the intern read, each of the
// court's verdicts, the user's decisions and theses, what the ears transcribed and the notes kept. Each passage gets
// a vector from Apple's on-device sentence embedding (NaturalLanguage, Portuguese) and its words go into an inverted
// index; a question is answered by both at once (meaning and words), with the document, its date and its page.
// Nothing leaves the iPhone: the index is two files in the app's own folder.

struct MemoryChunk: Codable, Sendable, Equatable {
    /// document, verdict, decision, thesis, transcript or note.
    var source: String
    /// The document's key, the record's id.
    var key: String
    var ticker: String
    var title: String
    /// YYYY-MM-DD, or "".
    var date: String
    /// 1-based; 0 when it has no pages.
    var page: Int
    var text: String
}

struct MemoryHit: Identifiable, Sendable, Equatable {
    let id: Int
    let chunk: MemoryChunk
    /// 0 to 1: how close it is, by meaning and by words together.
    let score: Double
    let meaning: Double
    let words: Double
}

struct MemoryStats: Sendable, Equatable {
    var passages = 0
    var sources = 0
    /// The sentence embedding is on this iPhone: the search is by meaning too.
    var byMeaning = false
    var bytes = 0
}

/// The index itself, off the main thread: embedding thousands of passages takes the processor a while.
actor AXYONMemoryIndex {
    static let shared = AXYONMemoryIndex()

    private struct Posting {
        let chunk: Int32
        let count: Float
    }

    private var chunks: [MemoryChunk] = []
    /// Chunk i's unit vector at i * dim (zeros when it could not be embedded).
    private var vectors: [Float] = []
    private var dim = 0
    private var postings: [String: [Posting]] = [:]
    private var lengths: [Float] = []
    private var loaded = false
    private let embedding: NLEmbedding? = NLEmbedding.sentenceEmbedding(for: .portuguese)

    private static var folder: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/memory", isDirectory: true)
    }

    private static let stopwords: Set<String> = [
        "de", "da", "do", "das", "dos", "e", "a", "o", "as", "os", "que", "em", "para", "com", "um", "uma", "no", "na",
        "nos", "nas", "por", "se", "ao", "aos", "ou", "como", "mais", "foi", "ser", "sao", "the", "of", "and", "to", "in",
    ]

    /// Words as the index keeps them: no case, no accents, no short or empty words.
    static func words(_ text: String) -> [String] {
        let folded = text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "pt_BR"))
        return folded.split { !($0.isLetter || $0.isNumber) }
            .map { String($0) }
            .filter { $0.count > 1 && !stopwords.contains($0) }
    }

    /// A page cut into passages of about `size` characters, at the ends of sentences.
    static func passages(_ text: String, size: Int = 900) -> [String] {
        let clean = text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }.joined(separator: " ")
        guard clean.count > size else { return clean.count > 20 ? [clean] : [] }
        var out: [String] = []
        var rest = Substring(clean)
        while rest.count > size {
            let window = rest.prefix(size)
            let cut = window.lastIndex(where: { ".!?;".contains($0) }).map { rest.index(after: $0) } ?? window.endIndex
            let piece = rest[rest.startIndex..<cut].trimmingCharacters(in: .whitespaces)
            if piece.count > 20 { out.append(piece) }
            rest = rest[cut...]
        }
        let tail = rest.trimmingCharacters(in: .whitespaces)
        if tail.count > 20 { out.append(tail) }
        return out
    }

    func load() {
        guard !loaded else { return }
        loaded = true
        let folder = Self.folder
        guard let data = try? Data(contentsOf: folder.appendingPathComponent("passages.json")),
              let kept = try? JSONDecoder().decode([MemoryChunk].self, from: data) else { return }
        chunks = kept
        let raw = (try? Data(contentsOf: folder.appendingPathComponent("vectors.f32"))) ?? Data()
        let width = embedding?.dimension ?? 0
        if width > 0, raw.count == kept.count * width * MemoryLayout<Float>.size {
            dim = width
            vectors = raw.withUnsafeBytes { Array($0.bindMemory(to: Float.self)) }
        } else {
            // The vectors do not match the passages (an older file, another embedding): made again.
            dim = width
            vectors = []
            for chunk in kept { vectors.append(contentsOf: vector(chunk.text)) }
        }
        rebuildWords()
    }

    private func rebuildWords() {
        postings = [:]
        lengths = []
        for (index, chunk) in chunks.enumerated() { addWords(chunk.text, at: index) }
    }

    private func addWords(_ text: String, at index: Int) {
        var counts: [String: Float] = [:]
        let all = Self.words(text)
        for word in all { counts[word, default: 0] += 1 }
        for (word, count) in counts {
            postings[word, default: []].append(Posting(chunk: Int32(index), count: count))
        }
        lengths.append(Float(all.count))
    }

    /// A text's unit vector (zeros when the embedding is not on this iPhone or does not know the text).
    private func vector(_ text: String) -> [Float] {
        guard dim > 0 else { return [] }
        guard let embedding, let values = embedding.vector(for: String(text.prefix(1200))), values.count == dim else {
            return [Float](repeating: 0, count: dim)
        }
        let out = values.map { Float($0) }
        var norm: Float = 0
        vDSP_svesq(out, 1, &norm, vDSP_Length(dim))
        norm = norm.squareRoot()
        return norm > 0 ? out.map { $0 / norm } : out
    }

    /// The keys of what is kept already.
    func keys() -> Set<String> {
        load()
        return Set(chunks.map(\.key))
    }

    func add(_ new: [MemoryChunk]) {
        load()
        if dim == 0 { dim = embedding?.dimension ?? 0 }
        for chunk in new {
            chunks.append(chunk)
            vectors.append(contentsOf: vector(chunk.text))
            addWords(chunk.text, at: chunks.count - 1)
        }
    }

    func remove(keys gone: Set<String>) {
        load()
        guard !gone.isEmpty, chunks.contains(where: { gone.contains($0.key) }) else { return }
        var kept: [MemoryChunk] = []
        var keptVectors: [Float] = []
        for (index, chunk) in chunks.enumerated() where !gone.contains(chunk.key) {
            kept.append(chunk)
            if dim > 0, vectors.count >= (index + 1) * dim {
                keptVectors.append(contentsOf: vectors[(index * dim)..<((index + 1) * dim)])
            }
        }
        chunks = kept
        vectors = keptVectors
        rebuildWords()
    }

    func clear() {
        chunks = []
        vectors = []
        postings = [:]
        lengths = []
        loaded = true
        save()
    }

    func save() {
        let folder = Self.folder
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(chunks) {
            try? data.write(to: folder.appendingPathComponent("passages.json"), options: .atomic)
        }
        let raw = vectors.withUnsafeBufferPointer { Data(buffer: $0) }
        try? raw.write(to: folder.appendingPathComponent("vectors.f32"), options: .atomic)
    }

    func stats() -> MemoryStats {
        load()
        let folder = Self.folder
        let size = ["passages.json", "vectors.f32"].reduce(0) { sum, name in
            sum + (((try? FileManager.default.attributesOfItem(atPath: folder.appendingPathComponent(name).path))?[.size] as? NSNumber)?.intValue ?? 0)
        }
        return MemoryStats(passages: chunks.count, sources: Set(chunks.map(\.key)).count, byMeaning: embedding != nil, bytes: size)
    }

    /// The passages closest to `query`, by meaning (the vectors) and by words (BM25) together.
    func search(_ query: String, limit: Int = 12, ticker: String? = nil) -> [MemoryHit] {
        load()
        let count = chunks.count
        guard count > 0, !query.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        // By words.
        var lexical = [Float](repeating: 0, count: count)
        let average = max(1, lengths.reduce(0, +) / Float(count))
        for word in Set(Self.words(query)) {
            guard let list = postings[word] else { continue }
            let idf = log(1 + (Float(count) - Float(list.count) + 0.5) / (Float(list.count) + 0.5))
            for p in list {
                let length = lengths[Int(p.chunk)]
                lexical[Int(p.chunk)] += idf * (p.count * 2.4) / (p.count + 1.4 * (0.25 + 0.75 * length / average))
            }
        }
        let top = lexical.max() ?? 0
        // By meaning.
        var meaning = [Float](repeating: 0, count: count)
        let asked = vector(query)
        if dim > 0, asked.count == dim, vectors.count == count * dim, asked.contains(where: { $0 != 0 }) {
            vectors.withUnsafeBufferPointer { all in
                asked.withUnsafeBufferPointer { q in
                    guard let base = all.baseAddress, let question = q.baseAddress else { return }
                    for index in 0..<count {
                        var dot: Float = 0
                        vDSP_dotpr(base + index * dim, 1, question, 1, &dot, vDSP_Length(dim))
                        meaning[index] = dot
                    }
                }
            }
        }
        let wanted = ticker?.uppercased()
        var scored: [(Int, Double, Double, Double)] = []
        for index in 0..<count {
            let byWords = top > 0 ? Double(lexical[index] / top) : 0
            // A cosine under 0.2 says little between sentences: the scale starts there.
            let byMeaning = max(0, min(1, (Double(meaning[index]) - 0.2) / 0.6))
            guard byWords > 0 || byMeaning > 0.25 else { continue }
            var score = top > 0 ? 0.5 * byWords + 0.5 * byMeaning : byMeaning
            if let wanted, chunks[index].ticker.uppercased() == wanted { score *= 1.25 }
            scored.append((index, min(1, score), byMeaning, byWords))
        }
        scored.sort { $0.1 > $1.1 }
        return scored.prefix(limit).map { MemoryHit(id: $0.0, chunk: chunks[$0.0], score: $0.1, meaning: $0.2, words: $0.3) }
    }
}

/// What the screen shows of the memory, and what brings it up to date.
@MainActor
@Observable
final class AXYONMemory {
    static let shared = AXYONMemory()

    private(set) var stats = MemoryStats()
    private(set) var syncing = false
    private(set) var done = 0
    private(set) var total = 0
    private(set) var problem: String?
    private(set) var lastSync: Date?

    private static var notesFile: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/memory/notes.json")
    }

    func refresh() async {
        stats = await AXYONMemoryIndex.shared.stats()
    }

    /// Everything new since the last time, read and indexed: the documents' pages, the court's verdicts, decisions,
    /// theses, the ears' transcripts.
    func sync() async {
        guard !syncing else { return }
        syncing = true
        problem = nil
        defer {
            syncing = false
            lastSync = .now
        }
        let index = AXYONMemoryIndex.shared
        let have = await index.keys()
        do {
            let listed = try await AXYONInsight.call("invest_docs", ["do": "index"])
            let documents = (listed["documents"] as? [[String: Any]] ?? []).filter { !have.contains($0["key"] as? String ?? "") }
            let records = listed["records"] as? [[String: Any]] ?? []
            let transcripts = AXYONEars.shared.transcripts.filter { !have.contains("ears:" + $0.id) }
            done = 0
            total = documents.count + transcripts.count + 1
            for document in documents {
                let key = document["key"] as? String ?? ""
                let whole = try await AXYONInsight.call("invest_docs", ["do": "text", "key": key])
                let pages = whole["texts"] as? [String] ?? []
                let title = document["title"] as? String ?? ""
                let kind = document["kind"] as? String ?? ""
                var chunks: [MemoryChunk] = []
                for (number, page) in pages.enumerated() {
                    for piece in AXYONMemoryIndex.passages(page) {
                        chunks.append(MemoryChunk(source: "document", key: key, ticker: document["ticker"] as? String ?? "",
                                                  title: kind.isEmpty ? title : "\(title) (\(kind))",
                                                  date: document["delivered"] as? String ?? "", page: number + 1, text: piece))
                    }
                }
                await index.add(chunks)
                done += 1
                if done % 8 == 0 { await index.save() }
            }
            for transcript in transcripts {
                await index.add(Self.chunks(of: transcript))
                done += 1
            }
            // What AXYON keeps in words changes (a thesis rewritten, a decision reviewed): indexed again whole.
            let ids = Set(records.compactMap { $0["id"] as? String })
            let stale = have.filter { $0.hasPrefix("verdict:") || $0.hasPrefix("decision:") || $0.hasPrefix("thesis:") }
            await index.remove(keys: stale.union(ids))
            var chunks: [MemoryChunk] = []
            for record in records {
                for piece in AXYONMemoryIndex.passages(record["text"] as? String ?? "") {
                    chunks.append(MemoryChunk(source: record["kind"] as? String ?? "verdict", key: record["id"] as? String ?? "",
                                              ticker: record["ticker"] as? String ?? "", title: record["title"] as? String ?? "",
                                              date: record["date"] as? String ?? "", page: 0, text: piece))
                }
            }
            await index.add(chunks)
            done = total
            await index.save()
        } catch {
            problem = AXYONInvest.describe(error)
        }
        await refresh()
    }

    static func chunks(of transcript: EarsTranscript) -> [MemoryChunk] {
        AXYONMemoryIndex.passages(transcript.text).map {
            MemoryChunk(source: "transcript", key: "ears:" + transcript.id, ticker: transcript.ticker, title: transcript.title,
                        date: AXYONWallet.iso(transcript.date), page: 0, text: $0)
        }
    }

    /// A note kept in the memory (by the user here, or by AXYON Core when asked to remember something).
    func note(_ text: String, title: String) async {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        let key = "note:" + UUID().uuidString.prefix(8).lowercased()
        let name = title.isEmpty ? String(localized: "Note") : title
        var pieces = AXYONMemoryIndex.passages(clean)
        if pieces.isEmpty { pieces = [clean] }
        let today = AXYONWallet.iso(.now)
        let chunks = pieces.map {
            MemoryChunk(source: "note", key: key, ticker: "", title: name, date: today, page: 0, text: $0)
        }
        await AXYONMemoryIndex.shared.add(chunks)
        await AXYONMemoryIndex.shared.save()
        await refresh()
    }

    func search(_ query: String, limit: Int = 12) async -> [MemoryHit] {
        let ticker = query.uppercased().split { !($0.isLetter || $0.isNumber) }
            .first { $0.count >= 5 && $0.count <= 6 && $0.last?.isNumber == true && $0.first?.isLetter == true }
        return await AXYONMemoryIndex.shared.search(query, limit: limit, ticker: ticker.map { String($0) })
    }

    func forget() async {
        await AXYONMemoryIndex.shared.clear()
        await refresh()
    }

    /// AXYON Core's own way in (axyon_hermes/memory.py, over the bridge): `search` gives the closest passages with
    /// where each comes from; `note` keeps a text.
    func answer(_ request: [String: Any]) async -> [String: Any] {
        switch request["do"] as? String ?? "search" {
        case "note":
            let text = request["text"] as? String ?? ""
            guard !text.trimmingCharacters(in: .whitespaces).isEmpty else { return ["error": "Nothing to remember."] }
            await note(text, title: request["title"] as? String ?? "")
            return ["ok": true]
        default:
            let query = request["query"] as? String ?? ""
            if stats.passages == 0 { await refresh() }
            let hits = await search(query, limit: min(max((request["limit"] as? NSNumber)?.intValue ?? 8, 1), 16))
            let passages: [[String: Any]] = hits.map { hit in
                var o: [String: Any] = ["source": hit.chunk.source, "title": hit.chunk.title, "text": String(hit.chunk.text.prefix(900)),
                                        "score": (hit.score * 100).rounded() / 100]
                if !hit.chunk.ticker.isEmpty { o["ticker"] = hit.chunk.ticker }
                if !hit.chunk.date.isEmpty { o["date"] = hit.chunk.date }
                if hit.chunk.page > 0 { o["page"] = hit.chunk.page }
                return o
            }
            var out: [String: Any] = ["passages": passages, "indexed": stats.passages]
            if passages.isEmpty {
                out["note"] = stats.passages == 0
                    ? "The memory is empty on this iPhone: the user opens AXYON's Core → Memory once to index what was read."
                    : "Nothing close to that in the memory."
            }
            return out
        }
    }
}

// MARK: - The screen

struct AXYONMemoryView: View {
    private let memory = AXYONMemory.shared
    @State private var query = ""
    @State private var hits: [MemoryHit] = []
    @State private var searched = false
    @State private var opened: MemoryHit?
    @State private var noting = false
    @State private var noteText = ""
    @FocusState private var typing: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                field
                status
                if searched, hits.isEmpty {
                    Text("Nothing close to that in what AXYON read.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 20)
                }
                ForEach(hits) { hit in
                    Button { opened = hit } label: { row(hit) }
                        .buttonStyle(PressableButtonStyle())
                }
                if !searched {
                    examples
                }
            }
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("Memory")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button { noting = true } label: { Label("Keep a note", systemImage: "square.and.pencil") }
                    Button { Task { await memory.sync() } } label: { Label("Index what is new", systemImage: "arrow.triangle.2.circlepath") }
                    Button(role: .destructive) {
                        Task {
                            await memory.forget()
                            hits = []
                        }
                    } label: { Label("Forget everything indexed", systemImage: "trash") }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
            }
        }
        .sheet(item: $opened) { hit in
            NavigationStack { MemoryPassageView(hit: hit, query: query) }
                .presentationDetents([.medium, .large])
        }
        .alert("Keep a note", isPresented: $noting) {
            TextField("What AXYON should remember", text: $noteText)
            Button("Cancel", role: .cancel) { noteText = "" }
            Button("Keep") {
                let text = noteText
                noteText = ""
                Task { await memory.note(text, title: "") }
            }
        }
        .task {
            await memory.refresh()
            await memory.sync()
            #if targetEnvironment(simulator)
            if AXYONTestScreen.base == "core-memory" {
                query = "vacância"
                await search()
            }
            #endif
        }
    }

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "sparkle.magnifyingglass").foregroundStyle(AXYONTheme.current.accent)
            TextField("Ask your memory: “what did the reports say about vacancy?”", text: $query, axis: .vertical)
                .lineLimit(1...3)
                .focused($typing)
                .submitLabel(.search)
                .onSubmit { Task { await search() } }
            if !query.isEmpty {
                Button {
                    Task { await search() }
                } label: {
                    Image(systemName: "arrow.up.circle.fill").font(.title2).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(14)
        .axyonCard(cornerRadius: 22)
    }

    private var status: some View {
        VStack(alignment: .leading, spacing: 8) {
            if memory.syncing {
                HStack(spacing: 10) {
                    ProgressView(value: Double(memory.done), total: Double(max(memory.total, 1)))
                        .tint(AXYONTheme.current.accent)
                    Text(verbatim: "\(memory.done)/\(memory.total)").font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                }
                Text("Reading and indexing what is new, on this iPhone…")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
            } else {
                Text(String(localized: "\(memory.stats.passages) passages of \(memory.stats.sources) documents and records, \(ByteCountFormatter.string(fromByteCount: Int64(memory.stats.bytes), countStyle: .file)) on this iPhone."))
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                Label(memory.stats.byMeaning ? String(localized: "Searched by meaning and by words")
                                             : String(localized: "Searched by words (this iPhone has no sentence embedding for Portuguese)"),
                      systemImage: memory.stats.byMeaning ? "brain" : "textformat")
                    .font(.caption)
                    .foregroundStyle(memory.stats.byMeaning ? InvestPalette.favor : AXYONPalette.dim)
            }
            if let problem = memory.problem {
                Text(problem).font(.caption).foregroundStyle(AXYONPalette.down)
            }
        }
        .padding(.horizontal, 4)
    }

    private var examples: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Try").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.dim)
            ForEach([String(localized: "vacancy going up"), String(localized: "why the court vetoed"),
                     String(localized: "debt and leverage"), String(localized: "dividend cut")], id: \.self) { example in
                Button {
                    query = example
                    Task { await search() }
                } label: {
                    Label(example, systemImage: "text.magnifyingglass")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .coucouGlass(.regular, in: Capsule())
                }
                .buttonStyle(PressableButtonStyle())
            }
        }
    }

    private func row(_ hit: MemoryHit) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: MemoryPassageView.icon(hit.chunk.source)).foregroundStyle(AXYONTheme.current.accent)
                Text(hit.chunk.title).font(.footnote.weight(.semibold)).foregroundStyle(.white).lineLimit(1)
                Spacer(minLength: 4)
                Text(verbatim: "\(Int((hit.score * 100).rounded()))%")
                    .font(.caption2.weight(.bold))
                    .monospacedDigit()
                    .foregroundStyle(AXYONPalette.dim)
            }
            Text(MemoryPassageView.marked(hit.chunk.text, query: query, limit: 260))
                .font(.callout)
                .foregroundStyle(Color.white.opacity(0.85))
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 6) {
                if !hit.chunk.ticker.isEmpty { InvestTag(text: hit.chunk.ticker, color: AXYONTheme.current.accent) }
                if !hit.chunk.date.isEmpty { InvestTag(text: investDay(hit.chunk.date)) }
                if hit.chunk.page > 0 { InvestTag(text: String(localized: "p. \(hit.chunk.page)")) }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }

    private func search() async {
        typing = false
        let asked = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty else { return }
        let found = await memory.search(asked)
        withAnimation(.spring(duration: 0.4)) {
            hits = found
            searched = true
        }
        if found.isEmpty { Haptics.warning() } else { Haptics.impact() }
    }
}

struct MemoryPassageView: View {
    let hit: MemoryHit
    let query: String
    @Environment(\.dismiss) private var dismiss

    static func icon(_ source: String) -> String {
        switch source {
        case "verdict": "building.columns.fill"
        case "decision": "checkmark.seal.fill"
        case "thesis": "lightbulb.fill"
        case "transcript": "waveform"
        case "note": "note.text"
        default: "doc.text.fill"
        }
    }

    /// The passage with the question's words in bold, around the first of them when it is long.
    static func marked(_ text: String, query: String, limit: Int) -> AttributedString {
        let wanted = Set(AXYONMemoryIndex.words(query))
        var shown = text
        if text.count > limit {
            let folded = text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "pt_BR"))
            let first = wanted.compactMap { folded.range(of: $0)?.lowerBound }.min()
            let offset = first.map { folded.distance(from: folded.startIndex, to: $0) } ?? 0
            let from = max(0, min(offset - 60, text.count - limit))
            let start = text.index(text.startIndex, offsetBy: from)
            let end = text.index(start, offsetBy: min(limit, text.count - from))
            shown = (from > 0 ? "…" : "") + String(text[start..<end]) + (end < text.endIndex ? "…" : "")
        }
        guard !wanted.isEmpty else { return AttributedString(shown) }
        // Built word by word: the question's words in bold white.
        var out = AttributedString()
        var cursor = shown.startIndex
        while cursor < shown.endIndex {
            let wordStart = shown[cursor...].firstIndex(where: { $0.isLetter || $0.isNumber }) ?? shown.endIndex
            if wordStart > cursor { out.append(AttributedString(String(shown[cursor..<wordStart]))) }
            guard wordStart < shown.endIndex else { break }
            let wordEnd = shown[wordStart...].firstIndex(where: { !($0.isLetter || $0.isNumber) }) ?? shown.endIndex
            let word = String(shown[wordStart..<wordEnd])
            var piece = AttributedString(word)
            if wanted.contains(word.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "pt_BR"))) {
                piece.font = Font.callout.bold()
                piece.foregroundColor = Color.white
            }
            out.append(piece)
            cursor = wordEnd
        }
        return out
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Label(hit.chunk.title, systemImage: Self.icon(hit.chunk.source))
                    .font(.headline)
                    .foregroundStyle(.white)
                HStack(spacing: 6) {
                    if !hit.chunk.ticker.isEmpty { InvestTag(text: hit.chunk.ticker, color: AXYONTheme.current.accent) }
                    if !hit.chunk.date.isEmpty { InvestTag(text: investDay(hit.chunk.date)) }
                    if hit.chunk.page > 0 { InvestTag(text: String(localized: "p. \(hit.chunk.page)")) }
                }
                Text(Self.marked(hit.chunk.text, query: query, limit: 4000))
                    .font(.callout)
                    .foregroundStyle(Color.white.opacity(0.9))
                    .textSelection(.enabled)
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .axyonCard(cornerRadius: 18)
                Text(String(localized: "Close by meaning: \(Int((hit.meaning * 100).rounded())) % · by words: \(Int((hit.words * 100).rounded())) %"))
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
}
