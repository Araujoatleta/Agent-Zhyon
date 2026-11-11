import SceneKit
import simd
import SwiftUI
import UIKit
#if canImport(RoomPlan)
@preconcurrency import RoomPlan
#endif

// The home in 3D: a room scanned with the iPhone's LiDAR (Apple's RoomPlan: walls, doors, windows and the furniture
// it knows), kept as a model the phone draws, with the home's devices placed in it. Tapping a device in the room
// turns it on or off; one that is on glows. A television the scan finds is where the room's TV goes by itself.
// Everything stays on the iPhone: the scan is a file in the app's own folder.

struct RoomMarker: Codable, Identifiable, Sendable, Equatable {
    /// The device's id in AXYON's home.
    var id: String
    var name: String
    var kind: String
    var x: Float
    var y: Float
    var z: Float
}

struct AXYONRoom: Codable, Identifiable, Sendable, Equatable {
    var id: String
    var name: String
    var date: Date
    /// The model's file, in the rooms' folder.
    var file: String
    var markers: [RoomMarker] = []
    /// Metres.
    var width = 0.0
    var length = 0.0
    var height = 0.0
    var walls = 0
    var doors = 0
    var windows = 0
    /// What the scan knew among the furniture: "sofa": 1, "television": 1…
    var objects: [String: Int] = [:]
    /// Where the scan saw televisions, for the room's TVs.
    var televisions: [[Float]] = []

    var area: Double { width * length }

    static func objectTitle(_ category: String, _ count: Int) -> String {
        let name: String = switch category {
        case "sofa": count == 1 ? "sofá" : "sofás"
        case "table": count == 1 ? "mesa" : "mesas"
        case "chair": count == 1 ? "cadeira" : "cadeiras"
        case "bed": count == 1 ? "cama" : "camas"
        case "television": count == 1 ? "TV" : "TVs"
        case "storage": count == 1 ? "armário" : "armários"
        case "refrigerator": count == 1 ? "geladeira" : "geladeiras"
        case "stove": count == 1 ? "fogão" : "fogões"
        case "oven": count == 1 ? "forno" : "fornos"
        case "sink": count == 1 ? "pia" : "pias"
        case "toilet": count == 1 ? "vaso" : "vasos"
        case "bathtub": count == 1 ? "banheira" : "banheiras"
        case "washerDryer": count == 1 ? "lavadora" : "lavadoras"
        case "dishwasher": count == 1 ? "lava-louças" : "lava-louças"
        case "fireplace": count == 1 ? "lareira" : "lareiras"
        case "stairs": count == 1 ? "escada" : "escadas"
        default: category
        }
        return "\(count) \(name)"
    }
}

@MainActor
@Observable
final class AXYONRooms {
    static let shared = AXYONRooms()

    private(set) var rooms: [AXYONRoom] = []

    static var folder: URL {
        HermesEngine.home.deletingLastPathComponent().appendingPathComponent("AXYON/rooms", isDirectory: true)
    }

    /// The iPhone has the LiDAR RoomPlan needs.
    static var canScan: Bool {
        #if canImport(RoomPlan)
        return RoomCaptureSession.isSupported
        #else
        return false
        #endif
    }

    private init() {
        if let data = try? Data(contentsOf: Self.folder.appendingPathComponent("rooms.json")),
           let kept = try? JSONDecoder().decode([AXYONRoom].self, from: data) {
            rooms = kept
        }
    }

    private func save() {
        try? FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(rooms) {
            try? data.write(to: Self.folder.appendingPathComponent("rooms.json"), options: .atomic)
        }
    }

    func url(_ room: AXYONRoom) -> URL {
        Self.folder.appendingPathComponent(room.file)
    }

    func add(_ room: AXYONRoom) {
        rooms.insert(room, at: 0)
        save()
    }

    func rename(_ id: String, _ name: String) {
        guard let index = rooms.firstIndex(where: { $0.id == id }), !name.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        rooms[index].name = name
        save()
    }

    func remove(_ id: String) {
        guard let room = rooms.first(where: { $0.id == id }) else { return }
        try? FileManager.default.removeItem(at: url(room))
        rooms.removeAll { $0.id == id }
        save()
    }

    func place(_ device: HermesDevice, in id: String, at point: SCNVector3) {
        guard let index = rooms.firstIndex(where: { $0.id == id }) else { return }
        rooms[index].markers.removeAll { $0.id == device.id }
        rooms[index].markers.append(RoomMarker(id: device.id, name: device.name, kind: device.kind, x: point.x, y: point.y, z: point.z))
        save()
    }

