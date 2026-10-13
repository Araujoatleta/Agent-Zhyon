import AVFoundation

/// Mochi's answers read aloud in the chat with the computer, after a message you spoke: you talked to him, he talks
/// back ("A TV da sala está ligada e o YouTube está aberto"). On the iPhone's own voices, in the dictation's language;
/// nothing leaves the iPhone. Off with the chat menu's switch.
@MainActor
final class MochiVoice {
    static let shared = MochiVoice()

    private let synthesizer = AVSpeechSynthesizer()

    /// Reads `markdown` as plain sentences (PCText.spoken), in `language` ("pt-BR") when the iPhone has that voice.
    func say(_ markdown: String, language: String) {
        let text = PCText.spoken(markdown)
        guard !text.isEmpty else { return }
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .spokenAudio, options: .duckOthers)
        try? session.setActive(true)
        synthesizer.stopSpeaking(at: .immediate)
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: language) ?? AVSpeechSynthesisVoice(language: Locale.current.identifier)
        synthesizer.speak(utterance)
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
    }
}
