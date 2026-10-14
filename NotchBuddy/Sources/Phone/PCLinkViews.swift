import SwiftUI
import UIKit

// The screens of the link to a Windows or Linux computer (PCLink): pairing with the code Coucou shows there, the
// link's state and, in a sideloaded build that has no Mac to talk to, the first-launch guide. The chat with Mochi on
// the computer is PCChatView.swift. In the app's deep dark glass (CoucouGlass.swift); the Mac's screens are unchanged.

extension PCPairing: Identifiable {
    var id: String { computer.fingerprint + token }
}

extension SessionItem {
    /// Where the agent runs, for "sent to …" and Face ID's reason: the Mac, or the paired computer by its name.
    var hostName: String {
        guard fromComputer else { return String(localized: "your Mac") }
        return macName.isEmpty ? String(localized: "your computer") : macName
    }
}

extension PCLink.Status {
    var text: String {
        switch self {
        case .unpaired: String(localized: "Not paired")
        case .connecting: String(localized: "Connecting…")
        case .searching: String(localized: "Looking for it on the network…")
        case .connected: String(localized: "Connected")
        case .offline(let reason): reason.isEmpty ? String(localized: "Not connected") : reason
        case .refused: String(localized: "It refused this iPhone: pair again with the code it shows now.")
        }
    }

    var color: Color {
        switch self {
        case .connected: CoucouPalette.ok
        case .connecting, .searching: .secondary
        case .refused: CoucouPalette.danger
        case .unpaired, .offline: .orange
        }
    }

    /// One or two words, for the Settings row.
    var shortText: String {
        switch self {
        case .unpaired: String(localized: "Not paired")
        case .connecting, .searching: String(localized: "Connecting…")
        case .connected: String(localized: "Connected")
        case .offline: String(localized: "Not connected")
        case .refused: String(localized: "Pair again")
        }
    }

    var mochi: BotState {
        switch self {
        case .connected: .idle
        case .connecting, .searching: .searching
        case .refused: .error
        case .unpaired, .offline: .sleeping
        }
    }
}

/// Settings → Windows or Linux computer.
struct PCLinkSettingsView: View {
    let pc: PCLink
    @State private var link = ""
    @State private var pending: PCPairing?
    @State private var error: String?
    @State private var scanning = false