    func unplace(_ device: String, in id: String) {
        guard let index = rooms.firstIndex(where: { $0.id == id }) else { return }
        rooms[index].markers.removeAll { $0.id == device }
        save()
    }

    /// The room's TVs go where the scan saw televisions, when nothing was placed there yet.
    func placeTelevisions(in id: String, devices: [HermesDevice]) {
        guard let index = rooms.firstIndex(where: { $0.id == id }) else { return }
        let free = devices.filter { device in device.kind == "tv" && !rooms[index].markers.contains { $0.id == device.id } }
        var changed = false
        for (spot, device) in zip(rooms[index].televisions, free) where spot.count == 3 {
            rooms[index].markers.append(RoomMarker(id: device.id, name: device.name, kind: device.kind, x: spot[0], y: spot[1], z: spot[2]))
            changed = true
        }
        if changed { save() }
    }
}

// MARK: - Scanning

#if canImport(RoomPlan)
/// RoomPlan's own scanning view, and what it found once the user ends the scan.
final class RoomScanController: UIViewController, RoomCaptureViewDelegate {
    private var capture: RoomCaptureView?
    var finished: ((CapturedRoom?) -> Void)?

    override func viewDidLoad() {
        super.viewDidLoad()
        let view = RoomCaptureView(frame: self.view.bounds)
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.delegate = self
        self.view.addSubview(view)
        capture = view
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        capture?.captureSession.run(configuration: RoomCaptureSession.Configuration())
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        capture?.captureSession.stop()
    }

    /// The user ended the scan: RoomPlan processes it and shows the result, then gives it here.
    func done() {
        capture?.captureSession.stop()
    }

    nonisolated func captureView(shouldPresent roomDataForProcessing: CapturedRoomData, error: (any Error)?) -> Bool {
        true
    }

    nonisolated func captureView(didPresent processedResult: CapturedRoom, error: (any Error)?) {
        let failed = error != nil
        Task { @MainActor in
            self.finished?(failed ? nil : processedResult)
        }
    }
}

struct RoomScanner: UIViewControllerRepresentable {
    /// Counts up when the user taps Done: the scan ends then.
    let ending: Int
    let finished: (CapturedRoom?) -> Void

    func makeUIViewController(context: Context) -> RoomScanController {
        let controller = RoomScanController()
        controller.finished = finished
        return controller
    }

    func updateUIViewController(_ controller: RoomScanController, context: Context) {
        controller.finished = finished
        if ending != context.coordinator.ended {
            context.coordinator.ended = ending
            controller.done()
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var ended = 0
    }
}

extension AXYONRooms {
    /// A scan kept: its model as a file, its measures and what it knew.
    func keep(_ scan: CapturedRoom, name: String) throws -> AXYONRoom {
        try FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        let id = String(UUID().uuidString.prefix(8)).lowercased()
        var room = AXYONRoom(id: id, name: name, date: .now, file: "room-\(id).usdz")
        try scan.export(to: url(room), exportOptions: .parametric)
        // The room's box, from where its walls are.
        var low = SIMD3<Float>(repeating: .greatestFiniteMagnitude)
        var high = SIMD3<Float>(repeating: -.greatestFiniteMagnitude)
        for wall in scan.walls {
            let centre = SIMD3<Float>(wall.transform.columns.3.x, wall.transform.columns.3.y, wall.transform.columns.3.z)
            let along = SIMD3<Float>(wall.transform.columns.0.x, wall.transform.columns.0.y, wall.transform.columns.0.z) * (wall.dimensions.x / 2)
            let up = SIMD3<Float>(0, wall.dimensions.y / 2, 0)
            for corner in [centre + along + up, centre - along + up, centre + along - up, centre - along - up] {
                low = simd_min(low, corner)
                high = simd_max(high, corner)
            }
        }
        if !scan.walls.isEmpty {
            room.width = Double(high.x - low.x)
            room.length = Double(high.z - low.z)
            room.height = Double(high.y - low.y)
        }
        room.walls = scan.walls.count
        room.doors = scan.doors.count
        room.windows = scan.windows.count
        for object in scan.objects {
            let category = String(describing: object.category)
            room.objects[category, default: 0] += 1
            if category == "television" {
                let at = object.transform.columns.3
                room.televisions.append([at.x, at.y, at.z])
            }
        }
        add(room)
        return room
    }
}
#endif

/// The scan, full screen, with its own Done.
struct RoomScanView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(HermesEngine.self) private var engine
    @State private var ending = 0
    @State private var working = false
    @State private var problem: String?
    let scanned: (AXYONRoom) -> Void

