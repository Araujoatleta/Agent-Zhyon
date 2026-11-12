import SwiftUI
import UIKit
import os
#if canImport(llama)
import llama
#endif
#if canImport(FoundationModels)
import FoundationModels
#endif

// AXYON's local brain: a language model that thinks on this iPhone's own chip, with the network off. Nothing asked
// here leaves the iPhone. Two engines:
//   - llama.cpp (the `llama` framework Hermes/build-ios.sh puts in the iPhone build), on the graphics processor through
//     Metal, with a model the user downloads once from a link they see (a GGUF file, kept in Application Support).
//     This is what an iPhone with no Apple Intelligence has: the 14 Pro Max and its 6 GB take a model of 1 to 3
//     billion parameters.
//   - Apple's own model (FoundationModels, iOS 26) where the iPhone has Apple Intelligence (15 Pro and after): nothing
//     to download.
// A model this size is not AXYON Core: it answers, sums up and drafts, grounded on the memory's passages
// (AXYONMemory.swift), and it does not use tools. The Simulator's build carries no llama: the screen shows the models
// and says so.

/// A model that can be downloaded: what it is, what it weighs, the memory the iPhone needs, and its link.
struct LocalModel: Identifiable, Codable, Sendable, Equatable {
    var id: String
    var name: String
    var maker: String
    /// The download's size, about.
    var bytes: Int64
    /// The memory the iPhone needs, in GB.
    var memory: Int
    var link: String
    /// Added by the user, from a link.
    var custom = false

    /// GGUF files quantized to 4 bits, each published by its maker or a known packager on Hugging Face.
    static let catalogue: [LocalModel] = [
        LocalModel(id: "qwen2.5-1.5b", name: "Qwen 2.5 · 1.5B", maker: "Alibaba", bytes: 1_120_000_000, memory: 4,
                   link: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf"),
        LocalModel(id: "llama-3.2-3b", name: "Llama 3.2 · 3B", maker: "Meta", bytes: 2_020_000_000, memory: 6,
                   link: "https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf"),
        LocalModel(id: "gemma-3-4b", name: "Gemma 3 · 4B", maker: "Google", bytes: 2_490_000_000, memory: 8,
                   link: "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/main/google_gemma-3-4b-it-Q4_K_M.gguf"),
        LocalModel(id: "qwen2.5-7b", name: "Qwen 2.5 · 7B", maker: "Alibaba", bytes: 4_680_000_000, memory: 12,
                   link: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"),
    ]

    /// What to expect of it, in a line.
    var note: String {
        switch id {
        case "qwen2.5-1.5b": String(localized: "Light and fast: short questions and summaries.")
        case "llama-3.2-3b": String(localized: "The one for 6 GB, like the 14 Pro Max: better answers, slower.")
        case "gemma-3-4b": String(localized: "For 8 GB, like the 16 Pro Max: the best Portuguese of the small ones.")
        case "qwen2.5-7b": String(localized: "For 12 GB, like the 17 Pro Max: the strongest here.")
        default: URL(string: link)?.host() ?? link
        }
    }

    var size: String { ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file) }
}

/// One line of a conversation, as the model reads it: system, user or assistant.
struct LocalBrainLine: Sendable, Equatable {
    let role: String
    let text: String
}

/// What one answer cost.
struct LocalBrainRun: Sendable, Equatable {
    /// Tokens read (what the context did not already hold) and written.
    var read = 0
    var written = 0
    var seconds: Double = 0

    var speed: Double { seconds > 0 ? Double(written) / seconds : 0 }
}

enum LocalBrainError: LocalizedError {
    case notBuilt, noModel, load, context, template, tooLong, decode(Int32), said(String)

    var errorDescription: String? {
        switch self {
        case .notBuilt: String(localized: "This build carries no model engine: it is in the iPhone's build, not in the Simulator's.")
        case .noModel: String(localized: "No model yet: download one first.")
        case .load: String(localized: "The model could not be opened: the file may be damaged. Delete it and download it again.")
        case .context: String(localized: "Not enough free memory for this model now: close other apps, or use a lighter model.")
        case .template: String(localized: "This model's conversation format is not one AXYON knows.")
        case .tooLong: String(localized: "That is too long for this model's memory: ask about a shorter text.")
        case .decode(let code): String(localized: "The model stopped (error \(Int(code))). Try again; if it repeats, use a lighter model.")
        case .said(let text): text
        }
    }
}

/// Where the models are kept, and what is there.
enum LocalBrainFiles {
    static var folder: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/brain", isDirectory: true)
    }

    static func file(_ id: String) -> URL { folder.appendingPathComponent(id + ".gguf") }

    static func installed() -> Set<String> {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        return Set(names.filter { $0.hasSuffix(".gguf") }.map { String($0.dropLast(5)) })
    }

