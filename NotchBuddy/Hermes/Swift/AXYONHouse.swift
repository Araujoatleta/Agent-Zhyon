import SwiftUI
import UIKit

// The Home tab of AXYON with Hermes inside: the devices this iPhone drives itself on the home's Wi-Fi (a Samsung TV,
// an Android TV or box over ADB, the entities of the user's Home Assistant: Hermes/app/axyon_hermes/home.py), and a
// paired computer's home beside them. A tile's power button and the device's controls are the owner's own taps; what
// Hermes does to a plug or a switch asks Face ID (HermesArms, "confirm").

struct AXYONHouseView: View {
    @Environment(HermesEngine.self) private var engine
    @State private var place: Place = .iphone
    @State private var adding = false
    @State private var opened: HermesDevice?
    @State private var busy: Set<String> = []
    @State private var problem: String?

    enum Place: Hashable { case iphone, computer }

    private let columns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]

    var body: some View {
        let pc = PCLink.shared
        let computer = pc.isPaired && pc.offers("home")
        Group {
            if computer, place == .computer {
                PCHomeView()
            } else {
                iphoneHome
            }
        }
        .navigationTitle("Home")
        .navigationBarTitleDisplayMode(computer ? .inline : .large)
        .toolbar {
            if computer {
                ToolbarItem(placement: .principal) {
                    Picker("Home", selection: $place) {
                        Text("This iPhone").tag(Place.iphone)
                        Text(verbatim: pc.name).tag(Place.computer)
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 260)
                }
            }
            if place == .iphone || !computer {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        adding = true
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel("Add a device")
                    .disabled(engine.state != .ready)
                }
            }
        }
        .sheet(isPresented: $adding, onDismiss: { Task { await engine.refreshHouse() } }) {
            NavigationStack { AXYONAddDeviceView() }
        }
        .sheet(item: $opened) { device in
            NavigationStack { AXYONDeviceView(deviceID: device.id) }
                .presentationDetents([.medium, .large])
        }
        .task { await engine.refreshHouse() }
    }

    private var iphoneHome: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.gold)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(14)
                        .axyonCard(cornerRadius: 20, tint: AXYONPalette.gold)
                }
                if let house = engine.house {
                    if house.devices.isEmpty {
                        empty
                    }
                    if !house.own.isEmpty {
                        AXYONHeader(title: String(localized: "On this iPhone's Wi-Fi"), systemImage: "wifi")
                        tiles(house.own)
                    }
                    if !house.assistant.isEmpty {
                        AXYONHeader(title: String(localized: "Home Assistant"), systemImage: "house.lodge.fill")
                        tiles(house.assistant)
                    }
                    if let trouble = house.assistantProblem {
                        Label(trouble, systemImage: "wifi.exclamationmark")
                            .font(.footnote)
                            .foregroundStyle(AXYONPalette.dim)
                            .padding(.horizontal, 6)
                    }
                } else if engine.state == .ready {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                        .padding(.top, 60)
                } else {
                    Label("Hermes is waking up inside the iPhone…", systemImage: "hourglass")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 60)
                }
            }
            .padding(16)
        }
        .axyonBackdrop()
        .refreshable { await engine.refreshHouse() }
    }

    private func tiles(_ devices: [HermesDevice]) -> some View {
        LazyVGrid(columns: columns, spacing: 12) {
            ForEach(devices) { device in
                AXYONDeviceTile(device: device, busy: busy.contains(device.id)) {
                    opened = device
                } power: {
                    Task { await power(device) }
                }
            }
        }
    }

    private func power(_ device: HermesDevice) async {
        let action = device.can("toggle") && device.on == nil ? "toggle" : (device.on == true ? "turn_off" : "turn_on")
        busy.insert(device.id)
        defer { busy.remove(device.id) }
        problem = await engine.houseAction(device, action)
        if problem == nil { Haptics.success() } else { Haptics.error() }
        await engine.refreshHouse(quick: !device.fromAssistant)
    }

    private var empty: some View {
        VStack(spacing: 14) {
            Image(systemName: "house.and.flag.fill")
                .font(.system(size: 44))
                .foregroundStyle(AXYONPalette.violet)
                .shadow(color: AXYONPalette.violet.opacity(0.6), radius: 12)
            Text("Your home, from the iPhone").font(.title3.weight(.semibold))
            Text("Add your Samsung TV, your Android TV or box, or your Home Assistant: AXYON reaches them from this iPhone on your Wi-Fi, with no computer. Then ask in the chat: “turn on the living room TV”.")
                .font(.callout)
                .foregroundStyle(AXYONPalette.dim)
                .multilineTextAlignment(.center)
            Button {
                adding = true
            } label: {
                Label("Add a device", systemImage: "plus")
                    .font(.headline)
            }
            .coucouGlassButton(prominent: true)
            .disabled(engine.state != .ready)
        }
        .padding(28)
        .frame(maxWidth: .infinity)
        .axyonCard(glow: AXYONPalette.violet)
        .padding(.top, 20)
    }
}

