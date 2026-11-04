import SwiftUI
import UserNotifications

// AXYON's look on the iPhone with Hermes inside (the build that carries it), after the designs the user chose: a night
// with coloured lights behind every screen (AXYONTheme), frosted glass cards that let those lights through with an
// edge of light, an accent that glows for what is chosen and the one thing to tap, and a floating bar of glass with
// the chat as a lit sphere in its middle. The cards are frosted glass on every iOS; what floats (the tab bar, round
// buttons, chips) is Liquid Glass on iOS 26, as Apple keeps glass for the controls over the content.

enum AXYONPalette {
    /// Behind everything, under the lights.
    static var void: Color { AXYONTheme.current.night.first ?? .black }
    /// Text and icons.
    static let light = Color.white
    /// Gains, what is on, what worked: a mint green.
    static let up = Color(hex: "#34D399")
    /// Losses and what failed: a coral red.
    static let down = Color(hex: "#FB7185")
    /// What needs a look: amber.
    static let warn = Color(hex: "#FBBF24")
    static let dim = Color.white.opacity(0.64)
    static let faint = Color.white.opacity(0.36)

    /// The accent of the chosen look.
    static var accent: Color { AXYONTheme.current.accent }

    /// A glass edge: light at the top left, where the light comes from, and the accent's at the bottom right.
    static var rim: LinearGradient { rim(nil) }