    /// A GGUF file starts with these four letters; an error page does not.
    static func isModel(_ url: URL) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        let head = try? handle.read(upToCount: 4)
        return head == Data("GGUF".utf8)
    }

    static func size(_ id: String) -> Int64 {
        let attributes = try? FileManager.default.attributesOfItem(atPath: file(id).path)
        return (attributes?[.size] as? NSNumber)?.int64Value ?? 0
    }

    /// The space left on the iPhone for something the user asked for.
    static var freeSpace: Int64 {
        let home = HermesEngine.home.deletingLastPathComponent()
        let values = try? home.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
        return values?.volumeAvailableCapacityForImportantUsage ?? 0
    }

    /// A name for a file from its link: "Phi-4-mini-Q4_K_M.gguf" is "phi-4-mini-q4_k_m".
    static func id(from link: URL) -> String {
        let name = link.deletingPathExtension().lastPathComponent.lowercased()
        let kept = name.map { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" || $0 == "." ? $0 : "-" }
        return String(String(kept).prefix(60))
    }

    /// How many of the bytes at the end are a whole character: a token can end in the middle of one ("ã" is two
    /// bytes), and what is shown waits for the rest.
    static func wholeCharacters(_ bytes: [UInt8]) -> Int {
        var index = bytes.count - 1
        var seen = 0
        while index >= 0, seen < 4 {
            let byte = bytes[index]
            if byte & 0b1100_0000 != 0b1000_0000 {
                let needs = byte >= 0b1111_0000 ? 4 : byte >= 0b1110_0000 ? 3 : byte >= 0b1100_0000 ? 2 : 1
                return needs > bytes.count - index ? index : bytes.count
            }
            index -= 1
            seen += 1
        }
        return bytes.count
    }
}

// MARK: - The downloads

/// The models' downloads, by the system itself: they go on with AXYON closed, and what finished meanwhile is handed
/// over when AXYON opens again.
final class LocalBrainDownloads: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    static let shared = LocalBrainDownloads()

    private var session: URLSession!
    private let told = OSAllocatedUnfairLock(initialState: [String: Date]())

    override init() {
        super.init()
        let setup = URLSessionConfiguration.background(withIdentifier: "axyon.brain.models")
        setup.isDiscretionary = false
        setup.sessionSendsLaunchEvents = false
        session = URLSession(configuration: setup, delegate: self, delegateQueue: nil)
    }

    func start(_ id: String, from link: URL) {
        let task = session.downloadTask(with: link)
        task.taskDescription = id
        task.resume()
    }

    func cancel(_ id: String) {
        session.getAllTasks { tasks in
            for task in tasks where task.taskDescription == id { task.cancel() }
        }
    }

    /// What is being downloaded now, with how far each is.
    func running() async -> [String: Double] {
        let tasks = await session.allTasks
        var found: [String: Double] = [:]
        for task in tasks where task.state == .running || task.state == .suspended {
            guard let id = task.taskDescription else { continue }
            let total = task.countOfBytesExpectedToReceive
            found[id] = total > 0 ? Double(task.countOfBytesReceived) / Double(total) : 0
        }
        return found
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let id = downloadTask.taskDescription, totalBytesExpectedToWrite > 0 else { return }
        // Four times a second is enough for a bar.
        let due = told.withLock { last -> Bool in
            if let before = last[id], Date.now.timeIntervalSince(before) < 0.25 { return false }
            last[id] = .now
            return true
        }
        guard due else { return }
        let fraction = Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)
        Task { @MainActor in AXYONLocalBrain.shared.downloading(id, fraction) }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        guard let id = downloadTask.taskDescription else { return }
        // The file is gone when this returns: it is checked and moved here.
        var problem: String?
        let status = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 200
        if !(200..<300).contains(status) {
            problem = String(localized: "The link answered with the error \(status): check it, or choose another model.")
        } else if !LocalBrainFiles.isModel(location) {
            problem = String(localized: "That link did not give a model (a GGUF file).")
        } else {
            do {
                let target = LocalBrainFiles.file(id)
                try FileManager.default.createDirectory(at: LocalBrainFiles.folder, withIntermediateDirectories: true)
                try? FileManager.default.removeItem(at: target)
                try FileManager.default.moveItem(at: location, to: target)
                // A model is downloaded again in a minute: it does not weigh on the iPhone's backup.
                var kept = target
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                try? kept.setResourceValues(values)
            } catch {
                problem = error.localizedDescription
            }
        }
        let failure = problem
        Task { @MainActor in AXYONLocalBrain.shared.downloaded(id, problem: failure) }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let id = task.taskDescription, let error else { return }
        let cancelled = (error as NSError).code == NSURLErrorCancelled
        let message = error.localizedDescription
        Task { @MainActor in AXYONLocalBrain.shared.downloaded(id, problem: cancelled ? nil : message, kept: false) }
    }
}

// MARK: - llama.cpp

#if canImport(llama)
/// The model in memory and its context. Everything of llama's is touched on one queue, never the main one: reading a
/// long prompt and writing an answer take seconds.
final class LocalLlama: @unchecked Sendable {
    static let shared = LocalLlama()

    private let queue = DispatchQueue(label: "axyon.brain.llama", qos: .userInitiated)
    private let halted = OSAllocatedUnfairLock(initialState: false)
    private var model: OpaquePointer?
    private var context: OpaquePointer?
    private var vocab: OpaquePointer?
    private var sampler: UnsafeMutablePointer<llama_sampler>?
    private var path = ""
    /// The tokens the context holds now: a conversation's next turn only reads what is new.
    private var kept: [llama_token] = []

    private static let backend: Void = {
        llama_backend_init()
        llama_log_set({ _, _, _ in }, nil)
    }()

