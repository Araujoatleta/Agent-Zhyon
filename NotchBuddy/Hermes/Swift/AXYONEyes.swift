import AVFoundation
import CoreImage
import SwiftUI
import UIKit
import Vision

// AXYON's eyes, live: the camera's frames go through Apple's on-device vision models several times a second (the text
// in view, bar and QR codes, what the scene is, faces, animals), on the phone's neural engine, and nothing leaves the
// iPhone for that. What is found becomes something to do at once (an asset's ticker opens its folder, a boleto's
// line or a Pix code is copied, a date becomes a reminder, a link opens), and a question about what is in view goes
// to AXYON Core with the frame, its answer written over the camera.

// MARK: - What is seen

struct EyesBox: Sendable, Equatable {
    /// In the frame's own space, 0 to 1, origin at the top left.
    let rect: CGRect
    let strong: Bool
}

struct EyesSeen: Sendable, Equatable {
    var lines: [String] = []
    var codes: [String] = []
    /// What the scene is (the classifier's labels), most likely first.
    var labels: [String] = []
    var faces = 0
    var animals: [String] = []
    var boxes: [EyesBox] = []
    /// The frame's size in pixels, upright.
    var size = CGSize(width: 1080, height: 1920)
    /// How long the models took on this frame, milliseconds.
    var tookMs = 0
}

struct EyesFinding: Identifiable, Sendable, Equatable {
    enum Kind: String, Sendable { case ticker, link, pix, boleto, money, date, code }

    let kind: Kind
    let title: String
    let value: String
    var id: String { kind.rawValue + value }

    var icon: String {
        switch kind {
        case .ticker: "folder.fill"
        case .link: "safari.fill"
        case .pix: "qrcode"
        case .boleto: "barcode"
        case .money: "banknote.fill"
        case .date: "calendar.badge.plus"
        case .code: "doc.on.doc.fill"
        }
    }
}

