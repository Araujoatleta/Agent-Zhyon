import SwiftUI

// The committee's meeting as a trial: a courtroom of wood and carved marble, the presiding judge at the bench, the
// jury of the five minds in its box, the counsel for the asset (opinion A) and the prosecutor for the user's portfolio
// (opinion B) at their tables, the expert auditor at the stand, and the intern who brings the documents. They speak
// in turn, the transcript below follows them, each juror raises a placard with their vote, and the judge strikes the
// gavel at the sentence. Everyone is an archetype drawn in code: no real person, no image.

// MARK: - Who sits in the court

enum CourtRole: Hashable {
    case judge, clerk, expert, defense, prosecutor
    case juror(Int)

    var title: String {
        switch self {
        case .judge: String(localized: "Presiding judge")
        case .clerk: String(localized: "The intern")
        case .expert: String(localized: "Expert auditor")
        case .defense: String(localized: "Counsel for the asset")
        case .prosecutor: String(localized: "Prosecutor for your portfolio")
        case .juror: String(localized: "Juror")
        }
    }
}

/// How an archetype looks: drawn with a few shapes.
struct CourtLook {
    enum Hair { case short, long, bun, curly, bald, bob, receding, buzz }
    enum Wear { case robe, suit, blouse, sweater, shirt, cardigan }

    var skin: Color
    var hair: Color
    var style: Hair
    var outfit: Color
    var wear: Wear
    var tie: Color? = nil
    var glasses = false
    var beard = false
    /// Where its idle movements start: no two breathe together.
    var phase: Double = 0

    static func of(_ role: CourtRole, minds: [InvestMind] = []) -> CourtLook {
        switch role {
        case .judge:
            CourtLook(skin: Color(hex: "#E2B48E"), hair: Color(hex: "#D3D3D6"), style: .receding, outfit: Color(hex: "#121216"),
                      wear: .robe, glasses: true, phase: 0.3)
        case .clerk:
            CourtLook(skin: Color(hex: "#7A4E2D"), hair: Color(hex: "#1A120D"), style: .curly, outfit: Color(hex: "#8DB6DE"),
                      wear: .shirt, phase: 1.1)
        case .expert:
            CourtLook(skin: Color(hex: "#C68E63"), hair: Color(hex: "#3B2A20"), style: .bald, outfit: Color(hex: "#6B4A2E"),
                      wear: .suit, tie: Color(hex: "#D8C27A"), glasses: true, beard: true, phase: 2.0)
        case .defense:
            CourtLook(skin: Color(hex: "#F0C8A6"), hair: Color(hex: "#2B1B14"), style: .long, outfit: Color(hex: "#1F2D4D"),
                      wear: .blouse, phase: 0.7)
        case .prosecutor:
            CourtLook(skin: Color(hex: "#5A3A22"), hair: Color(hex: "#0E0A08"), style: .short, outfit: Color(hex: "#2E3036"),
                      wear: .suit, tie: Color(hex: "#B3202A"), phase: 1.6)
        case .juror(let i):
            juror(i < minds.count ? minds[i].id : "", index: i)
        }
    }

    static func juror(_ mind: String, index: Int) -> CourtLook {
        switch mind {
        case "valuation":
            CourtLook(skin: Color(hex: "#F2D3B5"), hair: Color(hex: "#D9B66A"), style: .bun, outfit: Color(hex: "#2F6B4F"), wear: .blouse, phase: 0.2)
        case "legal":
            CourtLook(skin: Color(hex: "#D9A27C"), hair: Color(hex: "#14100E"), style: .short, outfit: Color(hex: "#5A5F68"), wear: .suit,
                      tie: Color(hex: "#2F5BB3"), phase: 0.9)
        case "forensic":
            CourtLook(skin: Color(hex: "#E8B996"), hair: Color(hex: "#7A3B22"), style: .curly, outfit: Color(hex: "#6E2236"), wear: .cardigan,
                      glasses: true, phase: 1.4)
        case "risk":
            CourtLook(skin: Color(hex: "#8D5A3B"), hair: Color(hex: "#1D1410"), style: .buzz, outfit: Color(hex: "#2C4A3A"), wear: .sweater, phase: 2.2)
        default:
            CourtLook(skin: Color(hex: index % 2 == 0 ? "#C98F6B" : "#F1C9A5"), hair: Color(hex: "#101010"), style: .bob,
                      outfit: Color(hex: "#C9962B"), wear: .blouse, phase: 2.7)
        }
    }
}

// MARK: - What is said

struct CourtLine: Identifiable, Equatable {
    enum Moment: Equatable { case speech, open, document, vote(Int), alert, sentence, close }

    let id: Int
    let role: CourtRole
    let name: String
    let text: String
    /// The sources it cites ("F3, D1 p.4"): tags that open them.
    let cites: String
    /// for, against, neutral or "": the color of what it says.
    let tone: String
    let moment: Moment
}

/// The trial as it is said, from the committee's verdict: the judge opens it, the intern brings the documents and the
/// facts, the counsel for the asset and the prosecutor for the portfolio plead, the five jurors vote and say why, the
/// expert auditor tells what the audit found, and the judge reads the score and the sentence.
enum CourtScript {
    static func lines(_ v: InvestVerdict) -> [CourtLine] {
        var out: [CourtLine] = []
        func add(_ role: CourtRole, _ name: String? = nil, _ text: String, cites: String = "", tone: String = "",
                 _ moment: CourtLine.Moment = .speech) {
            let said = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !said.isEmpty else { return }
            out.append(CourtLine(id: out.count, role: role, name: name ?? role.title, text: said, cites: cites, tone: tone, moment: moment))
        }
        // The judge opens the session.
        var opening = String(localized: "The session on \(v.ticker) is open.")
        if v.reason == "document" {
            opening += " " + String(localized: "The court meets by itself: the intern brought a new document.")
        } else if v.reason == "thesis" {
            opening += " " + String(localized: "The court meets by itself: your thesis broke.")
        }
        if !v.proposal.isEmpty {
            opening += " " + String(localized: "On the agenda: \(v.proposal)")
        }
        add(.judge, opening, .open)
        // The intern, with what was read.
        if let doc = v.documents.first {
            let day = investDay(doc.delivered)
            let what = day.isEmpty ? doc.title : doc.title + " (" + day + ")"
            var said = String(localized: "Your Honor, I brought \(what).")
            if !v.facts.isEmpty {
                said += " " + String(localized: "And \(v.facts.count) facts from the official sources, each with its date.")
            }
            add(.clerk, said, cites: v.documents.map(\.id).joined(separator: ", "), .document)
        } else if !v.facts.isEmpty {
            add(.clerk, String(localized: "Your Honor, here are \(v.facts.count) facts from the official sources, each with its date."), .document)
        }
        // The two pleas.
        if !v.asset.isEmpty {
            add(.defense, plea(v.asset), cites: v.asset.sources.joined(separator: ", "), tone: v.asset.tone)
        }
        if !v.portfolio.isEmpty {
            add(.prosecutor, plea(v.portfolio), cites: v.portfolio.sources.joined(separator: ", "), tone: v.portfolio.tone)
        }
        // The jury, one by one.
        for (i, mind) in v.minds.enumerated() {
            var said = InvestMind.voteName(mind.vote)
            if mind.confidence > 0 { said += " (\(mind.confidence)%)" }
            said += ". " + (mind.why.isEmpty ? (mind.points.first?.text ?? "") : mind.why)
            if mind.veto {
                said += " " + String(localized: "And I use my veto: \(InvestMind.vetoName(mind.vetoReason)).")
            }
            add(.juror(i), mind.name, said, cites: mind.points.map(\.source).filter { !$0.isEmpty }.joined(separator: ", "),
                tone: mind.vote, .vote(i))
        }
        // The expert auditor.
        if v.critical.isEmpty && v.material.isEmpty {
            add(.expert, String(localized: "I checked the accounts against the sources: no critical alert."), tone: "for", .alert)
        }
        for item in v.critical.prefix(3) {
            add(.expert, String(localized: "Critical alert: \(item.text)"), cites: item.sources.joined(separator: ", "), tone: "against", .alert)
        }
        if let first = v.material.first {
            let count = v.material.count
            add(.expert, String(localized: "\(count) material points. The first: \(first.text)"), cites: first.sources.joined(separator: ", "),
                tone: "neutral", .alert)
        }
        if !v.toVerify.isEmpty {
            add(.expert, String(localized: "Still to verify: \(v.toVerify.prefix(3).joined(separator: "; "))."), tone: "neutral", .alert)
        }
        // The judge: the score and the sentence.
        var score = String(localized: "Score: \(v.favor) for, \(v.against) against")
        if v.neutral > 0 { score += ", " + String(localized: "\(v.neutral) neutral") }
        score += "."
        if v.vetoed {
            let names = v.vetoedBy.map { InvestMind.name($0) }.joined(separator: ", ")
            score += " " + String(localized: "Vetoed by \(names).")
        }
        add(.judge, score, tone: v.vetoed ? "against" : "")
        let ruling = [v.portfolio.isEmpty ? "" : v.portfolio.viewName + ".", v.question.isEmpty ? v.proposal : v.question]
            .filter { !$0.isEmpty }.joined(separator: " ")
        add(.judge, String(localized: "Sentence: \(ruling)"), tone: v.portfolio.tone, .sentence)
        if !v.dissent.isEmpty {
            add(.judge, String(localized: "The losing side's best argument: \(v.dissent)"))
        }
        if !v.wouldChange.isEmpty {
            add(.judge, String(localized: "What would change the picture: \(v.wouldChange)"))
        }
        add(.judge, String(localized: "The session is closed. Nothing is bought or sold here: the decision is yours."), .close)
        return out
    }

    private static func plea(_ o: InvestOpinion) -> String {
        var said = o.viewName
        if let c = o.confidence { said += " (" + String(localized: "\(c)% sure") + ")" }
        return said + ". " + o.why
    }
}

// MARK: - The scene

/// The courtroom, alive: who speaks is lit and moves their lips, the jurors' placards rise with their votes, the
/// gavel falls at the sentence. `still` draws one frame (a thumbnail), which then costs nothing.
struct CourtroomScene: View {
    let verdict: InvestVerdict
    var speaker: CourtRole?
    var since: Date = .distantPast
    var opened: Date = .distantPast
    var voted: Set<Int> = []
    var sentence: Date?
    var clerkCame: Date?
    var still = false
    @State private var visible = false