    /// Opens the model (a few seconds), on the graphics processor or, when memory is short, on the processor alone.
    func load(path: String, tokens: Int, onGraphics: Bool) async throws {
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
            queue.async {
                do {
                    try self.open(path: path, tokens: tokens, onGraphics: onGraphics)
                    done.resume()
                } catch {
                    done.resume(throwing: error)
                }
            }
        }
    }

    func unload() async {
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            queue.async {
                self.close()
                done.resume()
            }
        }
    }

    /// The answer to a conversation, each piece handed over as it is written.
    func reply(to lines: [LocalBrainLine], most: Int, piece: @escaping @Sendable (String) -> Void) async throws -> LocalBrainRun {
        halted.withLock { $0 = false }
        return try await withCheckedThrowingContinuation { (done: CheckedContinuation<LocalBrainRun, Error>) in
            queue.async {
                do {
                    done.resume(returning: try self.run(lines, most: most, piece: piece))
                } catch {
                    done.resume(throwing: error)
                }
            }
        }
    }

    /// Stops what is being written, at the next token.
    func stop() {
        halted.withLock { $0 = true }
    }

    private func open(path: String, tokens: Int, onGraphics: Bool) throws {
        _ = Self.backend
        if self.path == path, context != nil { return }
        close()
        var wanted = llama_model_default_params()
        #if targetEnvironment(simulator)
        wanted.n_gpu_layers = 0
        #else
        if !onGraphics { wanted.n_gpu_layers = 0 }
        #endif
        guard let model = llama_model_load_from_file(path, wanted) else { throw LocalBrainError.load }
        var settings = llama_context_default_params()
        settings.n_ctx = UInt32(tokens)
        settings.n_batch = 512
        // The performance cores, and a little more: the efficiency ones slow the rest down.
        let threads = Int32(max(2, min(4, ProcessInfo.processInfo.activeProcessorCount - 2)))
        settings.n_threads = threads
        settings.n_threads_batch = threads
        guard let context = llama_init_from_model(model, settings) else {
            llama_model_free(model)
            throw LocalBrainError.context
        }
        let vocab = llama_model_get_vocab(model)
        let chain = llama_sampler_chain_init(llama_sampler_chain_default_params())
        llama_sampler_chain_add(chain, llama_sampler_init_top_k(40))
        llama_sampler_chain_add(chain, llama_sampler_init_penalties(llama_vocab_n_tokens(vocab), 128, 1.08, 0, 0))
        llama_sampler_chain_add(chain, llama_sampler_init_top_p(0.9, 1))
        llama_sampler_chain_add(chain, llama_sampler_init_temp(0.6))
        llama_sampler_chain_add(chain, llama_sampler_init_dist(UInt32.random(in: 1..<UInt32.max)))
        self.model = model
        self.context = context
        self.vocab = vocab
        self.sampler = chain
        self.path = path
        kept = []
    }

    private func close() {
        if let sampler { llama_sampler_free(sampler) }
        if let context { llama_free(context) }
        if let model { llama_model_free(model) }
        sampler = nil
        context = nil
        model = nil
        vocab = nil
        path = ""
        kept = []
    }

    private func run(_ asked: [LocalBrainLine], most: Int, piece: (String) -> Void) throws -> LocalBrainRun {
        guard let context, let model, let vocab, let sampler else { throw LocalBrainError.noModel }
        let room = Int(llama_n_ctx(context))
        // When the conversation no longer fits, its oldest turns go first.
        var lines = asked
        var tokens = try tokenize(try prompt(lines, model: model), vocab: vocab)
        while tokens.count + most + 8 > room, lines.count > 2 {
            lines.remove(at: lines.first?.role == "system" ? 1 : 0)
            tokens = try tokenize(try prompt(lines, model: model), vocab: vocab)
        }
        guard tokens.count + 16 < room else { throw LocalBrainError.tooLong }
        let budget = min(most, room - tokens.count - 4)

        // What the context already holds of this prompt is not read again.
        var shared = 0
        while shared < kept.count, shared < tokens.count - 1, kept[shared] == tokens[shared] { shared += 1 }
        let memory = llama_get_memory(context)
        if shared == 0 || !llama_memory_seq_rm(memory, 0, Int32(shared), -1) {
            llama_memory_clear(memory, true)
            shared = 0
        }
        kept = Array(tokens[..<shared])
        llama_sampler_reset(sampler)

        var batch = llama_batch_init(512, 0, 1)
        defer { llama_batch_free(batch) }
        var at = shared
        while at < tokens.count {
            if halted.withLock({ $0 }) { throw CancellationError() }
            let end = min(at + 512, tokens.count)
            batch.n_tokens = 0
            for index in at..<end {
                Self.add(&batch, tokens[index], Int32(index), last: index == tokens.count - 1)
            }
            let result = llama_decode(context, batch)
            guard result == 0 else {
                llama_memory_clear(memory, true)
                kept = []
                throw LocalBrainError.decode(result)
            }
            at = end
            kept = Array(tokens[..<end])
        }

        var run = LocalBrainRun(read: tokens.count - shared)
        var pending: [UInt8] = []
        var position = Int32(tokens.count)
        let started = Date()
        while run.written < budget {
            if halted.withLock({ $0 }) { break }
            let token = llama_sampler_sample(sampler, context, -1)
            if llama_vocab_is_eog(vocab, token) { break }
            pending += Self.bytes(of: token, vocab: vocab)
            let whole = LocalBrainFiles.wholeCharacters(pending)
            if whole > 0 {
                piece(String(decoding: pending[..<whole], as: UTF8.self))
                pending.removeFirst(whole)
            }
            batch.n_tokens = 0
            Self.add(&batch, token, position, last: true)
            let result = llama_decode(context, batch)
            guard result == 0 else {
                llama_memory_clear(memory, true)
                kept = []
                throw LocalBrainError.decode(result)
            }
            kept.append(token)
            position += 1
            run.written += 1
        }
        if !pending.isEmpty { piece(String(decoding: pending, as: UTF8.self)) }
        run.seconds = Date().timeIntervalSince(started)
        return run
    }

    /// The conversation in the model's own format (its GGUF says which); ChatML when it says nothing AXYON knows.
    private func prompt(_ lines: [LocalBrainLine], model: OpaquePointer) throws -> String {
        let roles = lines.map { strdup($0.role) }
        let texts = lines.map { strdup($0.text) }
        defer {
            roles.forEach { free($0) }
            texts.forEach { free($0) }
        }
        var chat: [llama_chat_message] = []
        for index in lines.indices {
            chat.append(llama_chat_message(role: UnsafePointer(roles[index]), content: UnsafePointer(texts[index])))
        }
        let template = llama_model_chat_template(model, nil)
        var size = lines.reduce(1024) { $0 + ($1.role.utf8.count + $1.text.utf8.count) * 2 }
        for _ in 0..<2 {
            var buffer = [CChar](repeating: 0, count: size)
            let capacity = Int32(size)
            var written = llama_chat_apply_template(template, chat, chat.count, true, &buffer, capacity)
            if written < 0 { written = llama_chat_apply_template("chatml", chat, chat.count, true, &buffer, capacity) }
            guard written >= 0 else { throw LocalBrainError.template }
            if written <= capacity {
                return String(decoding: buffer.prefix(Int(written)).map { UInt8(bitPattern: $0) }, as: UTF8.self)
            }
            size = Int(written) + 1
        }
        throw LocalBrainError.template
    }

    private func tokenize(_ text: String, vocab: OpaquePointer) throws -> [llama_token] {
        let length = Int32(text.utf8.count)
        var tokens = [llama_token](repeating: 0, count: Int(length) + 16)
        var capacity = Int32(tokens.count)
        var count = llama_tokenize(vocab, text, length, &tokens, capacity, true, true)
        if count < 0, count > Int32.min {
            tokens = [llama_token](repeating: 0, count: Int(-count))
            capacity = Int32(tokens.count)
            count = llama_tokenize(vocab, text, length, &tokens, capacity, true, true)
        }
        guard count >= 0 else { throw LocalBrainError.tooLong }
        return Array(tokens.prefix(Int(count)))
    }

    private static func bytes(of token: llama_token, vocab: OpaquePointer) -> [UInt8] {
        var buffer = [CChar](repeating: 0, count: 64)
        var count = llama_token_to_piece(vocab, token, &buffer, 64, 0, false)
        if count < 0 {
            buffer = [CChar](repeating: 0, count: Int(-count))
            let capacity = Int32(buffer.count)
            count = llama_token_to_piece(vocab, token, &buffer, capacity, 0, false)
        }
        return buffer.prefix(Int(max(count, 0))).map { UInt8(bitPattern: $0) }
    }

    private static func add(_ batch: inout llama_batch, _ token: llama_token, _ position: Int32, last: Bool) {
        let index = Int(batch.n_tokens)
        batch.token[index] = token
        batch.pos[index] = position
        batch.n_seq_id[index] = 1
        batch.seq_id[index]![0] = 0
        batch.logits[index] = last ? 1 : 0
        batch.n_tokens += 1
    }
}
#endif

