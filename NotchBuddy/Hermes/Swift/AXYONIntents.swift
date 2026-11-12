import AppIntents
import Foundation

// AXYON for Siri, Spotlight and the Shortcuts app: run a routine ("Rodar Boa noite no AXYON") and ask AXYON Core a
// question, both with AXYON closed. In the Shortcuts app, "Run AXYON routine" is an action any personal automation
// can run (a time, arriving or leaving, an NFC tag, a Wi-Fi, CarPlay, an alarm stopped, a Focus…), without asking,
// which gives the routines every trigger iOS has. The phrases in the user's language are in AppShortcuts.xcstrings.

struct AXYONRoutineEntity: AppEntity {
    static var typeDisplayRepresentation: TypeDisplayRepresentation { TypeDisplayRepresentation(name: "AXYON routine") }
    static var defaultQuery: AXYONRoutineQuery { AXYONRoutineQuery() }

    let id: String
    let name: String
    let icon: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", image: DisplayRepresentation.Image(systemName: icon))
    }
}

/// The routines as AXYON last saw them (AXYONRoutineStore's copy): Siri reads them with AXYON closed.
struct AXYONRoutineQuery: EntityStringQuery {
    func entities(for identifiers: [String]) async throws -> [AXYONRoutineEntity] {
        AXYONRoutineStore.mirrored.filter { identifiers.contains($0.id) }.map(Self.entity)
    }

    func entities(matching string: String) async throws -> [AXYONRoutineEntity] {
        let wanted = Self.fold(string)
        return AXYONRoutineStore.mirrored.filter { wanted.isEmpty || Self.fold($0.name).contains(wanted) }.map(Self.entity)
    }

    func suggestedEntities() async throws -> [AXYONRoutineEntity] {
        AXYONRoutineStore.mirrored.map(Self.entity)
    }

    private static func entity(_ r: AXYONRoutineMirror) -> AXYONRoutineEntity {
        AXYONRoutineEntity(id: r.id, name: r.name, icon: r.icon)
    }

    private static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil).trimmingCharacters(in: .whitespaces)
    }
}

struct RunAXYONRoutine: AppIntent {
    static var title: LocalizedStringResource { "Run AXYON routine" }
    static var description: IntentDescription {
        IntentDescription("Runs one of your AXYON routines (its devices, notifications and questions), even with AXYON closed.")
    }
    static var openAppWhenRun: Bool { false }

    @Parameter(title: "Routine")
    var routine: AXYONRoutineEntity

    init() {}

    init(routine: AXYONRoutineEntity) {
        self.routine = routine
    }

    static var parameterSummary: some ParameterSummary {
        Summary("Run \(\.$routine)")
    }

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let said = await AXYONRoutineStore.runAway(routine.id, origin: "siri", budget: 25)
        return .result(dialog: IntentDialog(stringLiteral: said))
    }
}

struct AskAXYON: AppIntent {
    static var title: LocalizedStringResource { "Ask AXYON" }
    static var description: IntentDescription {
        IntentDescription("Asks AXYON Core, with everything it reaches: your agenda, reminders, e-mails, portfolio, the web and your home. The question and its answer stay in AXYON's chat.")
    }
    static var openAppWhenRun: Bool { false }

    @Parameter(title: "Question", requestValueDialog: IntentDialog("What do you want to ask AXYON?"))
    var question: String

    init() {}

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog & ReturnsValue<String> {
        let answer = await AXYONSiri.ask(question, budget: 25)
        return .result(value: answer, dialog: IntentDialog(stringLiteral: answer))
    }
}

/// A question from Siri or a shortcut, asked in AXYON's own chat: the answer, or where it will come.
@MainActor
enum AXYONSiri {
    static func ask(_ question: String, budget: TimeInterval) async -> String {
        let engine = HermesEngine.shared
        let started = Date.now
        if engine.state == .off { engine.start() }
        while engine.state != .ready, Date.now.timeIntervalSince(started) < budget - 4 {
            if case .failed = engine.state { break }
            try? await Task.sleep(for: .milliseconds(300))
        }
        guard engine.canChat else {
            return String(localized: "AXYON Core is still waking up: open AXYON and ask again.")
        }
        guard engine.answering == nil else {
            return String(localized: "AXYON is answering something else: ask again in a moment.")
        }
        engine.send(question)
        while engine.answering != nil, Date.now.timeIntervalSince(started) < budget {
            try? await Task.sleep(for: .milliseconds(300))
        }
        if engine.answering != nil {
            // The answer goes on with the time iOS grants, then comes as a notification.
            engine.appWentAway()
            return String(localized: "Still on it: the answer comes as a notification and stays in the chat.")
        }
        let text = engine.conversation.entries.last?.text ?? ""
        return text.count > 700 ? String(text.prefix(700)) + "…" : text
    }
}

struct AXYONShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: RunAXYONRoutine(), phrases: [
            "Run \(\.$routine) in \(.applicationName)",
            "Start \(\.$routine) with \(.applicationName)",
            "\(.applicationName) \(\.$routine)",
            "Run a routine in \(.applicationName)",
        ], shortTitle: "Run a routine", systemImageName: "sparkles")
        AppShortcut(intent: AskAXYON(), phrases: [
            "Ask \(.applicationName)",
            "Talk to \(.applicationName)",
        ], shortTitle: "Ask AXYON", systemImageName: "bubble.left.and.text.bubble.right.fill")
    }

    static var shortcutTileColor: ShortcutTileColor { .navy }
}