    var body: some View {
        Group {
            if still {
                Canvas { context, size in
                    CourtPainter(scene: self, now: .distantFuture, size: size).paint(&context)
                }
            } else {
                TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !visible)) { timeline in
                    Canvas { context, size in
                        CourtPainter(scene: self, now: timeline.date, size: size).paint(&context)
                    }
                }
            }
        }
        .onAppear { visible = true }
        .onDisappear { visible = false }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(speaker.map { String(localized: "The court: \($0.title) is speaking") } ?? String(localized: "The court")))
    }
}

/// One frame of the courtroom.
private struct CourtPainter {
    let scene: CourtroomScene
    let now: Date
    let size: CGSize

    private var w: CGFloat { size.width }
    private var h: CGFloat { size.height }
    /// The scene's unit: 1 on a 360-point-wide court.
    private var u: CGFloat { w / 360 }
    private var t: Double { now == .distantFuture ? 0 : now.timeIntervalSinceReferenceDate }

    private func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x * w, y: y * h) }
    private func r(_ x: CGFloat, _ y: CGFloat, _ x2: CGFloat, _ y2: CGFloat) -> CGRect {
        CGRect(x: x * w, y: y * h, width: (x2 - x) * w, height: (y2 - y) * h)
    }

    private func since(_ date: Date?) -> Double? {
        guard let date, now != .distantFuture else { return date == nil ? nil : 99 }
        return now.timeIntervalSince(date)
    }

    private func ease(_ x: Double) -> Double {
        let c = min(max(x, 0), 1)
        return c * c * (3 - 2 * c)
    }

    // MARK: Where everyone is

    private func spot(_ role: CourtRole) -> (CGPoint, CGFloat) {
        switch role {
        case .judge: return (p(0.5, 0.265), 0.95)
        case .expert: return (p(0.155, 0.44), 0.78)
        case .defense: return (p(0.2, 0.705), 1.12)
        case .prosecutor: return (p(0.565, 0.705), 1.12)
        case .clerk:
            let walked = ease((since(scene.clerkCame) ?? 0) / 1.6)
            let x = 0.05 + (0.385 - 0.05) * walked
            let y = 0.62 + (0.655 - 0.62) * walked
            return (p(x, y), 0.9 + 0.06 * walked)
        case .juror(let i):
            let seats: [(CGFloat, CGFloat)] = [(0.775, 0.455), (0.865, 0.455), (0.955, 0.455), (0.82, 0.555), (0.91, 0.555)]
            let seat = seats[min(i, seats.count - 1)]
            return (p(seat.0, seat.1), 0.6)
        }
    }

    private var gavelStrike: Double? { since(scene.sentence) }

    // MARK: The frame

    func paint(_ context: inout GraphicsContext) {
        var c = context
        // The gavel shakes the room.
        if let g = gavelStrike, g > 0.45, g < 0.8 {
            let fade = 1 - (g - 0.45) / 0.35
            c.translateBy(x: sin(g * 90) * 3 * u * fade, y: cos(g * 70) * 1.5 * u * fade)
        }
        wall(&c)
        lights(&c)
        marble(&c)
        crest(&c)
        door(&c)
        floor(&c)
        chair(&c)
        person(&c, .judge)
        bench(&c)
        stand(&c)
        person(&c, .expert)
        standFront(&c)
        juryBack(&c)
        for i in 0..<min(scene.verdict.minds.count, 5) where i < 3 { person(&c, .juror(i)) }
        juryMiddle(&c)
        for i in 0..<min(scene.verdict.minds.count, 5) where i >= 3 { person(&c, .juror(i)) }
        juryRail(&c)
        placards(&c)
        person(&c, .clerk)
        person(&c, .defense)
        person(&c, .prosecutor)
        tables(&c)
        gavelEffects(&c)
        spotlight(&c)
        motes(&c)
        vignette(&c)
        lightsOn(&c)
    }

    // MARK: The room

    private func wall(_ c: inout GraphicsContext) {
        let wall = Path(r(0, 0, 1, 0.64))
        c.fill(wall, with: .linearGradient(Gradient(colors: [Color(hex: "#24150E"), Color(hex: "#3E2618"), Color(hex: "#2E1C12")]),
                                           startPoint: p(0.5, 0), endPoint: p(0.5, 0.64)))
        // Panels of wood, each with its grain.
        var x: CGFloat = 0
        while x < 1 {
            var line = Path()
            line.move(to: p(x, 0.06))
            line.addLine(to: p(x, 0.64))
            c.stroke(line, with: .color(.black.opacity(0.35)), lineWidth: 1.2 * u)
            var light = Path()
            light.move(to: p(x + 0.004, 0.06))
            light.addLine(to: p(x + 0.004, 0.64))
            c.stroke(light, with: .color(Color(hex: "#C08A5A").opacity(0.10)), lineWidth: 0.8 * u)
            x += 0.05
        }
        // The ceiling's band and its recessed lights.
        c.fill(Path(r(0, 0, 1, 0.06)), with: .linearGradient(Gradient(colors: [Color(hex: "#120B07"), Color(hex: "#21140D")]),
                                                             startPoint: p(0.5, 0), endPoint: p(0.5, 0.06)))
        for lx in [0.1, 0.3, 0.5, 0.7, 0.9] as [CGFloat] {
            let lamp = CGRect(x: lx * w - 7 * u, y: 0.025 * h, width: 14 * u, height: 4 * u)
            c.fill(Path(ellipseIn: lamp.insetBy(dx: -6 * u, dy: -4 * u)), with: .radialGradient(
                Gradient(colors: [Color(hex: "#FFE7B0").opacity(0.55), .clear]), center: CGPoint(x: lamp.midX, y: lamp.midY),
                startRadius: 0, endRadius: 14 * u))
            c.fill(Path(ellipseIn: lamp), with: .color(Color(hex: "#FFF3D6")))
        }
        // The dado: a darker band of wood along the floor.
        c.fill(Path(r(0, 0.56, 1, 0.64)), with: .linearGradient(Gradient(colors: [Color(hex: "#1C110B"), Color(hex: "#2A1910")]),
                                                                startPoint: p(0.5, 0.56), endPoint: p(0.5, 0.64)))
    }

    private func lights(_ c: inout GraphicsContext) {
        var glow = c
        glow.blendMode = .plusLighter
        for lx in [0.1, 0.3, 0.7, 0.9] as [CGFloat] {
            var cone = Path()
            cone.move(to: p(lx - 0.015, 0.04))
            cone.addLine(to: p(lx + 0.015, 0.04))
            cone.addLine(to: p(lx + 0.11, 0.95))
            cone.addLine(to: p(lx - 0.11, 0.95))
            cone.closeSubpath()
            glow.fill(cone, with: .linearGradient(Gradient(colors: [Color(hex: "#FFD99A").opacity(0.10), .clear]),
                                                  startPoint: p(lx, 0.04), endPoint: p(lx, 0.95)))
        }
    }

    /// The marble behind the bench, carved in rows of arcs.
    private func marble(_ c: inout GraphicsContext) {
        let panel = r(0.25, 0.06, 0.75, 0.5)
        c.fill(Path(panel), with: .linearGradient(Gradient(colors: [Color(hex: "#E9DFC9"), Color(hex: "#D3C3A2"), Color(hex: "#BFAE8B")]),
                                                  startPoint: CGPoint(x: panel.midX, y: panel.minY), endPoint: CGPoint(x: panel.midX, y: panel.maxY)))
        var carved = c
        carved.clip(to: Path(panel))
        let row = 0.034 * h
        let col = 0.032 * w
        var y = panel.minY + row * 0.7
        var shift = false
        while y < panel.maxY {
            var x = panel.minX + (shift ? col / 2 : 0)
            while x < panel.maxX {
                var arc = Path()
                arc.addArc(center: CGPoint(x: x, y: y), radius: 3.2 * u, startAngle: .degrees(200), endAngle: .degrees(340), clockwise: false)
                carved.stroke(arc, with: .color(Color(hex: "#7E6A49").opacity(0.42)), lineWidth: 0.9 * u)
                var lit = Path()
                lit.addArc(center: CGPoint(x: x, y: y + 0.8 * u), radius: 3.2 * u, startAngle: .degrees(205), endAngle: .degrees(335), clockwise: false)
                carved.stroke(lit, with: .color(.white.opacity(0.35)), lineWidth: 0.6 * u)
                x += col
            }
            y += row
            shift.toggle()
        }
        // Light falls on the marble from above.
        carved.fill(Path(panel), with: .radialGradient(Gradient(colors: [Color(hex: "#FFF4DA").opacity(0.35), .clear]),
                                                       center: p(0.5, 0.08), startRadius: 0, endRadius: 0.32 * w))
        // Its frame of wood.
        c.stroke(Path(panel), with: .color(Color(hex: "#1B100A")), lineWidth: 3 * u)
        c.stroke(Path(panel.insetBy(dx: 2.5 * u, dy: 2.5 * u)), with: .color(Color(hex: "#8A5A35").opacity(0.6)), lineWidth: 0.8 * u)
    }

    /// The court's crest: a ring of gold around a sphere of the accent's light, with laurels.
    private func crest(_ c: inout GraphicsContext) {
        let theme = AXYONTheme.current
        let center = p(0.5, 0.125)
        let radius = 13 * u
        c.fill(Path(ellipseIn: CGRect(x: center.x - radius * 1.6, y: center.y - radius * 1.6, width: radius * 3.2, height: radius * 3.2)),
               with: .radialGradient(Gradient(colors: [theme.accent.opacity(0.35), .clear]), center: center, startRadius: 0, endRadius: radius * 1.6))
        let orb = CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
        c.fill(Path(ellipseIn: orb), with: .radialGradient(Gradient(colors: [theme.accent, theme.accentDeep, Color(hex: "#0B0B1A")]),
                                                           center: CGPoint(x: center.x - radius * 0.3, y: center.y - radius * 0.35),
                                                           startRadius: 0, endRadius: radius * 1.2))
        c.stroke(Path(ellipseIn: orb), with: .linearGradient(Gradient(colors: [Color(hex: "#FBE3A1"), Color(hex: "#A87B2A")]),
                                                             startPoint: CGPoint(x: orb.minX, y: orb.minY), endPoint: CGPoint(x: orb.maxX, y: orb.maxY)),
                 lineWidth: 2.2 * u)
        c.fill(Path(ellipseIn: CGRect(x: center.x - radius * 0.55, y: center.y - radius * 0.7, width: radius * 0.7, height: radius * 0.35)),
               with: .color(.white.opacity(0.55)))
        // Laurels, leaf by leaf.
        for side in [-1.0, 1.0] as [CGFloat] {
            for k in 0..<6 {
                let a = Angle.degrees(Double(100 + k * 22)).radians
                let lx = center.x + side * cos(a) * radius * -1.45
                let ly = center.y + sin(a) * radius * 1.45
                var leaf = Path(ellipseIn: CGRect(x: -2.4 * u, y: -1.1 * u, width: 4.8 * u, height: 2.2 * u))
                leaf = leaf.applying(CGAffineTransform(rotationAngle: side * CGFloat(a) + .pi / 2).concatenating(CGAffineTransform(translationX: lx, y: ly)))
                c.fill(leaf, with: .color(Color(hex: "#D9B45A")))
            }
        }
        let title = c.resolve(Text(verbatim: "TRIBUNAL AXYON").font(.system(size: 6.5 * u, weight: .heavy)).tracking(1.6 * u)
            .foregroundStyle(Color(hex: "#7A5A2A")))
        c.draw(title, at: p(0.5, 0.205), anchor: .center)
    }

    private func door(_ c: inout GraphicsContext) {
        let frame = r(0.0, 0.2, 0.085, 0.64)
        c.fill(Path(frame), with: .color(Color(hex: "#0F0906")))
        c.stroke(Path(frame.insetBy(dx: 1.5 * u, dy: 1.5 * u)), with: .color(Color(hex: "#7A4E2E").opacity(0.7)), lineWidth: 1.4 * u)
        c.fill(Path(ellipseIn: CGRect(x: frame.maxX - 6 * u, y: frame.midY, width: 2.4 * u, height: 2.4 * u)), with: .color(Color(hex: "#D9B45A")))
    }

    /// The carpet, green as a plenary's, in perspective.
    private func floor(_ c: inout GraphicsContext) {
        let carpet = Path(r(0, 0.6, 1, 1))
        c.fill(carpet, with: .linearGradient(Gradient(colors: [Color(hex: "#1B3F2C"), Color(hex: "#123022"), Color(hex: "#0B2117")]),
                                             startPoint: p(0.5, 0.6), endPoint: p(0.5, 1)))
        var lines = c
        lines.clip(to: carpet)
        for k in 0...12 {
            var l = Path()
            l.move(to: p(0.5, 0.3))
            l.addLine(to: p(-0.6 + CGFloat(k) * 0.183, 1.05))
            lines.stroke(l, with: .color(.white.opacity(0.035)), lineWidth: 0.8 * u)
        }
        // The aisle's runner to the bench.
        var runner = Path()
        runner.move(to: p(0.44, 0.6))
        runner.addLine(to: p(0.56, 0.6))
        runner.addLine(to: p(0.66, 1))
        runner.addLine(to: p(0.34, 1))
        runner.closeSubpath()
        c.fill(runner, with: .color(Color(hex: "#0A1A12").opacity(0.55)))
        // The bench's shadow on the carpet.
        c.fill(Path(ellipseIn: r(0.24, 0.57, 0.76, 0.66)), with: .radialGradient(Gradient(colors: [.black.opacity(0.55), .clear]),
                                                                               center: p(0.5, 0.6), startRadius: 0, endRadius: 0.27 * w))
    }

    private func chair(_ c: inout GraphicsContext) {
        let back = r(0.425, 0.15, 0.575, 0.42)
        let shape = Path(roundedRect: back, cornerRadius: 12 * u, style: .continuous)
        c.fill(shape, with: .linearGradient(Gradient(colors: [Color(hex: "#5C2222"), Color(hex: "#2E0E0E")]),
                                            startPoint: CGPoint(x: back.minX, y: back.minY), endPoint: CGPoint(x: back.maxX, y: back.maxY)))
        var tufts = c
        tufts.clip(to: shape)
        var y = back.minY + 9 * u
        while y < back.maxY {
            var x = back.minX + 8 * u
            while x < back.maxX {
                tufts.fill(Path(ellipseIn: CGRect(x: x - 1 * u, y: y - 1 * u, width: 2 * u, height: 2 * u)), with: .color(.black.opacity(0.45)))
                x += 9 * u
            }
            y += 9 * u
        }
        c.stroke(shape, with: .color(Color(hex: "#D9B45A").opacity(0.55)), lineWidth: 1.2 * u)
    }

    /// The bench: raised, of wood, its panels inlaid, the presidency's plate, two screens with the score.
    private func bench(_ c: inout GraphicsContext) {
        let top = r(0.255, 0.385, 0.745, 0.415)
        let front = r(0.27, 0.41, 0.73, 0.6)
        c.fill(Path(front), with: .linearGradient(Gradient(colors: [Color(hex: "#7C4A28"), Color(hex: "#5A3218"), Color(hex: "#3E220F")]),
                                                  startPoint: CGPoint(x: front.midX, y: front.minY), endPoint: CGPoint(x: front.midX, y: front.maxY)))
        for k in 0..<3 {
            let inset = CGRect(x: front.minX + front.width * (0.04 + CGFloat(k) * 0.32), y: front.minY + front.height * 0.22,
                               width: front.width * 0.28, height: front.height * 0.62)
            c.stroke(Path(roundedRect: inset, cornerRadius: 2 * u), with: .color(.black.opacity(0.4)), lineWidth: 1.4 * u)
            c.stroke(Path(roundedRect: inset.offsetBy(dx: 0.8 * u, dy: 0.8 * u), cornerRadius: 2 * u),
                     with: .color(Color(hex: "#C08A5A").opacity(0.25)), lineWidth: 0.7 * u)
        }
        c.fill(Path(roundedRect: top, cornerRadius: 2 * u), with: .linearGradient(Gradient(colors: [Color(hex: "#A26A3E"), Color(hex: "#6E401F")]),
                                                                                  startPoint: CGPoint(x: top.midX, y: top.minY), endPoint: CGPoint(x: top.midX, y: top.maxY)))
        var edge = Path()
        edge.move(to: CGPoint(x: top.minX, y: top.minY + 0.5 * u))
        edge.addLine(to: CGPoint(x: top.maxX, y: top.minY + 0.5 * u))
        c.stroke(edge, with: .color(Color(hex: "#FFD9A8").opacity(0.45)), lineWidth: 0.8 * u)
        // The presidency's plate.
        let plate = CGRect(x: 0.5 * w - 26 * u, y: front.minY + 4 * u, width: 52 * u, height: 9 * u)
        c.fill(Path(roundedRect: plate, cornerRadius: 1.5 * u), with: .linearGradient(Gradient(colors: [Color(hex: "#F3D98F"), Color(hex: "#A97C2E")]),
                                                                                      startPoint: CGPoint(x: plate.minX, y: plate.minY), endPoint: CGPoint(x: plate.maxX, y: plate.maxY)))
        let label = c.resolve(Text(String(localized: "PRESIDENCY")).font(.system(size: 5 * u, weight: .heavy)).tracking(0.8 * u)
            .foregroundStyle(Color(hex: "#3A2A12")))
        c.draw(label, at: CGPoint(x: plate.midX, y: plate.midY), anchor: .center)
        // The two screens: the score as the jurors vote.
        let minds = scene.verdict.minds
        let favor = scene.voted.filter { $0 < minds.count && minds[$0].vote == "for" }.count
        let against = scene.voted.filter { $0 < minds.count && minds[$0].vote == "against" }.count
        for sx in [0.32, 0.68] as [CGFloat] {
            let screen = CGRect(x: sx * w - 15 * u, y: top.minY - 13 * u, width: 30 * u, height: 13 * u)
            c.fill(Path(roundedRect: screen, cornerRadius: 1.5 * u), with: .color(Color(hex: "#0B0F18")))
            c.stroke(Path(roundedRect: screen, cornerRadius: 1.5 * u), with: .color(Color(hex: "#2A2F3A")), lineWidth: 1 * u)
            if !scene.voted.isEmpty {
                let score = c.resolve(Text(verbatim: "\(favor) × \(against)").font(.system(size: 6.5 * u, weight: .bold, design: .rounded))
                    .foregroundStyle(AXYONTheme.current.accent))
                c.draw(score, at: CGPoint(x: screen.midX, y: screen.midY), anchor: .center)
            }
        }
        // The gavel at rest, until the judge takes it.
        if (gavelStrike ?? 99) > 1.0 || scene.sentence == nil {
            gavel(&c, at: CGPoint(x: 0.6 * w, y: top.minY - 1 * u), angle: .degrees(-12), scale: 1)
        }
    }

    private func gavel(_ c: inout GraphicsContext, at point: CGPoint, angle: Angle, scale: CGFloat) {
        var g = c
        g.translateBy(x: point.x, y: point.y)
        g.rotate(by: angle)
        let s = u * scale
        g.fill(Path(roundedRect: CGRect(x: -1 * s, y: -12 * s, width: 2 * s, height: 13 * s), cornerRadius: 1 * s), with: .color(Color(hex: "#5A3218")))
        g.fill(Path(roundedRect: CGRect(x: -6 * s, y: -15.5 * s, width: 12 * s, height: 5 * s), cornerRadius: 1.6 * s),
               with: .linearGradient(Gradient(colors: [Color(hex: "#9A6236"), Color(hex: "#4A2812")]), startPoint: CGPoint(x: 0, y: -15.5 * s), endPoint: CGPoint(x: 0, y: -10.5 * s)))
        g.fill(Path(CGRect(x: -6 * s, y: -13.6 * s, width: 12 * s, height: 1.2 * s)), with: .color(Color(hex: "#D9B45A")))
    }

    private func stand(_ c: inout GraphicsContext) {
        // The expert's stand: a small box with its rail and a microphone.
        let back = r(0.075, 0.47, 0.24, 0.52)
        c.fill(Path(back), with: .color(Color(hex: "#2A170C")))
    }

    private func standFront(_ c: inout GraphicsContext) {
        let front = r(0.07, 0.505, 0.245, 0.665)
        c.fill(Path(front), with: .linearGradient(Gradient(colors: [Color(hex: "#704222"), Color(hex: "#3F230F")]),
                                                  startPoint: CGPoint(x: front.midX, y: front.minY), endPoint: CGPoint(x: front.midX, y: front.maxY)))
        c.fill(Path(r(0.065, 0.495, 0.25, 0.512)), with: .color(Color(hex: "#9A6236")))
        var mic = Path()
        mic.move(to: p(0.2, 0.497))
        mic.addLine(to: p(0.185, 0.455))
        c.stroke(mic, with: .color(Color(hex: "#1A1A1A")), lineWidth: 1.2 * u)
        c.fill(Path(roundedRect: CGRect(x: 0.185 * w - 2 * u, y: 0.455 * h - 3.5 * u, width: 4 * u, height: 5 * u), cornerRadius: 2 * u),
               with: .color(Color(hex: "#2B2B2B")))
        let label = c.resolve(Text(String(localized: "EXPERT")).font(.system(size: 4.6 * u, weight: .heavy)).tracking(0.6 * u)
            .foregroundStyle(Color(hex: "#E9C97C").opacity(0.85)))
        c.draw(label, at: p(0.157, 0.6), anchor: .center)
    }

    private func juryBack(_ c: inout GraphicsContext) {
        c.fill(Path(r(0.73, 0.43, 1, 0.47)), with: .color(Color(hex: "#24140B")))
    }

    private func juryMiddle(_ c: inout GraphicsContext) {
        let tier = r(0.73, 0.505, 1, 0.535)
        c.fill(Path(tier), with: .linearGradient(Gradient(colors: [Color(hex: "#6E401F"), Color(hex: "#3E220F")]),
                                                 startPoint: CGPoint(x: tier.midX, y: tier.minY), endPoint: CGPoint(x: tier.midX, y: tier.maxY)))
    }

    private func juryRail(_ c: inout GraphicsContext) {
        let rail = r(0.725, 0.6, 1, 0.68)
        c.fill(Path(rail), with: .linearGradient(Gradient(colors: [Color(hex: "#7A4826"), Color(hex: "#40230F")]),
                                                 startPoint: CGPoint(x: rail.midX, y: rail.minY), endPoint: CGPoint(x: rail.midX, y: rail.maxY)))
        c.fill(Path(r(0.72, 0.59, 1, 0.607)), with: .color(Color(hex: "#A06A3E")))
        var x: CGFloat = 0.745
        while x < 1 {
            c.stroke(Path(r(x, 0.62, x + 0.04, 0.665)), with: .color(.black.opacity(0.35)), lineWidth: 1 * u)
            x += 0.055
        }
        let label = c.resolve(Text(String(localized: "JURY")).font(.system(size: 5 * u, weight: .heavy)).tracking(1 * u)
            .foregroundStyle(Color(hex: "#E9C97C").opacity(0.85)))
        c.draw(label, at: p(0.86, 0.645), anchor: .center)
    }

    /// The placards the jurors raise with their votes: green ✓, red ✗, grey –.
    private func placards(_ c: inout GraphicsContext) {
        let minds = scene.verdict.minds
        for i in scene.voted.sorted() where i < min(minds.count, 5) {
            let (head, s) = spot(.juror(i))
            let vote = minds[i].vote
            let color = InvestPalette.vote(vote)
            let glyph = vote == "for" ? "✓" : vote == "against" ? "✗" : "–"
            let bob = CGFloat(sin(t * 2.2 + Double(i))) * 0.8 * u
            let card = CGRect(x: head.x + 6 * s * u, y: head.y - 30 * s * u + bob, width: 18 * s * u, height: 14 * s * u)
            var stick = Path()
            stick.move(to: CGPoint(x: card.midX, y: card.maxY))
            stick.addLine(to: CGPoint(x: card.midX - 2 * u, y: head.y + 10 * s * u))
            c.stroke(stick, with: .color(Color(hex: "#C9A26A")), lineWidth: 1.2 * u)
            c.fill(Path(roundedRect: card.insetBy(dx: -1.5 * u, dy: -1.5 * u), cornerRadius: 3 * u), with: .radialGradient(
                Gradient(colors: [color.opacity(0.45), .clear]), center: CGPoint(x: card.midX, y: card.midY), startRadius: 0, endRadius: 16 * u))
            c.fill(Path(roundedRect: card, cornerRadius: 2 * u), with: .color(color))
            let mark = c.resolve(Text(verbatim: glyph).font(.system(size: 9 * s * u, weight: .black)).foregroundStyle(Color.black.opacity(0.8)))
            c.draw(mark, at: CGPoint(x: card.midX, y: card.midY), anchor: .center)
        }
    }

    private func tables(_ c: inout GraphicsContext) {
        for (x1, x2, title) in [(0.02, 0.4, String(localized: "DEFENSE")), (0.43, 0.72, String(localized: "PROSECUTION"))] as [(CGFloat, CGFloat, String)] {
            let top = r(x1, 0.84, x2, 0.87)
            let front = r(x1 + 0.01, 0.87, x2 - 0.01, 1.0)
            c.fill(Path(front), with: .linearGradient(Gradient(colors: [Color(hex: "#6A3D1E"), Color(hex: "#2E190A")]),
                                                      startPoint: CGPoint(x: front.midX, y: front.minY), endPoint: CGPoint(x: front.midX, y: front.maxY)))
            c.fill(Path(roundedRect: top, cornerRadius: 1.5 * u), with: .linearGradient(Gradient(colors: [Color(hex: "#A26A3E"), Color(hex: "#6E401F")]),
                                                                                        startPoint: CGPoint(x: top.midX, y: top.minY), endPoint: CGPoint(x: top.midX, y: top.maxY)))
            // Papers and a glass of water.
            let paper = CGRect(x: top.minX + top.width * 0.12, y: top.minY - 1.5 * u, width: 14 * u, height: 4 * u)
            c.fill(Path(paper.applying(CGAffineTransform(translationX: 0, y: 0))), with: .color(Color(hex: "#F4EFE4")))
            c.fill(Path(paper.offsetBy(dx: 5 * u, dy: -1 * u)), with: .color(Color(hex: "#E9E2D2")))
            let glass = CGRect(x: top.maxX - top.width * 0.18, y: top.minY - 7 * u, width: 4 * u, height: 7 * u)
            c.fill(Path(roundedRect: glass, cornerRadius: 1 * u), with: .color(Color.white.opacity(0.25)))
            let plate = c.resolve(Text(title).font(.system(size: 4.8 * u, weight: .heavy)).tracking(0.8 * u)
                .foregroundStyle(Color(hex: "#E9C97C").opacity(0.85)))
            c.draw(plate, at: CGPoint(x: front.midX, y: front.minY + 9 * u), anchor: .center)
        }
    }

    // MARK: The light on who speaks

    private func spotlight(_ c: inout GraphicsContext) {
        guard let who = scene.speaker else { return }
        let (head, s) = spot(who)
        let rise = ease((since(scene.since) ?? 1) / 0.4)
        var glow = c
        glow.blendMode = .plusLighter
        glow.opacity = rise
        let accent = AXYONTheme.current.accent
        var cone = Path()
        cone.move(to: CGPoint(x: head.x - 6 * u, y: 0))
        cone.addLine(to: CGPoint(x: head.x + 6 * u, y: 0))
        cone.addLine(to: CGPoint(x: head.x + 34 * s * u, y: head.y + 60 * s * u))
        cone.addLine(to: CGPoint(x: head.x - 34 * s * u, y: head.y + 60 * s * u))
        cone.closeSubpath()
        glow.fill(cone, with: .linearGradient(Gradient(colors: [Color(hex: "#FFF1CF").opacity(0.05), Color(hex: "#FFE3A8").opacity(0.20)]),
                                              startPoint: CGPoint(x: head.x, y: 0), endPoint: CGPoint(x: head.x, y: head.y + 60 * s * u)))
        glow.fill(Path(ellipseIn: CGRect(x: head.x - 30 * s * u, y: head.y - 26 * s * u, width: 60 * s * u, height: 60 * s * u)),
                  with: .radialGradient(Gradient(colors: [accent.opacity(0.35), .clear]), center: head, startRadius: 0, endRadius: 32 * s * u))
        // The waves of the voice, over the head.
        for k in 0..<3 {
            let phase = (t * 1.6 + Double(k) * 0.33).truncatingRemainder(dividingBy: 1)
            let radius = (8 + 10 * phase) * s * u
            var arc = Path()
            arc.addArc(center: CGPoint(x: head.x + 12 * s * u, y: head.y - 10 * s * u), radius: radius,
                       startAngle: .degrees(-60), endAngle: .degrees(10), clockwise: false)
            glow.stroke(arc, with: .color(accent.opacity((1 - phase) * 0.8)), lineWidth: 1.3 * u)
        }
    }

    // MARK: The gavel

    private func gavelEffects(_ c: inout GraphicsContext) {
        guard let g = gavelStrike, g < 1.6 else { return }
        let point = CGPoint(x: 0.6 * w, y: 0.385 * h)
        if g >= 0.45 {
            let k = (g - 0.45) / 1.1
            var ring = c
            ring.blendMode = .plusLighter
            for n in 0..<2 {
                let radius = CGFloat(k) * (70 + CGFloat(n) * 40) * u
                ring.stroke(Path(ellipseIn: CGRect(x: point.x - radius, y: point.y - radius * 0.45, width: radius * 2, height: radius * 0.9)),
                            with: .color(Color(hex: "#FFE3A8").opacity(max(0, 0.7 - k * 0.7))), lineWidth: (2.4 - CGFloat(n)) * u)
            }
            if g < 0.65 {
                c.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.white.opacity(0.16 * (1 - (g - 0.45) / 0.2))))
            }
            // Splinters of light.
            for s in 0..<10 {
                let a = Double(s) * 0.628 + 0.3
                let d = CGFloat(k) * 46 * u
                let dot = CGPoint(x: point.x + CGFloat(cos(a)) * d, y: point.y - abs(CGFloat(sin(a))) * d * 0.7)
                c.fill(Path(ellipseIn: CGRect(x: dot.x - 1.2 * u, y: dot.y - 1.2 * u, width: 2.4 * u, height: 2.4 * u)),
                       with: .color(Color(hex: "#FFE3A8").opacity(max(0, 1 - k))))
            }
        }
    }

    /// Dust in the light, drifting.
    private func motes(_ c: inout GraphicsContext) {
        var dust = c
        dust.blendMode = .plusLighter
        for k in 0..<18 {
            let seed = Double(k) * 12.9898
            let fx = (sin(seed) * 43758.5453).truncatingRemainder(dividingBy: 1)
            let fy = (sin(seed * 1.7) * 24634.6345).truncatingRemainder(dividingBy: 1)
            let speed = 0.004 + abs(fx) * 0.006
            let x = (abs(fx) + sin(t * 0.2 + seed) * 0.02)
            let y = (abs(fy) + t * speed).truncatingRemainder(dividingBy: 1) * 0.9 + 0.05
            let radius = (0.6 + abs(fy) * 1.1) * u
            dust.fill(Path(ellipseIn: CGRect(x: CGFloat(x) * w - radius, y: CGFloat(y) * h - radius, width: radius * 2, height: radius * 2)),
                      with: .color(Color(hex: "#FFE9C2").opacity(0.18 + abs(fx) * 0.2)))
        }
    }

    private func vignette(_ c: inout GraphicsContext) {
        c.fill(Path(CGRect(origin: .zero, size: size)), with: .radialGradient(Gradient(colors: [.clear, .black.opacity(0.55)]),
                                                                          center: p(0.5, 0.45), startRadius: 0.32 * w, endRadius: 0.78 * w))
    }

    /// The session opens: the room comes out of the dark.
    private func lightsOn(_ c: inout GraphicsContext) {
        guard let s = since(scene.opened), s < 1.6 else { return }
        c.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.black.opacity(0.85 * (1 - ease(s / 1.6)))))
    }

    // MARK: People

    private func person(_ c: inout GraphicsContext, _ role: CourtRole) {
        let (head, scale) = spot(role)
        let look = CourtLook.of(role, minds: scene.verdict.minds)
        let speaking = scene.speaker == role
        let s = scale * u
        let time = t + look.phase * 3.1
        let breath = CGFloat(sin(time * 1.6)) * 0.35 * s
        let bob = speaking ? CGFloat(sin(time * 6)) * 0.7 * s : 0
        let mouth: CGFloat = speaking ? CGFloat(abs(sin(time * 9.5)) * (0.55 + 0.45 * sin(time * 2.3))) : 0
        let blink = (time.truncatingRemainder(dividingBy: 4.3)) < 0.13
        var arm: CGFloat = speaking ? 0.45 + 0.3 * CGFloat(sin(time * 2.7)) : 0
        var holding: Holding = .none
        switch role {
        case .clerk: holding = .folder
        case .expert: holding = .clipboard
        case .judge:
            if let g = gavelStrike, g < 1.0 {
                arm = g < 0.35 ? CGFloat(ease(g / 0.35)) : g < 0.45 ? CGFloat(1 - (g - 0.35) / 0.1) : 0
                holding = .gavel
            }
        default: break
        }
        CourtFigure.draw(&c, look: look, head: CGPoint(x: head.x, y: head.y + bob - breath * 0.4), s: s, mouth: mouth, blink: blink,
                         breath: breath, arm: arm, holding: holding, now: time)
    }
}

