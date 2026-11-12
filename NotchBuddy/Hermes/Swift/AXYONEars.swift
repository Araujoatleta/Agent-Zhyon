import Accelerate
import AVFoundation
import Speech
import SwiftUI
import UniformTypeIdentifiers

// AXYON's ears: hours of audio turned into text on the iPhone itself (Apple's on-device speech recognition, nothing
// sent anywhere). A recording made here or an audio or video file (a results call, a class, a meeting) is read in
// windows of under a minute, each cut at a silence so no word is split, at the speed the processor allows. The text
// goes into the memory (searched by meaning), to AXYON Core for a summary, and, when it is about an asset, to the
// intern as a document: the court then meets on it by itself.

struct EarsSegment: Codable, Sendable, Equatable {
    /// Where it starts in the audio, seconds.
    var start: Double
    var text: String
}

struct EarsTranscript: Codable, Identifiable, Sendable, Equatable {
    var id: String
    var title: String
    var date: Date
    var seconds: Double
    var ticker: String
    var segments: [EarsSegment]
    /// Given to the intern as a document of `ticker`.
    var delivered = false
    /// Its summary, written by the local brain on the iPhone (AXYONLocalBrain.swift).
    var summary: String?

    var text: String { segments.map(\.text).joined(separator: " ") }
    var words: Int { text.split(separator: " ").count }

    static func clock(_ seconds: Double) -> String {
        let s = Int(seconds.rounded())
        return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60) : String(format: "%d:%02d", s / 60, s % 60)
    }
}

enum EarsError: Error {
    case said(String)
}

/// One window of audio given to the recognizer, and its text once the recognizer ends it.
final class EarsWindow: @unchecked Sendable {
    private let request = SFSpeechAudioBufferRecognitionRequest()
    private var task: SFSpeechRecognitionTask?
    private let lock = NSLock()
    private var best = ""
    private var finished = false
    private var waiter: CheckedContinuation<String, Never>?

    init(recognizer: SFSpeechRecognizer) {
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            let words = result?.bestTranscription.formattedString
            let ended = error != nil || (result?.isFinal ?? false)
            self.heardMore(words, ended: ended)
        }
    }

    private func heardMore(_ words: String?, ended: Bool) {
        let resume: (CheckedContinuation<String, Never>?, String) = lock.withLock {
            if let words, !words.isEmpty { best = words }
            guard ended, !finished else { return (nil, best) }
            finished = true
            let waiting = waiter
            waiter = nil
            return (waiting, best)
        }
        resume.0?.resume(returning: resume.1)
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        request.append(buffer)
    }

    var heard: String { lock.withLock { best } }

    /// No more audio: the window's text, once the recognizer is through with it (it gets a minute at most).
    func finish() async -> String {
        request.endAudio()
        let timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(60))
            self?.heardMore(nil, ended: true)
        }
        let text = await withCheckedContinuation { (continuation: CheckedContinuation<String, Never>) in
            let ready: String? = lock.withLock {
                if finished { return best }
                waiter = continuation
                return nil
            }
            if let ready { continuation.resume(returning: ready) }
        }
        timeout.cancel()
        task?.cancel()
        return text
    }

    func cancel() {
        task?.cancel()
        heardMore(nil, ended: true)
    }
}

enum EarsTranscriber {
    /// What the recognizer takes best: one channel at 16 kHz.
    private static let rate = 16_000.0