    var body: some View {
        ZStack(alignment: .bottom) {
            #if canImport(RoomPlan)
            RoomScanner(ending: ending) { result in
                guard let result else {
                    problem = String(localized: "The scan could not be finished. Try again, moving slowly along the walls.")
                    working = false
                    return
                }
                do {
                    let count = AXYONRooms.shared.rooms.count + 1
                    let room = try AXYONRooms.shared.keep(result, name: String(localized: "Room \(count)"))
                    AXYONRooms.shared.placeTelevisions(in: room.id, devices: engine.house?.devices ?? [])
                    Haptics.success()
                    scanned(room)
                    dismiss()
                } catch {
                    problem = error.localizedDescription
                    working = false
                }
            }
            .ignoresSafeArea()
            #endif
            VStack(spacing: 10) {
                if let problem {
                    Text(problem).font(.footnote).foregroundStyle(.white).padding(12).background(Capsule().fill(Color.black.opacity(0.6)))
                }
                HStack(spacing: 12) {
                    Button("Cancel") { dismiss() }
                        .coucouGlassButton()
                    Button {
                        working = true
                        ending += 1
                    } label: {
                        if working { ProgressView().tint(.white) } else { Label("Done", systemImage: "checkmark").font(.headline) }
                    }
                    .coucouGlassButton(prominent: true)
                    .disabled(working)
                }
            }
            .padding(.bottom, 24)
        }
    }
}

// MARK: - The room, drawn

/// The scanned room in 3D, turned and zoomed with the fingers, the devices as spheres of light in it.
struct RoomScene: UIViewRepresentable {
    let model: URL
    let markers: [RoomMarker]
    /// The ids of the devices that are on.
    let lit: Set<String>
    let accent: UIColor
    /// A device tapped, or a spot of the room (to place one there).
    let tapped: (String?, SCNVector3?) -> Void

    func makeUIView(context: Context) -> SCNView {
        let view = SCNView()
        view.backgroundColor = .clear
        view.allowsCameraControl = true
        view.autoenablesDefaultLighting = true
        view.antialiasingMode = .multisampling4X
        let scene = (try? SCNScene(url: model, options: nil)) ?? SCNScene()
        // The room as glass: the walls let the inside be seen.
        scene.rootNode.enumerateChildNodes { node, _ in
            for material in node.geometry?.materials ?? [] {
                material.diffuse.contents = UIColor.white.withAlphaComponent(0.22)
                material.emission.contents = accent.withAlphaComponent(0.10)
                material.isDoubleSided = true
                material.lightingModel = .physicallyBased
                material.roughness.contents = 0.25
                material.transparencyMode = .dualLayer
            }
        }
        let holder = SCNNode()
        holder.name = "axyon-markers"
        scene.rootNode.addChildNode(holder)
        view.scene = scene
        view.addGestureRecognizer(UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tap(_:))))
        context.coordinator.view = view
        return view
    }

    func updateUIView(_ view: SCNView, context: Context) {
        context.coordinator.tapped = tapped
        guard let holder = view.scene?.rootNode.childNode(withName: "axyon-markers", recursively: false) else { return }
        holder.childNodes.forEach { $0.removeFromParentNode() }
        for marker in markers {
            let on = lit.contains(marker.id)
            let sphere = SCNSphere(radius: 0.09)
            let material = SCNMaterial()
            material.diffuse.contents = on ? UIColor.white : accent.withAlphaComponent(0.85)
            material.emission.contents = on ? UIColor(red: 1, green: 0.92, blue: 0.6, alpha: 1) : accent.withAlphaComponent(0.5)
            material.lightingModel = .constant
            sphere.materials = [material]
            let node = SCNNode(geometry: sphere)
            node.name = "device:" + marker.id
            node.position = SCNVector3(marker.x, marker.y, marker.z)
            if on {
                // A light of its own, and a slow breath.
                let light = SCNLight()
                light.type = .omni
                light.color = UIColor(red: 1, green: 0.9, blue: 0.6, alpha: 1)
                light.intensity = 420
                light.attenuationEndDistance = 3.5
                node.light = light
                let breathe = SCNAction.sequence([SCNAction.scale(to: 1.35, duration: 0.9), SCNAction.scale(to: 1.0, duration: 0.9)])
                node.runAction(SCNAction.repeatForever(breathe))
            }
            holder.addChildNode(node)
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(tapped: tapped) }

    @MainActor
    final class Coordinator: NSObject {
        var tapped: (String?, SCNVector3?) -> Void
        weak var view: SCNView?

        init(tapped: @escaping (String?, SCNVector3?) -> Void) {
            self.tapped = tapped
        }

        @objc func tap(_ gesture: UITapGestureRecognizer) {
            guard let view else { return }
            let hits = view.hitTest(gesture.location(in: view), options: [.searchMode: SCNHitTestSearchMode.all.rawValue])
            if let device = hits.compactMap({ $0.node.name }).first(where: { $0.hasPrefix("device:") }) {
                tapped(String(device.dropFirst(7)), nil)
            } else if let spot = hits.first {
                tapped(nil, spot.worldCoordinates)
            }
        }
    }
}