    static func rim(_ tint: Color?) -> LinearGradient {
        LinearGradient(colors: [.white.opacity(0.34), .white.opacity(0.06), (tint ?? AXYONTheme.current.accent).opacity(0.30)],
                       startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

/// The night behind every AXYON screen with its lights, from the chosen look. Drawn once: nothing moves, so it costs
/// nothing while it shows.
struct AXYONBackdrop: View {
    var intensity: Double = 1
    @AppStorage(AXYONTheme.key) private var chosen = AXYONTheme.aurora.rawValue

    var body: some View {
        let theme = AXYONTheme(rawValue: chosen) ?? .aurora
        GeometryReader { geo in
            let side = max(geo.size.width, geo.size.height, 1)
            ZStack {
                LinearGradient(colors: theme.night, startPoint: .top, endPoint: .bottom)
                ForEach(Array(theme.lights.enumerated()), id: \.offset) { _, light in
                    RadialGradient(colors: [light.color.opacity(light.strength * intensity), light.color.opacity(0)],
                                   center: UnitPoint(x: light.x, y: light.y), startRadius: 0, endRadius: side * light.reach)
                }
                // The edges a little darker, so the light gathers in the middle.
                RadialGradient(colors: [.clear, .black.opacity(0.38)], center: .center,
                               startRadius: side * 0.28, endRadius: side * 0.78)
            }
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// A card of frosted glass in the chosen look: the lights behind it come through, its glass deeper at the bottom, a
/// sheen at the top and an edge of light.
struct AXYONGlassCard: ViewModifier {
    var radius: CGFloat
    var tint: Color?
    @AppStorage(AXYONTheme.key) private var chosen = AXYONTheme.aurora.rawValue

    func body(content: Content) -> some View {
        let theme = AXYONTheme(rawValue: chosen) ?? .aurora
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        content
            .background {
                ZStack {
                    shape.fill(.ultraThinMaterial)
                    shape.fill(LinearGradient(colors: [theme.glass.opacity(0.42), theme.glass.opacity(0.62)],
                                              startPoint: .top, endPoint: .bottom))
                    if let tint {
                        shape.fill(LinearGradient(colors: [tint.opacity(0.22), tint.opacity(0.05)],
                                                  startPoint: .topLeading, endPoint: .bottomTrailing))
                    }
                    shape.fill(LinearGradient(colors: [.white.opacity(0.10), .white.opacity(0)],
                                              startPoint: .top, endPoint: UnitPoint(x: 0.5, y: 0.42)))
                }
                .compositingGroup()
                .shadow(color: .black.opacity(0.40), radius: 20, y: 12)
            }
            .overlay {
                shape.strokeBorder(AXYONPalette.rim(tint), lineWidth: 1)
                    .allowsHitTesting(false)
            }
    }
}

extension View {
    /// A card of frosted glass with an edge of light; `tint` colours it for what failed, gained or waits for you.
    func axyonCard(cornerRadius: CGFloat = 26, tint: Color? = nil) -> some View {
        modifier(AXYONGlassCard(radius: cornerRadius, tint: tint))
    }

    /// The night and its lights behind a List or a ScrollView.
    func axyonBackdrop(intensity: Double = 1) -> some View {
        scrollContentBackground(.hidden)
            .background { AXYONBackdrop(intensity: intensity) }
    }

    /// The accent's glow under something lit: a soft halo of its colour.
    func axyonGlow(_ color: Color? = nil, radius: CGFloat = 14, strength: Double = 0.65) -> some View {
        shadow(color: (color ?? AXYONTheme.current.accent).opacity(strength), radius: radius)
    }
}

/// A round button of glass with a symbol, as the designs put in a card's corner (↗) or a screen's top.
struct AXYONRoundButton: View {
    var systemImage: String
    var size: CGFloat = 38
    var lit = false
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: size * 0.40, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: size, height: size)
                .background {
                    if lit {
                        Circle().fill(AXYONTheme.current.accentGradient).axyonGlow(radius: 10)
                    }
                }
                .coucouGlass(.regular, in: Circle(), interactive: true)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
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

    var glow: Color { self == .trouble ? AXYONPalette.down : AXYONTheme.current.accent }
}

/// Hermes' orb: a sphere of glass full of the accent's light, brightest at the top left, a light breathing inside it,
/// and an arc of light running around its edge while he works; red only when he could not start. Drawn in code;
/// `still` draws it once, for the many small ones (an older message's), which then cost nothing.
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
        let deep = mood == .trouble ? AXYONPalette.down.opacity(0.55) : AXYONTheme.current.accentDeep
        return ZStack {
            // The light it gives off, as the designs' spheres do.
            Circle()
                .fill(RadialGradient(colors: [mood.glow.opacity(0.34 * lit), .clear],
                                     center: .center, startRadius: size * 0.3, endRadius: size * 0.9))
                .frame(width: size * 1.8, height: size * 1.8)
            // The ball: glass full of the accent's light, brightest at the top left, deep at its edge.
            Circle()
                .fill(RadialGradient(colors: [mood.glow.opacity(0.30 + 0.6 * lit), deep.opacity(0.25 + 0.6 * lit),
                                              (AXYONTheme.current.night.last ?? .black).opacity(0.92)],
                                     center: UnitPoint(x: 0.4, y: 0.34), startRadius: 0, endRadius: size * 0.62))
                .frame(width: size, height: size)
                .coucouGlass(.clear, in: Circle())
            // The light gathered at its bottom, through the glass.
            Ellipse()
                .fill(RadialGradient(colors: [mood.glow.opacity(0.55 * lit), .clear],
                                     center: .center, startRadius: 0, endRadius: size * 0.34))
                .frame(width: size * 0.7, height: size * 0.3)
                .offset(y: size * 0.3)
                .blur(radius: size < 40 ? 0.5 : 3)
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
            .padding(.horizontal, 13)
            .padding(.vertical, 8)
            .coucouGlass(.regular, in: Capsule(), tint: tint ?? AXYONTheme.current.glass, interactive: true)
            .overlay(Capsule().strokeBorder(AXYONPalette.rim(tint), lineWidth: 0.8))
    }
}

/// A section's title over its cards, as the Health and Fitness apps write it: bold, white, no symbol.
struct AXYONHeader: View {
    var title: String

    var body: some View {
        HStack(spacing: 8) {
            Capsule()
                .fill(AXYONTheme.current.accentGradient)
                .frame(width: 4, height: 18)
                .axyonGlow(radius: 6)
            Text(title)
                .font(.title3.weight(.bold))
                .foregroundStyle(.white)
        }
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
                .frame(width: 36, height: 36)
                .background {
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .fill(LinearGradient(colors: [AXYONTheme.current.accent.opacity(0.55), AXYONTheme.current.accentDeep.opacity(0.35)],
                                             startPoint: .topLeading, endPoint: .bottomTrailing))
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .strokeBorder(.white.opacity(0.22), lineWidth: 0.8)
                }
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

/// The switches of AXYON: the accent of the chosen look when on.
struct AXYONSwitch: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Toggle(configuration)
            .toggleStyle(.switch)
            .tint(AXYONTheme.current.accent)
    }
}

/// AXYON's one button to tap: a capsule of the accent's light, white on it, glowing. (Its name is the old one, white;
/// the screens that use it keep it.)
struct WhiteCapsuleButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    @Environment(\.controlSize) private var controlSize

    func makeBody(configuration: Configuration) -> some View {
        let large = controlSize == .large || controlSize == .extraLarge
        let theme = AXYONTheme.current
        return configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, large ? 22 : 18)
            .padding(.vertical, large ? 12 : 10)
            .background {
                Capsule()
                    .fill(theme.accentGradient)
                    .opacity(enabled ? 1 : 0.4)
                    .axyonGlow(radius: configuration.isPressed ? 8 : 14, strength: enabled ? 0.6 : 0)
                Capsule().strokeBorder(.white.opacity(0.32), lineWidth: 0.8)
            }
            .contentShape(Capsule())
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .opacity(configuration.isPressed ? 0.8 : 1)
            .animation(.spring(duration: 0.25, bounce: 0.3), value: configuration.isPressed)
    }
}

// MARK: - The floating bar

/// The floating bar of glass the designs draw: the tabs as symbols, the chosen one in a circle of the accent's light,
/// the chat a lit sphere raised in the middle.
struct AXYONTabBar: View {
    @Binding var selection: AXYONTab
    @Namespace private var lit

    static let height: CGFloat = 64

    var body: some View {
        let theme = AXYONTheme.current
        let shape = RoundedRectangle(cornerRadius: Self.height / 2, style: .continuous)
        HStack(spacing: 0) {
            item(.wallet, "briefcase.fill", String(localized: "Wallet"))
            item(.invest, "chart.pie.fill", String(localized: "Investments"))
            orb(theme)
            item(.house, "house.fill", String(localized: "Home"))
            item(.settings, "gearshape.fill", String(localized: "Settings"))
        }
        .padding(.horizontal, 8)
        .frame(height: Self.height)
        .coucouGlass(.regular, in: shape, tint: theme.glass)
        .overlay(shape.strokeBorder(AXYONPalette.rim, lineWidth: 1).allowsHitTesting(false))
        .shadow(color: .black.opacity(0.5), radius: 24, y: 12)
        .padding(.horizontal, 18)
        .sensoryFeedback(.selection, trigger: selection)
    }

    private func item(_ tab: AXYONTab, _ icon: String, _ title: String) -> some View {
        let chosen = selection == tab
        return Button {
            withAnimation(.spring(duration: 0.4, bounce: 0.28)) { selection = tab }
        } label: {
            ZStack {
                if chosen {
                    Circle()
                        .fill(AXYONTheme.current.accentGradient)
                        .overlay(Circle().strokeBorder(.white.opacity(0.35), lineWidth: 0.8))
                        .frame(width: 46, height: 46)
                        .axyonGlow(radius: 12, strength: 0.7)
                        .matchedGeometryEffect(id: "lit", in: lit)
                }
                Image(systemName: icon)
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(chosen ? Color.white : Color.white.opacity(0.55))
                    .symbolEffect(.bounce, value: chosen)
            }
            .frame(maxWidth: .infinity)
            .frame(height: Self.height)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(title))
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    /// The chat: a sphere of the accent's light with a highlight at its top left, raised over the bar.
    private func orb(_ theme: AXYONTheme) -> some View {
        let chosen = selection == .chat
        return Button {
            withAnimation(.spring(duration: 0.4, bounce: 0.28)) { selection = .chat }
        } label: {
            ZStack {
                Circle()
                    .fill(RadialGradient(colors: [theme.accent.opacity(0.55), theme.accent.opacity(0)],
                                         center: .center, startRadius: 18, endRadius: 46))
                    .frame(width: 92, height: 92)
                Circle()
                    .fill(RadialGradient(colors: [theme.accent, theme.accentDeep, theme.night.last ?? .black],
                                         center: UnitPoint(x: 0.38, y: 0.32), startRadius: 2, endRadius: 40))
                    .frame(width: 58, height: 58)
                    .overlay {
                        Circle().strokeBorder(AngularGradient(colors: [.white.opacity(0.9), .white.opacity(0.1), .white.opacity(0.45), .white.opacity(0.1), .white.opacity(0.9)],
                                                              center: .center, angle: .degrees(-135)),
                                              lineWidth: chosen ? 1.6 : 1)
                    }
                    .overlay(alignment: .topLeading) {
                        Ellipse()
                            .fill(LinearGradient(colors: [.white.opacity(0.75), .white.opacity(0)], startPoint: .top, endPoint: .bottom))
                            .frame(width: 24, height: 12)
                            .offset(x: 11, y: 7)
                            .blur(radius: 1)
                    }
                    .shadow(color: theme.accent.opacity(chosen ? 0.9 : 0.6), radius: chosen ? 18 : 12)
                Image(systemName: "sparkles")
                    .font(.system(size: 21, weight: .semibold))
                    .foregroundStyle(.white)
                    .symbolEffect(.bounce, value: chosen)
            }
            .frame(width: 70, height: Self.height)
            .offset(y: -14)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text("Chat"))
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

// MARK: - Asking for notifications

/// Asks iOS to let AXYON notify; in CI's Simulator runs (Hermes/tests/simulator.sh), where nobody would answer and the
/// alert would stay over every screenshot, it does not ask.
func axyonAskNotifications(_ options: UNAuthorizationOptions = [.alert, .sound, .badge]) async -> Bool {
    #if targetEnvironment(simulator)
    if ProcessInfo.processInfo.environment["AXYON_TEST_PORT"] != nil { return false }
    #endif
    return (try? await UNUserNotificationCenter.current().requestAuthorization(options: options)) ?? false
}

// MARK: - Room for the floating bar

private struct AXYONBarRoomKey: EnvironmentKey {
    static let defaultValue: CGFloat = 0
}

extension EnvironmentValues {
    /// How much of the bottom the floating bar covers (none while the keyboard is up): what stays at the bottom of a
    /// screen, as the chat's field, sits above it.
    var axyonBarRoom: CGFloat {
        get { self[AXYONBarRoomKey.self] }
        set { self[AXYONBarRoomKey.self] = newValue }
    }
}