    static func authorized() async -> Bool {
        await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0 == .authorized) }
        }
    }

    /// The audio of `url` as text, window by window; `progress` gets how far it is (0 to 1) and the last words.
    static func transcribe(_ url: URL, locale: String, progress: @escaping @Sendable (Double, String) -> Void) async throws
        -> (seconds: Double, segments: [EarsSegment]) {
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: locale)), recognizer.isAvailable else {
            throw EarsError.said(String(localized: "Speech recognition for this language is not available on this iPhone right now."))
        }
        guard recognizer.supportsOnDeviceRecognition else {
            throw EarsError.said(String(localized: "On-device recognition for this language is not installed: turn on Dictation for it (iPhone Settings → General → Keyboard → Dictation), wait for the download and try again. AXYON only transcribes on the iPhone itself."))
        }
        let audio = try await readable(url)
        defer {
            if audio != url { try? FileManager.default.removeItem(at: audio) }
        }
        let file = try AVAudioFile(forReading: audio)
        let source = file.processingFormat
        guard let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: rate, channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: source, to: target) else {
            throw EarsError.said(String(localized: "This audio's format could not be read."))
        }
        let total = max(0.1, Double(file.length) / source.sampleRate)
        let step = AVAudioFrameCount(max(1024, source.sampleRate / 5))
        var segments: [EarsSegment] = []
        var window = EarsWindow(recognizer: recognizer)
        var began = 0.0
        var reads = 0
        while file.framePosition < file.length {
            if Task.isCancelled {
                window.cancel()
                throw CancellationError()
            }
            guard let buffer = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: step) else { break }
            try file.read(into: buffer, frameCount: step)
            if buffer.frameLength == 0 { break }
            guard let mono = convert(buffer, with: converter, to: target) else { continue }
            window.append(mono)
            reads += 1
            let now = Double(file.framePosition) / source.sampleRate
            let age = now - began
            // A window ends at a silence after forty seconds, or at fifty-five whatever is said.
            if age >= 55 || (age >= 40 && loudness(mono) < 0.012) {
                let text = await window.finish()
                if !text.isEmpty { segments.append(EarsSegment(start: began, text: text)) }
                progress(min(1, now / total), text)
                window = EarsWindow(recognizer: recognizer)
                began = now
            } else if reads % 20 == 0 {
                progress(min(1, now / total), window.heard)
                // The recognizer reads slower than the file is fed: it gets its turn.
                await Task.yield()
            }
        }
        let last = await window.finish()
        if !last.isEmpty { segments.append(EarsSegment(start: began, text: last)) }
        progress(1, last)
        return (total, segments)
    }

    private static func loudness(_ buffer: AVAudioPCMBuffer) -> Float {
        guard let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
        var level: Float = 0
        vDSP_rmsqv(samples, 1, &level, vDSP_Length(buffer.frameLength))
        return level
    }

    private static func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter, to target: AVAudioFormat) -> AVAudioPCMBuffer? {
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * target.sampleRate / buffer.format.sampleRate) + 256
        guard let out = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return nil }
        let feed = EarsFeed(buffer)
        var error: NSError?
        let status = converter.convert(to: out, error: &error) { _, state in
            if let next = feed.take() {
                state.pointee = .haveData
                return next
            }
            state.pointee = .noDataNow
            return nil
        }
        guard status != .error, error == nil, out.frameLength > 0 else { return nil }
        return out
    }

    /// An audio file as AVAudioFile reads it: the file itself, or a video's sound taken out into an .m4a first.
    private static func readable(_ url: URL) async throws -> URL {
        if (try? AVAudioFile(forReading: url)) != nil { return url }
        let asset = AVURLAsset(url: url)
        guard let export = AVAssetExportSession(asset: asset, presetName: AVAssetExportPresetAppleM4A) else {
            throw EarsError.said(String(localized: "This file has no sound AXYON can read."))
        }
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("axyon-ears-\(UUID().uuidString).m4a")
        do {
            try await export.export(to: out, as: .m4a)
        } catch {
            throw EarsError.said(String(localized: "This file has no sound AXYON can read."))
        }
        return out
    }
}

/// One buffer handed to the converter once.
private final class EarsFeed: @unchecked Sendable {
    private var buffer: AVAudioPCMBuffer?

    init(_ buffer: AVAudioPCMBuffer) {
        self.buffer = buffer
    }

    func take() -> AVAudioPCMBuffer? {
        let next = buffer
        buffer = nil
        return next
    }
}

@MainActor
@Observable
final class AXYONEars {
    static let shared = AXYONEars()

    private(set) var transcripts: [EarsTranscript] = []
    private(set) var working = false
    private(set) var progress = 0.0
    private(set) var heard = ""
    private(set) var problem: String?
    private(set) var recording = false
    private(set) var recordingSince: Date?
    private(set) var startedAt: Date?