enum Holding { case none, folder, clipboard, gavel }

/// An archetype drawn from its head down to its waist: shoulders, the clothes of its part, a face that blinks and
/// speaks, hair, glasses, a beard, an arm that gestures, what it holds.
enum CourtFigure {
    static func draw(_ c: inout GraphicsContext, look: CourtLook, head: CGPoint, s: CGFloat, mouth: CGFloat, blink: Bool,
                     breath: CGFloat, arm: CGFloat, holding: Holding, now: Double) {
        let x = head.x
        let y = head.y
        // Hair behind the head (long, bob).
        if look.style == .long || look.style == .bob {
            let drop: CGFloat = look.style == .long ? 30 : 15
            let back = Path(roundedRect: CGRect(x: x - 14 * s, y: y - 14 * s, width: 28 * s, height: (26 + drop) * s), cornerRadius: 12 * s)
            c.fill(back, with: .color(look.hair))
        }
        // The body.
        var body = Path()
        body.move(to: CGPoint(x: x - 7 * s, y: y + 11 * s))
        body.addQuadCurve(to: CGPoint(x: x - 25 * s, y: y + 21 * s - breath), control: CGPoint(x: x - 18 * s, y: y + 12 * s))
        body.addQuadCurve(to: CGPoint(x: x - 29 * s, y: y + 72 * s), control: CGPoint(x: x - 31 * s, y: y + 30 * s))
        body.addLine(to: CGPoint(x: x + 29 * s, y: y + 72 * s))
        body.addQuadCurve(to: CGPoint(x: x + 25 * s, y: y + 21 * s - breath), control: CGPoint(x: x + 31 * s, y: y + 30 * s))
        body.addQuadCurve(to: CGPoint(x: x + 7 * s, y: y + 11 * s), control: CGPoint(x: x + 18 * s, y: y + 12 * s))
        body.closeSubpath()
        c.fill(body, with: .linearGradient(Gradient(colors: [look.outfit.opacity(1), look.outfit.mix(.black, 0.35)]),
                                           startPoint: CGPoint(x: x, y: y + 12 * s), endPoint: CGPoint(x: x, y: y + 72 * s)))
        // A rim of light on the shoulders.
        var rim = Path()
        rim.move(to: CGPoint(x: x - 23 * s, y: y + 21 * s - breath))
        rim.addQuadCurve(to: CGPoint(x: x - 8 * s, y: y + 12 * s), control: CGPoint(x: x - 16 * s, y: y + 13 * s))
        c.stroke(rim, with: .color(.white.opacity(0.18)), lineWidth: 1 * s)
        // The neck.
        c.fill(Path(roundedRect: CGRect(x: x - 5 * s, y: y + 5 * s, width: 10 * s, height: 9 * s), cornerRadius: 2 * s), with: .color(look.skin.mix(.black, 0.12)))
        clothes(&c, look: look, x: x, y: y, s: s)
        // An arm that gestures (or brings down the gavel).
        if arm > 0.02 {
            let shoulder = CGPoint(x: x + 22 * s, y: y + 26 * s)
            let hand = CGPoint(x: x + 30 * s + 4 * s * arm, y: y + 30 * s - 30 * s * arm)
            var sleeve = Path()
            sleeve.move(to: shoulder)
            sleeve.addLine(to: hand)
            c.stroke(sleeve, with: .color(look.outfit.mix(.black, 0.1)), style: StrokeStyle(lineWidth: 9 * s, lineCap: .round))
            c.fill(Path(ellipseIn: CGRect(x: hand.x - 3.6 * s, y: hand.y - 3.6 * s, width: 7.2 * s, height: 7.2 * s)), with: .color(look.skin))
            if holding == .gavel {
                var g = c
                g.translateBy(x: hand.x, y: hand.y)
                g.rotate(by: .degrees(-40 + 70 * Double(1 - arm)))
                g.fill(Path(roundedRect: CGRect(x: -1 * s, y: -13 * s, width: 2 * s, height: 14 * s), cornerRadius: 1 * s), with: .color(Color(hex: "#5A3218")))
                g.fill(Path(roundedRect: CGRect(x: -6 * s, y: -17 * s, width: 12 * s, height: 5 * s), cornerRadius: 1.6 * s), with: .color(Color(hex: "#8A5530")))
                g.fill(Path(CGRect(x: -6 * s, y: -15 * s, width: 12 * s, height: 1.2 * s)), with: .color(Color(hex: "#D9B45A")))
            }
        }
        // What it holds before it.
        switch holding {
        case .folder:
            let folder = CGRect(x: x - 13 * s, y: y + 30 * s, width: 22 * s, height: 16 * s)
            c.fill(Path(roundedRect: folder, cornerRadius: 1.5 * s), with: .color(Color(hex: "#E2C27A")))
            c.fill(Path(roundedRect: CGRect(x: folder.minX + 2 * s, y: folder.minY - 3 * s, width: 9 * s, height: 4 * s), cornerRadius: 1 * s),
                   with: .color(Color(hex: "#D3AE5F")))
            c.fill(Path(CGRect(x: folder.minX + 3 * s, y: folder.minY + 3 * s, width: 16 * s, height: 1.2 * s)), with: .color(.white.opacity(0.7)))
            c.fill(Path(ellipseIn: CGRect(x: folder.maxX - 4 * s, y: folder.minY + 9 * s, width: 7 * s, height: 7 * s)), with: .color(look.skin))
        case .clipboard:
            let board = CGRect(x: x - 15 * s, y: y + 28 * s, width: 16 * s, height: 20 * s)
            c.fill(Path(roundedRect: board, cornerRadius: 1.5 * s), with: .color(Color(hex: "#7A5232")))
            c.fill(Path(CGRect(x: board.minX + 2 * s, y: board.minY + 3 * s, width: 12 * s, height: 15 * s)), with: .color(Color(hex: "#F4EFE4")))
            for k in 0..<4 {
                c.fill(Path(CGRect(x: board.minX + 3.5 * s, y: board.minY + (6 + CGFloat(k) * 3) * s, width: 9 * s, height: 0.8 * s)),
                       with: .color(Color.black.opacity(0.35)))
            }
            c.fill(Path(roundedRect: CGRect(x: board.midX - 3 * s, y: board.minY - 1 * s, width: 6 * s, height: 3 * s), cornerRadius: 1 * s),
                   with: .color(Color(hex: "#B8B8B8")))
        default: break
        }
        // Ears, head and its light.
        for side in [-1.0, 1.0] as [CGFloat] {
            c.fill(Path(ellipseIn: CGRect(x: x + side * 11 * s - 2.5 * s, y: y - 3 * s, width: 5 * s, height: 7 * s)), with: .color(look.skin.mix(.black, 0.08)))
        }
        let face = CGRect(x: x - 11 * s, y: y - 13.5 * s, width: 22 * s, height: 26 * s)
        c.fill(Path(ellipseIn: face), with: .radialGradient(Gradient(colors: [look.skin.mix(.white, 0.12), look.skin, look.skin.mix(.black, 0.18)]),
                                                            center: CGPoint(x: x - 3 * s, y: y - 5 * s), startRadius: 0, endRadius: 17 * s))
        hair(&c, look: look, x: x, y: y, s: s)
        // Brows, eyes and their light.
        for side in [-1.0, 1.0] as [CGFloat] {
            var brow = Path()
            brow.move(to: CGPoint(x: x + side * 2.2 * s, y: y - 5.2 * s))
            brow.addLine(to: CGPoint(x: x + side * 6.6 * s, y: y - 5.8 * s))
            c.stroke(brow, with: .color(look.style == .bald || look.style == .receding ? look.hair.mix(.black, 0.4) : look.hair.mix(.black, 0.2)),
                     style: StrokeStyle(lineWidth: 1.1 * s, lineCap: .round))
            let eye = CGRect(x: x + side * 4.3 * s - 1.3 * s, y: y - (blink ? 1.4 : 2.6) * s, width: 2.6 * s, height: (blink ? 0.5 : 2.9) * s)
            c.fill(Path(ellipseIn: eye), with: .color(Color(hex: "#1B1410")))
            if !blink {
                c.fill(Path(ellipseIn: CGRect(x: eye.midX - 0.2 * s, y: eye.minY + 0.5 * s, width: 0.8 * s, height: 0.8 * s)), with: .color(.white.opacity(0.85)))
            }
        }
        if look.glasses {
            for side in [-1.0, 1.0] as [CGFloat] {
                let lens = CGRect(x: x + side * 4.3 * s - 3.4 * s, y: y - 4.2 * s, width: 6.8 * s, height: 5.4 * s)
                c.stroke(Path(roundedRect: lens, cornerRadius: 1.8 * s), with: .color(Color(hex: "#1A1A1A").opacity(0.85)), lineWidth: 0.8 * s)
                c.fill(Path(roundedRect: lens, cornerRadius: 1.8 * s), with: .color(.white.opacity(0.08)))
            }
            var bridge = Path()
            bridge.move(to: CGPoint(x: x - 1 * s, y: y - 2 * s))
            bridge.addLine(to: CGPoint(x: x + 1 * s, y: y - 2 * s))
            c.stroke(bridge, with: .color(Color(hex: "#1A1A1A")), lineWidth: 0.8 * s)
        }
        // The nose's shadow, the cheeks.
        var nose = Path()
        nose.move(to: CGPoint(x: x + 0.4 * s, y: y - 1 * s))
        nose.addQuadCurve(to: CGPoint(x: x - 0.6 * s, y: y + 3.4 * s), control: CGPoint(x: x + 1.8 * s, y: y + 2.4 * s))
        c.stroke(nose, with: .color(look.skin.mix(.black, 0.35).opacity(0.6)), lineWidth: 0.8 * s)
        for side in [-1.0, 1.0] as [CGFloat] {
            c.fill(Path(ellipseIn: CGRect(x: x + side * 6 * s - 2.5 * s, y: y + 2 * s, width: 5 * s, height: 3 * s)), with: .color(Color(hex: "#E8746A").opacity(0.14)))
        }
        if look.beard {
            var beard = Path()
            beard.move(to: CGPoint(x: x - 10.5 * s, y: y + 1 * s))
            beard.addQuadCurve(to: CGPoint(x: x, y: y + 14 * s), control: CGPoint(x: x - 10 * s, y: y + 13 * s))
            beard.addQuadCurve(to: CGPoint(x: x + 10.5 * s, y: y + 1 * s), control: CGPoint(x: x + 10 * s, y: y + 13 * s))
            beard.addQuadCurve(to: CGPoint(x: x - 10.5 * s, y: y + 1 * s), control: CGPoint(x: x, y: y + 9 * s))
            c.fill(beard, with: .color(look.hair.opacity(0.92)))
        }
        // The mouth: open as it speaks.
        if mouth > 0.06 {
            let lips = CGRect(x: x - 2.8 * s, y: y + 5.6 * s, width: 5.6 * s, height: (1 + 3.4 * mouth) * s)
            c.fill(Path(ellipseIn: lips), with: .color(Color(hex: "#5A1E1A")))
            c.fill(Path(ellipseIn: CGRect(x: lips.minX + 1 * s, y: lips.maxY - 1.2 * s, width: lips.width - 2 * s, height: 1 * s)),
                   with: .color(Color(hex: "#C25A55").opacity(0.7)))
        } else {
            var smile = Path()
            smile.move(to: CGPoint(x: x - 2.6 * s, y: y + 6.6 * s))
            smile.addQuadCurve(to: CGPoint(x: x + 2.6 * s, y: y + 6.6 * s), control: CGPoint(x: x, y: y + 7.8 * s))
            c.stroke(smile, with: .color(Color(hex: "#6A2A24").opacity(0.85)), style: StrokeStyle(lineWidth: 0.9 * s, lineCap: .round))
        }
    }