    var body: some View {
        List {
            Section {
                HStack(spacing: 14) {
                    MochiLive(state: pc.status.mochi)
                        .padding(6)
                        .frame(width: 56, height: 56)
                        .background(Color.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(pc.computer.map { $0.name.isEmpty ? String(localized: "Your computer") : $0.name }
                             ?? String(localized: "No computer yet"))
                            .font(.headline)
                        Text(pc.status.text)
                            .font(.subheadline)
                            .foregroundStyle(pc.status.color)
                        if pc.status == .connected, pc.viaOutside {
                            Text("Over the internet, directly to your computer")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .padding(.vertical, 4)
                if pc.isPaired, ![.connected, .connecting, .searching, .refused].contains(pc.status) {
                    Button("Try again now") { pc.reconnectIfNeeded() }
                }
            }
            .listRowBackground(GlassRowBackground())

            if let computer = pc.computer {
                Section {
                    LabeledContent("Address", value: "\(computer.host):\(computer.port)")
                    LabeledContent("Certificate") {
                        Text(computer.shortFingerprint).font(.callout.monospaced())
                    }
                    if !pc.model.os.isEmpty {
                        LabeledContent("System", value: pc.model.os.capitalized)
                    }
                    if let away = awayFromHome(computer) {
                        LabeledContent("Away from home", value: away)
                    }
                } header: {
                    Text("Link")
                } footer: {
                    Text("Only this computer is accepted: its certificate is checked at every connection. If its address changes, Coucou finds it again on the local network.")
                }
                .listRowBackground(GlassRowBackground())

                if pc.status == .connected {
                    Section {
                        capability("Sessions and permission requests", on: true)
                        capability("Chat with Mochi", on: pc.model.chatAllowed)
                        capability("Mochi's hands from the phone", on: pc.model.handsAllowed)
                        capability("Answer Claude Code's questions", on: pc.model.caps.contains("answers"))
                        capability("Session details", on: pc.model.caps.contains("details"))
                        capability("Files an agent changed", on: pc.model.caps.contains("diffs"))
                        capability("Plan usage", on: pc.model.caps.contains("usage"))
                        capability("Service cards", on: pc.model.caps.contains("services"))
                        capability("Change the assistant", on: pc.model.caps.contains("assistant"))
                        capability("Mochi's home", on: pc.model.caps.contains("home"))
                    } header: {
                        Text("Allowed by the computer")
                    } footer: {
                        Text("Each one is a switch in Coucou's Settings → Phone on the computer, off until you turn it on there. Mochi's hands also need Settings → Mochi's hands, and Mochi's home needs Settings → Mochi's home.")
                    }
                    .listRowBackground(GlassRowBackground())
                }
            }

            Section {
                Button {
                    scanning = true
                } label: {
                    Label("Scan the code", systemImage: "qrcode.viewfinder")
                }
                TextField("coucou://pair?…", text: $link, axis: .vertical)
                    .font(.callout.monospaced())
                    .lineLimit(1...4)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Paste the link") {
                    link = UIPasteboard.general.string ?? ""
                    review()
                }
                Button("Pair") { review() }
                    .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if let error {
                    Text(error).font(.footnote).foregroundStyle(CoucouPalette.danger)
                }
            } header: {
                Text(pc.isPaired ? "Pair again" : "Pair")
            } footer: {
                Text("On the computer: Coucou → Settings → Phone (Android and iPhone) → turn the link on and show the pairing code, then scan it here. Or copy the link there and paste it here. (The iPhone's Camera app can't open Coucou codes.)")
            }
            .listRowBackground(GlassRowBackground())

            if pc.isPaired {
                Section {
                    Button("Forget this computer", role: .destructive) {
                        pc.unpair()
                        Haptics.impact()
                    }
                }
                .listRowBackground(GlassRowBackground())
            }

            Section {
                Text("Requests reach the iPhone while Coucou is open, and for about half a minute after you leave it: iOS then pauses the app, and only Apple's push could wake it, which a direct link has not. Coucou connects again as soon as you open it.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            .listRowBackground(GlassRowBackground())
        }
        .listStyle(.insetGrouped)
        .deepBackdrop()
        .navigationTitle("Windows or Linux computer")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $pending) { pairing in
            PCPairingConfirm(pairing: pairing, pc: pc) { link = "" }
        }
        .sheet(isPresented: $scanning) {
            PCScanSheet(pc: pc) { link = "" }
        }
    }

    private func review() {
        error = nil
        guard let pairing = PCPairing.parse(link) else {
            error = String(localized: "This isn't a Coucou pairing link. It starts with coucou://pair?v=1")
            Haptics.warning()
            return
        }
        pending = pairing
    }

    /// What the iPhone can do away from home (windows/src-tauri/src/phone_link/remote.rs). Nil when the computer
    /// has never said: an older Coucou there.
    private func awayFromHome(_ computer: PCComputer) -> String? {
        if pc.status == .connected, pc.viaOutside { return String(localized: "Connected") }
        if !computer.remote.isEmpty {
            return computer.remote.count == 1 ? String(localized: "Ready, 1 address")
                : String(localized: "Ready, \(computer.remote.count) addresses")
        }
        if pc.model.caps.contains("remote") { return String(localized: "Off on the computer") }
        return nil
    }

    private func capability(_ title: LocalizedStringKey, on: Bool) -> some View {
        LabeledContent(title) {
            Image(systemName: on ? "checkmark.circle.fill" : "minus.circle")
                .foregroundStyle(on ? CoucouPalette.ok : Color.secondary)
        }
    }
}

/// Before pairing: which computer, where, and what pairing means. Nothing is paired without this tap.
struct PCPairingConfirm: View {
    let pairing: PCPairing
    let pc: PCLink
    var paired: () -> Void = {}
    @Environment(\.dismiss) private var dismiss
    @State private var failed = false

    private var replaces: Bool {
        guard let current = pc.computer else { return false }
        return current.fingerprint != pairing.computer.fingerprint
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                MochiLive(state: .question)
                    .frame(width: 72, height: 72)
                Text(pairing.computer.name.isEmpty ? "Pair with this computer?" : "Pair with \(pairing.computer.name)?")
                    .font(.title2.weight(.bold))
                Text("This iPhone will show the agents running on it and answer their permission requests, on your local network only. Allow always asks for Face ID.")
                    .foregroundStyle(.secondary)
                VStack(spacing: 8) {
                    LabeledContent("Address", value: "\(pairing.computer.host):\(pairing.computer.port)")
                    LabeledContent("Certificate") {
                        Text(pairing.computer.shortFingerprint).font(.callout.monospaced())
                    }
                }
                .font(.callout)
                .padding(14)
                .glassCard(cornerRadius: 16)
                if replaces {
                    Text("It replaces \(pc.name), paired before.")
                        .font(.footnote)
                        .foregroundStyle(.orange)
                }
                if failed {
                    Text(pc.lastError ?? String(localized: "Couldn't pair."))
                        .font(.footnote)
                        .foregroundStyle(CoucouPalette.danger)
                }
            }
            .padding(24)
        }
        .background { DeepBackdrop() }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                Button {
                    if pc.pair(pairing) {
                        Haptics.success()
                        paired()
                        dismiss()
                    } else {
                        failed = true
                        Haptics.error()
                    }
                } label: {
                    Text("Pair").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .coucouGlassButton(prominent: true)
                .controlSize(.large)
                Button("Cancel", role: .cancel) { dismiss() }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
        }
        .presentationDetents([.large])
        .preferredColorScheme(.dark)
    }
}

/// The first launch of a build with no Mac to talk to (sideloaded): how to pair with Coucou on Windows or Linux.
struct PCOnboardingView: View {
    let pc: PCLink
    @Environment(\.dismiss) private var dismiss
    @State private var pending: PCPairing?
    @State private var error: String?
    @State private var scanning = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                VStack(alignment: .leading, spacing: 12) {
                    MochiLive(state: .finished)
                        .frame(width: 88, height: 88)
                    Text("Your agents, in your pocket")
                        .font(.largeTitle.weight(.bold))
                    Text("Coucou on iPhone shows what Claude Code and Mochi are doing on your Windows or Linux computer, and answers their requests. Three things to set up, once.")
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                CoucouGlassGroup(spacing: 12) {
                    VStack(alignment: .leading, spacing: 22) {
                        step(1, "Coucou on the computer",
                             "Install Coucou for Windows or Linux and set up its Claude Code hooks.",
                             icon: "desktopcomputer")
                        step(2, "Turn on the phone link",
                             "On the computer: Coucou Settings → Phone (Android and iPhone) → turn it on and show the pairing code. Same Wi-Fi as this iPhone.",
                             icon: "switch.2")
                        step(3, "Scan the code",
                             "Tap Scan the pairing code below and point the camera at the computer's screen. Or copy the link on the computer and paste it here.",
                             icon: "qrcode.viewfinder")
                    }
                }
                if let error {
                    Text(error).font(.footnote).foregroundStyle(CoucouPalette.danger)
                }
            }
            .padding(24)
        }
        .background { DeepBackdrop() }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                Button {
                    scanning = true
                } label: {
                    Label("Scan the pairing code", systemImage: "qrcode.viewfinder")
                        .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .coucouGlassButton(prominent: true)
                .controlSize(.large)
                HStack(spacing: 24) {
                    Button("Paste the link") {
                        guard let pairing = PCPairing.parse(UIPasteboard.general.string ?? "") else {
                            error = String(localized: "No pairing link to paste. Copy it on the computer first (Settings → Phone).")
                            Haptics.warning()
                            return
                        }
                        pending = pairing
                    }
                    Button("Later") { dismiss() }
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
        }
        .sheet(item: $pending) { pairing in
            PCPairingConfirm(pairing: pairing, pc: pc) { dismiss() }
        }
        .sheet(isPresented: $scanning) {
            PCScanSheet(pc: pc) { dismiss() }
        }
        .preferredColorScheme(.dark)
    }

    private func step(_ number: Int, _ title: LocalizedStringKey, _ text: LocalizedStringKey, icon: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 44, height: 44)
                .coucouGlass(.regular, cornerRadius: 12)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 0) {
                    Text(verbatim: "\(number). ")
                    Text(title)
                }
                .font(.headline)
                Text(text).font(.callout).foregroundStyle(.secondary)
            }
        }
    }
}