// MARK: - The brain

/// One message of the conversation on screen.
struct BrainTurn: Identifiable, Equatable, Sendable {
    let id = UUID()
    let mine: Bool
    var text: String
    /// The memory's passages the answer was given: their titles.
    var sources: [String] = []
}

@MainActor
@Observable
final class AXYONLocalBrain {
    static let shared = AXYONLocalBrain()
    /// The id of Apple's own model, among the models' ids.
    static let apple = "apple"

    private(set) var models: [LocalModel] = LocalModel.catalogue
    private(set) var installed: Set<String> = []
    /// The downloads going on: 0 to 1.
    private(set) var progress: [String: Double] = [:]
    private(set) var turns: [BrainTurn] = []
    private(set) var thinking = false
    /// The model is being opened: a few seconds, once.
    private(set) var waking = false
    private(set) var problem: String?
    /// Tokens a second, of the last answer.
    private(set) var speed: Double = 0
    /// Memory was short: the model runs on the processor alone, slower.
    private(set) var onProcessor = false
    /// A long text being summed up: 0 to 1.
    private(set) var digesting: Double?

    var chosen: String = UserDefaults.standard.string(forKey: "axyon.brain.model") ?? "" {
        didSet { UserDefaults.standard.set(chosen, forKey: "axyon.brain.model") }
    }
    /// The answers are grounded on what AXYON read: the memory's closest passages go with the question.
    var useMemory: Bool = UserDefaults.standard.object(forKey: "axyon.brain.memory") as? Bool ?? true {
        didSet { UserDefaults.standard.set(useMemory, forKey: "axyon.brain.memory") }
    }

    @ObservationIgnored private var awake: String?
    @ObservationIgnored private var appleSession: Any?

    /// The engine for downloaded models is in this build.
    static var built: Bool {
        #if canImport(llama)
        return true
        #else
        return false
        #endif
    }