/// One device as a glass tile: what it is, whether it is on, and its power button.
struct AXYONDeviceTile: View {
    let device: HermesDevice
    var busy = false
    let open: () -> Void
    let power: () -> Void

    private var lit: Bool { device.on == true }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                Image(systemName: AXYONDeviceTile.icon(device.kind))
                    .font(.title2)
                    .foregroundStyle(lit ? AXYONPalette.gold : AXYONPalette.faint)
                    .shadow(color: lit ? AXYONPalette.gold.opacity(0.6) : .clear, radius: 8)
                Spacer()
                if device.can("turn_on") || device.can("toggle") || device.can("run") {
                    Button(action: power) {
                        Group {
                            if busy {
                                ProgressView().controlSize(.small)
                            } else {
                                Image(systemName: device.can("run") ? "play.fill" : "power")
                                    .font(.callout.weight(.bold))
                            }
                        }
                        .foregroundStyle(lit ? .black : .white)
                        .frame(width: 36, height: 36)
                        .background(lit ? AnyShapeStyle(AXYONPalette.gold) : AnyShapeStyle(Color.white.opacity(0.12)),
                                    in: Circle())
                    }
                    .buttonStyle(PressableButtonStyle())
                    .disabled(busy)
                    .accessibilityLabel(lit ? "Turn off" : "Turn on")
                }
            }
            Spacer(minLength: 0)
            Text(device.name)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(2)
            Text(status)
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
                .lineLimit(1)
        }
        .padding(14)
        .frame(maxWidth: .infinity, minHeight: 128, alignment: .topLeading)
        .axyonCard(cornerRadius: 22, tint: lit ? AXYONPalette.gold : nil)
        .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        .onTapGesture(perform: open)
    }

    private var status: String {
        if !device.paired { return String(localized: "Not paired yet") }
        if device.online == false { return String(localized: "Not answering") }
        var parts: [String] = []
        if let on = device.on { parts.append(on ? String(localized: "On") : String(localized: "Off")) }
        if let brightness = device.brightness, device.on == true { parts.append("\(brightness)%") }
        if let volume = device.volume { parts.append(String(localized: "Vol. \(volume)")) }
        if !device.room.isEmpty { parts.append(device.room) }
        return parts.isEmpty ? device.source : parts.joined(separator: " · ")
    }

    static func icon(_ kind: String) -> String {
        switch kind {
        case "tv": "tv.fill"
        case "light": "lightbulb.fill"
        case "switch": "poweroutlet.type.b.fill"
        case "fan": "fan.fill"
        case "cover": "blinds.horizontal.closed"
        case "scene": "theatermasks.fill"
        case "routine": "list.bullet.rectangle.fill"
        case "climate": "thermometer.medium"
        default: "house.fill"
        }
    }
}

/// One device up close: every control it said it has, its apps, and pairing or removing it.
struct AXYONDeviceView: View {
    let deviceID: String
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    @State private var working: String?
    @State private var said: String?
    @State private var failed = false
    @State private var level: Double = 50

    private var device: HermesDevice? { engine.house?.devices.first { $0.id == deviceID } }

    var body: some View {
        ScrollView {
            if let device {
                VStack(spacing: 18) {
                    header(device)
                    if let said {
                        Label(said, systemImage: failed ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                            .font(.footnote)
                            .foregroundStyle(failed ? AXYONPalette.gold : AXYONPalette.up)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if !device.paired {
                        pairCard(device)
                    } else {
                        controls(device)
                    }
                    if !device.fromAssistant {
                        Button(role: .destructive) {
                            Task {
                                await engine.removeDevice(device.id)
                                dismiss()
                            }
                        } label: {
                            Label("Remove from AXYON", systemImage: "trash")
                        }
                        .coucouGlassButton()
                        .padding(.top, 8)
                    }
                }
                .padding(16)
            } else {
                Text("This device is no longer in AXYON's home.")
                    .foregroundStyle(AXYONPalette.dim)
                    .padding(40)
            }
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } }
        }
        .onAppear {
            if let device { level = Double(device.brightness ?? device.volume ?? 50) }
        }
    }

