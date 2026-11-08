import Foundation

/// The language Whisper transcribes in, cycled with ⌥⇧Space (ADR-0009).
/// Remembered across launches; English is the default.
enum DictationLanguage: String, CaseIterable {
    case english
    case ukrainian
    case french
    case auto

    /// ISO 639-1 code passed to the core, or nil for auto-detect.
    var whisperCode: String? {
        switch self {
        case .english: return "en"
        case .ukrainian: return "uk"
        case .french: return "fr"
        case .auto: return nil
        }
    }

    /// What the HUD shows after switching.
    var displayName: String {
        switch self {
        case .english: return "English"
        case .ukrainian: return "Українська"
        case .french: return "Français"
        case .auto: return "Auto-detect"
        }
    }

    var next: DictationLanguage {
        let all = Self.allCases
        let index = all.firstIndex(of: self)!
        return all[(index + 1) % all.count]
    }

    private static let defaultsKey = "dictationLanguage"

    static var saved: DictationLanguage {
        UserDefaults.standard.string(forKey: defaultsKey).flatMap(DictationLanguage.init) ?? .english
    }

    func save() {
        UserDefaults.standard.set(rawValue, forKey: Self.defaultsKey)
    }
}