    private init() {
        if let data = UserDefaults.standard.data(forKey: "axyon.brain.custom"),
           let custom = try? JSONDecoder().decode([LocalModel].self, from: data) {
            models += custom
        }
        installed = LocalBrainFiles.installed()
        settle()
        NotificationCenter.default.addObserver(forName: UIApplication.didReceiveMemoryWarningNotification, object: nil, queue: .main) { _ in
            Task { @MainActor in await AXYONLocalBrain.shared.rest() }
        }
        NotificationCenter.default.addObserver(forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main) { _ in
            // The graphics processor is the foreground's: what is being written stops there.
            Task { @MainActor in AXYONLocalBrain.shared.stop() }
        }
        Task { @MainActor in
            let running = await LocalBrainDownloads.shared.running()
            for (id, fraction) in running where self.progress[id] == nil { self.progress[id] = fraction }
        }
    }

    // MARK: What there is

    /// Apple's own model is on this iPhone (Apple Intelligence, iOS 26).
    var appleReady: Bool {
        #if canImport(FoundationModels)
        if #available(iOS 26.0, *) {
            if case .available = SystemLanguageModel.default.availability { return true }
        }
        #endif
        return false
    }

    var current: LocalModel? { models.first { $0.id == chosen } }

    /// There is a model to think with now.
    var canThink: Bool {
        if chosen == Self.apple { return appleReady }
        return Self.built && installed.contains(chosen)
    }

    /// The name of what thinks.
    var engine: String {
        if chosen == Self.apple, appleReady { return String(localized: "Apple's model") }
        return current?.name ?? ""
    }

    /// A line for the core's grid.
    var status: String {
        if canThink { return String(localized: "\(engine), with the network off") }
        if !progress.isEmpty { return String(localized: "Downloading a model…") }
        return String(localized: "A model on the chip, with the network off")
    }

    /// A chosen model that is there; else Apple's, else the first one downloaded.
    private func settle() {
        if canThink { return }
        if appleReady {
            chosen = Self.apple
        } else if let first = models.first(where: { installed.contains($0.id) }) {
            chosen = first.id
        }
    }

    func fits(_ model: LocalModel) -> Bool { model.custom || model.memory <= AXYONDevice.memory }

    // MARK: The models

    func download(_ model: LocalModel) {
        guard let link = URL(string: model.link), progress[model.id] == nil else { return }
        problem = nil
        let free = LocalBrainFiles.freeSpace
        if free > 0, free < model.bytes + 500_000_000 {
            problem = String(localized: "Not enough space on the iPhone: this model needs \(model.size).")
            return
        }
        progress[model.id] = 0
        LocalBrainDownloads.shared.start(model.id, from: link)
    }

    func cancel(_ model: LocalModel) {
        LocalBrainDownloads.shared.cancel(model.id)
        progress[model.id] = nil
    }

    /// A model from a link the user gives: any GGUF file.
    func add(link text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let link = URL(string: trimmed), link.scheme == "https", link.pathExtension.lowercased() == "gguf" else {
            problem = String(localized: "A model's link starts with https:// and ends in .gguf.")
            return
        }
        let id = LocalBrainFiles.id(from: link)
        if !models.contains(where: { $0.id == id }) {
            models.append(LocalModel(id: id, name: link.deletingPathExtension().lastPathComponent, maker: link.host() ?? "",
                                     bytes: 0, memory: 0, link: trimmed, custom: true))
            keepCustom()
        }
        if let model = models.first(where: { $0.id == id }), !installed.contains(id) { download(model) }
    }

    func delete(_ model: LocalModel) async {
        if awake == model.id { await rest() }
        try? FileManager.default.removeItem(at: LocalBrainFiles.file(model.id))
        installed = LocalBrainFiles.installed()
        if model.custom {
            models.removeAll { $0.id == model.id }
            keepCustom()
        }
        if chosen == model.id {
            chosen = ""
            settle()
        }
    }

    func use(_ id: String) {
        guard id != chosen else { return }
        chosen = id
        newConversation()
    }

    private func keepCustom() {
        let custom = models.filter(\.custom)
        if let data = try? JSONEncoder().encode(custom) { UserDefaults.standard.set(data, forKey: "axyon.brain.custom") }
    }

    fileprivate func downloading(_ id: String, _ fraction: Double) {
        if progress[id] != nil { progress[id] = fraction }
    }

    fileprivate func downloaded(_ id: String, problem failure: String?, kept: Bool = true) {
        progress[id] = nil
        if let failure { problem = failure }
        installed = LocalBrainFiles.installed()
        if kept, failure == nil { settle() }
    }

    // MARK: Thinking

    /// What the model is told it is. Short: every word of it is read again at each conversation.
    static let soul = """
    Você é o AXYON rodando direto no iPhone do usuário, sem internet. Responda em português do Brasil, de forma \
    direta e curta. Quando receber trechos da memória, baseie a resposta neles e diga de qual trecho veio cada \
    informação; se os trechos não respondem, diga que não encontrou. Nunca invente números, datas ou fatos. Você \
    não tem ferramentas nem acesso a cotações ao vivo aqui: para isso o usuário usa o AXYON Core.
    """

    func newConversation() {
        guard !thinking else { return }
        turns = []
        problem = nil
        appleSession = nil
    }

    func stop() {
        #if canImport(llama)
        LocalLlama.shared.stop()
        #endif
    }

    /// Frees the model's memory (iOS asked for it, or the model was deleted). It opens again at the next question.
    func rest() async {
        guard !thinking, digesting == nil else { return }
        #if canImport(llama)
        if awake != nil { await LocalLlama.shared.unload() }
        #endif
        awake = nil
    }

    /// A question of the conversation: the memory's closest passages go with it, and the answer is written here.
    func ask(_ text: String) async {
        let question = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !question.isEmpty, !thinking, canThink else { return }
        problem = nil
        thinking = true
        defer { thinking = false }

        var passages: [MemoryHit] = []
        if useMemory {
            passages = await AXYONMemory.shared.search(question, limit: 3).filter { $0.score >= 0.3 }
        }
        var lines = [LocalBrainLine(role: "system", text: Self.soul)]
        for turn in turns where !turn.text.isEmpty {
            lines.append(LocalBrainLine(role: turn.mine ? "user" : "assistant", text: turn.text))
        }
        let asked = Self.grounded(question, on: passages)
        lines.append(LocalBrainLine(role: "user", text: asked))
        turns.append(BrainTurn(mine: true, text: question))
        turns.append(BrainTurn(mine: false, text: "", sources: passages.map { Self.label($0.chunk) }))
        let slot = turns.count - 1
        do {
            let run = try await write(lines, most: 700, apple: asked) { self.turns[slot].text += $0 }
            if run.written > 0 { speed = run.speed }
            turns[slot].text = turns[slot].text.trimmingCharacters(in: .whitespacesAndNewlines)
            if turns[slot].text.isEmpty { turns.remove(at: slot) }
        } catch {
            if turns[slot].text.isEmpty { turns.remove(at: slot) }
            if !(error is CancellationError) { problem = error.localizedDescription }
        }
    }

    /// A long text summed up on the iPhone: read in parts the model's memory takes, each part's points kept, then the
    /// points put together. Minutes for an hour of talk. Nil when it could not be done (`problem` says why).
    func digest(_ text: String, title: String) async -> String? {
        guard canThink, !thinking, digesting == nil else { return nil }
        problem = nil
        thinking = true
        digesting = 0
        defer {
            thinking = false
            digesting = nil
        }
        // About 1,700 tokens a part, which leaves room for the points in a context of 4,096.
        let parts = Self.parts(of: text, size: 5_500)
        guard !parts.isEmpty else { return nil }
        do {
            var points: [String] = []
            for (index, part) in parts.enumerated() {
                let asked = "Trecho \(index + 1) de \(parts.count) de \"\(title)\":\n\n\(part)\n\nListe em tópicos curtos os pontos principais deste trecho, com os números ditos. Só os tópicos."
                points.append(try await once(asked, most: 320))
                digesting = Double(index + 1) / Double(parts.count + 1)
            }
            guard parts.count > 1 else { return points[0] }
            let joined = String(points.joined(separator: "\n").prefix(7_000))
            let asked = "Pontos anotados de \"\(title)\", em ordem:\n\n\(joined)\n\nEscreva o resumo final: os pontos principais, os números ditos e o que merece atenção. Sem repetir."
            return try await once(asked, most: 500)
        } catch {
            if !(error is CancellationError) { problem = error.localizedDescription }
            return nil
        }
    }

    /// One answer, with no conversation kept.
    private func once(_ asked: String, most: Int) async throws -> String {
        var answer = ""
        let lines = [LocalBrainLine(role: "system", text: Self.soul), LocalBrainLine(role: "user", text: asked)]
        _ = try await write(lines, most: most, apple: asked, fresh: true) { answer += $0 }
        return answer.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The model writes: llama's piece by piece, Apple's at once.
    private func write(_ lines: [LocalBrainLine], most: Int, apple asked: String, fresh: Bool = false,
                       piece: @MainActor (String) -> Void) async throws -> LocalBrainRun {
        if chosen == Self.apple {
            let started = Date()
            let answer = try await appleAnswer(asked, fresh: fresh)
            piece(answer)
            return LocalBrainRun(read: 0, written: 0, seconds: Date().timeIntervalSince(started))
        }
        #if canImport(llama)
        try await wake()
        let (stream, feed) = AsyncStream<String>.makeStream()
        let job = Task.detached { () throws -> LocalBrainRun in
            defer { feed.finish() }
            return try await LocalLlama.shared.reply(to: lines, most: most) { feed.yield($0) }
        }
        for await written in stream { piece(written) }
        return try await job.value
        #else
        throw LocalBrainError.notBuilt
        #endif
    }

    #if canImport(llama)
    /// Opens the chosen model if it is not open yet.
    private func wake() async throws {
        guard let model = current, installed.contains(model.id) else { throw LocalBrainError.noModel }
        if awake == model.id { return }
        waking = true
        defer { waking = false }
        let size = LocalBrainFiles.size(model.id)
        // What this app may still take before iOS closes it. On the graphics processor the model's weights count in
        // it; on the processor alone they are read from the file as needed, and do not.
        let free = Int64(os_proc_available_memory())
        let graphics = free == 0 || free > size + 300_000_000
        let tokens = AXYONDevice.memory >= 8 ? 8_192 : 4_096
        try await LocalLlama.shared.load(path: LocalBrainFiles.file(model.id).path, tokens: tokens, onGraphics: graphics)
        awake = model.id
        onProcessor = !graphics
    }
    #endif

    private func appleAnswer(_ asked: String, fresh: Bool) async throws -> String {
        #if canImport(FoundationModels)
        if #available(iOS 26.0, *) {
            let session: LanguageModelSession
            if !fresh, let kept = appleSession as? LanguageModelSession {
                session = kept
            } else {
                session = LanguageModelSession(instructions: Self.soul)
                if !fresh { appleSession = session }
            }
            let response = try await session.respond(to: asked)
            return response.content
        }
        #endif
        throw LocalBrainError.noModel
    }

    // MARK: The words

    /// The question with the memory's passages before it.
    static func grounded(_ question: String, on passages: [MemoryHit]) -> String {
        guard !passages.isEmpty else { return question }
        var text = "Trechos da memória do AXYON:\n"
        for (index, hit) in passages.enumerated() {
            text += "\n[\(index + 1)] \(label(hit.chunk))\n\(hit.chunk.text.prefix(1_100))\n"
        }
        return text + "\nPergunta: \(question)"
    }

    static func label(_ chunk: MemoryChunk) -> String {
        var parts: [String] = []
        if !chunk.ticker.isEmpty { parts.append(chunk.ticker) }
        if !chunk.title.isEmpty { parts.append(chunk.title) }
        if !chunk.date.isEmpty { parts.append(chunk.date) }
        if chunk.page > 0 { parts.append("p. \(chunk.page)") }
        return parts.joined(separator: " · ")
    }

    /// A text cut in parts of about `size` characters, at the end of a sentence where there is one.
    static func parts(of text: String, size: Int) -> [String] {
        var parts: [String] = []
        var rest = Substring(text.trimmingCharacters(in: .whitespacesAndNewlines))
        while !rest.isEmpty {
            guard rest.count > size else {
                parts.append(String(rest))
                break
            }
            let limit = rest.index(rest.startIndex, offsetBy: size)
            let window = rest[..<limit]
            let cut = window.lastIndex(where: { ".!?\n".contains($0) }).map { rest.index(after: $0) } ?? limit
            // A cut too early (a text with no sentences) would make crumbs.
            let end = rest.distance(from: rest.startIndex, to: cut) > size / 2 ? cut : limit
            parts.append(String(rest[..<end]))
            rest = rest[end...].drop(while: { $0 == " " || $0 == "\n" })
        }
        return parts
    }
}