    @ObservationIgnored private var recorder: AVAudioRecorder?
    @ObservationIgnored private var job: Task<Void, Never>?

    /// The language heard: Portuguese first, as the app speaks.
    var locale: String = UserDefaults.standard.string(forKey: "axyon.ears.locale") ?? "pt-BR" {
        didSet { UserDefaults.standard.set(locale, forKey: "axyon.ears.locale") }
    }

    private static var file: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/ears/transcripts.json")
    }

    private init() {
        if let data = try? Data(contentsOf: Self.file), let kept = try? JSONDecoder().decode([EarsTranscript].self, from: data) {
            transcripts = kept
        }
    }

    private func save() {
        try? FileManager.default.createDirectory(at: Self.file.deletingLastPathComponent(), withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(transcripts) { try? data.write(to: Self.file, options: .atomic) }
    }

    /// How long is left, from how fast it went so far.
    var remaining: TimeInterval? {
        guard working, let startedAt, progress > 0.03 else { return nil }
        let spent = Date.now.timeIntervalSince(startedAt)
        return spent / progress * (1 - progress)
    }

    // MARK: A file

    /// A file picked in Files (audio or video): copied in, transcribed, kept.
    func transcribe(picked: URL) {
        let scoped = picked.startAccessingSecurityScopedResource()
        let copy = FileManager.default.temporaryDirectory.appendingPathComponent("axyon-ears-\(UUID().uuidString).\(picked.pathExtension)")
        do {
            try FileManager.default.copyItem(at: picked, to: copy)
        } catch {
            problem = error.localizedDescription
            if scoped { picked.stopAccessingSecurityScopedResource() }
            return
        }
        if scoped { picked.stopAccessingSecurityScopedResource() }
        transcribe(file: copy, title: picked.deletingPathExtension().lastPathComponent)
    }

    private func transcribe(file: URL, title: String) {
        guard !working else { return }
        working = true
        progress = 0
        heard = ""
        problem = nil
        startedAt = .now
        UIApplication.shared.isIdleTimerDisabled = true
        let locale = locale
        job = Task {
            defer {
                working = false
                UIApplication.shared.isIdleTimerDisabled = false
                try? FileManager.default.removeItem(at: file)
            }
            guard await EarsTranscriber.authorized() else {
                problem = String(localized: "Allow Speech Recognition for AXYON in the iPhone's Settings.")
                return
            }
            do {
                let result = try await EarsTranscriber.transcribe(file, locale: locale) { fraction, words in
                    Task { @MainActor in
                        AXYONEars.shared.progress = fraction
                        if !words.isEmpty { AXYONEars.shared.heard = words }
                    }
                }
                guard !result.segments.isEmpty else {
                    problem = String(localized: "No speech was heard in this audio.")
                    return
                }
                let transcript = EarsTranscript(id: String(UUID().uuidString.prefix(8)).lowercased(), title: title, date: .now,
                                                seconds: result.seconds, ticker: "", segments: result.segments)
                transcripts.insert(transcript, at: 0)
                save()
                Haptics.success()
                // Into the memory at once: what was said is searched by meaning.
                await AXYONMemoryIndex.shared.add(AXYONMemory.chunks(of: transcript))
                await AXYONMemoryIndex.shared.save()
                await AXYONMemory.shared.refresh()
            } catch is CancellationError {
                problem = nil
            } catch EarsError.said(let text) {
                problem = text
                Haptics.error()
            } catch {
                problem = error.localizedDescription
                Haptics.error()
            }
        }
    }

    func cancel() {
        job?.cancel()
    }

    // MARK: A recording

    func startRecording() async {
        guard !recording, !working else { return }
        guard await AVAudioApplication.requestRecordPermission() else {
            problem = String(localized: "Allow the microphone for AXYON in the iPhone's Settings.")
            return
        }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("axyon-ears-\(UUID().uuidString).m4a")
        let settings: [String: Any] = [AVFormatIDKey: Int(kAudioFormatMPEG4AAC), AVSampleRateKey: 22_050, AVNumberOfChannelsKey: 1,
                                       AVEncoderAudioQualityKey: AVAudioQuality.medium.rawValue]
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .default)
            try session.setActive(true)
            let made = try AVAudioRecorder(url: url, settings: settings)
            made.isMeteringEnabled = true
            guard made.record() else { throw CocoaError(.fileWriteUnknown) }
            recorder = made
            recording = true
            recordingSince = .now
            problem = nil
            Haptics.impact()
        } catch {
            problem = error.localizedDescription
        }
    }

    /// How loud the microphone hears now, 0 to 1.
    func level() -> Double {
        guard let recorder, recording else { return 0 }
        recorder.updateMeters()
        return Double(max(0, min(1, (recorder.averagePower(forChannel: 0) + 50) / 50)))
    }

    func stopRecording() {
        guard let recorder, recording else { return }
        recorder.stop()
        let url = recorder.url
        self.recorder = nil
        recording = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        let day = Date.now.formatted(.dateTime.day().month().hour().minute().locale(Locale(identifier: "pt_BR")))
        transcribe(file: url, title: String(localized: "Recording of \(day)"))
        recordingSince = nil
    }

    // MARK: What is kept

    func rename(_ id: String, title: String, ticker: String) {
        guard let index = transcripts.firstIndex(where: { $0.id == id }) else { return }
        transcripts[index].title = title.trimmingCharacters(in: .whitespaces).isEmpty ? transcripts[index].title : title
        transcripts[index].ticker = ticker.trimmingCharacters(in: .whitespaces).uppercased()
        save()
    }

    /// The transcript summed up by the local brain, on the iPhone: read in parts, minutes for an hour of talk.
    func summarize(_ id: String) async throws {
        guard let transcript = transcripts.first(where: { $0.id == id }) else { return }
        let brain = AXYONLocalBrain.shared
        guard let summary = await brain.digest(transcript.text, title: transcript.title) else {
            throw AXYONInvestError.said(brain.problem ?? String(localized: "The local brain is busy: try again in a moment."))
        }
        guard let index = transcripts.firstIndex(where: { $0.id == id }) else { return }
        transcripts[index].summary = summary
        save()
    }

    func remove(_ id: String) async {
        transcripts.removeAll { $0.id == id }
        save()
        await AXYONMemoryIndex.shared.remove(keys: ["ears:" + id])
        await AXYONMemoryIndex.shared.save()
        await AXYONMemory.shared.refresh()
    }

    /// The transcript given to the intern as a document of its asset (the engine's reader): digested by AXYON Core,
    /// and the court meets on it by itself, as on any new document.
    func deliver(_ id: String) async throws {
        guard let index = transcripts.firstIndex(where: { $0.id == id }) else { return }
        let transcript = transcripts[index]
        guard !transcript.ticker.isEmpty else {
            throw AXYONInvestError.said(String(localized: "Which asset is it about? Write its ticker first."))
        }
        // Pages as the reader keeps them: up to 5,000 characters each.
        var pages: [String] = []
        var page = ""
        for segment in transcript.segments {
            let line = "[\(EarsTranscript.clock(segment.start))] \(segment.text)"
            if page.count + line.count > 4_800, !page.isEmpty {
                pages.append(page)
                page = ""
            }
            page += (page.isEmpty ? "" : "\n") + line
        }
        if !page.isEmpty { pages.append(page) }
        let pending: [String: Any] = ["key": "ears:" + transcript.id, "ticker": transcript.ticker, "kind": "release",
                                      "title": transcript.title, "reference": investDay(AXYONWallet.iso(transcript.date)),
                                      "delivered": AXYONWallet.iso(transcript.date), "url": "", "download": ""]
        _ = try await AXYONInsight.call("invest_docs", ["do": "ingest", "pending": pending, "pages": pages])
        transcripts[index].delivered = true
        save()
        Task { await AXYONInvest.shared.think() }
    }

    /// The transcript as a text file in AXYON Core's inbox, sent to the chat with what to do with it.
    func ask(_ id: String, _ prompt: String) throws {
        guard let transcript = transcripts.first(where: { $0.id == id }) else { return }
        guard HermesEngine.shared.canChat, HermesEngine.shared.answering == nil else {
            throw AXYONInvestError.said(String(localized: "AXYON Core is busy or not running yet: try again in a moment."))
        }
        let folder = AXYONInbox.folder
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let name = "\(UUID().uuidString.prefix(8).lowercased())-transcricao.txt"
        let body = transcript.segments.map { "[\(EarsTranscript.clock($0.start))] \($0.text)" }.joined(separator: "\n")
        try body.write(to: folder.appendingPathComponent(name), atomically: true, encoding: .utf8)
        let attachment = HermesAttachment(file: name, name: transcript.title + ".txt", kind: .file, textFile: name)
        HermesEngine.shared.send(prompt, attachments: [attachment])
    }

    #if targetEnvironment(simulator)
    /// CI's screenshots: an example transcript, as a results call would leave.
    func seedForScreens() {
        guard transcripts.isEmpty else { return }
        transcripts = [EarsTranscript(id: "exemplo1", title: "Teleconferência de resultados 3T26", date: .now, seconds: 3_184, ticker: "GARE11",
                                      segments: [EarsSegment(start: 0, text: "Boa tarde a todos. Vamos começar a apresentação dos resultados do terceiro trimestre."),
                                                 EarsSegment(start: 52, text: "A vacância física do portfólio recuou para seis por cento, com a locação do galpão de Guarulhos."),
                                                 EarsSegment(start: 104, text: "O rendimento distribuído se manteve em oito centavos e três décimos por cota.")])]
    }
    #endif
}