    private static func clothes(_ c: inout GraphicsContext, look: CourtLook, x: CGFloat, y: CGFloat, s: CGFloat) {
        switch look.wear {
        case .robe:
            // The robe's white band at the throat and its folds.
            var band = Path()
            band.move(to: CGPoint(x: x - 7 * s, y: y + 12 * s))
            band.addLine(to: CGPoint(x: x, y: y + 22 * s))
            band.addLine(to: CGPoint(x: x + 7 * s, y: y + 12 * s))
            band.closeSubpath()
            c.fill(band, with: .color(Color(hex: "#F6F3EC")))
            c.fill(Path(roundedRect: CGRect(x: x - 2.5 * s, y: y + 17 * s, width: 5 * s, height: 10 * s), cornerRadius: 1 * s), with: .color(Color(hex: "#F6F3EC")))
            for k in [-14.0, 14.0] as [CGFloat] {
                var fold = Path()
                fold.move(to: CGPoint(x: x + k * s, y: y + 26 * s))
                fold.addLine(to: CGPoint(x: x + k * 1.2 * s, y: y + 70 * s))
                c.stroke(fold, with: .color(.white.opacity(0.07)), lineWidth: 1.2 * s)
            }
        case .suit, .blouse:
            var shirt = Path()
            shirt.move(to: CGPoint(x: x - 7 * s, y: y + 12 * s))
            shirt.addLine(to: CGPoint(x: x, y: y + 30 * s))
            shirt.addLine(to: CGPoint(x: x + 7 * s, y: y + 12 * s))
            shirt.closeSubpath()
            c.fill(shirt, with: .color(Color(hex: "#F2F2F0")))
            if let tie = look.tie {
                var t = Path()
                t.move(to: CGPoint(x: x - 1.8 * s, y: y + 13 * s))
                t.addLine(to: CGPoint(x: x + 1.8 * s, y: y + 13 * s))
                t.addLine(to: CGPoint(x: x + 2.8 * s, y: y + 28 * s))
                t.addLine(to: CGPoint(x: x, y: y + 31 * s))
                t.addLine(to: CGPoint(x: x - 2.8 * s, y: y + 28 * s))
                t.closeSubpath()
                c.fill(t, with: .color(tie))
            }
            // Lapels.
            for side in [-1.0, 1.0] as [CGFloat] {
                var lapel = Path()
                lapel.move(to: CGPoint(x: x + side * 7 * s, y: y + 12 * s))
                lapel.addLine(to: CGPoint(x: x + side * 1.5 * s, y: y + 32 * s))
                lapel.addLine(to: CGPoint(x: x + side * 11 * s, y: y + 20 * s))
                lapel.closeSubpath()
                c.fill(lapel, with: .color(look.outfit.mix(.black, 0.22)))
            }
        case .shirt:
            var collar = Path()
            collar.move(to: CGPoint(x: x - 7 * s, y: y + 11 * s))
            collar.addLine(to: CGPoint(x: x - 1 * s, y: y + 18 * s))
            collar.addLine(to: CGPoint(x: x + 7 * s, y: y + 11 * s))
            c.stroke(collar, with: .color(.white.opacity(0.75)), lineWidth: 1.6 * s)
            // A lanyard and its badge.
            for side in [-1.0, 1.0] as [CGFloat] {
                var cord = Path()
                cord.move(to: CGPoint(x: x + side * 5 * s, y: y + 12 * s))
                cord.addLine(to: CGPoint(x: x + side * 1.5 * s, y: y + 31 * s))
                c.stroke(cord, with: .color(AXYONTheme.current.accent), lineWidth: 1.1 * s)
            }
            c.fill(Path(roundedRect: CGRect(x: x - 4.5 * s, y: y + 30 * s, width: 9 * s, height: 11 * s), cornerRadius: 1.4 * s), with: .color(.white))
            c.fill(Path(CGRect(x: x - 3.5 * s, y: y + 31.5 * s, width: 7 * s, height: 3 * s)), with: .color(AXYONTheme.current.accent))
        case .sweater:
            var neck = Path()
            neck.addArc(center: CGPoint(x: x, y: y + 11 * s), radius: 7.5 * s, startAngle: .degrees(10), endAngle: .degrees(170), clockwise: false)
            c.stroke(neck, with: .color(look.outfit.mix(.white, 0.25)), lineWidth: 2.2 * s)
        case .cardigan:
            var shirt = Path()
            shirt.move(to: CGPoint(x: x - 7 * s, y: y + 12 * s))
            shirt.addLine(to: CGPoint(x: x, y: y + 34 * s))
            shirt.addLine(to: CGPoint(x: x + 7 * s, y: y + 12 * s))
            shirt.closeSubpath()
            c.fill(shirt, with: .color(Color(hex: "#EDE6D8")))
            for k in 0..<3 {
                c.fill(Path(ellipseIn: CGRect(x: x + 4 * s, y: y + (22 + CGFloat(k) * 8) * s, width: 2 * s, height: 2 * s)), with: .color(Color(hex: "#D9B45A")))
            }
        }
    }

