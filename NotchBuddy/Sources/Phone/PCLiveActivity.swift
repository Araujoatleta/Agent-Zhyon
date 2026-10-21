import ActivityKit
import Foundation

/// Mochi in the Dynamic Island and on the Lock Screen for the Windows or Linux computer (PCLink).
///
/// The Mac starts and updates its Live Activity through Apple's push (LiveActivityLink), which a build signed with a
/// free Apple Account — the sideload build — cannot receive. So here the iPhone starts the activity itself, while
/// Coucou is open and an agent of the computer is working or waiting, and keeps it current from what the link brings;
/// Allow and Deny on it answer through the link too (PhoneLink.decide hands the computer's requests to PCLink).
/// iOS suspends Coucou about half a minute after it leaves the screen: from then nothing can update the activity, so
/// it says it is out of date (its staleDate) until Coucou opens again. It ends a little after nothing is going on.
@MainActor
final class PCLiveActivity {
    static let shared = PCLiveActivity()

    /// The activity on screen, by its id: ActivityKit's own objects stay away from the main actor (`send`).
    private var activityID: String?
    private var shown: MochiActivityState?
    /// When the current run of work began (Unix seconds): the activity counts the time from it.
    private var since: Int?

    /// How long what the activity shows counts as current. On screen the link is up and every change comes at once;
    /// away, only for the half minute iOS lets the link live.
    private static let freshOnScreen: TimeInterval = 15 * 60
    private static let freshAway: TimeInterval = 40

    private init() {
        // An activity of ours still there from before Coucou last stopped: take it up again rather than add one.
        if let activity = Activity<MochiActivityAttributes>.activities.first(where: { $0.content.state.computer == true }) {
            activityID = activity.id
            shown = activity.content.state
            since = shown?.since
        }
    }

    /// Only a build without the Mac's push link starts its own: with iCloud, the Mac drives the activity.
    private var enabled: Bool {
        !PhoneLink.cloudAvailable && ActivityAuthorizationInfo().areActivitiesEnabled
    }

    /// The most urgent of the computer's sessions as the Live Activity shows it, the same way the Mac picks its own;
    /// nil when no agent is working or waiting.
    static func state(for items: [SessionItem], since: Int) -> MochiActivityState? {
        let ranked = items
            .filter(\.fromComputer)
            .map { item in
                (urgency: MochiActivityState.urgency(state: item.state, waitingForOK: item.needsApproval,
                                                     hasQuestion: !item.question.isEmpty),
                 item: item)
            }
            .filter { $0.urgency <= 3 }   // waiting for an OK, a question, an error, working
            .sorted { $0.urgency < $1.urgency }
        guard let top = ranked.first else { return nil }
        let item = top.item
        let stepCount = item.steps.count
        return MochiActivityState(
            pillId: item.id,
            agent: item.agentName.isEmpty ? item.name : item.agentName,
            color: item.color,
            state: item.state.rawValue,
            statusText: MochiActivityState.statusText(state: item.state, urgency: top.urgency,
                                                      stepIndex: item.stepIndex, stepCount: stepCount),
            tone: MochiActivityState.tone(urgency: top.urgency),
            stepIndex: item.stepIndex,
            stepCount: stepCount,
            others: ranked.count - 1,
            since: since,
            approval: item.needsApproval && !item.approvalFingerprint.isEmpty ? item.approvalFingerprint : nil,
            computer: true)
    }

    /// The computer's sessions changed (PCLink.rebuild). `linked`: the link is up, so `items` is what the computer
    /// says now. `onScreen`: Coucou is on screen, the only time an activity may start.
    func update(items: [SessionItem], linked: Bool, onScreen: Bool) {
        guard enabled else { return }
        guard linked else {
            // The link is gone (iOS suspended Coucou, or the computer is away): what is shown stays, out of date.
            markStale()
            return
        }
        let now = Int(Date.now.timeIntervalSince1970)
        guard let next = Self.state(for: items, since: since ?? now) else {
            finish()
            return
        }
        if since == nil { since = now }
        let staleDate = Date.now.addingTimeInterval(onScreen ? Self.freshOnScreen : Self.freshAway)
        if let activityID {
            // On screen an unchanged state needs nothing; away, each message from the computer pushes the
            // out-of-date time further.
            guard next != shown || !onScreen else { return }
            shown = next
            Self.send(activityID, state: next, staleDate: staleDate, end: false)
        } else if onScreen {
            do {
                let activity = try Activity.request(attributes: MochiActivityAttributes(),
                                                    content: ActivityContent(state: next, staleDate: staleDate),
                                                    pushType: nil)
                activityID = activity.id
                shown = next
            } catch {
                // Live Activities off for Coucou in iOS Settings, or too many at once: the app works without one.
            }
        }
    }

    /// Coucou left the screen: what the activity shows holds only for the time iOS lets the link live.
    func leftScreen() {
        guard enabled, let activityID, let shown else { return }
        Self.send(activityID, state: shown, staleDate: Date.now.addingTimeInterval(Self.freshAway), end: false)
    }

    private func markStale() {
        guard let activityID, let shown else { return }
        Self.send(activityID, state: shown, staleDate: Date.now, end: false)
    }

    /// Nothing is going on any more: the last state, done, for a moment, then gone.
    private func finish() {
        since = nil
        guard let activityID else { return }
        var last = shown ?? MochiActivityState.placeholder
        last.state = BotState.finished.rawValue
        last.tone = "done"
        last.statusText = MochiActivityState.statusText(state: .finished, urgency: 4,
                                                        stepIndex: last.stepIndex, stepCount: last.stepCount)
        last.approval = nil
        last.others = 0
        last.computer = true
        self.activityID = nil
        shown = nil
        Self.send(activityID, state: last, staleDate: nil, end: true)
    }

    /// Updates or ends the activity `id` off the main actor, where its ActivityKit object is looked up and used:
    /// only plain values cross.
    nonisolated private static func send(_ id: String, state: MochiActivityState, staleDate: Date?, end: Bool) {
        Task.detached {
            guard let activity = Activity<MochiActivityAttributes>.activities.first(where: { $0.id == id }) else { return }
            let content = ActivityContent(state: state, staleDate: staleDate)
            if end {
                await activity.end(content, dismissalPolicy: .after(Date.now.addingTimeInterval(30)))
            } else {
                await activity.update(content)
            }
        }
    }
}
