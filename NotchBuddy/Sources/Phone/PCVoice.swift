import AVFoundation

/// Mochi's answers read aloud in the chat with the computer: after a message you spoke (you talked to him, he talks
/// back: "A TV da sala está ligada e o YouTube está aberto"), or any message with Read aloud in its menu. On the
/// iPhone's own voices, in the dictation's language; nothing leaves the iPhone. Off with the chat menu's switch.
@MainActor
final class MochiVoice {
    static let shared = MochiVoice()

    private let synthesizer = AVSpeechSynthesizer()
    /// The audio session is the voice's: given back once it is done, so music ducked under it comes back up.
    private var holdsAudio = false
    private var watch: Task<Void, Never>?

    /// Reading now (read when a menu opens, to offer Stop).
    var isSpeaking: Bool { synthesizer.isSpeaking }

    /// Reads `markdown` as plain sentences (PCText.spoken), in `language` ("pt-BR") when the iPhone has that voice.
    func say(_ markdown: String, language: String) {
        let text = PCText.spoken(markdown)
        guard !text.isEmpty else { return }
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .spokenAudio, options: .duckOthers)
        try? session.setActive(true)
        holdsAudio = true
        synthesizer.stopSpeaking(at: .immediate)
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: language) ?? AVSpeechSynthesisVoice(language: Locale.current.identifier)
        synthesizer.speak(utterance)
        releaseWhenDone()
    }

    func stop() {
        watch?.cancel()
        synthesizer.stopSpeaking(at: .immediate)
        // At once, not later: dictation may take the audio session right after.
        release()
    }

    /// Looks twice a second whether the voice is done, then gives the audio back.
    private func releaseWhenDone() {
        watch?.cancel()
        watch = Task { [weak self] in
            while true {
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled, let voice = self else { return }
                if !voice.synthesizer.isSpeaking {
                    voice.release()
                    return
                }
            }
        }
    }

    private func release() {
        guard holdsAudio else { return }
        holdsAudio = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
