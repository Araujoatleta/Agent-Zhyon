import SwiftUI
import UIKit

// AXYON's core: what runs on this iPhone's own processor, in one place. Its eyes (the camera through on-device vision
// models), its ears (hours of audio into text), its memory (everything read, searched by meaning), the market's radar
// and the portfolio's futures (the Rust engine on every core), the home scanned in 3D with the LiDAR, and the local
// brain (AXYONLocalBrain.swift). And the base mode: on the charger, AXYON stays awake and is the home's computer.

/// What this iPhone is, read from the system.
@MainActor
enum AXYONDevice {
    /// "iPhone15,3".
    static var identifier: String {
        var info = utsname()
        uname(&info)
        return withUnsafeBytes(of: &info.machine) { raw in
            String(decoding: raw.prefix { $0 != 0 }, as: UTF8.self)
        }
    }

    /// The model and its chip, for those known; else the system's own name.
    static var model: (name: String, chip: String) {
        switch identifier {
        case "iPhone14,2": ("iPhone 13 Pro", "A15 Bionic")
        case "iPhone14,3": ("iPhone 13 Pro Max", "A15 Bionic")
        case "iPhone14,7": ("iPhone 14", "A15 Bionic")
        case "iPhone14,8": ("iPhone 14 Plus", "A15 Bionic")
        case "iPhone15,2": ("iPhone 14 Pro", "A16 Bionic")
        case "iPhone15,3": ("iPhone 14 Pro Max", "A16 Bionic")
        case "iPhone15,4": ("iPhone 15", "A16 Bionic")
        case "iPhone15,5": ("iPhone 15 Plus", "A16 Bionic")
        case "iPhone16,1": ("iPhone 15 Pro", "A17 Pro")
        case "iPhone16,2": ("iPhone 15 Pro Max", "A17 Pro")
        case "iPhone17,1": ("iPhone 16 Pro", "A18 Pro")
        case "iPhone17,2": ("iPhone 16 Pro Max", "A18 Pro")
        case "iPhone17,3": ("iPhone 16", "A18")
        case "iPhone17,4": ("iPhone 16 Plus", "A18")
        case "arm64", "x86_64": ("Simulator", "Mac")
        default: (UIDevice.current.model, identifier)
        }
    }

    static var cores: Int { ProcessInfo.processInfo.activeProcessorCount }

    /// The memory the iPhone has, in GB (rounded up to what is sold).
    static var memory: Int { Int((Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824).rounded(.up)) }

    static var heat: String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: String(localized: "cool")
        case .fair: String(localized: "warm")
        case .serious: String(localized: "hot: iOS slows the processor")
        default: String(localized: "very hot: let it rest")
        }
    }
}

/// The base mode: on the charger with AXYON open, the screen stays on (dimmed, black) and AXYON keeps working, so
/// time routines run to the minute and the heavy jobs (the intern's reading, the memory, the radar) are done there.
@MainActor
@Observable
final class AXYONBase {
    static let shared = AXYONBase()

    var enabled: Bool = UserDefaults.standard.bool(forKey: "axyon.base") {
        didSet {
            UserDefaults.standard.set(enabled, forKey: "axyon.base")
            apply()
        }
    }
    private(set) var charging = false
    private(set) var lastWork: Date?
    @ObservationIgnored private var worker: Task<Void, Never>?

    private init() {
        UIDevice.current.isBatteryMonitoringEnabled = true
        charging = UIDevice.current.batteryState == .charging || UIDevice.current.batteryState == .full
        NotificationCenter.default.addObserver(forName: UIDevice.batteryStateDidChangeNotification, object: nil, queue: .main) { _ in
            Task { @MainActor in AXYONBase.shared.apply() }
        }
    }

    /// The base is on now: enabled and on the charger.
    var active: Bool { enabled && charging }

    func apply() {
        charging = UIDevice.current.batteryState == .charging || UIDevice.current.batteryState == .full
        UIApplication.shared.isIdleTimerDisabled = active
        if active, worker == nil {
            worker = Task { await self.work() }
        } else if !active {
            worker?.cancel()
            worker = nil
        }
    }

    /// While the base is on: every half hour the intern reads what is new and the memory takes it in; once a day the
    /// whole market is swept.
    private func work() async {
        while !Task.isCancelled, active {
            await AXYONInvest.shared.read()
            await AXYONInvest.shared.think()
            await AXYONMemory.shared.sync()
            let today = AXYONWallet.iso(.now)
            if UserDefaults.standard.string(forKey: "axyon.base.radar") != today {
                await AXYONRadar.shared.scan()
                if AXYONRadar.shared.problem == nil { UserDefaults.standard.set(today, forKey: "axyon.base.radar") }
            }
            lastWork = .now
            try? await Task.sleep(for: .seconds(30 * 60))
        }
    }
}

struct AXYONNucleusView: View {
    @Environment(HermesEngine.self) private var engine
    private let base = AXYONBase.shared
    private let memory = AXYONMemory.shared
    private let ears = AXYONEars.shared
    private let radar = AXYONRadar.shared
    private let rooms = AXYONRooms.shared
    private let brain = AXYONLocalBrain.shared
    @State private var docked = false