// MARK: - The screens

struct AXYONLocalBrainView: View {
    private let brain = AXYONLocalBrain.shared
    @State private var draft = ""
    @State private var choosing = false
    @FocusState private var typing: Bool

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { reader in
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        if brain.canThink {
                            engine
                            if brain.turns.isEmpty { ideas }
                            ForEach(brain.turns) { turn in bubble(turn) }
                        } else {
                            welcome
                            LocalBrainShelf()
                        }
                        if let problem = brain.problem {
                            Label(problem, systemImage: "exclamationmark.triangle.fill")
                                .font(.footnote)
                                .foregroundStyle(AXYONPalette.warn)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .padding(20)
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: brain.turns.last?.text) { reader.scrollTo("end", anchor: .bottom) }
            }
            if brain.canThink { composer }
        }
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("Local brain")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if brain.canThink {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button { brain.newConversation() } label: { Label("New conversation", systemImage: "square.and.pencil") }
                        Toggle(isOn: Binding(get: { brain.useMemory }, set: { brain.useMemory = $0 })) {
                            Label("Answer from my memory", systemImage: "brain")
                        }
                        Button { choosing = true } label: { Label("Models", systemImage: "shippingbox") }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
        .sheet(isPresented: $choosing) {
            NavigationStack {
                ScrollView {
                    LocalBrainShelf().padding(20)
                }
                .axyonBackdrop(intensity: 0.7)
                .navigationTitle("Models")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) { Button("Done") { choosing = false } }
                }
            }
        }
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("A model that thinks on this iPhone", systemImage: "cpu.fill")
                .font(.headline)
                .foregroundStyle(.white)
            Text("Download a model once and AXYON answers with the network off: questions, summaries and drafts, grounded on what it read. Nothing asked here leaves the iPhone.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
                .fixedSize(horizontal: false, vertical: true)
            Text("A model that fits a phone is weaker than AXYON Core's: it does not use tools, and it can be wrong with confidence.")
                .font(.caption)
                .foregroundStyle(AXYONPalette.faint)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 22)
    }

    private var engine: some View {
        HStack(spacing: 12) {
            Image(systemName: "cpu.fill")
                .font(.title3.weight(.semibold))
                .foregroundStyle(.white)
                .frame(width: 42, height: 42)
                .background(Circle().fill(AXYONTheme.current.accentGradient))
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: brain.engine).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Button { choosing = true } label: { Text("Change").font(.footnote.weight(.semibold)) }
                .coucouGlassButton()
        }
        .padding(14)
        .axyonCard(cornerRadius: 22)
    }

    private var detail: String {
        if brain.waking { return String(localized: "Opening the model…") }
        if let done = brain.digesting { return String(localized: "Summing up: \(Int(done * 100))%") }
        var parts = [String(localized: "On this iPhone, with the network off")]
        if brain.speed > 0 { parts.append(String(localized: "\(Int(brain.speed.rounded())) tokens a second")) }
        if brain.onProcessor { parts.append(String(localized: "on the processor alone: memory is short")) }
        return parts.joined(separator: " · ")
    }

    private var ideas: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Try").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
            ForEach([String(localized: "What did the last reports say about vacancy?"),
                     String(localized: "Explain P/VP as if I were starting today."),
                     String(localized: "Write a short message asking the building manager about the water bill.")], id: \.self) { idea in
                Button {
                    Task { await brain.ask(idea) }
                } label: {
                    Text(idea)
                        .font(.footnote)
                        .foregroundStyle(Color.white.opacity(0.9))
                        .multilineTextAlignment(.leading)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .axyonCard(cornerRadius: 16)
                }
                .buttonStyle(PressableButtonStyle())
            }
        }
    }

    private func bubble(_ turn: BrainTurn) -> some View {
        VStack(alignment: turn.mine ? .trailing : .leading, spacing: 6) {
            if turn.text.isEmpty {
                ProgressView().tint(.white).padding(12)
            } else {
                Text(turn.text)
                    .font(.callout)
                    .foregroundStyle(turn.mine ? Color.black : Color.white.opacity(0.94))
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(RoundedRectangle(cornerRadius: 18, style: .continuous)
                        .fill(turn.mine ? AnyShapeStyle(Color.white) : AnyShapeStyle(Color.white.opacity(0.08))))
            }
            if !turn.sources.isEmpty {
                Text(String(localized: "From your memory: \(turn.sources.joined(separator: "; "))"))
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: turn.mine ? .trailing : .leading)
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Ask with the network off", text: $draft, axis: .vertical)
                .lineLimit(1...5)
                .focused($typing)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Color.white.opacity(0.08)))
            if brain.thinking {
                Button { brain.stop() } label: {
                    Image(systemName: "stop.circle.fill").font(.system(size: 34)).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Stop")
            } else {
                Button {
                    let text = draft
                    draft = ""
                    Task { await brain.ask(text) }
                } label: {
                    Image(systemName: "arrow.up.circle.fill").font(.system(size: 34)).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityLabel("Send")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial)
    }
}

