import PDFKit
import SafariServices
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import Vision

// What the chat with Hermes adds on this iPhone: the photos and files the user sends (Hermes/app/axyon_hermes/
// attachments.py), the images Hermes makes, an e-mail waiting for the user's tap on Send, Google's sign-in inside the
// app (google.py), and Hermes' scheduled tasks when iOS wakes AXYON (scheduled.py).

/// The files the user sends Hermes: copied into Hermes' inbox (Application Support/Hermes/inbox), a photo as a JPEG at
/// most 2048 pixels long, and a PDF's text read here, with PDFKit, or with the iPhone's own text recognition for the
/// pages that are scans. Nothing leaves the iPhone but what Hermes then sends its brain.
enum AXYONInbox {
    static var folder: URL { HermesEngine.home.appendingPathComponent("inbox", isDirectory: true) }

    static func url(_ file: String) -> URL { folder.appendingPathComponent(file) }

    static let photoExtensions: Set<String> = ["jpg", "jpeg", "png", "heic", "heif", "gif", "webp", "tif", "tiff", "bmp"]

    /// A new name in the inbox for a file called `name`.
    private static func place(_ name: String) throws -> String {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let clean = String(name.map { "/:\\".contains($0) ? "_" : $0 }.prefix(80))
        return UUID().uuidString.prefix(8).lowercased() + "-" + (clean.isEmpty ? "file" : clean)
    }

    /// A photo (the library's or the camera's), as a JPEG.
    static func addPhoto(_ data: Data, name: String) async throws -> HermesAttachment {
        try await Task.detached(priority: .userInitiated) { try storePhoto(data, name: name) }.value
    }

    /// A file picked in Files: copied in (a photo becomes a JPEG), and a PDF's text read now.
    static func addFile(_ picked: URL) async throws -> HermesAttachment {
        try await Task.detached(priority: .userInitiated) {
            let name = picked.lastPathComponent
            let data = try read(picked)
            if photoExtensions.contains(picked.pathExtension.lowercased()) {
                return try storePhoto(data, name: name)
            }
            let file = try place(name)
            try data.write(to: url(file), options: .completeFileProtectionUntilFirstUserAuthentication)
            var attachment = HermesAttachment(file: file, name: name, kind: .file)
            if picked.pathExtension.lowercased() == "pdf" {
                let textFile = file + ".txt.extracted"
                try readPDF(url(file)).write(to: url(textFile), atomically: true, encoding: .utf8)
                attachment.textFile = textFile
            }
            return attachment
        }.value
    }

    private static func storePhoto(_ data: Data, name: String) throws -> HermesAttachment {
        guard let jpeg = jpeg(data) else { throw CocoaError(.fileReadCorruptFile) }
        let base = (name as NSString).deletingPathExtension
        let shown = (base.isEmpty ? "photo" : base) + ".jpg"
        let file = try place(shown)
        try jpeg.write(to: url(file), options: .completeFileProtectionUntilFirstUserAuthentication)
        return HermesAttachment(file: file, name: shown, kind: .image)
    }

    /// A file of another app (Files, iCloud Drive), read once it is there.
    private static func read(_ picked: URL) throws -> Data {
        let scoped = picked.startAccessingSecurityScopedResource()
        defer { if scoped { picked.stopAccessingSecurityScopedResource() } }
        // Written by the coordinator's reader, which runs before `coordinate` returns.
        nonisolated(unsafe) var data: Data?
        nonisolated(unsafe) var failure: Error?
        var coordination: NSError?
        NSFileCoordinator().coordinate(readingItemAt: picked, options: .withoutChanges, error: &coordination) { real in
            do { data = try Data(contentsOf: real) } catch { failure = error }
        }
        if let error = coordination ?? failure { throw error }
        guard let data else { throw CocoaError(.fileReadUnknown) }
        return data
    }

    private static func jpeg(_ data: Data) -> Data? {
        guard let image = UIImage(data: data) else { return nil }
        let longest = max(image.size.width, image.size.height, 1)
        let scale = min(1, 2048 / longest)
        let size = CGSize(width: (image.size.width * scale).rounded(), height: (image.size.height * scale).rounded())
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        let drawn = UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return drawn.jpegData(compressionQuality: 0.82)
    }

    /// A PDF's text, page by page (Hermes asks for one it found in Drive, too).
    static func pdfText(_ file: URL) async -> String {
        await Task.detached(priority: .userInitiated) { readPDF(file) }.value
    }

