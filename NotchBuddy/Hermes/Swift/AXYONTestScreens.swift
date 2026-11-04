#if targetEnvironment(simulator)
import Foundation
import SwiftUI

/// CI's screenshots in the Simulator (Hermes/tests/simulator.sh, the AXYON-screens artifact): which screen to open and
/// where to scroll it, with the example lançamentos and committee verdict the script leaves in the app's temporary
/// folder (Hermes/tests/screens). Compiled for the Simulator only: no build for an iPhone carries it.
enum AXYONTestScreen {
    /// "wallet", "wallet-income-end", "invest-verdict-mid"…: a screen, then where it is scrolled to.
    static var name: String { ProcessInfo.processInfo.environment["AXYON_TEST_TAB"] ?? "" }

    /// The screen, without where it is scrolled to.
    static var base: String {
        for suffix in ["-mid", "-end"] where name.hasSuffix(suffix) {
            return String(name.dropLast(suffix.count))
        }
        return name
    }

    /// The middle or the bottom of a long screen; nil: its top.
    static var anchor: UnitPoint? {
        if name.hasSuffix("-end") { return .bottom }
        if name.hasSuffix("-mid") { return .center }
        return nil
    }

    private static func file(_ name: String) -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent(name)
    }

    /// The example lançamentos, entered once through the engine, which reads their quotes and proventos like any
    /// other's; then a mark for the script, once what the screens rest on is read.
    @MainActor
    static func seed(_ wallet: AXYONWallet) async {
        let url = file("axyon-trades.json")
        guard let data = try? Data(contentsOf: url),
              let list = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] else { return }
        try? FileManager.default.removeItem(at: url)
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .current
        for trade in list {
            let ticker = trade["ticker"] as? String ?? ""
            let day = (trade["date"] as? String ?? "").split(separator: "-").compactMap { Int($0) }
            guard day.count == 3, let date = calendar.date(from: DateComponents(year: day[0], month: day[1], day: day[2])) else {
                continue
            }
            do {
                try await wallet.add(ticker: ticker, sell: trade["kind"] as? String == "sell", date: date,
                                     quantity: trade["quantity"] as? String ?? "", price: trade["price"] as? String ?? "",
                                     fees: trade["fees"] as? String ?? "", opening: true)
            } catch {
                print("AXYON screens: \(ticker) not entered: \(AXYONInvest.describe(error))")
            }
        }
        // What the screens rest on (the closes, the funds' and the companies' proventos), read over a few calls.
        for _ in 0..<3 {
            await wallet.load()
        }
        try? Data("ok".utf8).write(to: file("axyon-seeded"))
    }

    /// The example verdict, for the committee's screen.
    static var verdict: InvestVerdict? {
        guard base == "invest-verdict", let data = try? Data(contentsOf: file("axyon-verdict.json")),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        return InvestVerdict(object)
    }
}
#endif