enum EyesReader {
    /// What there is to do with what is in view.
    static func findings(lines: [String], codes: [String]) -> [EyesFinding] {
        var out: [EyesFinding] = []
        func add(_ finding: EyesFinding) {
            if !out.contains(where: { $0.id == finding.id }) { out.append(finding) }
        }
        for code in codes {
            let trimmed = code.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.hasPrefix("000201"), trimmed.lowercased().contains("br.gov.bcb.pix") {
                add(EyesFinding(kind: .pix, title: String(localized: "Copy the Pix code"), value: trimmed))
            } else if trimmed.lowercased().hasPrefix("http://") || trimmed.lowercased().hasPrefix("https://") {
                add(EyesFinding(kind: .link, title: String(localized: "Open \(host(trimmed))"), value: trimmed))
            } else if let line = boleto(trimmed) {
                add(line)
            } else if !trimmed.isEmpty {
                add(EyesFinding(kind: .code, title: String(localized: "Copy the code"), value: trimmed))
            }
        }
        let text = lines.joined(separator: "\n")
        for match in matches(#"\b[A-Z]{4}(?:3|4|5|6|11)\b"#, in: text.uppercased()).prefix(3) {
            add(EyesFinding(kind: .ticker, title: String(localized: "\(match)'s folder"), value: match))
        }
        for match in matches(#"https?://[^\s]+"#, in: text).prefix(1) {
            add(EyesFinding(kind: .link, title: String(localized: "Open \(host(match))"), value: match))
        }
        // A boleto's line is printed on one line of its own.
        if let line = lines.lazy.compactMap({ boleto($0) }).first { add(line) }
        let values = matches(#"R\$\s?\d{1,3}(?:\.\d{3})*,\d{2}"#, in: text)
        if let biggest = values.max(by: { amount($0) < amount($1) }) {
            add(EyesFinding(kind: .money, title: String(localized: "Copy \(biggest)"), value: biggest))
        }
        for match in matches(#"\b\d{2}/\d{2}/\d{4}\b"#, in: text).prefix(1) where day(match).map({ $0 > Date.now.addingTimeInterval(-86_400) }) == true {
            add(EyesFinding(kind: .date, title: String(localized: "Reminder for \(match)"), value: match))
        }
        return Array(out.prefix(6))
    }

    /// A line that may hold something to act on (a ticker, a value, a link, a date, a long number): a quick test, for
    /// every line of every frame.
    static func worthALook(_ line: String) -> Bool {
        if line.contains("R$") || line.lowercased().contains("http") { return true }
        if line.filter(\.isNumber).count >= 40 { return true }
        for token in line.split(whereSeparator: { !($0.isLetter || $0.isNumber) }) {
            if token.count == 10, token.filter({ $0 == "/" }).isEmpty, token.allSatisfy(\.isNumber) { continue }
            guard token.count == 5 || token.count == 6 else { continue }
            let letters = token.prefix(4)
            let number = String(token.dropFirst(4))
            if letters.allSatisfy({ $0.isUppercase && $0.isASCII }), ["3", "4", "5", "6", "11"].contains(number) { return true }
        }
        return line.contains("/") && line.filter(\.isNumber).count >= 8
    }

    static func matches(_ pattern: String, in text: String) -> [String] {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
        let whole = NSRange(text.startIndex..., in: text)
        return regex.matches(in: text, range: whole).compactMap { Range($0.range, in: text).map { String(text[$0]) } }
    }

    static func amount(_ text: String) -> Double {
        Double(text.filter { $0.isNumber || $0 == "," }.replacingOccurrences(of: ",", with: ".")) ?? 0
    }

    static func day(_ text: String) -> Date? {
        let parts = text.split(separator: "/").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return Calendar.current.date(from: DateComponents(year: parts[2], month: parts[1], day: parts[0], hour: 9))
    }

    static func host(_ link: String) -> String {
        URL(string: link)?.host() ?? link
    }

    /// A bank boleto's line (47 digits) in a text: its value and due day read from it.
    static func boleto(_ text: String) -> EyesFinding? {
        let digits = text.filter(\.isNumber)
        guard digits.count >= 47, digits.count <= 60 else { return nil }
        // The line as printed has dots and spaces between its fields: 47 digits in a row once they are gone.
        let compact = text.filter { $0.isNumber || $0.isLetter || $0 == "\n" }
        guard let line = matches(#"\d{47}"#, in: compact).first else { return nil }
        let value = (Double(line.suffix(10)) ?? 0) / 100
        var title = String(localized: "Copy the boleto's line")
        if value > 0 { title += " · " + AXYONMoney.text(value) }
        if let factor = Int(line.dropLast(10).suffix(4)), factor >= 1000, let due = due(factor) {
            title += " · " + due.formatted(.dateTime.day().month(.twoDigits).locale(Locale(identifier: "pt_BR")))
        }
        return EyesFinding(kind: .boleto, title: title, value: line)
    }

    /// The due day of a boleto's factor: days since 07/10/1997, or since 22/02/2025 (factor 1000 again) when that is
    /// the one near today.
    static func due(_ factor: Int) -> Date? {
        let calendar = Calendar(identifier: .gregorian)
        guard let old = calendar.date(from: DateComponents(year: 1997, month: 10, day: 7)),
              let new = calendar.date(from: DateComponents(year: 2025, month: 2, day: 22)) else { return nil }
        let first = calendar.date(byAdding: .day, value: factor, to: old)
        let second = calendar.date(byAdding: .day, value: factor - 1000, to: new)
        return [first, second].compactMap { $0 }.min { abs($0.timeIntervalSinceNow) < abs($1.timeIntervalSinceNow) }
    }

    /// The classifier's label in Portuguese, for what it sees most often; else as it says it.
    static func label(_ identifier: String) -> String {
        let known: [String: String] = [
            "document": "documento", "text": "texto", "paper": "papel", "book": "livro", "screenshot": "tela", "computer": "computador",
            "laptop": "notebook", "computer_keyboard": "teclado", "computer_monitor": "monitor", "television": "TV", "phone": "celular",
            "people": "pessoas", "person": "pessoa", "adult": "adulto", "child": "criança", "portrait": "retrato", "cat": "gato",
            "dog": "cachorro", "food": "comida", "drink": "bebida", "coffee": "café", "fruit": "fruta", "table": "mesa", "chair": "cadeira",
            "sofa": "sofá", "bed": "cama", "lamp": "luminária", "light": "luz", "window": "janela", "door": "porta", "room": "cômodo",
            "interior_room": "ambiente interno", "living_room": "sala", "kitchen": "cozinha", "bedroom": "quarto", "bathroom": "banheiro",
            "office": "escritório", "building": "prédio", "house": "casa", "street": "rua", "car": "carro", "vehicle": "veículo",
            "motorcycle": "moto", "bicycle": "bicicleta", "plant": "planta", "tree": "árvore", "flower": "flor", "sky": "céu",
            "outdoor": "ao ar livre", "night_sky": "céu à noite", "receipt": "recibo", "money": "dinheiro", "credit_card": "cartão",
            "handwriting": "manuscrito", "whiteboard": "quadro branco", "chart": "gráfico", "diagram": "diagrama", "map": "mapa",
            "clock": "relógio", "watch": "relógio", "bottle": "garrafa", "cup": "xícara", "glasses": "óculos", "shoe": "sapato",
            "clothing": "roupa", "bag": "bolsa", "key": "chave", "remote_control": "controle remoto", "speaker": "caixa de som",
            "headphones": "fones", "camera": "câmera", "refrigerator": "geladeira", "medicine": "remédio", "toy": "brinquedo",
        ]
        return known[identifier] ?? identifier.replacingOccurrences(of: "_", with: " ")
    }
}

// MARK: - The camera

/// The camera and the vision models on its frames, on a queue of their own.
final class EyesCamera: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate, @unchecked Sendable {
    let session = AVCaptureSession()
    private let queue = DispatchQueue(label: "fr.louisraille.Coucou.eyes", qos: .userInitiated)
    private let output = AVCaptureVideoDataOutput()
    private let context = CIContext()
    private var configured = false
    private var lastLook = Date.distantPast
    private var latest: CVPixelBuffer?
    private var device: AVCaptureDevice?
    /// Each look's result (on the camera's queue: hop to the main actor to show it).
    var seen: (@Sendable (EyesSeen) -> Void)?

    /// False when there is no camera to use (the Simulator, a refusal).
    func start() async -> Bool {
        guard await AVCaptureDevice.requestAccess(for: .video) else { return false }
        return await withCheckedContinuation { continuation in
            queue.async {
                if !self.configured { self.configured = self.configure() }
                if self.configured, !self.session.isRunning { self.session.startRunning() }
                continuation.resume(returning: self.configured)
            }
        }
    }

    func stop() {
        queue.async {
            if self.session.isRunning { self.session.stopRunning() }
        }
    }

    private func configure() -> Bool {
        guard let camera = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
              let input = try? AVCaptureDeviceInput(device: camera), session.canAddInput(input) else { return false }
        session.beginConfiguration()
        session.sessionPreset = .hd1920x1080
        session.addInput(input)
        output.alwaysDiscardsLateVideoFrames = true
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange]
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddOutput(output) else {
            session.commitConfiguration()
            return false
        }
        session.addOutput(output)
        // Upright frames: what the models see is what the screen shows.
        if let connection = output.connection(with: .video), connection.isVideoRotationAngleSupported(90) {
            connection.videoRotationAngle = 90
        }
        session.commitConfiguration()
        device = camera
        return true
    }

    func torch(_ on: Bool) {
        queue.async {
            guard let device = self.device, device.hasTorch, (try? device.lockForConfiguration()) != nil else { return }
            device.torchMode = on ? .on : .off
            device.unlockForConfiguration()
        }
    }

    /// The frame in view now, as a JPEG.
    func snapshot() async -> Data? {
        await withCheckedContinuation { continuation in
            queue.async {
                guard let pixels = self.latest else {
                    continuation.resume(returning: nil)
                    return
                }
                let image = CIImage(cvPixelBuffer: pixels)
                guard let rendered = self.context.createCGImage(image, from: image.extent) else {
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: UIImage(cgImage: rendered).jpegData(compressionQuality: 0.8))
            }
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let pixels = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        latest = pixels
        // About three looks a second: the models take the rest of the time.
        guard Date.now.timeIntervalSince(lastLook) > 0.3 else { return }
        lastLook = .now
        let result = Self.look(pixels)
        seen?(result)
    }

    /// One frame through the models: text, codes, the scene, faces, animals.
    static func look(_ pixels: CVPixelBuffer) -> EyesSeen {
        let started = Date.now
        var out = EyesSeen()
        out.size = CGSize(width: CVPixelBufferGetWidth(pixels), height: CVPixelBufferGetHeight(pixels))
        let text = VNRecognizeTextRequest()
        text.recognitionLevel = .fast
        text.usesLanguageCorrection = false
        text.recognitionLanguages = ["pt-BR", "en-US"]
        let codes = VNDetectBarcodesRequest()
        let scene = VNClassifyImageRequest()
        let faces = VNDetectFaceRectanglesRequest()
        let animals = VNRecognizeAnimalsRequest()
        let handler = VNImageRequestHandler(cvPixelBuffer: pixels, orientation: .up, options: [:])
        try? handler.perform([text, codes, scene, faces, animals])
        for observation in text.results ?? [] {
            guard let best = observation.topCandidates(1).first, best.confidence > 0.3 else { continue }
            out.lines.append(best.string)
            out.boxes.append(EyesBox(rect: flipped(observation.boundingBox), strong: EyesReader.worthALook(best.string)))
        }
        for observation in codes.results ?? [] {
            if let payload = observation.payloadStringValue, !payload.isEmpty { out.codes.append(payload) }
            out.boxes.append(EyesBox(rect: flipped(observation.boundingBox), strong: true))
        }
        out.labels = (scene.results ?? []).filter { $0.confidence > 0.35 }.prefix(3).map { EyesReader.label($0.identifier) }
        out.faces = faces.results?.count ?? 0
        out.animals = (animals.results ?? []).compactMap { $0.labels.first?.identifier }.map { $0 == "Cat" ? "gato" : $0 == "Dog" ? "cachorro" : $0.lowercased() }
        out.tookMs = Int(Date.now.timeIntervalSince(started) * 1000)
        return out
    }

    /// Vision's boxes have their origin at the bottom left: the screen's is at the top.
    private static func flipped(_ box: CGRect) -> CGRect {
        CGRect(x: box.minX, y: 1 - box.maxY, width: box.width, height: box.height)
    }
}

/// The camera's picture, filling its view.
struct EyesPreview: UIViewRepresentable {
    let session: AVCaptureSession

    final class Surface: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var preview: AVCaptureVideoPreviewLayer? { layer as? AVCaptureVideoPreviewLayer }
    }

    func makeUIView(context: Context) -> Surface {
        let view = Surface()
        view.backgroundColor = .black
        view.preview?.session = session
        view.preview?.videoGravity = .resizeAspectFill
        return view
    }

    func updateUIView(_ view: Surface, context: Context) {}
}

// MARK: - The screen

@MainActor
@Observable
final class AXYONEyes {
    var seen = EyesSeen()
    var findings: [EyesFinding] = []
    var running = false
    var available = true
    var torch = false
    /// The answer being written to a question about the frame (the chat's entry).
    var asking: String?
    var said: String?

