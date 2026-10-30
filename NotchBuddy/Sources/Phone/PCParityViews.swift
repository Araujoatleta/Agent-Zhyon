import SwiftUI
import UIKit

// What the paired Windows or Linux computer shares besides its sessions, each behind a switch of its own there
// (docs/ANDROID_LINK.md), all read-only: the plan usage (`usage`), the files a session changed with their lines
// (`diffs`) and the service cards (`services`). Also what the screens of the assistant and of Mochi's home share: what
// they say while there is nothing to show, and a request's answer in words.

// MARK: - Nothing to show yet

/// A screen of the computer's with nothing to show: no computer, no link, or its switch off there (and which one).
struct PCNotice: View {
    let symbol: String
    let title: String
    var text = ""
    /// Waiting for the computer: a spinner instead of the symbol.
    var waiting = false
    /// "Try again now", while the link is down.
    var retry: (@MainActor () -> Void)? = nil

    var body: some View {
        VStack(spacing: 12) {
            if waiting {
                ProgressView()
                    .controlSize(.large)
                    .frame(height: 40)
            } else {
                Image(systemName: symbol)
                    .font(.system(size: 34, weight: .semibold))
                    .foregroundStyle(CoucouPalette.accent)
                    .frame(height: 40)
            }
            Text(verbatim: title)
                .font(.headline)
                .multilineTextAlignment(.center)
            if !text.isEmpty {
                Text(verbatim: text)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            if let retry {
                Button("Try again now") { retry() }
                    .borderedProminentButton()
                    .tint(CoucouPalette.accent)
                    .padding(.top, 4)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity)
        .coucouGlass(.regular, cornerRadius: 26)
    }
}

extension PCNotice {
    /// Why the screen of capability `cap` has nothing yet. `off`: what to turn on, in Settings → Phone on the computer.
    @MainActor static func missing(_ cap: String, off: String) -> PCNotice {
        let pc = PCLink.shared
        guard pc.isPaired else {
            return PCNotice(symbol: "desktopcomputer", title: String(localized: "No computer paired"),
                            text: String(localized: "Pair your Windows or Linux computer first, in Settings → Windows or Linux computer."))
        }
        switch pc.status {
        case .connected:
            if pc.offers(cap) {
                return PCNotice(symbol: "hourglass", title: String(localized: "Asking \(pc.name)…"), waiting: true)
            }
            return PCNotice(symbol: "switch.2", title: String(localized: "Turned off on \(pc.name)"), text: off)
        case .connecting, .searching:
            return PCNotice(symbol: "antenna.radiowaves.left.and.right", title: pc.status.text, waiting: true)
        case .refused:
            return PCNotice(symbol: "exclamationmark.lock", title: String(localized: "Not connected"), text: pc.status.text)
        case .unpaired, .offline:
            return PCNotice(symbol: "wifi.slash", title: String(localized: "Not connected"), text: pc.status.text,
                            retry: { PCLink.shared.reconnectIfNeeded() })
        }
    }
}

/// An assistant's or a home request's answer, in words. Nil when there is nothing to say (Face ID cancelled).
enum PCRequestWords {
    /// `cap`: "assistant" or "home". `computer`: its name.
    static func explain(_ result: PCRequestResult, cap: String, computer: String) -> String? {
        guard !result.ok else { return nil }
        let home = cap == "home"
        switch result.reason {
        case "declined":
            return nil
        case "off":
            return home
                ? String(localized: "Mochi's home is off for the phone on \(computer): turn on Settings → Mochi's home, and Settings → Phone → “Let the phone use Mochi's home” there.")
                : String(localized: "Changing the assistant from the phone is off on \(computer): turn on Settings → Phone → “Let the phone change the assistant (name, tone, brain, skills)” there.")
        case "invalid":
            return home ? String(localized: "The device can't do this.") : String(localized: "The computer can't take this change.")
        case "not_installed":
            return String(localized: "Coucou doesn't install and run Hermes on \(computer) yet: set it up there first, in Settings → Hermes Agent.")
        case "not_ready":
            return String(localized: "\(computer) has no key or server address for that brain yet: add it there, in Settings → Hermes Agent.")
        case "busy":
            return home ? String(localized: "Another action is still running. Try again in a moment.")
                : String(localized: "\(computer) is still busy with the assistant. Try again in a moment.")
        case "rate":
            return home ? String(localized: "Too many actions. Try again in a few minutes.")
                : String(localized: "Too many changes. Try again in a few minutes.")
        case "unknown":
            return String(localized: "This device or routine isn't shared with the phone any more.")
        case "confirm":
            return String(localized: "It asks first: confirm with Face ID to go on.")
        case "offline":
            return String(localized: "Can't reach \(computer). Is Coucou open on it?")
        case "timeout":
            return String(localized: "\(computer) didn't answer in time.")
        default:
            // `failed`: the computer's own words for what went wrong (a device's answer, for the home).
            return result.message.isEmpty ? String(localized: "It didn't go through on \(computer).") : result.message
        }
    }
}

// MARK: - Plan usage

/// How much of the Claude and Codex plans is used, as the island on the computer shows it (capability `usage`): one
/// gauge per window. Nothing when the computer has nothing to say.
struct PCUsageCard: View {
    private var pc: PCLink { PCLink.shared }

    var body: some View {
        if let usage = pc.model.usage, !usage.isEmpty {
            // A window whose reset has passed counts as 0: looked at again every minute while on screen.
            TimelineView(.everyMinute) { context in
                VStack(alignment: .leading, spacing: 14) {
                    Label("Plan usage", systemImage: "speedometer")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                    if let claude = usage.claude {
                        plan("Claude", claude, now: context.date)
                    }
                    if let codex = usage.codex {
                        plan("Codex", codex, now: context.date)
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .coucouGlass(.regular, cornerRadius: 22)
            }
        }
    }

    private func plan(_ title: String, _ plan: PCUsage.Plan, now: Date) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(verbatim: title)
                    .font(.headline)
                if !plan.plan.isEmpty {
                    Text(verbatim: plan.plan.capitalized)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 8)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(CoucouPalette.accent.opacity(0.22)))
                }
                Spacer(minLength: 8)
                if let credits = plan.resetCredits {
                    Text("\(credits) free resets")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            HStack(spacing: 16) {
                if let window = plan.fiveHour {
                    PCUsageGauge(title: String(localized: "5 hours"), window: window, now: now)
                }
                if let window = plan.sevenDay {
                    PCUsageGauge(title: String(localized: "7 days"), window: window, now: now)
                }
            }
        }
    }
}

/// One window of a plan: a ring with its percent, and when it resets.
private struct PCUsageGauge: View {
    let title: String
    let window: PCUsage.Window
    let now: Date