    private func header(_ device: HermesDevice) -> some View {
        HStack(spacing: 14) {
            Image(systemName: AXYONDeviceTile.icon(device.kind))
                .font(.title)
                .foregroundStyle(device.on == true ? AXYONPalette.gold : AXYONPalette.cyan)
                .frame(width: 54, height: 54)
                .coucouGlass(.regular, in: Circle())
            VStack(alignment: .leading, spacing: 3) {
                Text(device.name).font(.title3.weight(.bold)).foregroundStyle(.white)
                Text([device.source, device.room].filter { !$0.isEmpty }.joined(separator: " · "))
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            }
            Spacer()
        }
    }

    private func pairCard(_ device: HermesDevice) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(device.source == "Android"
                 ? String(localized: "Tap Pair, then on the device accept “Allow USB debugging?” (tick “Always allow”).")
                 : String(localized: "Tap Pair, then accept AXYON on the TV when it asks."))
                .font(.callout)
                .foregroundStyle(.white)
            Button {
                Task { await pair(device) }
            } label: {
                if working == "pair" {
                    ProgressView()
                } else {
                    Label("Pair", systemImage: "link")
                }
            }
            .coucouGlassButton(prominent: true)
            .disabled(working != nil)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .axyonCard(tint: AXYONPalette.violet)
    }

    @ViewBuilder private func controls(_ device: HermesDevice) -> some View {
        let row = [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)]
        LazyVGrid(columns: row, spacing: 10) {
            if device.can("turn_on") { control(device, "turn_on", String(localized: "Turn on"), "power") }
            if device.can("turn_off") { control(device, "turn_off", String(localized: "Turn off"), "poweroff") }
            if device.can("toggle") { control(device, "toggle", String(localized: "Switch"), "switch.2") }
            if device.can("run") { control(device, "run", String(localized: "Run"), "play.fill") }
            if device.can("volume_down") { control(device, "volume_down", String(localized: "Volume −"), "speaker.wave.1.fill") }
            if device.can("volume_up") { control(device, "volume_up", String(localized: "Volume +"), "speaker.wave.3.fill") }
            if device.can("mute") { control(device, "mute", String(localized: "Mute"), "speaker.slash.fill") }
            if device.can("channel_down") { control(device, "channel_down", String(localized: "Channel −"), "chevron.down.circle.fill") }
            if device.can("channel_up") { control(device, "channel_up", String(localized: "Channel +"), "chevron.up.circle.fill") }
            if device.can("play_pause") { control(device, "play_pause", String(localized: "Play / pause"), "playpause.fill") }
            if device.can("open") { control(device, "open", String(localized: "Open"), "arrow.up.to.line") }
            if device.can("close") { control(device, "close", String(localized: "Close"), "arrow.down.to.line") }
            if device.can("stop") { control(device, "stop", String(localized: "Stop"), "stop.fill") }
        }
        if device.can("set_brightness") || device.can("set_volume") {
            let brightness = device.can("set_brightness")
            VStack(alignment: .leading, spacing: 8) {
                Label(brightness ? String(localized: "Brightness") : String(localized: "Volume"),
                      systemImage: brightness ? "sun.max.fill" : "speaker.wave.2.fill")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(AXYONPalette.dim)
                Slider(value: $level, in: 0...100, step: 5, onEditingChanged: { editing in
                    if !editing {
                        Task { await act(device, brightness ? "set_brightness" : "set_volume", value: Int(level)) }
                    }
                })
                .tint(brightness ? AXYONPalette.gold : AXYONPalette.cyan)
            }
            .padding(16)
            .axyonCard(cornerRadius: 22)
        }
        if device.can("key") { remote(device) }
        if device.can("open_app"), !device.apps.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                AXYONHeader(title: String(localized: "Apps"), systemImage: "square.grid.2x2.fill")
                FlowChips(items: device.apps) { app in
                    Task { await act(device, "open_app", value: app) }
                }
            }
        }
    }

    private func remote(_ device: HermesDevice) -> some View {
        VStack(spacing: 10) {
            key(device, "up", "chevron.up")
            HStack(spacing: 10) {
                key(device, "left", "chevron.left")
                key(device, "ok", "circle.fill")
                key(device, "right", "chevron.right")
            }
            key(device, "down", "chevron.down")
            HStack(spacing: 10) {
                key(device, "back", "arrow.uturn.backward")
                key(device, "home", "house.fill")
                key(device, "play_pause", "playpause.fill")
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity)
        .axyonCard(cornerRadius: 26)
    }

    private func key(_ device: HermesDevice, _ name: String, _ icon: String) -> some View {
        Button {
            Task { await act(device, "key", value: name) }
        } label: {
            Image(systemName: icon)
                .font(.title3.weight(.semibold))
                .foregroundStyle(.white)
                .frame(width: 58, height: 50)
                .coucouGlass(.regular, cornerRadius: 16, interactive: true)
        }
        .buttonStyle(PressableButtonStyle())
        .disabled(working != nil)
    }

    private func control(_ device: HermesDevice, _ action: String, _ title: String, _ icon: String) -> some View {
        Button {
            Task { await act(device, action) }
        } label: {
            VStack(spacing: 6) {
                if working == action {
                    ProgressView().controlSize(.small).frame(height: 24)
                } else {
                    Image(systemName: icon).font(.title3).frame(height: 24)
                }
                Text(title).font(.caption.weight(.semibold)).lineLimit(1).minimumScaleFactor(0.7)
            }
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .axyonCard(cornerRadius: 18)
        }
        .buttonStyle(PressableButtonStyle())
        .disabled(working != nil)
    }

    private func act(_ device: HermesDevice, _ action: String, value: Any? = nil) async {
        working = action
        defer { working = nil }
        let problem = await engine.houseAction(device, action, value: value)
        failed = problem != nil
        said = problem ?? String(localized: "Done.")
        if problem == nil { Haptics.success() } else { Haptics.error() }
        if action == "turn_on" || action == "turn_off" || action == "toggle" {
            await engine.refreshHouse(quick: !device.fromAssistant)
        }
    }

    private func pair(_ device: HermesDevice) async {
        working = "pair"
        defer { working = nil }
        do {
            said = try await engine.pairDevice(device.id)
            failed = false
            Haptics.success()
        } catch {
            said = HermesEngine.describe(error)
            failed = true
            Haptics.error()
        }
    }
}

