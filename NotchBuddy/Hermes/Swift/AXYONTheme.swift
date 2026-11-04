import SwiftUI

// AXYON's three looks, one for each design the user chose: Aurora (an indigo night and a glass sphere lit violet and
// blue), Copper (dark wood and an ember of orange) and Ocean (deep navy glass and an electric blue). Each is a night
// with its lights behind every screen, a colour for the glass and an accent that glows: the chosen tab, the one
// button to tap, the bars of a chart. Chosen in Settings → Appearance; Aurora until then.

/// A light behind the screens: its colour, where it shines (0 to 1 across and down), how far it reaches (a share of
/// the screen's longer side) and how strong it is.
struct AXYONLight: Sendable {
    let color: Color
    let x: Double
    let y: Double
    let reach: Double
    let strength: Double
}

enum AXYONTheme: String, CaseIterable, Identifiable, Sendable {
    case aurora, copper, ocean

    /// Where the choice is kept.
    static let key = "axyon.theme"

    /// The look chosen now.
    static var current: AXYONTheme {
        UserDefaults.standard.string(forKey: key).flatMap(AXYONTheme.init(rawValue:)) ?? .aurora
    }

    var id: String { rawValue }

    var name: String {
        switch self {
        case .aurora: String(localized: "Aurora")
        case .copper: String(localized: "Copper")
        case .ocean: String(localized: "Ocean")
        }
    }

    var detail: String {
        switch self {
        case .aurora: String(localized: "Indigo night, a sphere of violet and blue light")
        case .copper: String(localized: "Dark wood and an ember of orange")
        case .ocean: String(localized: "Deep navy glass and an electric blue")
        }
    }

    /// The night behind everything, from the top down.
    var night: [Color] {
        switch self {
        case .aurora: [Color(hex: "#05060F"), Color(hex: "#090B1F"), Color(hex: "#0C0E26")]
        case .copper: [Color(hex: "#0B0706"), Color(hex: "#140C08"), Color(hex: "#1B100A")]
        case .ocean: [Color(hex: "#040812"), Color(hex: "#071024"), Color(hex: "#0A1631")]
        }
    }

    /// Its lights.
    var lights: [AXYONLight] {
        switch self {
        case .aurora:
            [AXYONLight(color: Color(hex: "#4F46E5"), x: 0.10, y: 0.02, reach: 0.62, strength: 0.55),
             AXYONLight(color: Color(hex: "#2563EB"), x: 1.00, y: 0.30, reach: 0.50, strength: 0.42),
             AXYONLight(color: Color(hex: "#7C3AED"), x: 0.20, y: 0.92, reach: 0.58, strength: 0.40),
             AXYONLight(color: Color(hex: "#F59E0B"), x: 0.95, y: 0.98, reach: 0.30, strength: 0.16)]
        case .copper:
            [AXYONLight(color: Color(hex: "#F97316"), x: 0.92, y: 0.00, reach: 0.60, strength: 0.50),
             AXYONLight(color: Color(hex: "#B45309"), x: 0.00, y: 0.45, reach: 0.55, strength: 0.42),
             AXYONLight(color: Color(hex: "#EA580C"), x: 0.60, y: 1.00, reach: 0.55, strength: 0.38),
             AXYONLight(color: Color(hex: "#FDE68A"), x: 0.10, y: 0.05, reach: 0.25, strength: 0.08)]
        case .ocean:
            [AXYONLight(color: Color(hex: "#2563EB"), x: 0.08, y: 0.06, reach: 0.65, strength: 0.55),
             AXYONLight(color: Color(hex: "#E11D48"), x: 1.00, y: 0.42, reach: 0.40, strength: 0.22),
             AXYONLight(color: Color(hex: "#0EA5E9"), x: 0.40, y: 1.00, reach: 0.55, strength: 0.36),
             AXYONLight(color: Color(hex: "#6366F1"), x: 0.95, y: 0.95, reach: 0.35, strength: 0.20)]
        }
    }

    /// What glows: the chosen tab, the one button to tap, a chart's bars.
    var accent: Color {
        switch self {
        case .aurora: Color(hex: "#8B8CFF")
        case .copper: Color(hex: "#FF8A3D")
        case .ocean: Color(hex: "#4C8DFF")
        }
    }

