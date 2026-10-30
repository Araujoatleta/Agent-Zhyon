import SwiftUI
import UIKit

// The Home tab: Mochi's home on the paired Windows or Linux computer (capability `home`, docs/ANDROID_LINK.md), the
// devices the user ticked for Mochi there, by room, as glass tiles; a tap opens what each one said it can do, and
// nothing else. A device or a routine that asks first asks Face ID here, and the computer checks again. Nothing here
// says where a device is: the computer sends no address, no MAC address, no token.

struct PCHomeView: View {
    @State private var openDevice: OpenDevice?
    @State private var problem: String?
    /// What a routine just did, in the computer's words, by routine.
    @State private var ran: [String: String] = [:]

    private var pc: PCLink { PCLink.shared }
    private var connected: Bool { pc.status == .connected }
    private var columns: [GridItem] { [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)] }

    private struct OpenDevice: Identifiable {
        let id: String
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if let home = pc.model.home {
                    content(home)
                } else {
                    PCNotice.missing("home", off: String(localized: "On \(pc.name), open Coucou → Settings → Phone and turn on “Let the phone use Mochi's home”. Mochi's home must be on there too (Settings → Mochi's home)."))
                        .padding(.top, 24)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
        .background { DeepBackdrop() }
        .navigationTitle("Mochi's home")
        .refreshable { await refresh() }
        .onAppear { pc.refreshHome() }
        .sheet(item: $openDevice) { item in
            PCDeviceControls(deviceID: item.id)
        }
        .alert(Text("Mochi's home"), isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(verbatim: problem ?? "")
        }
    }

    @ViewBuilder
    private func content(_ home: PCHome) -> some View {
        if !connected { offline }
        if !home.enabled {
            PCNotice(symbol: "house", title: String(localized: "Mochi's home is off"),
                     text: String(localized: "Turn it on in Coucou → Settings → Mochi's home on \(pc.name)."))
        } else if home.devices.isEmpty {
            PCNotice(symbol: "lightbulb", title: String(localized: "No device for Mochi yet"),
                     text: String(localized: "On \(pc.name), tick the devices Mochi may use in Coucou → Settings → Mochi's home: they show up here."))
        } else {
            if home.askAll {
                Label("Every action asks Face ID first: the computer is set to ask before each one.", systemImage: "faceid")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            ForEach(home.rooms) { room in
                roomSection(room)
            }
        }
        if home.enabled, !home.routines.isEmpty {
            routines(home.routines)
        }
        if !home.log.isEmpty {
            record(home.log)
        }
    }

    private var offline: some View {
        HStack(spacing: 10) {
            Image(systemName: "wifi.slash")
                .foregroundStyle(CoucouPalette.warn)
            VStack(alignment: .leading, spacing: 2) {
                Text("Not connected")
                    .font(.subheadline.weight(.semibold))
                Text(verbatim: pc.status.text)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if pc.status != .connecting, pc.status != .searching, pc.status != .refused {
                Button("Try again now") { pc.reconnectIfNeeded() }
                    .buttonStyle(.bordered)
                    .font(.footnote.weight(.semibold))
            }
        }
        .padding(12)
        .coucouGlass(.regular, cornerRadius: 18, tint: CoucouPalette.warn)
    }

    // MARK: Devices

    private func roomSection(_ room: PCHome.Room) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            title(room.name.isEmpty ? String(localized: "Other devices") : room.name)
            CoucouGlassGroup(spacing: 0) {
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(room.devices) { device in
                        PCDeviceTile(device: device, working: pc.homeWorking == device.id,
                                     enabled: connected && pc.homeWorking == nil,
                                     open: { openDevice = OpenDevice(id: device.id) },
                                     power: { action in Task { await control(device, action) } })
                    }
                }
            }
        }
    }

    private func control(_ device: PCHome.Device, _ action: String) async {
        let result = await pc.homeControl(device, action: action)
        if result.ok {
            Haptics.success()
        } else if let text = PCRequestWords.explain(result, cap: "home", computer: pc.name) {
            Haptics.warning()
            problem = text
        }
    }

    // MARK: Routines

    private func routines(_ list: [PCHome.Routine]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            title(String(localized: "Routines"))
            CoucouGlassGroup(spacing: 0) {
                VStack(spacing: 10) {
                    ForEach(list) { routine in
                        routineRow(routine)
                    }
                }
            }
        }
    }

    private func routineRow(_ routine: PCHome.Routine) -> some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(verbatim: routine.name)
                        .font(.headline)
                    if routine.ask || pc.model.home?.askAll == true {
                        Image(systemName: "faceid")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Asks first")
                    }
                }
                if !routine.steps.isEmpty {
                    Text(verbatim: routine.steps.joined(separator: " → "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                }
                ForEach(Array(routine.problems.enumerated()), id: \.offset) { _, problem in
                    Label(problem, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(CoucouPalette.warn)
                }
                if let done = ran[routine.id] {
                    Label(done, systemImage: "checkmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(CoucouPalette.ok)
                }
            }
            Spacer(minLength: 8)
            Button {
                Task { await run(routine) }
            } label: {
                if pc.homeWorking == routine.id {
                    ProgressView()
                } else {
                    Text("Run")
                }
            }
            .borderedProminentButton()
            .tint(CoucouPalette.accent)
            .disabled(!connected || pc.homeWorking != nil)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .coucouGlass(.regular, cornerRadius: 20)
    }

    private func run(_ routine: PCHome.Routine) async {
        ran[routine.id] = nil
        let result = await pc.runRoutine(routine)
        if result.ok {
            ran[routine.id] = result.detail.isEmpty ? String(localized: "Ran just now") : result.detail
            Haptics.success()
        } else if let text = PCRequestWords.explain(result, cap: "home", computer: pc.name) {
            Haptics.warning()
            problem = text
        }
    }

    // MARK: The record

    private func record(_ log: [PCHome.Entry]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            title(String(localized: "Recently"))
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(log.prefix(12).enumerated()), id: \.offset) { index, entry in
                    if index > 0 {
                        Rectangle().fill(CoucouPalette.hairline).frame(height: 1)
                    }
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Image(systemName: entry.ok ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(entry.ok ? CoucouPalette.ok : CoucouPalette.danger)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: [entry.device, entry.action].filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.subheadline)
                            if !entry.detail.isEmpty {
                                Text(verbatim: entry.detail)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer(minLength: 8)
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(verbatim: Self.when(entry.at))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                            Text(verbatim: Self.origin(entry.origin))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .padding(.vertical, 8)
                    .accessibilityElement(children: .combine)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
            .coucouGlass(.regular, cornerRadius: 20)
        }
    }

    /// "2026-10-13 21:05:12" as the computer writes it: the time only when it is today.
    private static func when(_ at: String) -> String {
        let parts = at.split(separator: " ")
        guard parts.count == 2, parts[1].count >= 5 else { return at }
        let day = Calendar.current.dateComponents([.year, .month, .day], from: .now)
        let today = String(format: "%04d-%02d-%02d", day.year ?? 0, day.month ?? 0, day.day ?? 0)
        let time = String(parts[1].prefix(5))
        return parts[0] == today ? time : "\(parts[0]) \(time)"
    }

    /// Who asked for it, as the computer's record says.
    private static func origin(_ origin: String) -> String {
        switch origin {
        case "phone": String(localized: "Phone")
        case "settings": String(localized: "Settings")
        case "routine": String(localized: "Routine")
        case "island", "mochi", "agent": String(localized: "Mochi")
        default: origin
        }
    }

    private func title(_ text: String) -> some View {
        Text(verbatim: text)
            .font(.title3.weight(.semibold))
            .padding(.leading, 4)
    }

    private func refresh() async {
        pc.refreshHome()
        // The home comes back in a moment; the pull's spinner stays that long.
        try? await Task.sleep(for: .milliseconds(700))
    }
}

/// How a kind of device looks: its symbol, its color, and what it says now in a few words.
private enum PCHomeLook {
    static func symbol(_ kind: String) -> String {
        switch kind {
        case "tv": "tv"
        case "speaker": "hifispeaker"
        case "light": "lightbulb"
        case "plug": "powerplug"
        case "switch": "switch.2"
        default: "house"
        }
    }

    static func color(_ kind: String) -> Color {
        #if HERMES
        // AXYON has no colour but for what is on or failed: every kind of device in white.
        return .white
        #else
        return switch kind {
        case "light": CoucouPalette.warn
        case "plug", "switch": CoucouPalette.ok
        case "speaker": Color.cyan
        default: CoucouPalette.accent
        }
        #endif
    }

    /// "On · 80%", "On · Volume 12 · YouTube", "Offline".
    static func summary(_ device: PCHome.Device) -> String {
        let state = device.state
        guard state.online else { return String(localized: "Offline") }
        var parts: [String] = []
        // A device's power has words of its own (allumé, not activé).
        if let on = state.on {
            parts.append(on ? String(localized: "home.state.on", defaultValue: "On")
                            : String(localized: "home.state.off", defaultValue: "Off"))
        }
        if state.on != false {
            if let brightness = state.brightness { parts.append("\(brightness)%") }
            if state.muted == true {
                parts.append(String(localized: "Muted"))
            } else if let volume = state.volume {
                parts.append(String(localized: "Volume \(volume)"))
            }
            if let app = state.app { parts.append(app) }
            if let channel = state.channel { parts.append(String(localized: "Channel \(channel)")) }
        }
        if parts.isEmpty { return device.source.isEmpty ? String(localized: "Online") : device.source }
        return parts.joined(separator: " · ")
    }

    /// Turn it on when it is off, off when it is on, as far as it said it can.
    static func powerAction(_ device: PCHome.Device) -> String? {
        switch device.state.on {
        case true?: device.can("turnOff") ? "turn_off" : nil
        case false?: device.can("turnOn") ? "turn_on" : nil
        case nil: device.can("turnOn") ? "turn_on" : device.can("turnOff") ? "turn_off" : nil
        }
    }
}

/// One device as a glass tile: what it is, what it says now, and its power button when it reported one.
private struct PCDeviceTile: View {
    let device: PCHome.Device
    let working: Bool
    let enabled: Bool
    let open: @MainActor () -> Void
    let power: @MainActor (String) -> Void

    var body: some View {
        let on = device.state.online && device.state.on == true
        let color = PCHomeLook.color(device.kind)
        ZStack(alignment: .topTrailing) {
            Button {
                open()
            } label: {
                VStack(alignment: .leading, spacing: 6) {
                    Image(systemName: PCHomeLook.symbol(device.kind))
                        .symbolVariant(on ? .fill : .none)
                        .font(.title2)
                        .foregroundStyle(on ? color : Color.secondary)
                        .frame(height: 32)
                    Spacer(minLength: 6)
                    Text(verbatim: device.name)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                        .multilineTextAlignment(.leading)
                        .lineLimit(2)
                    HStack(spacing: 4) {
                        if device.ask {
                            Image(systemName: "faceid")
                                .accessibilityLabel("Asks first")
                        }
                        Text(verbatim: PCHomeLook.summary(device))
                            .lineLimit(1)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                .padding(14)
                .frame(maxWidth: .infinity, minHeight: 126, alignment: .topLeading)
                .contentShape(Rectangle())
            }
            .buttonStyle(PressableButtonStyle())
            .accessibilityHint(Text("Shows its controls"))
            if let action = PCHomeLook.powerAction(device) {
                Button {
                    power(action)
                } label: {
                    ZStack {
                        Circle()
                            .fill(on ? color : Color.white.opacity(0.12))
                        if working {
                            ProgressView()
                                .tint(.white)
                        } else {
                            Image(systemName: "power")
                                .font(.body.weight(.bold))
                                .foregroundStyle(on ? Color.black.opacity(0.75) : Color.white)
                        }
                    }
                    .frame(width: 40, height: 40)
                }
                .buttonStyle(PressableButtonStyle())
                .disabled(!enabled)
                .padding(10)
                .accessibilityLabel(action == "turn_off" ? Text("Turn off") : Text("Turn on"))
            }
        }
        .coucouGlass(.regular, cornerRadius: 22, tint: on ? color : nil)
        .opacity(device.state.online ? 1 : 0.65)
    }
}

/// A device's controls, as many as it said it has: power, volume and mute, brightness, channels, apps. Every action
/// goes through PCLink (Face ID first for a device that asks), and the computer checks it again.
private struct PCDeviceControls: View {
    let deviceID: String
    @Environment(\.dismiss) private var dismiss
    /// A slider being moved, until the device says its own value.
    @State private var volume: Double?
    @State private var brightness: Double?
    @State private var note: String?
    @State private var failed = false

    private var pc: PCLink { PCLink.shared }
    private var device: PCHome.Device? { pc.model.home?.devices.first { $0.id == deviceID } }
    private var enabled: Bool { pc.status == .connected && pc.homeWorking == nil }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let device {
                        header(device)
                        controls(device)
                    } else {
                        PCNotice(symbol: "questionmark.circle", title: String(localized: "Not shared any more"),
                                 text: String(localized: "This device isn't shared with the phone any more."))
                    }
                    if let note {
                        Label(note, systemImage: failed ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                            .font(.footnote)
                            .foregroundStyle(failed ? CoucouPalette.danger : CoucouPalette.ok)
                    }
                }
                .padding(16)
            }
            .background { DeepBackdrop() }
            .navigationTitle(device?.name ?? "")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        // The device said its own value: the slider shows it again.
        .onChange(of: device?.state.volume) { volume = nil }
        .onChange(of: device?.state.brightness) { brightness = nil }
    }

    private func header(_ device: PCHome.Device) -> some View {
        HStack(spacing: 14) {
            Image(systemName: PCHomeLook.symbol(device.kind))
                .symbolVariant(device.state.on == true ? .fill : .none)
                .font(.title)
                .foregroundStyle(PCHomeLook.color(device.kind))
                .frame(width: 56, height: 56)
                .background(Circle().fill(PCHomeLook.color(device.kind).opacity(0.16)))
            VStack(alignment: .leading, spacing: 3) {
                Text(verbatim: PCHomeLook.summary(device))
                    .font(.headline)
                let place = [device.room, device.source].filter { !$0.isEmpty }.joined(separator: " · ")
                if !place.isEmpty {
                    Text(verbatim: place)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                if device.ask || pc.model.home?.askAll == true {
                    Label("Face ID before each action", systemImage: "faceid")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            if pc.homeWorking == device.id {
                ProgressView()
            }
        }
    }

    @ViewBuilder
    private func controls(_ device: PCHome.Device) -> some View {
        if device.can("turnOn") || device.can("turnOff") {
            group(String(localized: "Power")) {
                HStack(spacing: 10) {
                    if device.can("turnOn") {
                        actionButton(String(localized: "Turn on"), symbol: "power", device, "turn_on",
                                     prominent: device.state.on != true)
                    }
                    if device.can("turnOff") {
                        actionButton(String(localized: "Turn off"), symbol: "poweroff", device, "turn_off",
                                     prominent: device.state.on == true)
                    }
                }
            }
        }
        if device.can("volumeSet") || device.can("volumeStep") || device.can("mute") {
            group(String(localized: "Volume")) {
                if device.can("volumeSet") {
                    slider(Binding(get: { volume ?? Double(device.state.volume ?? 50) }, set: { volume = $0 }),
                           in: 0...100, known: volume != nil || device.state.volume != nil) {
                        if let volume { send(device, "set_volume", .number(Int(volume.rounded()))) }
                    }
                }
                HStack(spacing: 10) {
                    if device.can("volumeStep") {
                        stepButton(symbol: "minus", label: Text("Volume down"), device, "volume_down")
                        stepButton(symbol: "plus", label: Text("Volume up"), device, "volume_up")
                    }
                    if device.can("mute") {
                        if device.state.muted != true {
                            actionButton(String(localized: "Mute"), symbol: "speaker.slash", device, "mute")
                        }
                        if device.state.muted != false {
                            actionButton(String(localized: "Unmute"), symbol: "speaker.wave.2", device, "unmute",
                                         prominent: device.state.muted == true)
                        }
                    }
                }
            }
        }
        if device.can("brightness") {
            group(String(localized: "Brightness")) {
                slider(Binding(get: { brightness ?? Double(device.state.brightness ?? 50) }, set: { brightness = $0 }),
                       in: 1...100, known: brightness != nil || device.state.brightness != nil) {
                    if let brightness { send(device, "set_brightness", .number(max(1, Int(brightness.rounded())))) }
                }
            }
        }
        if device.can("channelStep") || (device.can("channels") && !device.channels.isEmpty) {
            group(String(localized: "Channels")) {
                if device.can("channelStep") {
                    HStack(spacing: 10) {
                        stepButton(symbol: "chevron.down", label: Text("Previous channel"), device, "channel_down")
                        stepButton(symbol: "chevron.up", label: Text("Next channel"), device, "channel_up")
                    }
                }
                if device.can("channels"), !device.channels.isEmpty {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 92), spacing: 8)], spacing: 8) {
                        ForEach(device.channels, id: \.self) { channel in
                            let current = device.state.channel.map { $0 == channel.number || $0 == channel.name } ?? false
                            Button {
                                send(device, "set_channel", .text(channel.number.isEmpty ? channel.name : channel.number))
                            } label: {
                                VStack(spacing: 1) {
                                    if !channel.number.isEmpty {
                                        Text(verbatim: channel.number)
                                            .font(.footnote.weight(.bold).monospacedDigit())
                                    }
                                    if !channel.name.isEmpty {
                                        Text(verbatim: channel.name)
                                            .font(.caption2)
                                            .lineLimit(1)
                                    }
                                }
                                .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.bordered)
                            .tint(current ? CoucouPalette.accent : Color.secondary)
                            .disabled(!enabled)
                        }
                    }
                }
            }
        }
        if device.can("apps"), !device.apps.isEmpty {
            group(String(localized: "Apps")) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 104), spacing: 8)], spacing: 8) {
                    ForEach(device.apps, id: \.self) { app in
                        Button {
                            send(device, "open_app", .text(app))
                        } label: {
                            Text(verbatim: app)
                                .font(.footnote.weight(.semibold))
                                .lineLimit(1)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.bordered)
                        .tint(device.state.app == app ? CoucouPalette.accent : Color.secondary)
                        .disabled(!enabled)
                    }
                }
            }
        }
    }

    private func group<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(verbatim: title)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .coucouGlass(.regular, cornerRadius: 20)
    }

    /// A percent slider: what is being set, else what the device said (a dash when it says nothing). The action goes
    /// once the slider is let go, not while it moves.
    private func slider(_ value: Binding<Double>, in range: ClosedRange<Double>, known: Bool,
                        release: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Slider(value: value, in: range, step: 1, onEditingChanged: { editing in
                if !editing { release() }
            })
            .tint(CoucouPalette.accent)
            .disabled(!enabled)
            Text(verbatim: known ? "\(Int(value.wrappedValue.rounded()))%" : "–")
                .font(.footnote.monospacedDigit().weight(.semibold))
                .frame(width: 44, alignment: .trailing)
        }
    }

    private func actionButton(_ title: String, symbol: String, _ device: PCHome.Device, _ action: String,
                              value: PCHomeValue? = nil, prominent: Bool = false) -> some View {
        Button {
            send(device, action, value)
        } label: {
            Label(title, systemImage: symbol)
                .font(.footnote.weight(.semibold))
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .tint(prominent ? CoucouPalette.accent : Color.secondary)
        .disabled(!enabled)
    }

    private func stepButton(symbol: String, label: Text, _ device: PCHome.Device, _ action: String) -> some View {
        Button {
            send(device, action)
        } label: {
            Image(systemName: symbol)
                .font(.body.weight(.semibold))
                .frame(maxWidth: .infinity, minHeight: 22)
        }
        .buttonStyle(.bordered)
        .disabled(!enabled)
        .accessibilityLabel(label)
    }

    private func send(_ device: PCHome.Device, _ action: String, _ value: PCHomeValue? = nil) {
        note = nil
        Task {
            let result = await pc.homeControl(device, action: action, value: value)
            if result.ok {
                failed = false
                note = result.detail.isEmpty ? nil : result.detail
                Haptics.success()
            } else if let text = PCRequestWords.explain(result, cap: "home", computer: pc.name) {
                failed = true
                note = text
                Haptics.warning()
            }
        }
    }
}
