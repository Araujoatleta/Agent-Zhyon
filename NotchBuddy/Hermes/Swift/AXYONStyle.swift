import SwiftUI

// AXYON's look on the iPhone with Hermes inside (the build that carries it): black, with a slow aurora behind the
// glass so Liquid Glass has light to bend, glass cards with a lit rim, and Hermes' orb, whose color says what he is
// doing. Built on CoucouGlass (real Liquid Glass on iOS 26, frosted before), never glass on glass.

enum AXYONPalette {
    /// Behind everything: true black, so the aurora and the glass carry all the light.
    static let void = Color.black
    /// Cards that are not glass: #08080D.
    static let ink = Color(red: 0.031, green: 0.031, blue: 0.051)
    /// AXYON's violet: #8B5CF6.
    static let violet = Color(red: 0.545, green: 0.361, blue: 0.965)
    /// Its cyan: #22D3EE.
    static let cyan = Color(red: 0.133, green: 0.827, blue: 0.933)
    /// Its magenta: #E879F9.
    static let magenta = Color(red: 0.910, green: 0.475, blue: 0.976)
    static let gold = Color(red: 0.984, green: 0.749, blue: 0.141)
    static let up = Color(red: 0.204, green: 0.827, blue: 0.600)
    static let down = Color(red: 0.973, green: 0.443, blue: 0.443)
    static let dim = Color.white.opacity(0.55)
    static let faint = Color.white.opacity(0.32)

    /// The rim light of a card: brighter at the top left, where the light comes from.
    static let rim = LinearGradient(colors: [.white.opacity(0.42), .white.opacity(0.06), violet.opacity(0.35)],
                                    startPoint: .topLeading, endPoint: .bottomTrailing)
}

/// The black behind every AXYON screen, with three slow lights drifting under it. Twenty frames a second at most, and
/// none while the screen does not show.
struct AXYONBackdrop: View {
    var intensity: Double = 1
    @State private var visible = false

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 20, paused: !visible)) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            Canvas { context, size in
                context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(AXYONPalette.void))
                let lights: [(Color, Double, Double, Double, Double)] = [
                    (AXYONPalette.violet, 0.18, 0.12, 0.62, 0.050),
                    (AXYONPalette.cyan, 0.86, 0.36, 0.48, 0.037),
                    (AXYONPalette.magenta, 0.32, 0.92, 0.55, 0.043),
                ]
                for (index, light) in lights.enumerated() {
                    let (color, x, y, radius, speed) = light
                    let phase = t * speed + Double(index) * 2.1
                    let center = CGPoint(x: size.width * (x + 0.08 * sin(phase)),
                                         y: size.height * (y + 0.06 * cos(phase * 1.3)))
                    let r = max(size.width, size.height) * radius
                    let gradient = Gradient(colors: [color.opacity(0.30 * intensity), color.opacity(0.08 * intensity), .clear])
                    context.fill(Path(ellipseIn: CGRect(x: center.x - r, y: center.y - r, width: r * 2, height: r * 2)),
                                 with: .radialGradient(gradient, center: center, startRadius: 0, endRadius: r))
                }
            }
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onAppear { visible = true }
        .onDisappear { visible = false }
    }
}

extension View {
    /// A glass card with AXYON's lit rim and a glow of `glow` under it.
    func axyonCard(cornerRadius: CGFloat = 28, tint: Color? = nil, glow: Color? = nil) -> some View {
        self
            .coucouGlass(.regular, cornerRadius: cornerRadius, tint: tint)
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(AXYONPalette.rim, lineWidth: 0.8)
                    .allowsHitTesting(false)
            }
            .shadow(color: (glow ?? .clear).opacity(0.35), radius: 24, y: 10)
    }

    /// The black and its aurora behind a List or a ScrollView.
    func axyonBackdrop(intensity: Double = 1) -> some View {
        scrollContentBackground(.hidden)
            .background { AXYONBackdrop(intensity: intensity) }
    }
}

/// What Hermes is doing, as his orb shows it.
enum AXYONMood: Equatable {
    case sleeping, starting, idle, thinking, acting, speaking, trouble

