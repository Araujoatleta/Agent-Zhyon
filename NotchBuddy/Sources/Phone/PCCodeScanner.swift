import AVFoundation
import SwiftUI
import Vision
import VisionKit

/// The camera, reading the pairing code Coucou for Windows or Linux shows in Settings → Phone. The iPhone's own Camera
/// app does not open `coucou://` codes (it says "No usable data found"), so the app reads them itself, as the Android
/// app does. The camera runs only while this screen is open; nothing is recorded or kept.
struct PCScanSheet: View {
    let pc: PCLink
    /// After pairing: the screen that opened this one moves on.
    var paired: () -> Void = {}
    @Environment(\.dismiss) private var dismiss
    /// nil while iOS asks for the camera.
    @State private var camera: Bool?
    @State private var found: PCPairing?
    @State private var problem: String?

    var body: some View {
        if let found {
            PCPairingConfirm(pairing: found, pc: pc, paired: paired)
        } else {
            ZStack(alignment: .bottom) {
                Color.black.ignoresSafeArea()
                if camera == true {
                    PCCodeScanner { text in read(text) }
                        .ignoresSafeArea()
                } else if camera == false {
                    ContentUnavailableView {
                        Label("No camera for Coucou", systemImage: "camera")
                    } description: {
                        Text("Allow the camera for Coucou in iPhone Settings, or copy the link on the computer and paste it instead.")
                    } actions: {
                        Button("Open iPhone Settings") {
                            if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                        }
                    }
                }
                VStack(spacing: 12) {
                    Text(problem ?? "Point the camera at the code in Coucou → Settings → Phone (Android and iPhone) on the computer.")
                        .font(.callout)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .glassCard(cornerRadius: 18)
                    Button("Cancel") { dismiss() }
                        .buttonStyle(.bordered)
                        .controlSize(.large)
                }
                .padding(24)
            }
            .preferredColorScheme(.dark)
            .task {
                let allowed = await AVCaptureDevice.requestAccess(for: .video)
                camera = allowed && DataScannerViewController.isSupported
            }
        }
    }

    private func read(_ text: String) {
        guard found == nil else { return }
        if let pairing = PCPairing.parse(text) {
            Haptics.success()
            found = pairing
        } else if problem == nil {
            problem = "This QR code isn't a Coucou pairing code. Use the one in Settings → Phone (Android and iPhone) on the computer."
            Haptics.warning()
        }
    }
}

/// VisionKit's live scanner, for QR codes only.
struct PCCodeScanner: UIViewControllerRepresentable {
    let read: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])],
                                                qualityLevel: .balanced,
                                                recognizesMultipleItems: false,
                                                isHighFrameRateTrackingEnabled: false,
                                                isHighlightingEnabled: true)
        scanner.delegate = context.coordinator
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {
        context.coordinator.read = read
        if !scanner.isScanning { try? scanner.startScanning() }
    }

    static func dismantleUIViewController(_ scanner: DataScannerViewController, coordinator: Coordinator) {
        scanner.stopScanning()
    }

    func makeCoordinator() -> Coordinator { Coordinator(read: read) }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        var read: (String) -> Void

        init(read: @escaping (String) -> Void) {
            self.read = read
        }

        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            for item in addedItems {
                if case .barcode(let code) = item, let text = code.payloadStringValue {
                    read(text)
                    return
                }
            }
        }

        func dataScanner(_ dataScanner: DataScannerViewController, didTapOn item: RecognizedItem) {
            if case .barcode(let code) = item, let text = code.payloadStringValue { read(text) }
        }
    }
}
