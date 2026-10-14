import SwiftUI
import UIKit

/// Settings and About: the computer and its assistant, plan usage, how the
/// iPhone is linked, notifications, the look, the links Apple and people look
/// for, and the setup guide again. A tab of its own with a Windows or Linux
/// computer; behind the Agents tab's gear with a Mac only.
struct AboutView: View {
    let link: PhoneLink
    @State private var showGuide = false
    @AppStorage(PhoneSettings.notifyDoneKey) private var notifyDone = true
    @AppStorage(PhoneSettings.mochiSoundsKey) private var mochiSounds = true
    @AppStorage(PhoneSettings.quietHoursKey) private var quietHours = false
    @AppStorage(PhoneSettings.quietFromKey) private var quietFrom = 22 * 60
    @AppStorage(PhoneSettings.quietToKey) private var quietTo = 8 * 60
    @AppStorage(SpotlightIndex.enabledKey) private var spotlight = true
    @State private var currentIcon = AppIconChoice.current

    private var pc: PCLink { PCLink.shared }

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(short) (\(build))"
    }

    var body: some View {
        List {
            Section {
                HStack(spacing: 14) {
                    MochiLive(state: .idle)
                        .padding(6)
                        .frame(width: 64, height: 64)
                        .coucouGlass(.regular, cornerRadius: 16)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: "Coucou").font(.title3.weight(.semibold))
                        Text("Version \(version)").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }
            .listRowBackground(Color.clear)

            Section {
                NavigationLink {
                    PCLinkSettingsView(pc: pc)
                } label: {
                    LabeledContent {
                        Text(pc.status.shortText)
                            .foregroundStyle(pc.status == .connected ? CoucouPalette.ok : Color.secondary)
                    } label: {
                        Label("Windows or Linux computer", systemImage: "desktopcomputer")
                    }
                }
                // The assistant on the computer (Hermes Agent): name, tone, brain, skills, its gateway.
                if pc.isPaired {
                    NavigationLink {
                        PCAssistantSettingsView()
                    } label: {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Assistant")
                                Text("Name, tone, brain and skills of the assistant on \(pc.name)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "sparkles")
                        }
                    }
                }
            } header: {
                Text("Computer")
            }
            .listRowBackground(GlassRowBackground())

            // How much of the Claude and Codex plans is used, when the computer shares it (a List row would stay
            // as an empty gap otherwise).
            if pc.offers("usage"), pc.model.usage?.isEmpty == false {
                Section {
                    PCUsageCard()
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
            }

            Section("Connection") {
                if PhoneLink.cloudAvailable {
                    LabeledContent("iCloud") {
                        Label(iCloudText, systemImage: link.status == .ready ? "checkmark.icloud" : "exclamationmark.icloud")
                            .foregroundStyle(link.status == .ready ? CoucouPalette.ok : .orange)
                    }
                }
                LabeledContent("Notifications") {
                    Text(link.notificationsAllowed == false ? "Off" : "On")
                        .foregroundStyle(link.notificationsAllowed == false ? .orange : .secondary)
                }
                LabeledContent("Live Activities") {
                    Text(LiveActivityLink.shared.activitiesEnabled ? "On" : "Off")
                        .foregroundStyle(LiveActivityLink.shared.activitiesEnabled ? Color.secondary : Color.orange)
                }
                if link.notificationsAllowed == false || !LiveActivityLink.shared.activitiesEnabled {
                    Button("Open iPhone Settings") {
                        if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                    }
                }
                Button(PhoneLink.cloudAvailable ? "How to connect your Mac" : "How to connect your computer") { showGuide = true }
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Toggle("When an agent finishes or fails", isOn: $notifyDone)
                Toggle("Mochi's sounds", isOn: $mochiSounds)
                    .onChange(of: mochiSounds) { link.soundsChanged() }
                Toggle("Quiet hours", isOn: $quietHours)
                if quietHours {
                    DatePicker("From", selection: time($quietFrom), displayedComponents: .hourAndMinute)
                    DatePicker("To", selection: time($quietTo), displayedComponents: .hourAndMinute)
                }
            } header: {
                Text("Notifications")
            } footer: {
                Text(quietHours
                     ? "In the quiet hours, only what waits on you (a command to allow, a question) makes a sound. The rest arrives silently."
                     : "Approvals and questions always notify you. Mochi's sounds are the ones he makes in your Mac's notch.")
            }
            .listRowBackground(GlassRowBackground())

            Section {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 14) {
                        ForEach(AppIconChoice.allCases) { choice in
                            Button {
                                AppIconChoice.apply(choice)
                                currentIcon = choice
                            } label: {
                                VStack(spacing: 6) {
                                    Image(choice.previewName)
                                        .resizable()
                                        .frame(width: 58, height: 58)
                                        .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                                        .overlay(RoundedRectangle(cornerRadius: 13, style: .continuous)
                                            .strokeBorder(currentIcon == choice ? CoucouPalette.accent : .clear, lineWidth: 2.5)
                                            .padding(-4))
                                    Text(choice.title).font(.caption2).foregroundStyle(.secondary)
                                }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 4)
                }
                Toggle("Find my turns in Spotlight", isOn: $spotlight)
                    .onChange(of: spotlight) { _, on in if !on { SpotlightIndex.removeAll() } }
            } header: {
                Text("Look")
            } footer: {
                Text("The Home Screen icon follows your choice; dark and tinted icons follow the Home Screen style. Spotlight's index stays on this iPhone.")
            }
            .listRowBackground(GlassRowBackground())

            // With the computer's tabs, what the Mac-only layout has as tabs of its own.
            if CoucouTab.computerLayout {
                Section("Activity") {
                    NavigationLink {
                        HistoryView(link: link)
                    } label: {
                        Label("Decisions you took", systemImage: "clock.arrow.circlepath")
                    }
                    if PhoneLink.cloudAvailable {
                        NavigationLink {
                            SearchScreen(link: link)
                        } label: {
                            Label("Search your Mac's turns", systemImage: "magnifyingglass")
                        }
                    }
                }
                .listRowBackground(GlassRowBackground())
            }

            Section("Coucou") {
                Link("Website", destination: URL(string: "https://louis-cfm.github.io/coucou/")!)
                Link("Support", destination: URL(string: "https://louis-cfm.github.io/coucou/support.html")!)
                Link("Privacy Policy", destination: URL(string: "https://louis-cfm.github.io/coucou/privacy.html")!)
                Link("Terms", destination: URL(string: "https://louis-cfm.github.io/coucou/terms.html")!)
            }
            .listRowBackground(GlassRowBackground())

            Section {
                Text(PhoneLink.cloudAvailable
                     ? "No account, no analytics, no ads. Your sessions travel through your own private iCloud, encrypted with your keys."
                     : "No account, no analytics, no ads. Your sessions come straight from your computer, encrypted, on your local network.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            .listRowBackground(GlassRowBackground())
        }
        .listStyle(.insetGrouped)
        .deepBackdrop()
        .navigationTitle("Settings")
        .sheet(isPresented: $showGuide) {
            if PhoneLink.cloudAvailable {
                OnboardingView()
            } else {
                PCOnboardingView(pc: pc)
            }
        }
    }

    /// Minutes after midnight, as a time for the pickers.
    private func time(_ minutes: Binding<Int>) -> Binding<Date> {
        Binding {
            Calendar.current.startOfDay(for: .now).addingTimeInterval(TimeInterval(minutes.wrappedValue * 60))
        } set: { date in
            let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
            minutes.wrappedValue = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
        }
    }

    private var iCloudText: String {
        switch link.status {
        case .ready: String(localized: "Linked")
        case .starting: String(localized: "Connecting…")
        case .zoneMissing: String(localized: "Waiting for your Mac")
        case .noAccount: String(localized: "Not signed in")
        case .failed: String(localized: "Can't reach iCloud")
        }
    }
}

/// First launch: what Coucou needs on the Mac to show anything here.
struct OnboardingView: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                VStack(alignment: .leading, spacing: 12) {
                    MochiLive(state: .finished)
                        .frame(width: 88, height: 88)
                    Text("Your agents, in your pocket")
                        .font(.largeTitle.weight(.bold))
                    Text("Coucou on iPhone shows what Claude Code, Cursor, Codex and your services are doing on your Mac. Three things to set up, once.")
                        .font(.body)
                        .foregroundStyle(.secondary)
                }
                CoucouGlassGroup(spacing: 12) {
                    VStack(alignment: .leading, spacing: 22) {
                        step(1, "Coucou on your Mac",
                             "Install Coucou for Mac (Mac App Store or the website) and set up its Claude Code hooks.",
                             icon: "laptopcomputer")
                        step(2, "Turn on the iPhone switch",
                             "On the Mac: Coucou Settings → General → iPhone → \"Show my agent sessions on my iPhone\".",
                             icon: "switch.2")
                        step(3, "Same Apple Account",
                             "Sign in to iCloud with the same Apple Account on both. Your sessions go through your private iCloud, encrypted.",
                             icon: "icloud")
                    }
                }
                VStack(alignment: .leading, spacing: 6) {
                    Text("Then, from here").font(.headline)
                    Text("• Allow or deny a command, right from the Lock Screen\n• Answer Claude's questions\n• See what Claude did, file by file\n• Write or dictate the next instruction, or ask Siri (GitHub version on the Mac)\n• Mochi in your Dynamic Island when your Mac is locked")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassCard(cornerRadius: 18)
            }
            .padding(24)
        }
        .background { DeepBackdrop() }
        .safeAreaInset(edge: .bottom) {
            Button {
                dismiss()
            } label: {
                Text("Got it").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
            }
            .coucouGlassButton(prominent: true)
            .controlSize(.large)
            .padding(.horizontal, 24)
            .padding(.bottom, 12)
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

/// Mochi's colors for the Home Screen icon (alternate app icons).
enum AppIconChoice: String, CaseIterable, Identifiable {
    case orange = "", blue = "AppIcon-Blue", green = "AppIcon-Green", purple = "AppIcon-Purple",
         pink = "AppIcon-Pink", graphite = "AppIcon-Graphite"

    var id: String { rawValue }

    /// Shown under the icon, in the iPhone's language.
    var title: LocalizedStringKey {
        switch self {
        case .orange: "Orange"
        case .blue: "Blue"
        case .green: "Green"
        case .purple: "Purple"
        case .pink: "Pink"
        case .graphite: "Graphite"
        }
    }

    /// The preview's image in the asset catalog (English names, whatever the language).
    var previewName: String {
        switch self {
        case .orange: "IconPreview-Orange"
        case .blue: "IconPreview-Blue"
        case .green: "IconPreview-Green"
        case .purple: "IconPreview-Purple"
        case .pink: "IconPreview-Pink"
        case .graphite: "IconPreview-Graphite"
        }
    }

    @MainActor static var current: AppIconChoice {
        AppIconChoice(rawValue: UIApplication.shared.alternateIconName ?? "") ?? .orange
    }

    @MainActor static func apply(_ choice: AppIconChoice) {
        guard UIApplication.shared.supportsAlternateIcons else { return }
        UIApplication.shared.setAlternateIconName(choice == .orange ? nil : choice.rawValue)
    }
}
