import SwiftUI

// Coucou's deep dark Liquid Glass, for the iPhone app only: the desktop's palette made deeper, glass that is real
// Liquid Glass on iOS 26 and a frosted material before it, and the backdrop every screen sits on, so the glass has a
// dim glow to bend instead of flat black (glass on pure black reads as grey). Rules kept everywhere: never glass on
// glass, and glass shapes close to each other share a CoucouGlassGroup.

/// The colors of the iPhone app. The status colors are the desktop's (windows/src/settings/settings.css).
enum CoucouPalette {
    /// Behind everything: #05060A.
    static let abyss = Color(red: 0.020, green: 0.024, blue: 0.039)
    /// Cards that are not glass: #0B0D14.
    static let night = Color(red: 0.043, green: 0.051, blue: 0.078)
    /// Fills one step up: #11131C.
    static let surface = Color(red: 0.067, green: 0.075, blue: 0.110)
    static let hairline = Color.white.opacity(0.08)
    /// Hermes' violet: #C084FC.
    static let accent = Color(red: 0.753, green: 0.518, blue: 0.988)
    /// #22C55E
    static let ok = Color(red: 0.133, green: 0.773, blue: 0.369)
    /// #F5A524
    static let warn = Color(red: 0.961, green: 0.647, blue: 0.141)
    /// #F4505E
    static let danger = Color(red: 0.957, green: 0.314, blue: 0.369)
    /// The backdrop's second glow, a deep blue: #3B5BDB.
    static let glow = Color(red: 0.231, green: 0.357, blue: 0.859)
}

/// How much a glass surface stands out: regular for cards, clear for what floats over busy content, prominent for
/// the one thing to tap (strongly tinted, Hermes' violet unless told otherwise).
enum CoucouGlassKind: Sendable, Equatable { case regular, clear, prominent }

/// Liquid Glass in `shape` on iOS 26; before it, a frosted material with a hairline and a soft shadow.
struct CoucouGlass<S: InsettableShape>: ViewModifier {
    var kind: CoucouGlassKind
    var shape: S
    var tint: Color?
    var interactive: Bool

    private var color: Color? { tint ?? (kind == .prominent ? CoucouPalette.accent : nil) }

    @ViewBuilder func body(content: Content) -> some View {
        if #available(iOS 26.0, *) {
            content.glassEffect(glass, in: shape)
        } else {
            content.background {
                ZStack {
                    shape.fill(kind == .clear ? Material.ultraThinMaterial : Material.thinMaterial)
                    if let color { shape.fill(color.opacity(kind == .prominent ? 0.45 : 0.16)) }
                    shape.strokeBorder(CoucouPalette.hairline, lineWidth: 0.75)
                }
                .shadow(color: .black.opacity(0.4), radius: 12, y: 6)
            }
        }
    }

    @available(iOS 26.0, *)
    private var glass: Glass {
        var made: Glass = kind == .clear ? .clear : .regular
        if let color { made = made.tint(color.opacity(kind == .prominent ? 0.6 : 0.22)) }
        if interactive { made = made.interactive() }
        return made
    }
}

extension View {
    /// Glass in a rounded rectangle.
    func coucouGlass(_ kind: CoucouGlassKind = .regular, cornerRadius: CGFloat = 24,
                     tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(CoucouGlass(kind: kind, shape: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous),
                             tint: tint, interactive: interactive))
    }

    /// Glass in any shape that can be stroked from the inside (capsule, circle…).
    func coucouGlass<S: InsettableShape>(_ kind: CoucouGlassKind = .regular, in shape: S,
                                         tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(CoucouGlass(kind: kind, shape: shape, tint: tint, interactive: interactive))
    }

    /// The system's glass button on iOS 26, a bordered one before.
    @ViewBuilder func coucouGlassButton(prominent: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            if prominent { buttonStyle(.glassProminent) } else { buttonStyle(.glass) }
        } else {
            if prominent { buttonStyle(.borderedProminent) } else { buttonStyle(.bordered) }
        }
    }

    /// Lets this glass morph into, or out of, the glass of the same group with another id (iOS 26).
    @ViewBuilder func coucouGlassID(_ id: String, in ns: Namespace.ID) -> some View {
        if #available(iOS 26.0, *) { glassEffectID(id, in: ns) } else { self }
    }

    /// The tab bar folds away while scrolling down (iOS 26).
    @ViewBuilder func coucouMinimizingTabBar() -> some View {
        if #available(iOS 26.0, *) { tabBarMinimizeBehavior(.onScrollDown) } else { self }
    }

    /// A List or a ScrollView on the deep backdrop.
    func deepBackdrop() -> some View {
        scrollContentBackground(.hidden)
            .background { DeepBackdrop() }
    }
}

/// Glass shapes drawn together: on iOS 26 they share their light and blend when they come within `spacing` of
/// each other (an island growing a panel, a send button out of its field). Before iOS 26, just the content.
struct CoucouGlassGroup<Content: View>: View {
    var spacing: CGFloat = 12
    @ViewBuilder var content: Content

    var body: some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
    }
}

/// The deep dark behind every screen: near-black with a dim violet glow from the top and a blue one from the
/// bottom, so glass has something to bend. Drawn once, no timeline: it costs nothing while it shows.
struct DeepBackdrop: View {
    var body: some View {
        ZStack {
            CoucouPalette.abyss
            EllipticalGradient(colors: [CoucouPalette.accent.opacity(0.17), .clear],
                               center: UnitPoint(x: 0.12, y: -0.02), startRadiusFraction: 0, endRadiusFraction: 0.75)
            EllipticalGradient(colors: [CoucouPalette.glow.opacity(0.15), .clear],
                               center: UnitPoint(x: 0.95, y: 0.9), startRadiusFraction: 0, endRadiusFraction: 0.8)
            LinearGradient(colors: [.clear, CoucouPalette.abyss.opacity(0.55)], startPoint: .center, endPoint: .bottom)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// The background of a List row on the deep backdrop: frosted like glass, but one surface per section (real glass in
/// every row would draw a rim at each row's edge).
struct GlassRowBackground: View {
    var tint: Color? = nil

    var body: some View {
        ZStack {
            Rectangle().fill(.ultraThinMaterial)
            CoucouPalette.surface.opacity(0.55)
            if let tint { tint.opacity(0.12) }
        }
    }
}

/// A List row drawn as a glass card of its own, a little apart from the next one.
struct GlassRowCard: View {
    var cornerRadius: CGFloat = 20
    var tint: Color? = nil

    var body: some View {
        Color.clear
            .coucouGlass(.regular, cornerRadius: cornerRadius, tint: tint)
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
    }
}