    @ObservationIgnored let camera = EyesCamera()

    func start() async {
        camera.seen = { result in
            Task { @MainActor in AXYONEyes.shared.take(result) }
        }
        available = await camera.start()
        running = available
        #if targetEnvironment(simulator)
        if !available {
            // CI's screenshots: what a boleto in view would give.
            seen = EyesSeen(lines: ["GARE11 · Relatório gerencial", "Vencimento 15/12/2026", "Valor R$ 1.284,90"], codes: [], labels: ["documento", "texto"],
                            faces: 0, animals: [], boxes: [], size: CGSize(width: 1080, height: 1920), tookMs: 41)
            findings = EyesReader.findings(lines: seen.lines, codes: [])
        }
        #endif
    }

    func stop() {
        camera.stop()
        if torch { camera.torch(false) }
        torch = false
        running = false
    }

    static let shared = AXYONEyes()

    private func take(_ result: EyesSeen) {
        seen = result
        let found = EyesReader.findings(lines: result.lines, codes: result.codes)
        // What was found stays a moment after it leaves the view: it can be tapped.
        if !found.isEmpty || result.lines.isEmpty { findings = found }
    }

    /// In a sentence: what the models see now.
    var summary: String {
        var parts = seen.labels
        if seen.faces == 1 { parts.append(String(localized: "1 face")) }
        if seen.faces > 1 { parts.append(String(localized: "\(seen.faces) faces")) }
        parts.append(contentsOf: seen.animals)
        if !seen.lines.isEmpty { parts.append(String(localized: "\(seen.lines.count) lines of text")) }
        if !seen.codes.isEmpty { parts.append(String(localized: "\(seen.codes.count) codes")) }
        return parts.isEmpty ? String(localized: "Looking…") : parts.joined(separator: " · ")
    }