    /// The accent's deeper end, for its gradient.
    var accentDeep: Color {
        switch self {
        case .aurora: Color(hex: "#5B3DF5")
        case .copper: Color(hex: "#E2530A")
        case .ocean: Color(hex: "#1E5BEF")
        }
    }

    /// A second light, for what stands out beside the accent (a dot, a line).
    var spark: Color {
        switch self {
        case .aurora: Color(hex: "#FFB86B")
        case .copper: Color(hex: "#FFE0B5")
        case .ocean: Color(hex: "#FF8A4C")
        }
    }

    /// The glass's own colour.
    var glass: Color {
        switch self {
        case .aurora: Color(hex: "#151A3D")
        case .copper: Color(hex: "#2A170E")
        case .ocean: Color(hex: "#0E1E45")
        }
    }

    /// A ring's or a chart's colours, in the order its slices take them.
    var series: [Color] {
        switch self {
        case .aurora: ["#8B9CFF", "#C084FC", "#5EEAD4", "#F9A8D4", "#FCD34D", "#60A5FA", "#A5B4FC", "#FDBA74"].map { Color(hex: $0) }
        case .copper: ["#FF8A3D", "#FFC48A", "#F2545B", "#FFE0B5", "#C97A40", "#8BD3C7", "#FFB703", "#E9C46A"].map { Color(hex: $0) }
        case .ocean: ["#60A5FA", "#38BDF8", "#F472B6", "#34D399", "#FBBF24", "#A78BFA", "#93C5FD", "#FB923C"].map { Color(hex: $0) }
        }
    }

    /// The accent as a light: brighter at the top left.
    var accentGradient: LinearGradient {
        LinearGradient(colors: [accent, accentDeep], startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

// MARK: - Choosing the look

/// The three looks side by side, each a small night with its lights and its accent; the chosen one ringed in light.
struct AXYONThemePicker: View {
    @AppStorage(AXYONTheme.key) private var chosen = AXYONTheme.aurora.rawValue

    var body: some View {
        HStack(spacing: 12) {
            ForEach(AXYONTheme.allCases) { theme in
                let picked = theme.rawValue == chosen
                Button {
                    withAnimation(.snappy) { chosen = theme.rawValue }
                    Haptics.impact()
                } label: {
                    VStack(spacing: 8) {
                        AXYONThemeSwatch(theme: theme)
                            .frame(height: 96)
                            .overlay {
                                RoundedRectangle(cornerRadius: 18, style: .continuous)
                                    .strokeBorder(picked ? theme.accent : .white.opacity(0.14), lineWidth: picked ? 2 : 1)
                            }
                            .shadow(color: picked ? theme.accent.opacity(0.55) : .clear, radius: 12)
                        HStack(spacing: 4) {
                            if picked {
                                Image(systemName: "checkmark.circle.fill").foregroundStyle(theme.accent)
                            }
                            Text(theme.name).fontWeight(picked ? .semibold : .regular)
                        }
                        .font(.footnote)
                        .foregroundStyle(.white)
                    }
                    .frame(maxWidth: .infinity)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text(theme.name))
                .accessibilityHint(Text(theme.detail))
                .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
        .padding(.vertical, 6)
    }
}

/// A look in small: its night, its lights, a card of its glass and its accent.
struct AXYONThemeSwatch: View {
    let theme: AXYONTheme

    var body: some View {
        GeometryReader { geo in
            let side = max(geo.size.width, geo.size.height)
            ZStack {
                LinearGradient(colors: theme.night, startPoint: .top, endPoint: .bottom)
                ForEach(Array(theme.lights.enumerated()), id: \.offset) { _, light in
                    RadialGradient(colors: [light.color.opacity(light.strength * 1.4), light.color.opacity(0)],
                                   center: UnitPoint(x: light.x, y: light.y), startRadius: 0, endRadius: side * light.reach)
                }
                VStack(spacing: 6) {
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(theme.glass.opacity(0.6))
                        .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(.white.opacity(0.25), lineWidth: 0.6))
                        .frame(height: 26)
                    Circle()
                        .fill(theme.accentGradient)
                        .frame(width: 16, height: 16)
                        .shadow(color: theme.accent.opacity(0.8), radius: 6)
                }
                .padding(12)
            }
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .accessibilityHidden(true)
    }
}