/// The models: Apple's where the iPhone has it, the ones to download, and one from a link.
struct LocalBrainShelf: View {
    private let brain = AXYONLocalBrain.shared
    @State private var link = ""
    @State private var removing: LocalModel?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !AXYONLocalBrain.built {
                Label("This build carries no model engine: it is in the iPhone's build, not in the Simulator's.", systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.warn)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if brain.appleReady { apple }
            ForEach(brain.models) { model in card(model) }
            custom
            Text(String(localized: "\(ByteCountFormatter.string(fromByteCount: LocalBrainFiles.freeSpace, countStyle: .file)) free on this iPhone. Models come from Hugging Face, from the link shown; a download goes on with AXYON closed."))
                .font(.caption2)
                .foregroundStyle(AXYONPalette.faint)
                .fixedSize(horizontal: false, vertical: true)
        }
        .confirmationDialog("Delete this model?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
                            titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let model = removing { Task { await brain.delete(model) } }
                removing = nil
            }
            Button("Cancel", role: .cancel) { removing = nil }
        }
    }

    private var apple: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Apple's model").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                Text("Already on this iPhone with Apple Intelligence: nothing to download.")
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            choice(AXYONLocalBrain.apple)
        }
        .padding(14)
        .axyonCard(cornerRadius: 20)
    }

    private func card(_ model: LocalModel) -> some View {
        let fits = brain.fits(model)
        let here = brain.installed.contains(model.id)
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(verbatim: model.name).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                Text(verbatim: model.maker).font(.caption).foregroundStyle(AXYONPalette.faint)
                Spacer(minLength: 0)
                if model.bytes > 0 {
                    Text(verbatim: model.size).font(.caption.weight(.semibold)).monospacedDigit().foregroundStyle(AXYONTheme.current.accent)
                }
            }
            Text(model.note)
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
                .fixedSize(horizontal: false, vertical: true)
            if !fits {
                Label(String(localized: "Needs \(model.memory) GB of memory: this iPhone has \(AXYONDevice.memory)."), systemImage: "memorychip")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.warn)
            }
            if let fraction = brain.progress[model.id] {
                HStack(spacing: 10) {
                    ProgressView(value: fraction).tint(AXYONTheme.current.accent)
                    Text(verbatim: "\(Int(fraction * 100))%").font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                    Button { brain.cancel(model) } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(AXYONPalette.dim) }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Cancel")
                }
            } else if here {
                HStack(spacing: 10) {
                    choice(model.id)
                    Spacer(minLength: 0)
                    Button { removing = model } label: { Image(systemName: "trash").foregroundStyle(AXYONPalette.dim) }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Delete")
                }
            } else {
                Button { brain.download(model) } label: {
                    Label(model.bytes > 0 ? String(localized: "Download \(model.size)") : String(localized: "Download"), systemImage: "arrow.down.circle")
                        .font(.footnote.weight(.semibold))
                }
                .coucouGlassButton()
                .disabled(!fits || !AXYONLocalBrain.built)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }

    @ViewBuilder private func choice(_ id: String) -> some View {
        if brain.chosen == id {
            Label("In use", systemImage: "checkmark.circle.fill")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(AXYONPalette.up)
        } else {
            Button { brain.use(id) } label: { Text("Use").font(.footnote.weight(.semibold)) }
                .coucouGlassButton()
        }
    }

    private var custom: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Another model, from its link").font(.subheadline.weight(.semibold)).foregroundStyle(.white)
            TextField("", text: $link, prompt: Text(verbatim: "https://…/model.gguf"))
                .font(.footnote)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
            Button {
                brain.add(link: link)
                if brain.problem == nil { link = "" }
            } label: {
                Label("Download", systemImage: "arrow.down.circle").font(.footnote.weight(.semibold))
            }
            .coucouGlassButton()
            .disabled(link.trimmingCharacters(in: .whitespaces).isEmpty || !AXYONLocalBrain.built)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }
}