/// Chips that wrap onto as many lines as they need.
struct FlowChips: View {
    let items: [String]
    let tap: (String) -> Void

    var body: some View {
        AXYONFlow(spacing: 8) {
            ForEach(items, id: \.self) { item in
                Button {
                    tap(item)
                } label: {
                    AXYONChip(title: item, systemImage: "play.rectangle.fill")
                }
                .buttonStyle(PressableButtonStyle())
            }
        }
    }
}

/// Lays its views out left to right, onto a new line when one is full.
struct AXYONFlow: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 320
        var x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += line + spacing
                line = 0
            }
            x += size.width + spacing
            line = max(line, size.height)
        }
        return CGSize(width: width, height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, line: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += line + spacing
                line = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }
}

/// Adding a device: found on the Wi-Fi, or typed in, and the user's Home Assistant with its token.
struct AXYONAddDeviceView: View {
    @Environment(HermesEngine.self) private var engine
    @Environment(\.dismiss) private var dismiss
    @State private var found: [HermesFound] = []
    @State private var scanning = false
    @State private var scanned = false
    @State private var driver = "samsung"
    @State private var host = ""
    @State private var name = ""
    @State private var room = ""
    @State private var assistantURL = ""
    @State private var assistantToken = ""
    @State private var working = false
    @State private var said: String?
    @State private var failed = false