    /// A question about what is in view: the frame and the text read go to AXYON Core, its answer comes back here.
    func ask(_ question: String) async {
        let engine = HermesEngine.shared
        guard engine.canChat, engine.answering == nil else {
            said = String(localized: "AXYON Core is busy or not running yet: try again in a moment.")
            return
        }
        guard let jpeg = await camera.snapshot() else {
            said = String(localized: "There is no picture from the camera yet.")
            return
        }
        do {
            let photo = try await AXYONInbox.addPhoto(jpeg, name: "olhos.jpg")
            var prompt = question
            if !seen.lines.isEmpty {
                prompt += "\n\n" + String(localized: "(Text the iPhone read in the picture: \(seen.lines.prefix(40).joined(separator: " | ")))")
            }
            engine.send(prompt, attachments: [photo])
            asking = engine.answering
            said = nil
            Haptics.impact()
        } catch {
            said = HermesEngine.describe(error)
        }
    }
}

struct AXYONEyesView: View {
    @Environment(HermesEngine.self) private var engine
    private let eyes = AXYONEyes.shared
    @State private var question = ""
    @State private var folder: String?
    @State private var toast: String?
    @FocusState private var typing: Bool

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if eyes.available {
                EyesPreview(session: eyes.camera.session).ignoresSafeArea()
                overlay.ignoresSafeArea()
            } else {
                noCamera
            }
            VStack(spacing: 10) {
                seeing
                Spacer()
                if let answer { answerCard(answer) }
                if let toast {
                    Text(toast)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.black)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 8)
                        .background(Capsule().fill(Color.white))
                        .transition(.scale.combined(with: .opacity))
                }
                actions
                asker
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 10)
        }
        .navigationTitle("Eyes")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    eyes.torch.toggle()
                    eyes.camera.torch(eyes.torch)
                } label: {
                    Image(systemName: eyes.torch ? "flashlight.on.fill" : "flashlight.off.fill")
                }
                .disabled(!eyes.available)
            }
        }
        .toolbarBackground(.hidden, for: .navigationBar)
        .sheet(item: Binding(get: { folder.map { EyesFolder(id: $0) } }, set: { folder = $0?.id })) { item in
            NavigationStack { InvestFolderView(ticker: item.id) }
        }
        .task { await eyes.start() }
        .onDisappear { eyes.stop() }
    }

    private struct EyesFolder: Identifiable {
        let id: String
    }

    /// The answer AXYON Core is writing (or wrote) to the question about the frame.
    private var answer: String? {
        if let said = eyes.said { return said }
        guard let id = eyes.asking, let entry = engine.conversation.entries.first(where: { $0.id == id }) else { return nil }
        return entry.text.isEmpty ? String(localized: "AXYON is looking…") : entry.text
    }

    /// Boxes over what the models found, in the picture's own place.
    private var overlay: some View {
        GeometryReader { geometry in
            let frame = eyes.seen.size
            let scale = max(geometry.size.width / max(frame.width, 1), geometry.size.height / max(frame.height, 1))
            let offset = CGSize(width: (geometry.size.width - frame.width * scale) / 2, height: (geometry.size.height - frame.height * scale) / 2)
            let boxes = eyes.seen.boxes
            let accent = AXYONTheme.current.accent
            Canvas { canvas, _ in
                for box in boxes {
                    let rect = CGRect(x: offset.width + box.rect.minX * frame.width * scale, y: offset.height + box.rect.minY * frame.height * scale,
                                      width: box.rect.width * frame.width * scale, height: box.rect.height * frame.height * scale).insetBy(dx: -3, dy: -2)
                    let shape = Path(roundedRect: rect, cornerRadius: 5)
                    if box.strong {
                        canvas.fill(shape, with: .color(accent.opacity(0.28)))
                        canvas.stroke(shape, with: .color(.white), lineWidth: 1.6)
                    } else {
                        canvas.stroke(shape, with: .color(.white.opacity(0.35)), lineWidth: 0.8)
                    }
                }
            }
            .animation(.linear(duration: 0.15), value: eyes.seen.boxes)
        }
        .allowsHitTesting(false)
    }

    private var seeing: some View {
        HStack(spacing: 8) {
            Circle().fill(eyes.running ? AXYONPalette.up : AXYONPalette.dim).frame(width: 8, height: 8)
            Text(eyes.summary).font(.footnote.weight(.semibold)).foregroundStyle(.white).lineLimit(2)
            Spacer(minLength: 4)
            if eyes.seen.tookMs > 0 {
                Text(verbatim: "\(eyes.seen.tookMs) ms").font(.caption2).monospacedDigit().foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .coucouGlass(.regular, in: Capsule())
        .padding(.top, 6)
    }

    private var noCamera: some View {
        VStack(spacing: 10) {
            Image(systemName: "eye.slash.fill").font(.largeTitle).foregroundStyle(AXYONPalette.dim)
            Text("AXYON has no camera to look through here. On the iPhone, allow the camera for AXYON in Settings.")
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
        }
    }

    private func answerCard(_ text: String) -> some View {
        ScrollView {
            Text(text)
                .font(.callout)
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
        }
        .frame(maxHeight: 220)
        .padding(14)
        .coucouGlass(.regular, cornerRadius: 22)
        .overlay(alignment: .topTrailing) {
            Button {
                eyes.asking = nil
                eyes.said = nil
            } label: {
                Image(systemName: "xmark.circle.fill").font(.title3).foregroundStyle(.white.opacity(0.7))
            }
            .padding(8)
        }
    }

    private var actions: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(eyes.findings) { finding in
                    Button { act(finding) } label: {
                        Label(finding.title, systemImage: finding.icon)
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 9)
                            .coucouGlass(.regular, in: Capsule(), tint: AXYONTheme.current.accent)
                    }
                    .buttonStyle(PressableButtonStyle())
                }
                if !eyes.seen.lines.isEmpty {
                    Button {
                        UIPasteboard.general.string = eyes.seen.lines.joined(separator: "\n")
                        show(String(localized: "Text copied"))
                    } label: {
                        Label("Copy the text", systemImage: "text.viewfinder")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 9)
                            .coucouGlass(.regular, in: Capsule())
                    }
                    .buttonStyle(PressableButtonStyle())
                }
            }
        }
        .animation(.spring(duration: 0.3), value: eyes.findings)
    }

    private var asker: some View {
        VStack(spacing: 8) {
            if !typing {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach([String(localized: "What is this?"), String(localized: "Read and sum it up"),
                                 String(localized: "Is anything wrong here?"), String(localized: "Translate it")], id: \.self) { prompt in
                            Button {
                                Task { await eyes.ask(prompt) }
                            } label: {
                                Text(prompt)
                                    .font(.footnote.weight(.semibold))
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 8)
                                    .background(Capsule().fill(Color.black.opacity(0.45)))
                                    .overlay(Capsule().strokeBorder(Color.white.opacity(0.25), lineWidth: 0.8))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
            HStack(spacing: 10) {
                TextField("Ask about what AXYON sees", text: $question)
                    .focused($typing)
                    .submitLabel(.send)
                    .onSubmit { send() }
                Button { send() } label: {
                    Image(systemName: "arrow.up.circle.fill").font(.title2).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
                .disabled(question.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .coucouGlass(.regular, in: Capsule())
        }
    }

    private func send() {
        let asked = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty else { return }
        question = ""
        typing = false
        Task { await eyes.ask(asked) }
    }

    private func show(_ text: String) {
        Haptics.success()
        withAnimation(.spring(duration: 0.3)) { toast = text }
        Task {
            try? await Task.sleep(for: .seconds(2))
            withAnimation { toast = nil }
        }
    }

    private func act(_ finding: EyesFinding) {
        switch finding.kind {
        case .ticker:
            folder = finding.value
        case .link:
            if let url = URL(string: finding.value) { UIApplication.shared.open(url) }
        case .date:
            guard let due = EyesReader.day(finding.value) else { return }
            Task {
                let request: [String: Any] = ["action": "reminders", "do": "add", "title": String(localized: "Seen by AXYON: \(finding.value)"),
                                              "due": ISO8601DateFormatter().string(from: due)]
                let answer = HermesArms.object(await HermesArms.answer("iphone", HermesArms.data(request)))
                show(answer["error"] as? String ?? String(localized: "Reminder created for \(finding.value)"))
            }
        default:
            UIPasteboard.general.string = finding.value
            show(String(localized: "Copied"))
        }
    }
}