// MARK: - The screen

struct AXYONEarsView: View {
    private let ears = AXYONEars.shared
    @State private var picking = false
    @State private var opened: EarsTranscript?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                listener
                if let problem = ears.problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.down)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !ears.transcripts.isEmpty {
                    AXYONHeader(title: String(localized: "What was heard"))
                    ForEach(ears.transcripts) { transcript in
                        Button { opened = transcript } label: { row(transcript) }
                            .buttonStyle(PressableButtonStyle())
                    }
                }
                Text("Transcribed on this iPhone by Apple's on-device recognition: the audio is not sent anywhere and is deleted once it became text. Keep AXYON open while it works; an hour of audio takes several minutes.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
            .padding(20)
        }
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("Ears")
        .navigationBarTitleDisplayMode(.inline)
        .fileImporter(isPresented: $picking, allowedContentTypes: [.audio, .movie, .mpeg4Movie, .quickTimeMovie]) { result in
            if case .success(let url) = result { ears.transcribe(picked: url) }
        }
        .sheet(item: $opened) { transcript in
            NavigationStack { EarsTranscriptView(id: transcript.id) }
        }
        .task {
            #if targetEnvironment(simulator)
            if AXYONTestScreen.base == "core-ears" { ears.seedForScreens() }
            #endif
        }
    }

    private var listener: some View {
        VStack(spacing: 16) {
            EarsWave(active: ears.recording || ears.working, level: { ears.level() })
                .frame(height: 96)
            if ears.working {
                ProgressView(value: ears.progress).tint(AXYONTheme.current.accent)
                HStack {
                    Text(verbatim: "\(Int((ears.progress * 100).rounded()))%").font(.headline).monospacedDigit().foregroundStyle(.white)
                    Spacer()
                    if let left = ears.remaining {
                        Text(String(localized: "about \(EarsTranscript.clock(left)) left")).font(.caption).foregroundStyle(AXYONPalette.dim)
                    }
                }
                if !ears.heard.isEmpty {
                    Text(verbatim: "“…" + String(ears.heard.suffix(160)) + "”")
                        .font(.footnote)
                        .italic()
                        .foregroundStyle(AXYONPalette.dim)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .lineLimit(3)
                }
                Button(role: .cancel) { ears.cancel() } label: { Label("Stop", systemImage: "stop.fill") }
                    .coucouGlassButton()
            } else if ears.recording {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text(verbatim: EarsTranscript.clock(context.date.timeIntervalSince(ears.recordingSince ?? context.date)))
                        .font(.system(size: 40, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                }
                Button { ears.stopRecording() } label: {
                    Label("Stop and transcribe", systemImage: "stop.circle.fill").font(.headline)
                }
                .coucouGlassButton(prominent: true)
            } else {
                Text("Hours of audio into text, on the iPhone").font(.title3.weight(.bold)).foregroundStyle(.white)
                Text("A results call, a class, a meeting: record it here or bring the audio or video file. The text goes into AXYON's memory, to the chat for a summary, and to the court when it is about one of your assets.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
                    .multilineTextAlignment(.center)
                HStack(spacing: 10) {
                    Button { Task { await ears.startRecording() } } label: {
                        Label("Record", systemImage: "mic.fill").font(.headline).frame(maxWidth: .infinity)
                    }
                    .coucouGlassButton(prominent: true)
                    Button { picking = true } label: {
                        Label("Bring a file", systemImage: "folder.fill").font(.headline).frame(maxWidth: .infinity)
                    }
                    .coucouGlassButton()
                }
                Picker("Language", selection: Binding(get: { ears.locale }, set: { ears.locale = $0 })) {
                    Text(verbatim: "Português").tag("pt-BR")
                    Text(verbatim: "English").tag("en-US")
                    Text(verbatim: "Español").tag("es-ES")
                }
                .pickerStyle(.segmented)
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity)
        .axyonCard(cornerRadius: 28)
    }

    private func row(_ transcript: EarsTranscript) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Image(systemName: "waveform").foregroundStyle(AXYONTheme.current.accent)
                Text(transcript.title).font(.subheadline.weight(.semibold)).foregroundStyle(.white).lineLimit(1)
                Spacer(minLength: 4)
                Text(verbatim: EarsTranscript.clock(transcript.seconds)).font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
            }
            Text(transcript.text).font(.footnote).foregroundStyle(AXYONPalette.dim).lineLimit(2).multilineTextAlignment(.leading)
            HStack(spacing: 6) {
                if !transcript.ticker.isEmpty { InvestTag(text: transcript.ticker, color: AXYONTheme.current.accent) }
                InvestTag(text: String(localized: "\(transcript.words) words"))
                if transcript.delivered { InvestTag(text: String(localized: "with the intern"), color: InvestPalette.favor) }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }
}

/// A sound wave drawn in code: bars that follow the microphone while it records, and roll while the audio is read.
struct EarsWave: View {
    let active: Bool
    let level: () -> Double

    var body: some View {
        let accent = AXYONTheme.current.accent
        TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: !active)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            let loud = active ? max(0.18, level()) : 0.12
            Canvas { canvas, area in
                let bars = 41
                let width = area.width / CGFloat(bars)
                for bar in 0..<bars {
                    let phase = Double(bar) * 0.42
                    let swing = 0.5 + 0.5 * sin(time * 3.1 + phase) * cos(time * 1.7 + phase * 0.6)
                    let centre = 1 - abs(Double(bar) - Double(bars - 1) / 2) / (Double(bars) / 2)
                    let height = max(4, area.height * CGFloat((0.15 + 0.85 * swing) * (0.35 + 0.65 * centre) * (active ? loud + 0.25 : 0.3)))
                    let rect = CGRect(x: CGFloat(bar) * width + width * 0.22, y: (area.height - height) / 2, width: width * 0.56, height: height)
                    canvas.fill(Path(roundedRect: rect, cornerRadius: width * 0.28),
                                with: .color(accent.opacity(0.35 + 0.65 * centre)))
                }
            }
        }
        .accessibilityHidden(true)
    }
}

