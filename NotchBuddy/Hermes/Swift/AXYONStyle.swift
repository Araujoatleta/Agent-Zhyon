import SwiftUI

// AXYON's look on the iPhone with Hermes inside (the build that carries it): Apple's own dark. True black, white type,
// Liquid Glass bending a faint light from above, and no colour but the system's green and red, for money and for
// what failed. Built on CoucouGlass (real Liquid Glass on iOS 26, frosted before), never glass on glass.

enum AXYONPalette {
    /// Behind everything: true black.
    static let void = Color.black
    /// Text, icons and the one thing to tap.
    static let light = Color.white
    /// Gains, what is on, what worked: the system's green.
    static let up = CoucouPalette.ok
    /// Losses and what failed: the system's red.
    static let down = CoucouPalette.danger
    static let dim = Color.white.opacity(0.6)
    static let faint = Color.white.opacity(0.32)

    /// A glass edge: a hairline of light, brighter at the top left, where the light comes from.
    static let rim = LinearGradient(colors: [.white.opacity(0.30), .white.opacity(0.05), .white.opacity(0.10)],
                                    startPoint: .topLeading, endPoint: .bottomTrailing)
}

/// The black behind every AXYON screen, with a faint light from above for the glass to bend. Drawn once: nothing
/// moves, so it costs nothing while it shows.
struct AXYONBackdrop: View {
    var intensity: Double = 1

    var body: some View {
        ZStack {
            AXYONPalette.void
            EllipticalGradient(colors: [.white.opacity(0.10 * intensity), .white.opacity(0.025 * intensity), .clear],
                               center: UnitPoint(x: 0.5, y: -0.08), startRadiusFraction: 0, endRadiusFraction: 0.78)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

extension View {
    /// A glass card with a hairline of light at its edge; `tint` only for what failed (red) or waits for you (white).
    func axyonCard(cornerRadius: CGFloat = 26, tint: Color? = nil) -> some View {
        self
            .coucouGlass(.regular, cornerRadius: cornerRadius, tint: tint)
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(AXYONPalette.rim, lineWidth: 0.6)
                    .allowsHitTesting(false)
            }
    }

    /// The black and its light behind a List or a ScrollView.
    func axyonBackdrop(intensity: Double = 1) -> some View {
        scrollContentBackground(.hidden)
            .background { AXYONBackdrop(intensity: intensity) }
    }
}

/// What Hermes is doing, as his orb shows it: how lit it is, whether a light runs around it, red when he could not
/// start.
enum AXYONMood: Equatable {
    case sleeping, starting, idle, thinking, acting, speaking, trouble

    /// How lit the glass is.
    var light: Double {
        switch self {
        case .sleeping: 0.2
        case .idle: 0.55
        case .trouble: 0.5
        case .starting, .thinking, .acting, .speaking: 0.9
        }
    }

    /// A light runs around the orb while he works.
    var turns: Bool { self == .starting || self == .thinking || self == .acting }

    /// The light's turns per second.
    var speed: Double {
        switch self {
        case .acting: 0.9
        case .thinking: 0.7
        default: 0.5
        }
    }

    var glow: Color { self == .trouble ? AXYONPalette.down : .white }
}

/// Hermes' orb: a sphere of dark glass lit from the top left, a light breathing inside it, and an arc of light running
/// around its edge while he works. White on black, red only when he could not start. Drawn in code; `still` draws it
/// once, for the many small ones (an older message's), which then cost nothing.
struct AXYONOrb: View {
    var mood: AXYONMood
    var size: CGFloat = 120
    var still = false
    @State private var visible = false

    var body: some View {
        Group {
            if still {
                sphere(at: 0)
            } else {
                TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !visible)) { timeline in
                    sphere(at: timeline.date.timeIntervalSinceReferenceDate)
                }
            }
        }
        .frame(width: size * 1.3, height: size * 1.3)
        .accessibilityHidden(true)
        .onAppear { visible = true }
        .onDisappear { visible = false }
    }