    private var percent: Int { window.percent(at: now) }
    private var color: Color {
        percent >= 90 ? CoucouPalette.danger : percent >= 70 ? CoucouPalette.warn : CoucouPalette.ok
    }

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle()
                    .stroke(Color.white.opacity(0.1), lineWidth: 6)
                Circle()
                    .trim(from: 0, to: CGFloat(percent) / 100)
                    .stroke(color, style: StrokeStyle(lineWidth: 6, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(verbatim: "\(percent)%")
                    .font(.caption.weight(.bold).monospacedDigit())
            }
            .frame(width: 52, height: 52)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: title)
                    .font(.subheadline.weight(.semibold))
                if now < window.resetsAt {
                    Text("Resets \(window.resetsAt.formatted(.relative(presentation: .named)))")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - The files a session changed

/// The files a session on the computer changed (capability `diffs`), the newest first, each opening its lines.
/// Nothing when it changed none; a line on the switch to turn on when the computer does not share them.
struct PCSessionFilesSection: View {
    let pillID: String
    @State private var open: PCFileChange?

    private var pc: PCLink { PCLink.shared }
    private var files: [PCFileChange] { pc.model.sessions.first { $0.pillId == pillID }?.files ?? [] }

    var body: some View {
        if pc.offers("diffs") {
            if !files.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Label("Files changed", systemImage: "doc.on.doc")
                        .font(.subheadline.weight(.semibold))
                    VStack(spacing: 0) {
                        ForEach(files.reversed()) { file in
                            Button {
                                open = file
                            } label: {
                                row(file)
                            }
                            .buttonStyle(PressableButtonStyle())
                            if file.id != files.first?.id {
                                Rectangle().fill(CoucouPalette.hairline).frame(height: 1)
                            }
                        }
                    }
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .coucouGlass(.regular, cornerRadius: 20)
                .sheet(item: $open) { file in
                    PCDiffSheet(pillID: pillID, file: file)
                }
            }
        } else if pc.status == .connected {
            Label {
                Text("To see the files it changed, turn on Settings → Phone → “Show the files an agent changed on the phone” on \(pc.name).")
            } icon: {
                Image(systemName: "doc.badge.ellipsis")
            }
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
    }

    private func row(_ file: PCFileChange) -> some View {
        HStack(spacing: 10) {
            Image(systemName: file.isNew ? "doc.badge.plus" : "doc.text")
                .foregroundStyle(.secondary)
                .frame(width: 22)
            Text(verbatim: file.name)
                .font(.callout.monospaced())
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            Group {
                if file.tooLarge {
                    Text("Large")
                        .foregroundStyle(.secondary)
                }
                Text(verbatim: "+\(file.added)")
                    .foregroundStyle(CoucouPalette.ok)
                Text(verbatim: "−\(file.removed)")
                    .foregroundStyle(CoucouPalette.danger)
            }
            .font(.footnote.monospacedDigit().weight(.semibold))
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 9)
        .contentShape(Rectangle())
    }
}

/// A file's lines as the computer sends them (at most 200), held while this sheet is open and forgotten when it
/// closes. Drawn by the FileDiffView of a Mac turn's files, with its full screen for long lines.
struct PCDiffSheet: View {
    let pillID: String
    let file: PCFileChange
    @State private var loading = true
    @Environment(\.dismiss) private var dismiss

    private var pc: PCLink { PCLink.shared }

    /// The lines that came for this file.
    private var diff: PCDiff? {
        guard let diff = pc.model.diff, diff.pillId == pillID, diff.fileId == file.id else { return nil }
        return diff
    }

    var body: some View {
        NavigationStack {
            content
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Done") { dismiss() }
                    }
                }
        }
        .task { await load() }
        .onDisappear { pc.closeDiff(pillID: pillID, fileID: file.id) }
    }