struct AXYONRoomView: View {
    let id: String
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    private let rooms = AXYONRooms.shared
    @State private var spot: Spot?
    @State private var name = ""
    @State private var said: String?

    struct Spot: Identifiable {
        let id = UUID()
        let point: SCNVector3
    }

    private var room: AXYONRoom? { rooms.rooms.first { $0.id == id } }
    private var devices: [HermesDevice] { (engine.house?.devices ?? []).filter { $0.can("turn_on") || $0.can("toggle") || $0.can("run") } }

    var body: some View {
        ScrollView {
            if let room {
                VStack(alignment: .leading, spacing: 16) {
                    RoomScene(model: rooms.url(room), markers: room.markers,
                              lit: Set((engine.house?.devices ?? []).filter { $0.on == true }.map(\.id)),
                              accent: UIColor(AXYONTheme.current.accent)) { device, point in
                        if let device {
                            toggle(device)
                        } else if let point {
                            spot = Spot(point: point)
                        }
                    }
                    .frame(height: 380)
                    .axyonCard(cornerRadius: 28)
                    Text("Turn and zoom with your fingers. Tap a spot to put a device there; tap a device to turn it on or off.")
                        .font(.caption)
                        .foregroundStyle(AXYONPalette.dim)
                    if let said {
                        Label(said, systemImage: "info.circle.fill").font(.footnote).foregroundStyle(AXYONPalette.dim)
                    }
                    measures(room)
                    if !room.markers.isEmpty {
                        AXYONHeader(title: String(localized: "In this room"))
                        ForEach(room.markers) { marker in
                            HStack(spacing: 12) {
                                Image(systemName: AXYONDeviceTile.icon(marker.kind)).foregroundStyle(AXYONTheme.current.accent).frame(width: 26)
                                Text(marker.name).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                                Spacer()
                                Button { toggle(marker.id) } label: { Image(systemName: "power").font(.callout.weight(.bold)) }
                                    .buttonStyle(.borderless)
                                Button(role: .destructive) { rooms.unplace(marker.id, in: id) } label: { Image(systemName: "xmark.circle.fill") }
                                    .buttonStyle(.borderless)
                            }
                            .padding(14)
                            .axyonCard(cornerRadius: 18)
                        }
                    }
                    TextField("Room's name", text: $name)
                        .padding(14)
                        .axyonCard(cornerRadius: 18)
                        .onChange(of: name) { _, new in rooms.rename(id, new) }
                    Button(role: .destructive) {
                        rooms.remove(id)
                        dismiss()
                    } label: {
                        Label("Delete this room", systemImage: "trash")
                    }
                    .coucouGlassButton()
                }
                .padding(20)
            }
        }
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle(room?.name ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $spot) { spot in
            NavigationStack {
                List {
                    Section {
                        if devices.isEmpty {
                            Text("No device in AXYON's home yet: add one in the Home tab first.").foregroundStyle(AXYONPalette.dim)
                        }
                        ForEach(devices) { device in
                            Button {
                                rooms.place(device, in: id, at: spot.point)
                                self.spot = nil
                                Haptics.success()
                            } label: {
                                Label(device.room.isEmpty ? device.name : "\(device.name) · \(device.room)", systemImage: AXYONDeviceTile.icon(device.kind))
                            }
                        }
                    } header: {
                        Text("Which device is here?")
                    }
                    .listRowBackground(GlassRowBackground())
                }
                .scrollContentBackground(.hidden)
                .axyonBackdrop(intensity: 0.7)
                .navigationTitle("Place a device")
                .navigationBarTitleDisplayMode(.inline)
            }
            .presentationDetents([.medium, .large])
        }
        .task {
            name = room?.name ?? ""
            await engine.refreshHouse()
        }
    }