    var body: some View {
        List {
            Section {
                Button {
                    Task { await scan() }
                } label: {
                    HStack {
                        Label(scanned ? String(localized: "Look again") : String(localized: "Look on this Wi-Fi"),
                              systemImage: "dot.radiowaves.left.and.right")
                        Spacer()
                        if scanning { ProgressView() }
                    }
                }
                .disabled(scanning || working)
                ForEach(found) { item in
                    HStack(spacing: 12) {
                        Image(systemName: item.driver == "homeassistant" ? "house.lodge.fill" : "tv.fill")
                            .foregroundStyle(AXYONPalette.cyan)
                            .frame(width: 26)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.name).foregroundStyle(.white)
                            Text([item.model, item.host].filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.caption)
                                .foregroundStyle(AXYONPalette.dim)
                        }
                        Spacer()
                        if item.added {
                            Text("Added").font(.caption).foregroundStyle(AXYONPalette.up)
                        } else {
                            Button(item.driver == "homeassistant" ? String(localized: "Use") : String(localized: "Add")) {
                                if item.driver == "homeassistant" {
                                    assistantURL = item.url
                                } else {
                                    Task { await add(driver: item.driver, host: item.host, name: "") }
                                }
                            }
                            .font(.footnote.weight(.semibold))
                            .disabled(working)
                        }
                    }
                }
            } header: {
                Text("On this iPhone's Wi-Fi")
            } footer: {
                Text("AXYON looks for Samsung TVs, Android devices with network debugging on, and Home Assistant. iOS asks once to let it use the local network.")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Picker("Kind", selection: $driver) {
                    Text("Samsung TV").tag("samsung")
                    Text("Android TV or box").tag("android")
                }
                TextField("Address (192.168.0.20)", text: $host)
                    .keyboardType(.numbersAndPunctuation)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Name (optional)", text: $name)
                TextField("Room (optional)", text: $room)
                Button {
                    Task { await add(driver: driver, host: host, name: name) }
                } label: {
                    HStack {
                        Label("Add and pair", systemImage: "link.badge.plus")
                        Spacer()
                        if working { ProgressView() }
                    }
                }
                .disabled(working || host.trimmingCharacters(in: .whitespaces).isEmpty)
            } header: {
                Text("By its address")
            } footer: {
                Text(driver == "android"
                     ? String(localized: "On the device: Settings → About → tap Build 7 times, then Developer options → turn on USB debugging and network debugging (ADB over the network, port 5555). It asks once to allow this iPhone.")
                     : String(localized: "The TV on and on the same Wi-Fi. It asks once, on its screen, to allow AXYON."))
            }
            .listRowBackground(GlassRowBackground())

            Section {
                TextField("http://homeassistant.local:8123", text: $assistantURL)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("Long-lived access token", text: $assistantToken)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button {
                    Task { await connectAssistant() }
                } label: {
                    Label("Connect Home Assistant", systemImage: "house.lodge.fill")
                }
                .disabled(working || assistantURL.isEmpty)
                if engine.house?.assistantURL != nil {
                    Button(role: .destructive) {
                        Task { await engine.removeAssistant() }
                    } label: {
                        Label("Disconnect Home Assistant", systemImage: "xmark.circle")
                    }
                }
            } header: {
                Text("Home Assistant")
            } footer: {
                Text("In Home Assistant: your profile → Security → Long-lived access tokens → Create token. The token stays in this iPhone's Keychain; AXYON never acts on locks or alarms.")
            }
            .listRowBackground(GlassRowBackground())

            if let said {
                Section {
                    Label(said, systemImage: failed ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                        .foregroundStyle(failed ? AXYONPalette.gold : AXYONPalette.up)
                }
                .listRowBackground(GlassRowBackground())
            }
        }
        .axyonBackdrop(intensity: 0.7)
        .navigationTitle("Add a device")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } }
        }
        .onAppear {
            if assistantURL.isEmpty { assistantURL = engine.house?.assistantURL ?? "" }
        }
    }

    private func scan() async {
        scanning = true
        defer {
            scanning = false
            scanned = true
        }
        do {
            let result = try await engine.scanHouse()
            found = result.found
            if let problem = result.problem {
                said = problem
                failed = true
            } else if found.isEmpty {
                said = String(localized: "Nothing found: is the device on, on this Wi-Fi? Or add it by its address.")
                failed = true
            } else {
                said = nil
            }
        } catch {
            said = HermesEngine.describe(error)
            failed = true
        }
    }

    private func add(driver: String, host: String, name: String) async {
        working = true
        said = driver == "android"
            ? String(localized: "On the device, accept “Allow USB debugging?”…")
            : String(localized: "On the TV, allow AXYON…")
        failed = false
        defer { working = false }
        do {
            let message = try await engine.addDevice(driver: driver, host: host.trimmingCharacters(in: .whitespaces),
                                                     name: name, room: room)
            said = message.isEmpty ? String(localized: "Added.") : message
            failed = false
            Haptics.success()
            found = found.map { item in
                var item = item
                if item.host == host { item.added = true }
                return item
            }
        } catch {
            said = HermesEngine.describe(error)
            failed = true
            Haptics.error()
        }
    }

    private func connectAssistant() async {
        working = true
        defer { working = false }
        do {
            let count = try await engine.setAssistant(url: assistantURL, token: assistantToken)
            assistantToken = ""
            said = String(localized: "Home Assistant connected: \(count) devices.")
            failed = false
            Haptics.success()
        } catch {
            said = HermesEngine.describe(error)
            failed = true
            Haptics.error()
        }
    }
}