    private let columns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                chip
                LazyVGrid(columns: columns, spacing: 12) {
                    power(String(localized: "Eyes"), String(localized: "The camera through vision models, live"), "eye.fill") { AXYONEyesView() }
                    power(String(localized: "Ears"), ears.transcripts.isEmpty ? String(localized: "Hours of audio into text")
                                                                              : String(localized: "\(ears.transcripts.count) transcripts"), "waveform") { AXYONEarsView() }
                    power(String(localized: "Memory"), memory.stats.passages > 0 ? String(localized: "\(memory.stats.passages) passages, by meaning")
                                                                                 : String(localized: "Everything read, searched by meaning"), "brain") { AXYONMemoryView() }
                    power(String(localized: "Local brain"), brain.status, "cpu.fill") { AXYONLocalBrainView() }
                    power(String(localized: "Market radar"), radar.ranked > 0 ? String(localized: "\(radar.ranked) funds ranked")
                                                                              : String(localized: "Every fund on B3, ranked here"), "dot.radiowaves.left.and.right") { AXYONRadarView() }
                    power(String(localized: "Futures"), String(localized: "100,000 futures of your portfolio"), "chart.line.uptrend.xyaxis") { AXYONFuturesView() }
                    power(String(localized: "Home in 3D"), rooms.rooms.isEmpty ? String(localized: "Scan a room with the LiDAR")
                                                                               : String(localized: "\(rooms.rooms.count) rooms scanned"), "cube.transparent.fill") { AXYONRoomsView() }
                }
                dock
                Text("All of this runs on this iPhone's own processor: nothing here is sent to a server to be computed. Heavy jobs need AXYON open; on the charger, the base mode keeps it awake.")
                    .font(.caption2)
                    .foregroundStyle(AXYONPalette.faint)
            }
            .padding(20)
        }
        #if targetEnvironment(simulator)
        .defaultScrollAnchor(AXYONTestScreen.anchor)
        #endif
        .axyonBackdrop(intensity: 0.9)
        .navigationTitle("Core")
        .navigationBarTitleDisplayMode(.inline)
        .fullScreenCover(isPresented: $docked) { AXYONBaseView() }
        .task {
            await memory.refresh()
            await radar.load()
        }
    }

    private var chip: some View {
        let model = AXYONDevice.model
        let lidar = AXYONRooms.canScan ? String(localized: "yes") : String(localized: "no")
        return HStack(spacing: 16) {
            AXYONChipGlyph(cores: AXYONDevice.cores)
                .frame(width: 92, height: 92)
            VStack(alignment: .leading, spacing: 4) {
                Text(verbatim: model.name).font(.title3.weight(.bold)).foregroundStyle(.white)
                Text(verbatim: "\(model.chip) · \(AXYONDevice.cores) núcleos · \(AXYONDevice.memory) GB")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(AXYONTheme.current.accent)
                Text(String(localized: "LiDAR: \(lidar) · temperature: \(AXYONDevice.heat)"))
                    .font(.caption)
                    .foregroundStyle(AXYONPalette.dim)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .axyonCard(cornerRadius: 28)
    }

    private func power<Destination: View>(_ title: String, _ note: String, _ icon: String,
                                          @ViewBuilder _ destination: @escaping () -> Destination) -> some View {
        NavigationLink {
            destination()
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: icon)
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(.white)
                    .frame(width: 46, height: 46)
                    .background(Circle().fill(AXYONTheme.current.accentGradient))
                    .axyonGlow(radius: 10, strength: 0.5)
                Spacer(minLength: 0)
                Text(title).font(.headline).foregroundStyle(.white)
                Text(note).font(.caption).foregroundStyle(AXYONPalette.dim).lineLimit(2).multilineTextAlignment(.leading)
            }
            .padding(14)
            .frame(maxWidth: .infinity, minHeight: 138, alignment: .topLeading)
            .axyonCard(cornerRadius: 24)
        }
        .buttonStyle(PressableButtonStyle())
    }

    private var dock: some View {
        VStack(alignment: .leading, spacing: 10) {
            Toggle(isOn: Binding(get: { base.enabled }, set: { base.enabled = $0 })) {
                Label("Base mode on the charger", systemImage: "powerplug.fill")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
            }
            Text(base.active ? String(localized: "On: AXYON stays awake while it charges. Routines run to the minute; the intern, the memory and the radar work by themselves.")
                             : String(localized: "With AXYON open on the charger, the screen stays on and AXYON is the home's computer: routines to the minute, the intern reading, the memory indexing, the market swept once a day."))
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
                .fixedSize(horizontal: false, vertical: true)
            Button { docked = true } label: {
                Label("Open the base's screen", systemImage: "moon.stars.fill").font(.footnote.weight(.semibold))
            }
            .coucouGlassButton()
        }
        .padding(16)
        .axyonCard(cornerRadius: 22)
    }
}