    private func measures(_ room: AXYONRoom) -> some View {
        let found = room.objects.sorted { $0.key < $1.key }.map { AXYONRoom.objectTitle($0.key, $0.value) }
        let metres: (Double) -> String = { $0.formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: "pt_BR"))) }
        return VStack(alignment: .leading, spacing: 6) {
            Text(verbatim: "\(metres(room.width)) × \(metres(room.length)) m · \(metres(room.area)) m² · \(metres(room.height)) m")
                .font(.headline)
                .monospacedDigit()
                .foregroundStyle(.white)
            Text(String(localized: "\(room.walls) walls, \(room.doors) doors, \(room.windows) windows"))
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
            if !found.isEmpty {
                Text(verbatim: found.joined(separator: " · ")).font(.footnote).foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .axyonCard(cornerRadius: 20)
    }

    private func toggle(_ device: String) {
        guard let found = engine.house?.devices.first(where: { $0.id == device }) else {
            said = String(localized: "This device is no longer in AXYON's home.")
            return
        }
        let action = found.can("run") ? "run" : found.can("toggle") && found.on == nil ? "toggle" : (found.on == true ? "turn_off" : "turn_on")
        Task {
            let problem = await engine.houseAction(found, action)
            said = problem
            if problem == nil { Haptics.success() } else { Haptics.error() }
            await engine.refreshHouse(quick: found.standalone)
        }
    }
}

/// The rooms scanned, and the way to scan another.
struct AXYONRoomsView: View {
    private let rooms = AXYONRooms.shared
    @State private var scanning = false
    @State private var opened: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                intro
                ForEach(rooms.rooms) { room in
                    NavigationLink {
                        AXYONRoomView(id: room.id)
                    } label: {
                        HStack(spacing: 14) {
                            Image(systemName: "cube.transparent.fill")
                                .font(.title2)
                                .foregroundStyle(AXYONTheme.current.accent)
                                .frame(width: 44, height: 44)
                                .coucouGlass(.regular, in: Circle())
                            VStack(alignment: .leading, spacing: 3) {
                                Text(room.name).font(.headline).foregroundStyle(.white)
                                Text(String(localized: "\(Int(room.area.rounded())) m² · \(room.markers.count) devices"))
                                    .font(.caption)
                                    .foregroundStyle(AXYONPalette.dim)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(AXYONPalette.faint)
                        }
                        .padding(14)
                        .axyonCard(cornerRadius: 22)
                    }
                    .buttonStyle(PressableButtonStyle())
                }
            }
            .padding(20)
        }
        .axyonBackdrop(intensity: 0.8)
        .navigationTitle("Home in 3D")
        .navigationBarTitleDisplayMode(.inline)
        .fullScreenCover(isPresented: $scanning) {
            RoomScanView { room in opened = room.id }
        }
        .navigationDestination(item: $opened) { id in
            AXYONRoomView(id: id)
        }
    }

    private var intro: some View {
        VStack(spacing: 14) {
            Image(systemName: "cube.transparent")
                .font(.system(size: 54, weight: .light))
                .foregroundStyle(AXYONTheme.current.accent)
                .symbolEffect(.pulse)
            Text("Your home, scanned in 3D").font(.title3.weight(.bold)).foregroundStyle(.white)
            Text(AXYONRooms.canScan
                 ? String(localized: "Walk slowly along the walls with the camera: the iPhone's LiDAR measures the room, its doors, windows and furniture. Then put your devices where they are and control them by tapping them in the room.")
                 : String(localized: "Scanning needs an iPhone with LiDAR (a Pro model). This one cannot measure a room."))
                .font(.footnote)
                .foregroundStyle(AXYONPalette.dim)
                .multilineTextAlignment(.center)
            Button { scanning = true } label: {
                Label("Scan a room", systemImage: "viewfinder").font(.headline)
            }
            .coucouGlassButton(prominent: true)
            .disabled(!AXYONRooms.canScan)
        }
        .padding(24)
        .frame(maxWidth: .infinity)
        .axyonCard(cornerRadius: 28)
    }
}