    private static func hair(_ c: inout GraphicsContext, look: CourtLook, x: CGFloat, y: CGFloat, s: CGFloat) {
        let hair = look.hair
        switch look.style {
        case .short, .buzz:
            var cap = Path()
            cap.move(to: CGPoint(x: x - 11.5 * s, y: y - 2 * s))
            cap.addQuadCurve(to: CGPoint(x: x, y: y - 15.5 * s), control: CGPoint(x: x - 12 * s, y: y - 15 * s))
            cap.addQuadCurve(to: CGPoint(x: x + 11.5 * s, y: y - 2 * s), control: CGPoint(x: x + 12 * s, y: y - 15 * s))
            cap.addQuadCurve(to: CGPoint(x: x + 3 * s, y: y - 9.5 * s), control: CGPoint(x: x + 9 * s, y: y - 9 * s))
            cap.addQuadCurve(to: CGPoint(x: x - 11.5 * s, y: y - 2 * s), control: CGPoint(x: x - 8 * s, y: y - 10 * s))
            c.fill(cap, with: .color(look.style == .buzz ? hair.opacity(0.75) : hair))
        case .long, .bob:
            var top = Path()
            top.move(to: CGPoint(x: x - 12 * s, y: y + 2 * s))
            top.addQuadCurve(to: CGPoint(x: x + 1 * s, y: y - 16 * s), control: CGPoint(x: x - 13 * s, y: y - 16 * s))
            top.addQuadCurve(to: CGPoint(x: x + 12 * s, y: y + 2 * s), control: CGPoint(x: x + 13 * s, y: y - 15 * s))
            top.addQuadCurve(to: CGPoint(x: x + 8 * s, y: y - 8 * s), control: CGPoint(x: x + 11 * s, y: y - 6 * s))
            top.addQuadCurve(to: CGPoint(x: x - 9 * s, y: y - 6 * s), control: CGPoint(x: x - 2 * s, y: y - 11 * s))
            top.addQuadCurve(to: CGPoint(x: x - 12 * s, y: y + 2 * s), control: CGPoint(x: x - 11 * s, y: y - 3 * s))
            c.fill(top, with: .color(hair))
            var shine = Path()
            shine.move(to: CGPoint(x: x - 7 * s, y: y - 11 * s))
            shine.addQuadCurve(to: CGPoint(x: x + 2 * s, y: y - 14 * s), control: CGPoint(x: x - 3 * s, y: y - 14 * s))
            c.stroke(shine, with: .color(.white.opacity(0.22)), lineWidth: 1 * s)
        case .bun:
            c.fill(Path(ellipseIn: CGRect(x: x - 6 * s, y: y - 23 * s, width: 12 * s, height: 10 * s)), with: .color(hair))
            var top = Path()
            top.move(to: CGPoint(x: x - 11.5 * s, y: y - 1 * s))
            top.addQuadCurve(to: CGPoint(x: x + 11.5 * s, y: y - 1 * s), control: CGPoint(x: x, y: y - 24 * s))
            top.addQuadCurve(to: CGPoint(x: x - 11.5 * s, y: y - 1 * s), control: CGPoint(x: x, y: y - 9 * s))
            c.fill(top, with: .color(hair))
        case .curly:
            for k in 0..<9 {
                let a = Angle.degrees(195 + Double(k) * 18.5).radians
                let cx = x + CGFloat(cos(a)) * 11 * s
                let cy = y - 3 * s + CGFloat(sin(a)) * 12 * s
                c.fill(Path(ellipseIn: CGRect(x: cx - 4.2 * s, y: cy - 4.2 * s, width: 8.4 * s, height: 8.4 * s)), with: .color(hair))
            }
            c.fill(Path(ellipseIn: CGRect(x: x - 9 * s, y: y - 17 * s, width: 18 * s, height: 10 * s)), with: .color(hair))
        case .bald:
            c.fill(Path(ellipseIn: CGRect(x: x - 5 * s, y: y - 12.5 * s, width: 7 * s, height: 3.5 * s)), with: .color(.white.opacity(0.25)))
        case .receding:
            for side in [-1.0, 1.0] as [CGFloat] {
                var tuft = Path()
                tuft.move(to: CGPoint(x: x + side * 11.5 * s, y: y + 1 * s))
                tuft.addQuadCurve(to: CGPoint(x: x + side * 6 * s, y: y - 13 * s), control: CGPoint(x: x + side * 13 * s, y: y - 11 * s))
                tuft.addQuadCurve(to: CGPoint(x: x + side * 9 * s, y: y - 2 * s), control: CGPoint(x: x + side * 8 * s, y: y - 7 * s))
                tuft.closeSubpath()
                c.fill(tuft, with: .color(hair))
            }
            c.fill(Path(ellipseIn: CGRect(x: x - 5 * s, y: y - 12.5 * s, width: 7 * s, height: 3 * s)), with: .color(.white.opacity(0.22)))
        }
    }
}