/// One transcript: its text by the minute, and what to do with it.
struct EarsTranscriptView: View {
    let id: String
    private let ears = AXYONEars.shared
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var ticker = ""
    @State private var said: String?
    @State private var failed = false
    @State private var working = false

    private var transcript: EarsTranscript? { ears.transcripts.first { $0.id == id } }

    var body: some View {
        ScrollView {
            if let transcript {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(spacing: 10) {
                        TextField("Title", text: $title).font(.headline)
                        Divider().overlay(Color.white.opacity(0.1))
                        TextField("Asset it is about (GARE11), optional", text: $ticker)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                    }
                    .padding(14)
                    .axyonCard(cornerRadius: 20)
                    .onChange(of: title) { _, new in ears.rename(id, title: new, ticker: ticker) }
                    .onChange(of: ticker) { _, new in ears.rename(id, title: title, ticker: new) }

                    if let said {
                        Label(said, systemImage: failed ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                            .font(.footnote)
                            .foregroundStyle(failed ? AXYONPalette.down : AXYONPalette.up)
                    }
                    VStack(spacing: 10) {
                        action(String(localized: "Summarize with AXYON Core"), "sparkles") {
                            try ears.ask(id, String(localized: "Summarize this transcript for me: the main points, the figures said, and what deserves my attention."))
                            said = String(localized: "Sent: the summary is being written in the chat.")
                        }
                        if AXYONLocalBrain.shared.canThink {
                            action(String(localized: "Summarize on this iPhone, with the network off"), "cpu.fill") {
                                try await ears.summarize(id)
                                said = String(localized: "Summed up on this iPhone.")
                            }
                            if let done = AXYONLocalBrain.shared.digesting {
                                HStack(spacing: 10) {
                                    ProgressView(value: done).tint(AXYONTheme.current.accent)
                                    Text(verbatim: "\(Int(done * 100))%").font(.caption).monospacedDigit().foregroundStyle(AXYONPalette.dim)
                                }
                            }
                        }
                        action(String(localized: "Give it to the intern (the court meets on it)"), "building.columns.fill") {
                            try await ears.deliver(id)
                            said = String(localized: "Given to the intern as a document of \(ticker.uppercased()): the court meets on it by itself.")
                        }
                        ShareLink(item: transcript.segments.map { "[\(EarsTranscript.clock($0.start))] \($0.text)" }.joined(separator: "\n")) {
                            Label("Share the text", systemImage: "square.and.arrow.up").frame(maxWidth: .infinity)
                        }
                        .coucouGlassButton()
                    }
                    if let summary = transcript.summary, !summary.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            Label("Summary, written on this iPhone", systemImage: "cpu.fill")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(AXYONTheme.current.accent)
                            Text(summary)
                                .font(.callout)
                                .foregroundStyle(Color.white.opacity(0.92))
                                .textSelection(.enabled)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .axyonCard(cornerRadius: 20)
                    }
                    Text(String(localized: "\(EarsTranscript.clock(transcript.seconds)) of audio · \(transcript.words) words"))
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(Array(transcript.segments.enumerated()), id: \.offset) { _, segment in
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Text(verbatim: EarsTranscript.clock(segment.start))
                                    .font(.caption2.weight(.bold))
                                    .monospacedDigit()
                                    .foregroundStyle(AXYONTheme.current.accent)
                                    .frame(width: 46, alignment: .leading)
                                Text(segment.text)
                                    .font(.callout)
                                    .foregroundStyle(Color.white.opacity(0.9))
                                    .textSelection(.enabled)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .axyonCard(cornerRadius: 20)
                    Button(role: .destructive) {
                        Task {
                            await ears.remove(id)
                            dismiss()
                        }
                    } label: {
                        Label("Delete this transcript", systemImage: "trash")
                    }
                    .coucouGlassButton()
                }
                .padding(20)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .axyonBackdrop(intensity: 0.7)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } }
        }
        .onAppear {
            title = transcript?.title ?? ""
            ticker = transcript?.ticker ?? ""
        }
    }

    private func action(_ title: String, _ icon: String, _ work: @escaping @MainActor () async throws -> Void) -> some View {
        Button {
            Task {
                working = true
                defer { working = false }
                do {
                    try await work()
                    failed = false
                    Haptics.success()
                } catch {
                    said = AXYONInvest.describe(error)
                    failed = true
                    Haptics.error()
                }
            }
        } label: {
            Label(title, systemImage: icon).frame(maxWidth: .infinity)
        }
        .coucouGlassButton()
        .disabled(working)
    }
}