    private func sphere(at t: TimeInterval) -> some View {
        let breath = 0.5 + 0.5 * sin(t * (mood.turns ? 2.4 : 1.1))
        let lit = mood.light * (0.65 + 0.35 * breath)
        return ZStack {
            // The light it gives off, faint.
            Circle()
                .fill(RadialGradient(colors: [mood.glow.opacity(0.16 * lit), .clear],
                                     center: .center, startRadius: size * 0.35, endRadius: size * 0.9))
                .frame(width: size * 1.8, height: size * 1.8)
            // The ball: dark glass, deeper at its edge, a light inside it.
            Circle()
                .fill(RadialGradient(colors: [mood.glow.opacity(0.30 * lit), .white.opacity(0.04), .black.opacity(0.55)],
                                     center: UnitPoint(x: 0.5, y: 0.46), startRadius: 0, endRadius: size * 0.52))
                .frame(width: size, height: size)
                .coucouGlass(.clear, in: Circle())
            // Its edge: a hairline, brightest where the light comes from.
            Circle()
                .strokeBorder(AngularGradient(colors: [.white.opacity(0.75), .white.opacity(0.06), .white.opacity(0.28),
                                                       .white.opacity(0.06), .white.opacity(0.75)],
                                              center: .center, angle: .degrees(-135)),
                              lineWidth: size < 40 ? 0.75 : 1)
                .frame(width: size, height: size)
            // The highlight at the top left.
            Ellipse()
                .fill(LinearGradient(colors: [.white.opacity(0.5), .white.opacity(0)], startPoint: .top, endPoint: .bottom))
                .frame(width: size * 0.44, height: size * 0.22)
                .offset(x: -size * 0.13, y: -size * 0.28)
                .blur(radius: size < 40 ? 0.5 : 1.5)
            // A light running around the edge while he works.
            if mood.turns && !still {
                Circle()
                    .trim(from: 0, to: 0.24)
                    .stroke(LinearGradient(colors: [mood.glow, mood.glow.opacity(0)], startPoint: .leading, endPoint: .trailing),
                            style: StrokeStyle(lineWidth: size < 40 ? 1.5 : 2, lineCap: .round))
                    .frame(width: size * 1.14, height: size * 1.14)
                    .rotationEffect(.degrees(t * 360 * mood.speed))
                    .shadow(color: mood.glow.opacity(0.7), radius: size < 40 ? 2 : 5)
            }
        }
    }
}

/// A small capsule of glass: an icon and a word; `tint` for a gain or a loss.
struct AXYONChip: View {
    var title: String
    var systemImage: String
    var tint: Color? = nil

    var body: some View {
        Label(title, systemImage: systemImage)
            .font(.footnote.weight(.semibold))
            .lineLimit(1)
            .foregroundStyle(tint ?? .white)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .coucouGlass(.regular, in: Capsule(), interactive: true)
    }
}

/// A section's title over its cards, as the Health and Fitness apps write it: bold, white, no symbol.
struct AXYONHeader: View {
    var title: String

    var body: some View {
        Text(title)
            .font(.title3.weight(.bold))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 2)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A row as the iPhone's own Settings draw one: a white symbol in a small grey square, a title, a line under it, a
/// chevron.
struct AXYONRow: View {
    var icon: String
    var title: String
    var detail: String? = nil
    var chevron = true

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: icon)
                .font(.body.weight(.semibold))
                .foregroundStyle(.white)
                .frame(width: 34, height: 34)
                .background(Color.white.opacity(0.1), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let detail {
                    Text(detail)
                        .font(.footnote)
                        .foregroundStyle(AXYONPalette.dim)
                        .lineLimit(3)
                        .multilineTextAlignment(.leading)
                }
            }
            Spacer(minLength: 8)
            if chevron {
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(AXYONPalette.faint)
            }
        }
        .contentShape(Rectangle())
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
                .font(.title2.weight(.semibold))
                .monospacedDigit()
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

/// The switches of AXYON: the system's green when on, as in the iPhone's own Settings (white would hide the knob).
struct AXYONSwitch: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Toggle(configuration)
            .toggleStyle(.switch)
            .tint(AXYONPalette.up)
    }
}

/// AXYON's one button to tap, as Apple's own dark screens draw it: a white capsule with black on it.
struct WhiteCapsuleButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    @Environment(\.controlSize) private var controlSize

    func makeBody(configuration: Configuration) -> some View {
        let large = controlSize == .large || controlSize == .extraLarge
        return configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(.black)
            .padding(.horizontal, large ? 20 : 16)
            .padding(.vertical, large ? 11 : 9)
            .background(Color.white.opacity(enabled ? 1 : 0.35), in: Capsule())
            .contentShape(Capsule())
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .opacity(configuration.isPressed ? 0.8 : 1)
            .animation(.spring(duration: 0.25, bounce: 0.3), value: configuration.isPressed)
    }
}