private extension Color {
    /// This color with `amount` of `other` in it.
    func mix(_ other: Color, _ amount: Double) -> Color {
        let a = UIColor(self)
        let b = UIColor(other)
        var (r1, g1, b1, a1): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        var (r2, g2, b2, a2): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        a.getRed(&r1, green: &g1, blue: &b1, alpha: &a1)
        b.getRed(&r2, green: &g2, blue: &b2, alpha: &a2)
        let k = CGFloat(amount)
        return Color(red: Double(r1 + (r2 - r1) * k), green: Double(g1 + (g2 - g1) * k), blue: Double(b1 + (b2 - b1) * k),
                     opacity: Double(a1 + (a2 - a1) * k))
    }
}

// MARK: - A portrait

/// One of the court, head and shoulders, in a circle lit by the accent: in the transcript beside what they say.
struct CourtPortrait: View {
    let role: CourtRole
    var minds: [InvestMind] = []
    var lit = false
    var size: CGFloat = 46

    var body: some View {
        Canvas { context, canvas in
            let s = canvas.width / 52
            let look = CourtLook.of(role, minds: minds)
            context.fill(Path(ellipseIn: CGRect(origin: .zero, size: canvas)), with: .linearGradient(
                Gradient(colors: role == .judge ? [Color(hex: "#E9DFC9"), Color(hex: "#BFAE8B")] : [Color(hex: "#4A2E1E"), Color(hex: "#24150E")]),
                startPoint: .zero, endPoint: CGPoint(x: canvas.width, y: canvas.height)))
            var c = context
            c.clip(to: Path(ellipseIn: CGRect(origin: .zero, size: canvas)))
            CourtFigure.draw(&c, look: look, head: CGPoint(x: canvas.width / 2, y: canvas.height * 0.44), s: s, mouth: 0, blink: false,
                             breath: 0, arm: 0, holding: .none, now: 0)
        }
        .frame(width: size, height: size)
        .overlay(Circle().strokeBorder(lit ? AXYONTheme.current.accent : Color.white.opacity(0.18), lineWidth: lit ? 2 : 0.8))
        .shadow(color: lit ? AXYONTheme.current.accent.opacity(0.7) : .clear, radius: 8)
        .accessibilityHidden(true)
    }
}