/// A processor drawn in code: its cores light up one after the other.
struct AXYONChipGlyph: View {
    let cores: Int

    var body: some View {
        let accent = AXYONTheme.current.accent
        TimelineView(.animation(minimumInterval: 1.0 / 20.0)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            Canvas { canvas, area in
                let side = min(area.width, area.height)
                let body = CGRect(x: (area.width - side * 0.7) / 2, y: (area.height - side * 0.7) / 2, width: side * 0.7, height: side * 0.7)
                // The pins.
                for pin in 0..<6 {
                    let along = body.minX + body.width * (CGFloat(pin) + 0.5) / 6
                    for (from, to) in [(body.minY - side * 0.12, body.minY), (body.maxY, body.maxY + side * 0.12)] {
                        var line = Path()
                        line.move(to: CGPoint(x: along, y: from))
                        line.addLine(to: CGPoint(x: along, y: to))
                        canvas.stroke(line, with: .color(accent.opacity(0.55)), lineWidth: 2)
                    }
                    let across = body.minY + body.height * (CGFloat(pin) + 0.5) / 6
                    for (from, to) in [(body.minX - side * 0.12, body.minX), (body.maxX, body.maxX + side * 0.12)] {
                        var line = Path()
                        line.move(to: CGPoint(x: from, y: across))
                        line.addLine(to: CGPoint(x: to, y: across))
                        canvas.stroke(line, with: .color(accent.opacity(0.55)), lineWidth: 2)
                    }
                }
                let shape = Path(roundedRect: body, cornerRadius: side * 0.1)
                canvas.fill(shape, with: .color(Color.black.opacity(0.55)))
                canvas.stroke(shape, with: .color(accent), lineWidth: 1.6)
                // The cores, in rows of three.
                let count = max(1, min(cores, 9))
                let cell = body.width / 3.6
                for core in 0..<count {
                    let column = CGFloat(core % 3), row = CGFloat(core / 3)
                    let rows = CGFloat((count + 2) / 3)
                    let origin = CGPoint(x: body.midX + (column - 1) * cell * 1.1 - cell / 2,
                                         y: body.midY + (row - (rows - 1) / 2) * cell * 1.1 - cell / 2)
                    let pulse = 0.25 + 0.75 * max(0, sin(time * 2.4 - Double(core) * 0.9))
                    let square = Path(roundedRect: CGRect(origin: origin, size: CGSize(width: cell, height: cell)), cornerRadius: cell * 0.22)
                    canvas.fill(square, with: .color(accent.opacity(0.2 + 0.8 * pulse)))
                }
            }
        }
        .shadow(color: accent.opacity(0.5), radius: 12)
        .accessibilityHidden(true)
    }
}

/// The base's screen: black, a clock that drifts so nothing burns in, and what AXYON is doing. A tap leaves it.
struct AXYONBaseView: View {
    @Environment(\.dismiss) private var dismiss
    private let base = AXYONBase.shared
    private let routines = AXYONRoutineStore.shared

    var body: some View {
        TimelineView(.periodic(from: .now, by: 20)) { context in
            let minutes = context.date.timeIntervalSinceReferenceDate / 60
            ZStack {
                Color.black.ignoresSafeArea()
                VStack(spacing: 10) {
                    Text(context.date.formatted(.dateTime.hour().minute().locale(Locale(identifier: "pt_BR"))))
                        .font(.system(size: 86, weight: .thin, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(Color.white.opacity(0.8))
                    Text(context.date.formatted(.dateTime.weekday(.wide).day().month(.wide).locale(Locale(identifier: "pt_BR"))))
                        .font(.title3)
                        .foregroundStyle(Color.white.opacity(0.45))
                    Label(base.active ? String(localized: "Base on: AXYON is awake") : String(localized: "Put the iPhone on the charger and turn the base mode on"),
                          systemImage: base.active ? "bolt.fill" : "powerplug")
                        .font(.footnote)
                        .foregroundStyle(base.active ? AXYONTheme.current.accent.opacity(0.9) : Color.white.opacity(0.4))
                        .padding(.top, 14)
                    if let next = AXYONRoutineStore.nextTimes().min() {
                        Text(String(localized: "Next routine at \(next.formatted(.dateTime.hour().minute().locale(Locale(identifier: "pt_BR"))))"))
                            .font(.footnote)
                            .foregroundStyle(Color.white.opacity(0.4))
                    }
                    if let worked = base.lastWork {
                        Text(String(localized: "Read and indexed at \(worked.formatted(.dateTime.hour().minute().locale(Locale(identifier: "pt_BR"))))"))
                            .font(.footnote)
                            .foregroundStyle(Color.white.opacity(0.4))
                    }
                }
                // A slow drift: the same pixels are never lit for long.
                .offset(x: CGFloat(sin(minutes / 7) * 26), y: CGFloat(cos(minutes / 11) * 60))
                .animation(.easeInOut(duration: 8), value: Int(minutes))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { dismiss() }
        .statusBarHidden()
        .persistentSystemOverlays(.hidden)
        .task { base.apply() }
    }
}