    @ViewBuilder private var content: some View {
        if let diff, diff.gone {
            notice(symbol: "clock.arrow.circlepath", String(localized: "The computer no longer has this change."))
        } else if file.tooLarge || diff?.tooLarge == true {
            notice(symbol: "doc.text.magnifyingglass",
                   String(localized: "Too large to show line by line: \(file.added) added, \(file.removed) removed."))
        } else if let diff, !diff.received.isEmpty {
            FileDiffView(file: Self.turnFile(diff, file: file))
        } else if loading {
            ProgressView("Asking \(pc.name)…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background { DeepBackdrop() }
                .navigationTitle(file.name)
                .navigationBarTitleDisplayMode(.inline)
        } else {
            VStack(spacing: 16) {
                notice(symbol: "wifi.exclamationmark", String(localized: "Couldn't get the lines from \(pc.name)."))
                Button("Try again") {
                    Task { await load() }
                }
                .coucouGlassButton(prominent: true)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background { DeepBackdrop() }
        }
    }

    private func notice(symbol: String, _ text: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(verbatim: text)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background { DeepBackdrop() }
        .navigationTitle(file.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func load() async {
        // A change too large to show has no lines on the computer either.
        guard !file.tooLarge else {
            loading = false
            return
        }
        loading = true
        _ = await pc.loadDiff(pillID: pillID, fileID: file.id)
        loading = false
    }

    /// The computer's lines as a Mac turn's file: a hunk's start is the gap between two hunks, none before the first.
    private static func turnFile(_ diff: PCDiff, file: PCFileChange) -> TurnFile {
        var lines: [TurnDiffLine] = []
        for line in diff.lines {
            switch line.kind {
            case .added: lines.append(TurnDiffLine(kind: .added, text: line.text))
            case .removed: lines.append(TurnDiffLine(kind: .removed, text: line.text))
            case .context: lines.append(TurnDiffLine(kind: .context, text: line.text))
            case .hunk: if !lines.isEmpty { lines.append(TurnDiffLine(kind: .gap, text: "")) }
            }
        }
        return TurnFile(path: diff.name.isEmpty ? file.name : diff.name, added: diff.added, removed: diff.removed,
                        isNew: file.isNew, lines: lines, truncated: diff.truncated || !diff.complete)
    }
}

// MARK: - Service cards

/// The cards of the services the computer shares (capability `services`, one tick per service there), read-only as
/// their pills show them. Nothing when it shares none.
struct PCServicesSection: View {
    private var pc: PCLink { PCLink.shared }

    var body: some View {
        if !pc.model.services.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text("Services on \(pc.name)")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.leading, 4)
                CoucouGlassGroup(spacing: 0) {
                    VStack(spacing: 10) {
                        ForEach(pc.model.services) { card in
                            PCServiceCardView(card: card)
                        }
                    }
                }
            }
        }
    }
}

/// One service: its Mochi in its own color, its headline and its few lines.
private struct PCServiceCardView: View {
    let card: PCServiceCard

    var body: some View {
        let pill = PillCatalog.definition(for: card.id)
        let hex = pill?.color ?? "#C0C4CC"
        HStack(alignment: .top, spacing: 12) {
            MochiStill(state: .idle, bodyHex: hex, showBadge: false)
                .frame(width: 34, height: 34)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(verbatim: card.title.isEmpty ? (pill?.name ?? card.id) : card.title)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    if !card.headline.isEmpty {
                        Text(verbatim: card.headline)
                            .font(.subheadline.weight(.semibold).monospacedDigit())
                            .lineLimit(1)
                    }
                }
                if !card.reason.isEmpty {
                    Text(verbatim: card.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                ForEach(Array(card.items.enumerated()), id: \.offset) { _, item in
                    HStack(spacing: 8) {
                        Text(verbatim: item.label)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(verbatim: item.detail)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .font(.caption)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .coucouGlass(.regular, cornerRadius: 20, tint: Color(hex: hex).opacity(0.5))
        .accessibilityElement(children: .combine)
    }
}