// MARK: - The trial

/// A meeting of the committee as a trial: the courtroom above, alive, and below what each one says as they say it,
/// each point with the sources it cites (a tap opens one). It plays by itself; a pause, everything at once, again from
/// the start. At the end, the sentence, and the full record (every point, fact and document) a tap away.
struct InvestTrialView: View {
    let verdict: InvestVerdict
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var lines: [CourtLine] = []
    @State private var shown = 0
    @State private var typed = 0
    @State private var playing = true
    @State private var speaker: CourtRole?
    @State private var since = Date.now
    @State private var opened = Date.now
    @State private var voted: Set<Int> = []
    @State private var sentence: Date?
    @State private var clerkCame: Date?
    @State private var player: Task<Void, Never>?
    @State private var citation: InvestCitation?

    private var finished: Bool { shown >= lines.count && player == nil }

    var body: some View {
        VStack(spacing: 10) {
            CourtroomScene(verdict: verdict, speaker: speaker, since: since, opened: opened, voted: voted, sentence: sentence, clerkCame: clerkCame)
                .frame(height: 292)
                .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
                .overlay(alignment: .topLeading) { plaque.padding(10) }
                .overlay(RoundedRectangle(cornerRadius: 26, style: .continuous).strokeBorder(AXYONPalette.rim, lineWidth: 1))
                .shadow(color: .black.opacity(0.5), radius: 18, y: 10)
                .padding(.horizontal, 14)
            controls
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(lines.prefix(shown)) { line in
                            CourtLineView(line: line, minds: verdict.minds, typed: line.id == shown - 1 && !finished ? typed : nil,
                                          current: line.id == shown - 1 && speaker != nil) { citation = $0 }
                                .id(line.id)
                                .transition(.move(edge: .bottom).combined(with: .opacity))
                        }
                        if shown >= lines.count && !lines.isEmpty {
                            ruling.id("ruling").transition(.scale(scale: 0.9).combined(with: .opacity))
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
                .onChange(of: shown) {
                    withAnimation(.easeOut(duration: 0.35)) {
                        proxy.scrollTo(shown >= lines.count ? AnyHashable("ruling") : AnyHashable(shown - 1), anchor: .bottom)
                    }
                }
            }
        }
        .padding(.top, 6)
        .background { InvestBackdrop() }
        .navigationTitle(verdict.ticker)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $citation) { citation in
            InvestSourceSheet(citation: citation, verdict: verdict)
        }
        .onAppear(perform: start)
        .onDisappear {
            player?.cancel()
            player = nil
        }
    }

    /// The session's plate: the court, the asset, the day, and whether it is sitting.
    private var plaque: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(speaker != nil ? Color(hex: "#FF4D5E") : AXYONPalette.dim)
                .frame(width: 7, height: 7)
                .shadow(color: speaker != nil ? Color(hex: "#FF4D5E") : .clear, radius: 4)
            Text(speaker != nil ? String(localized: "IN SESSION") : String(localized: "SESSION CLOSED"))
                .font(.system(size: 10, weight: .heavy))
                .tracking(0.8)
            Text(verbatim: "· \(verdict.ticker)" + (verdict.date.isEmpty ? "" : " · " + investDay(verdict.date)))
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.white.opacity(0.8))
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(Capsule().fill(.black.opacity(0.45)))
        .overlay(Capsule().strokeBorder(.white.opacity(0.15), lineWidth: 0.6))
    }

    private var controls: some View {
        HStack(spacing: 10) {
            AXYONRoundButton(systemImage: "backward.end.fill", size: 40) { restart() }
                .accessibilityLabel(Text("From the start"))
            AXYONRoundButton(systemImage: finished ? "checkmark" : (playing ? "pause.fill" : "play.fill"), size: 46, lit: !finished) {
                if !finished { playing.toggle() }
            }
            .accessibilityLabel(Text(playing ? LocalizedStringKey("Pause") : LocalizedStringKey("Play")))
            AXYONRoundButton(systemImage: "forward.end.fill", size: 40) { showAll() }
                .accessibilityLabel(Text("Everything at once"))
            Spacer(minLength: 6)
            VStack(alignment: .trailing, spacing: 2) {
                Text(speaker.map { line(for: $0)?.name ?? $0.title } ?? String(localized: "The court"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text("\(min(shown, lines.count)) of \(lines.count)")
                    .font(.caption2)
                    .monospacedDigit()
                    .foregroundStyle(AXYONPalette.dim)
            }
        }
        .padding(.horizontal, 18)
    }

    private func line(for role: CourtRole) -> CourtLine? {
        lines.prefix(shown).last { $0.role == role }
    }

    /// The sentence, apart: the score ring, the ruling, the vetoes, and the full record.
    private var ruling: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 14) {
                Image(systemName: "hammer.fill")
                    .font(.title2)
                    .foregroundStyle(.white)
                    .frame(width: 50, height: 50)
                    .background(Circle().fill(AXYONTheme.current.accentGradient))
                    .axyonGlow(radius: 12, strength: 0.6)
                VStack(alignment: .leading, spacing: 3) {
                    Text("The sentence").font(.caption.weight(.heavy)).tracking(0.6).foregroundStyle(AXYONPalette.dim)
                    Text(verdict.portfolio.isEmpty ? verdict.score : verdict.portfolio.viewName)
                        .font(.title3.weight(.bold))
                        .foregroundStyle(verdict.portfolio.isEmpty ? .white : InvestPalette.vote(verdict.portfolio.tone))
                }
                Spacer(minLength: 4)
                InvestVoteRing(minds: verdict.minds, score: verdict.score)
            }
            if !verdict.question.isEmpty {
                Text(verdict.question).font(.callout.weight(.medium)).foregroundStyle(.white)
            }
            if verdict.vetoed {
                InvestTag(text: String(localized: "Vetoed by \(verdict.vetoedBy.map { InvestMind.name($0) }.joined(separator: ", "))"),
                          color: InvestPalette.against)
            }
            NavigationLink {
                InvestVerdictView(verdict: verdict)
            } label: {
                Label("The full record: every point, fact and document", systemImage: "books.vertical.fill")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Capsule().fill(Color.white.opacity(0.1)))
                    .overlay(Capsule().strokeBorder(AXYONPalette.rim, lineWidth: 0.8))
            }
            .buttonStyle(.plain)
            Text("The court never orders: its people are archetypes, every number is the code's and every point cites its source. Nothing is bought or sold; this is not an investment recommendation.")
                .font(.caption2)
                .foregroundStyle(AXYONPalette.faint)
        }
        .padding(18)
        .axyonCard(cornerRadius: 24, tint: AXYONTheme.current.accent)
    }

    // MARK: Playing

    private func start() {
        guard lines.isEmpty else { return }
        lines = CourtScript.lines(verdict)
        opened = .now
        #if targetEnvironment(simulator)
        if AXYONTestScreen.base == "invest-trial" {
            // CI's screenshots: a plea, the jury voting, the sentence.
            let anchor = AXYONTestScreen.anchor
            if anchor == .bottom {
                showAll()
                return
            }
            let at = anchor != nil ? (lines.lastIndex { if case .vote = $0.moment { return true } else { return false } } ?? 0)
                : (lines.firstIndex { $0.role == .defense } ?? min(1, lines.count - 1))
            for i in 0...at { begin(i) }
            typed = lines[at].text.count
            opened = .distantPast
            clerkCame = clerkCame.map { _ in .distantPast }
            playing = false
            return
        }
        #endif
        if reduceMotion {
            showAll()
        } else {
            play(from: 0)
        }
    }

    private func begin(_ i: Int) {
        let line = lines[i]
        withAnimation(.spring(duration: 0.4, bounce: 0.2)) {
            shown = i + 1
        }
        typed = 0
        speaker = line.role
        since = .now
        switch line.moment {
        case .vote(let m): voted.insert(m)
        case .sentence:
            sentence = .now
            Task {
                try? await Task.sleep(for: .milliseconds(450))
                Haptics.impact()
            }
        default: break
        }
        if line.role == .clerk && clerkCame == nil { clerkCame = .now }
    }

    private func play(from first: Int) {
        player?.cancel()
        player = Task { @MainActor in
            var i = first
            while i < lines.count {
                if Task.isCancelled { return }
                begin(i)
                let count = lines[i].text.count
                let step = max(1, count / 70)
                var c = 0
                while c < count {
                    if Task.isCancelled { return }
                    while !playing {
                        try? await Task.sleep(for: .milliseconds(150))
                        if Task.isCancelled { return }
                    }
                    c = min(count, c + step)
                    typed = c
                    try? await Task.sleep(for: .milliseconds(26))
                }
                let pause = min(6.5, 1.2 + Double(count) / 42)
                var waited = 0.0
                while waited < pause {
                    if Task.isCancelled { return }
                    try? await Task.sleep(for: .milliseconds(150))
                    if playing { waited += 0.15 }
                }
                i += 1
            }
            speaker = nil
            player = nil
        }
    }

    private func showAll() {
        player?.cancel()
        player = nil
        withAnimation(.spring(duration: 0.5, bounce: 0.15)) {
            shown = lines.count
        }
        typed = Int.max
        speaker = nil
        voted = Set(verdict.minds.indices)
        if sentence == nil { sentence = .now }
        if clerkCame == nil { clerkCame = .distantPast }
    }

    private func restart() {
        player?.cancel()
        player = nil
        shown = 0
        typed = 0
        voted = []
        sentence = nil
        clerkCame = nil
        speaker = nil
        playing = true
        opened = .now
        play(from: 0)
    }
}