    /// The text of each page; a page with almost none is a scan, read by the iPhone's text recognition (the first
    /// 30 such pages).
    private static func readPDF(_ file: URL) -> String {
        guard let document = PDFDocument(url: file) else { return "" }
        var pages: [String] = []
        var scans = 0
        for index in 0..<min(document.pageCount, 300) {
            guard let page = document.page(at: index) else { continue }
            var text = page.string ?? ""
            if text.trimmingCharacters(in: .whitespacesAndNewlines).count < 20, scans < 30 {
                scans += 1
                text = recognize(page)
            }
            pages.append("[Page \(index + 1)]\n" + text.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return pages.joined(separator: "\n\n")
    }

    private static func recognize(_ page: PDFPage) -> String {
        let box = page.bounds(for: .mediaBox)
        let scale = 2200 / max(box.width, box.height, 1)
        let image = page.thumbnail(of: CGSize(width: box.width * scale, height: box.height * scale), for: .mediaBox)
        guard let picture = image.cgImage else { return "" }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        request.recognitionLanguages = ["pt-BR", "en-US", "es-ES", "fr-FR"]
        try? VNImageRequestHandler(cgImage: picture).perform([request])
        return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
    }

    /// Whether `path` is a file of the inbox (the bridge reads nothing else).
    static func holds(_ path: String) -> Bool {
        let inside = folder.resolvingSymlinksInPath().path + "/"
        return URL(fileURLWithPath: path).resolvingSymlinksInPath().path.hasPrefix(inside)
    }

    /// Files sent more than a month ago leave the inbox (Hermes read them then).
    static func prune(olderThan days: Double = 30) {
        let limit = Date.now.addingTimeInterval(-days * 86_400)
        let names = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        for file in names {
            let changed = (try? file.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
            if let changed, changed < limit { try? FileManager.default.removeItem(at: file) }
        }
    }
}

// MARK: - The camera and Safari inside the app

/// The camera, for a photo to send.
struct AXYONCamera: UIViewControllerRepresentable {
    let done: @MainActor (UIImage?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    @MainActor
    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: @MainActor (UIImage?) -> Void

        init(done: @escaping @MainActor (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController,
                                   didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { done(nil) }
    }
}

/// A page inside the app (Google's sign-in): AXYON stays on screen, so Hermes hears Google coming back.
struct AXYONSafari: UIViewControllerRepresentable {
    let url: URL

    func makeUIViewController(context: Context) -> SFSafariViewController {
        let page = SFSafariViewController(url: url)
        page.dismissButtonStyle = .close
        page.preferredControlTintColor = .white
        page.preferredBarTintColor = .black
        return page
    }

    func updateUIViewController(_ page: SFSafariViewController, context: Context) {}
}

/// A link to open inside the app.
struct AXYONPage: Identifiable {
    let url: URL
    var id: String { url.absoluteString }
}

// MARK: - Files and images in the chat

/// A photo's small picture, read off the main thread.
struct AXYONThumb: View {
    let url: URL
    var side: CGFloat = 64
    @State private var image: UIImage?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12, style: .continuous).fill(.white.opacity(0.06))
            if let image {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                Image(systemName: "photo").foregroundStyle(.secondary)
            }
        }
        .frame(width: side, height: side)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .task(id: url) {
            image = await UIImage(contentsOfFile: url.path)?.byPreparingThumbnail(ofSize: CGSize(width: side * 3, height: side * 3))
        }
    }
}

/// The files of a message (the composer's can be taken away before sending).
struct AXYONAttachmentChips: View {
    let files: [HermesAttachment]
    var remove: ((HermesAttachment) -> Void)?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(files) { file in
                    ZStack(alignment: .topTrailing) {
                        switch file.kind {
                        case .image:
                            AXYONThumb(url: AXYONInbox.url(file.file))
                        case .file:
                            VStack(spacing: 4) {
                                Image(systemName: icon(file.name)).font(.title3).foregroundStyle(AXYONPalette.cyan)
                                Text(file.name).font(.caption2).lineLimit(2).multilineTextAlignment(.center)
                            }
                            .padding(6)
                            .frame(width: 92, height: 64)
                            .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        }
                        if let remove {
                            Button {
                                remove(file)
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .symbolRenderingMode(.palette)
                                    .foregroundStyle(.white, .black.opacity(0.7))
                            }
                            .offset(x: 6, y: -6)
                            .accessibilityLabel("Remove \(file.name)")
                        }
                    }
                }
            }
            .padding(.vertical, 6)
            .padding(.trailing, 6)
        }
    }

    private func icon(_ name: String) -> String {
        switch (name as NSString).pathExtension.lowercased() {
        case "pdf": "doc.richtext"
        case "xlsx", "xls", "csv", "tsv", "numbers": "tablecells"
        case "docx", "doc", "pages", "txt", "md", "rtf": "doc.text"
        default: "doc"
        }
    }
}