    var colors: [Color] {
        switch self {
        case .sleeping: [Color.gray.opacity(0.6), Color.gray.opacity(0.25)]
        case .starting: [AXYONPalette.cyan, AXYONPalette.violet]
        case .idle: [AXYONPalette.violet, AXYONPalette.cyan, AXYONPalette.magenta]
        case .thinking: [AXYONPalette.magenta, AXYONPalette.violet, AXYONPalette.cyan]
        case .acting: [AXYONPalette.gold, AXYONPalette.magenta, AXYONPalette.violet]
        case .speaking: [AXYONPalette.cyan, AXYONPalette.up, AXYONPalette.violet]
        case .trouble: [AXYONPalette.down, AXYONPalette.magenta]
        }
    }

    /// Turns a second at rest, faster when he works.
    var speed: Double {
        switch self {
        case .sleeping: 0.05
        case .starting, .thinking: 0.9
        case .acting: 1.2
        case .speaking: 0.6
        case .idle: 0.18
        case .trouble: 0.1
        }
    }
}

/// Hermes' orb: a sphere of light turning on itself, breathing, its colors his mood. Drawn in code.
struct AXYONOrb: View {
    var mood: AXYONMood
    var size: CGFloat = 120
    @State private var visible = false

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !visible)) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            let breath = 1 + 0.04 * sin(t * (mood == .idle ? 1.4 : 3.2))
            ZStack {
                // The glow around him.
                Circle()
                    .fill(RadialGradient(colors: [mood.colors[0].opacity(0.55), .clear],
                                         center: .center, startRadius: size * 0.2, endRadius: size * 0.75))
                    .frame(width: size * 1.6, height: size * 1.6)
                    .blur(radius: 18)
                // His body: a turning gradient, lit from the top left.
                Circle()
                    .fill(AngularGradient(colors: mood.colors + [mood.colors[0]], center: .center,
                                          angle: .degrees(t * 360 * mood.speed)))
                    .overlay {
                        Circle().fill(RadialGradient(colors: [.white.opacity(0.55), .clear],
                                                     center: UnitPoint(x: 0.32, y: 0.28),
                                                     startRadius: 0, endRadius: size * 0.42))
                    }
                    .overlay { Circle().strokeBorder(.white.opacity(0.35), lineWidth: 1) }
                    .frame(width: size, height: size)
                    .blur(radius: 0.5)
                    .shadow(color: mood.colors[0].opacity(0.6), radius: 20)
                // A ring that turns the other way while he works.
                if mood == .thinking || mood == .acting || mood == .starting {
                    Circle()
                        .trim(from: 0, to: 0.28)
                        .stroke(LinearGradient(colors: [.white, .clear], startPoint: .leading, endPoint: .trailing),
                                style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                        .frame(width: size * 1.18, height: size * 1.18)
                        .rotationEffect(.degrees(-t * 220))
                }
            }
            .scaleEffect(breath)
        }
        .frame(width: size * 1.3, height: size * 1.3)
        .accessibilityHidden(true)
        .onAppear { visible = true }
        .onDisappear { visible = false }
    }
}

/// A small capsule of glass: an icon and a word.
struct AXYONChip: View {
    var title: String
    var systemImage: String
    var tint: Color? = nil

    var body: some View {
        Label(title, systemImage: systemImage)
            .font(.footnote.weight(.semibold))
            .lineLimit(1)
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .coucouGlass(.regular, in: Capsule(), tint: tint, interactive: true)
    }
}

/// A section's title over its cards: small capitals, dimmed.
struct AXYONHeader: View {
    var title: String
    var systemImage: String? = nil

    var body: some View {
        HStack(spacing: 6) {
            if let systemImage { Image(systemName: systemImage) }
            Text(title.uppercased())
        }
        .font(.caption.weight(.bold))
        .kerning(1.2)
        .foregroundStyle(AXYONPalette.faint)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 6)
    }
}

/// A big number with its label under it, for the dashboard's cards.
struct AXYONFigure: View {
    var value: String
    var label: String
    var color: Color = .white

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value)
                .font(.system(.title2, design: .rounded, weight: .bold))
                .foregroundStyle(color)
                .contentTransition(.numericText())
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            Text(label)
                .font(.caption)
                .foregroundStyle(AXYONPalette.dim)
                .lineLimit(1)
        }
    }
}