/// What one of the court says: their portrait, who they are, the words (typed as they speak), the sources cited.
struct CourtLineView: View {
    let line: CourtLine
    let minds: [InvestMind]
    var typed: Int?
    var current = false
    var open: (InvestCitation) -> Void

    private var color: Color {
        switch line.tone {
        case "for", "against", "neutral": InvestPalette.vote(line.tone)
        default: .white
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            CourtPortrait(role: line.role, minds: minds, lit: current, size: 44)
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(line.name).font(.subheadline.weight(.bold)).foregroundStyle(.white).lineLimit(1)
                    if case .juror = line.role {
                        InvestTag(text: String(localized: "juror"), color: AXYONPalette.dim)
                    }
                    if case .vote = line.moment {
                        InvestTag(text: InvestMind.voteName(line.tone), color: color)
                    }
                    if line.moment == .sentence {
                        InvestTag(text: String(localized: "sentence"), color: AXYONTheme.current.accent)
                    }
                }
                Text(typed.map { String(line.text.prefix($0)) } ?? line.text)
                    .font(line.moment == .sentence ? .callout.weight(.semibold) : .callout)
                    .foregroundStyle(Color.white.opacity(0.9))
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if typed == nil || typed ?? 0 >= line.text.count {
                    InvestCitations(text: line.cites, open: open)
                }
            }
            .padding(14)
            .background {
                BubbleShape()
                    .fill(Color.white.opacity(current ? 0.1 : 0.06))
                    .overlay(BubbleShape().stroke(current ? AXYONTheme.current.accent.opacity(0.8) : AXYONPalette.rim, lineWidth: current ? 1.2 : 0.8))
            }
            .overlay(alignment: .leading) {
                if line.tone == "for" || line.tone == "against" {
                    Capsule().fill(color).frame(width: 3).padding(.vertical, 12).offset(x: 1)
                }
            }
        }
    }
}

/// A speech bubble with its tail toward the portrait.
private struct BubbleShape: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path(roundedRect: rect, cornerRadius: 18, style: .continuous)
        var tail = Path()
        tail.move(to: CGPoint(x: rect.minX + 1, y: rect.minY + 14))
        tail.addLine(to: CGPoint(x: rect.minX - 7, y: rect.minY + 19))
        tail.addLine(to: CGPoint(x: rect.minX + 1, y: rect.minY + 25))
        tail.closeSubpath()
        p.addPath(tail)
        return p
    }
}

// MARK: - The court on the investing screen

/// The court sits by itself: whenever the intern brings a new document, it meets on that asset and its trial goes into
/// the asset's folder. Here: whether it is sitting now, and its last sessions.
struct InvestCourtCard: View {
    @Environment(HermesEngine.self) private var engine
    let invest: AXYONInvest

    private var verdicts: [InvestVerdict] { invest.overview?.verdicts ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("The court").font(.title3.weight(.bold)).foregroundStyle(.white)
                    Text("It meets by itself on every new document the intern brings, and each trial goes into the asset's folder.")
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                }
                Spacer(minLength: 0)
                Image(systemName: "building.columns.fill")
                    .font(.title3)
                    .foregroundStyle(AXYONTheme.current.accent)
            }
            if let first = verdicts.first {
                NavigationLink {
                    InvestTrialView(verdict: first)
                } label: {
                    CourtroomScene(verdict: first, voted: Set(first.minds.indices), still: true)
                        .frame(height: 150)
                        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .overlay(alignment: .bottomLeading) {
                            Label(invest.meeting == nil ? String(localized: "Last session: \(first.ticker)") : String(localized: "In session"),
                                  systemImage: invest.meeting == nil ? "play.circle.fill" : "dot.radiowaves.left.and.right")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(Capsule().fill(.black.opacity(0.55)))
                                .padding(10)
                        }
                        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(AXYONPalette.rim, lineWidth: 0.8))
                }
                .buttonStyle(PressableButtonStyle())
            }
            if let meeting = invest.meeting {
                HStack(spacing: 10) {
                    Image(systemName: "hammer.fill")
                        .foregroundStyle(AXYONTheme.current.accent)
                        .symbolEffect(.bounce, options: .repeating)
                    Text("In session on \(meeting)…").font(.footnote).foregroundStyle(.white).lineLimit(2)
                    Spacer(minLength: 0)
                    ProgressView().tint(.white)
                }
            } else if !engine.canChat {
                Text("The court needs AXYON Core running with a brain: see Settings.")
                    .font(.footnote)
                    .foregroundStyle(AXYONPalette.dim)
            } else if let due = invest.overview?.meetings.first {
                Label(String(localized: "Next on the docket: \(due.about)"), systemImage: "bell.badge")
                    .font(.footnote)
                    .foregroundStyle(InvestPalette.mixed)
                    .lineLimit(3)
            }
            ForEach(verdicts.prefix(3)) { verdict in
                NavigationLink {
                    InvestTrialView(verdict: verdict)
                } label: {
                    InvestVerdictRow(verdict: verdict)
                }
                .buttonStyle(.plain)
            }
            if verdicts.count > 3 {
                NavigationLink {
                    InvestVerdictsList(verdicts: verdicts)
                } label: {
                    Text("All the sessions").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(18)
        .axyonCard(cornerRadius: 26)
    }
}