/// The images Hermes made in an answer: a tap shares or saves one.
struct AXYONMadeImages: View {
    let paths: [String]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 10) {
                ForEach(paths, id: \.self) { path in
                    let file = HermesEngine.home.appendingPathComponent(path)
                    ShareLink(item: file) {
                        AXYONThumb(url: file, side: 180)
                    }
                    .accessibilityLabel("Share the image")
                }
            }
        }
    }
}

// MARK: - An e-mail waiting for the user's tap

/// An e-mail Hermes wants to send from the user's Gmail (google.py): shown whole, it goes only with a tap on Send.
struct HermesMailAsk: Identifiable, Equatable {
    let id = UUID()
    let reply: Bool
    let to: String
    let cc: String
    let subject: String
    let body: String
}

struct AXYONMailCard: View {
    let ask: HermesMailAsk
    let answer: (Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(ask.reply ? String(localized: "Send this reply?") : String(localized: "Send this e-mail?"),
                  systemImage: "envelope.badge")
                .font(.headline)
            Group {
                Text("To: \(ask.to)")
                if !ask.cc.isEmpty { Text("Cc: \(ask.cc)") }
                Text(ask.subject).fontWeight(.semibold)
            }
            .font(.subheadline)
            ScrollView {
                Text(ask.body)
                    .font(.callout)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
            }
            .frame(maxHeight: 220)
            HStack(spacing: 12) {
                Button(role: .cancel) {
                    answer(false)
                    Haptics.impact()
                } label: {
                    Text("Cancel").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                Button {
                    answer(true)
                    Haptics.success()
                } label: {
                    Label("Send", systemImage: "paperplane.fill").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(AXYONPalette.violet)
            }
            Text("Sent from your Gmail only when you tap Send.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .axyonCard(tint: AXYONPalette.violet, glow: AXYONPalette.violet)
    }
}

// MARK: - Google in Settings

struct AXYONGoogleView: View {
    @Environment(HermesEngine.self) private var engine
    @State private var picking = false
    @State private var clientID = ""
    @State private var clientSecret = ""
    @State private var pasted = ""
    @State private var page: AXYONPage?
    @State private var working = false
    @State private var problem: String?

    var body: some View {
        let google = engine.google
        List {
            if let google, google.connected {
                Section {
                    LabeledContent("Account", value: google.account.isEmpty ? String(localized: "Connected") : google.account)
                    if !google.services.isEmpty {
                        LabeledContent("Allowed", value: google.services.map(Self.service).joined(separator: ", "))
                    }
                    Button(role: .destructive) {
                        Task { await engine.googleDisconnect() }
                    } label: {
                        Label("Disconnect Google", systemImage: "xmark.circle")
                    }
                } footer: {
                    Text("Ask AXYON in the chat: “what is new in my Gmail?”, “read my sheet's AXYON tab”, “add this to my spreadsheet”, “find my 2027 plan in Drive”. An e-mail goes only when you tap Send.")
                }
                .listRowBackground(GlassRowBackground())
            } else if let google, google.client {
                Section {
                    Button {
                        connect()
                    } label: {
                        Label("Sign in with Google", systemImage: "person.badge.key.fill")
                    }
                    .disabled(working)
                } footer: {
                    Text("Google's own page opens here: sign in and allow. If Google says the app is not verified, tap Advanced, then go to your app.")
                }
                .listRowBackground(GlassRowBackground())
                Section {
                    TextField("Paste the page's address", text: $pasted, axis: .vertical)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    Button("Finish") {
                        run { try await engine.googleFinish(pasted) }
                    }
                    .disabled(pasted.trimmingCharacters(in: .whitespaces).isEmpty || working)
                } header: {
                    Text("If the page did not finish")
                } footer: {
                    Text("When the page after allowing fails to load, copy its whole address and paste it here.")
                }
                .listRowBackground(GlassRowBackground())
            } else {
                Section {
                    Button {
                        picking = true
                    } label: {
                        Label("Choose the client's JSON file", systemImage: "doc.badge.plus")
                    }
                    TextField("Client ID", text: $clientID)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    SecureField("Client secret", text: $clientSecret)
                    Button("Save") {
                        run { try await engine.googleClient(path: nil, id: clientID, secret: clientSecret) }
                    }
                    .disabled(clientID.isEmpty || clientSecret.isEmpty || working)
                } header: {
                    Text("Your Google OAuth client")
                } footer: {
                    Text("Once, in Google Cloud (about 5 minutes): create a project, enable the Gmail, Sheets, Drive, Docs and Calendar APIs, create an OAuth client of type “Desktop app”, add your account as a test user, and download its JSON. A client made for gog or for Hermes on the computer works too.")
                }
                .listRowBackground(GlassRowBackground())
                Section {
                    Link(destination: URL(string: "https://console.cloud.google.com/apis/credentials")!) {
                        Label("Open Google Cloud's credentials", systemImage: "arrow.up.right.square")
                    }
                }
                .listRowBackground(GlassRowBackground())
            }
            if let problem {
                Section {
                    Text(problem).font(.footnote).foregroundStyle(AXYONPalette.down)
                }
                .listRowBackground(GlassRowBackground())
            }
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle("Google")
        .task { await engine.refreshGoogle() }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.json]) { result in
            guard case .success(let file) = result else { return }
            run {
                let copied = try await AXYONInbox.addFile(file)
                try await engine.googleClient(path: AXYONInbox.url(copied.file).path, id: "", secret: "")
            }
        }
        .sheet(item: $page, onDismiss: { Task { await engine.refreshGoogle() } }) { page in
            AXYONSafari(url: page.url).ignoresSafeArea()
        }
    }

    private func connect() {
        run { page = AXYONPage(url: try await engine.googleConnect()) }
    }

    private func run(_ work: @escaping @MainActor () async throws -> Void) {
        working = true
        problem = nil
        Task {
            do { try await work() } catch { problem = HermesEngine.describe(error) }
            working = false
        }
    }

    private static func service(_ id: String) -> String {
        switch id {
        case "email": "Gmail"
        case "calendar": String(localized: "Calendar")
        case "drive": "Drive"
        case "sheets": String(localized: "Sheets")
        case "docs": "Docs"
        case "contacts": String(localized: "Contacts")
        default: id
        }
    }
}

// MARK: - Scheduled tasks

/// Hermes' scheduled tasks, as its cronjob tool made them (asked in the chat).
struct AXYONTasksView: View {
    @Environment(HermesEngine.self) private var engine
    @State private var tasks: [HermesTask] = []
    @State private var loaded = false

    var body: some View {
        List {
            Section {
                if tasks.isEmpty {
                    Text(loaded ? String(localized: "No scheduled task yet. Ask AXYON: “every weekday at 9am, tell me how BTLG11 is”.") : String(localized: "Reading…"))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                ForEach(tasks) { task in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(task.name).font(.headline)
                            Spacer()
                            if !task.enabled { Text("Paused").font(.caption).foregroundStyle(.secondary) }
                        }
                        Text(task.schedule).font(.subheadline).foregroundStyle(.secondary)
                        if let next = task.next, task.enabled {
                            Text("Next: \(next.formatted(date: .abbreviated, time: .shortened))")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 2)
                }
            } footer: {
                Text("Each task's answer comes as a notification. They run on time while AXYON is open; with AXYON closed, iOS wakes it a few times a day and a task that came due runs then. To change or remove one, ask AXYON in the chat.")
            }
            .listRowBackground(GlassRowBackground())
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle("Scheduled tasks")
        .task {
            tasks = await engine.cronTasks()
            loaded = true
        }
    }
}

/// Hermes' scheduled tasks when iOS wakes AXYON in the background: their file (Hermes' cron/jobs.json) says when the
/// next is due, so Python starts only when one is.
enum HermesCron {
    static var jobsFile: URL { HermesEngine.home.appendingPathComponent("cron/jobs.json") }

    /// When each task that is on runs next.
    static func nextTimes() -> [Date] {
        guard let data = try? Data(contentsOf: jobsFile),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let jobs = object["jobs"] as? [[String: Any]] else { return [] }
        return jobs.compactMap { job in
            guard job["enabled"] as? Bool ?? true, let next = job["next_run_at"] as? String else { return nil }
            return HermesTask.date(next)
        }
    }

    static var hasTasks: Bool { !nextTimes().isEmpty }

    /// A task is due now (or within `seconds`).
    static func due(within seconds: TimeInterval = 60) -> Bool {
        nextTimes().contains { $0.timeIntervalSinceNow <= seconds }
    }

    /// Hermes started if it is not running, then the due tasks run, all within `budget` seconds.
    @MainActor
    static func run(budget: TimeInterval) async {
        let engine = HermesEngine.shared
        let started = Date.now
        if engine.state == .off { engine.start() }
        while engine.state != .ready, Date.now.timeIntervalSince(started) < budget - 2 {
            if case .failed = engine.state { return }
            try? await Task.sleep(for: .milliseconds(300))
        }
        guard engine.state == .ready else { return }
        _ = await engine.cronTick(timeout: max(3, budget - Date.now.timeIntervalSince(started)))
    }
}
