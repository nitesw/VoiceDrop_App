import ApplicationServices
import AppKit
import CoreGraphics

/// What had focus at `Idle -> Recording` — captured then and reused at
/// injection time, per the todo: "fixed at session start even if focus
/// later changes." Comparing `frontmostBundleIdentifier` against the
/// frontmost app at injection time is how a mid-session focus change gets
/// detected.
struct InjectionTarget {
    let axElement: AXUIElement?
    let frontmostBundleIdentifier: String?
}

enum InjectionOutcome {
    /// Direct AX text insertion worked — clipboard was never touched.
    case injected
    /// AX insertion wasn't possible, but the target app is still the same
    /// one focus started in, so text was copied to the clipboard AND a
    /// synthetic Cmd+V was sent automatically — the user sees it just
    /// appear, same as `.injected`, just via a different mechanism.
    case pasted
    /// Copied to clipboard only, with no auto-paste attempt — reserved for
    /// focus having moved to a different app since recording started
    /// (auto-paste would land in the wrong place). `reason` is for logging
    /// only — never shown to the user.
    case fallbackToClipboard(reason: String)
    /// The transcript was discarded entirely — nowhere, not even the
    /// clipboard. Reserved for secure fields: per "detect secure fields,"
    /// they must never receive the text via any mechanism, and the
    /// clipboard is exactly as readable by other apps as a direct paste
    /// would be. `reason` is for logging only.
    case discarded(reason: String)
}

/// Inserts the *Cleaned Transcript* into the Injection Target, in order:
///
/// 1. **Direct AX insertion** (setting `kAXSelectedTextAttribute`) —
///    preferred because it never touches the system clipboard and
///    participates in the target app's own undo stack. In practice this
///    only reaches native, fully AX-compliant Cocoa text views (TextEdit,
///    Notes, Mail): manual testing found it fails across terminal
///    emulators (no editable AX text field to begin with — the buffer is
///    custom-drawn, not a real text element) AND across Electron/Chromium
///    apps (WhatsApp, VS Code-likes) with notoriously incomplete
///    accessibility trees. That's a much larger slice of real usage than
///    "an edge case," which is why step 2 exists rather than treating
///    clipboard-only as good enough.
/// 2. **Auto-paste**: copy to clipboard, then synthesize Cmd+V — works
///    anywhere a human could paste, which covers the terminal/Electron
///    gap above. Skipped only when it would be unsafe to send a keystroke
///    at all (secure field, or focus moved to a different app since
///    recording started).
enum TextInjector {
    /// Call at `Idle -> Recording` — captures what currently has focus so
    /// injection still targets the right place even if the user's focus
    /// drifts during the seconds STT/Cleanup Pass take to run.
    static func captureCurrentTarget() -> InjectionTarget {
        let systemWide = AXUIElementCreateSystemWide()
        var focused: AnyObject?
        let status = AXUIElementCopyAttributeValue(
            systemWide, kAXFocusedUIElementAttribute as CFString, &focused)
        let axElement: AXUIElement? = status == .success ? (focused as! AXUIElement) : nil
        return InjectionTarget(
            axElement: axElement,
            frontmostBundleIdentifier: NSWorkspace.shared.frontmostApplication?.bundleIdentifier)
    }

    /// True if `target` is a secure field (e.g. a password entry) — per
    /// "detect secure fields," these must never receive synthetic text via
    /// any mechanism, injection, auto-paste, or even the clipboard.
    static func isSecureField(_ target: InjectionTarget) -> Bool {
        guard let element = target.axElement else { return false }
        return attributeString(element, kAXSubroleAttribute) == kAXSecureTextFieldSubrole
    }

    /// Inserts `text` into `target` via AX, auto-paste, or clipboard-only,
    /// in that preference order. Always returns an outcome describing what
    /// actually happened — the transcript is never silently dropped, per
    /// the phase's "Done when."
    static func inject(_ text: String, into target: InjectionTarget) -> InjectionOutcome {
        if isSecureField(target) {
            // Deliberately does NOT call copyToClipboard here — per
            // isSecureField's doc, secure fields must never receive the
            // text via any mechanism, and the clipboard is exactly as
            // readable by other apps/clipboard managers as a direct paste
            // would be. Discarding is the correct, safe behavior for a
            // password field — not "lost" in the sense the phase's "Done
            // when" cares about, which is about accidental data loss, not
            // an intentional privacy safeguard.
            return .discarded(reason: "secure field")
        }

        let currentBundleID = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        guard currentBundleID == target.frontmostBundleIdentifier else {
            copyToClipboard(text)
            return .fallbackToClipboard(reason: "focus changed since recording started")
        }

        if tryDirectInsertion(text, into: target) {
            return .injected
        }

        logFocusedElementDiagnostics(target.axElement)

        // A focused element that's *present but non-text* (Finder's icon
        // view, a Photos grid cell — both report a real AXGroup/AXCell,
        // just not a text one) is solid evidence there's nothing to paste
        // into. But a *missing* element is ambiguous, not the same signal:
        // Electron/Chromium apps (observed with Antigravity, an
        // Electron-based IDE) routinely return no system-wide focused
        // element at all even when a real, pasteable text input has focus
        // — their AX tree just doesn't surface it. Treating "no element"
        // as "nothing to paste into" wrongly routed those apps to
        // clipboard-only. So: only known-non-text roles block auto-paste;
        // "no element" falls through to attempting it, same as an unknown
        // role would.
        if let element = target.axElement, !isTextCapable(element) {
            copyToClipboard(text)
            return .fallbackToClipboard(reason: "focused element has a non-text role")
        }

        copyToClipboard(text)
        sendCommandV()
        return .pasted
    }

    /// Attempts direct AX insertion only — no clipboard involvement. False
    /// for any reason (no element, not settable, write failed, or the
    /// write couldn't be verified) means the caller should fall through to
    /// auto-paste instead.
    private static func tryDirectInsertion(_ text: String, into target: InjectionTarget) -> Bool {
        guard let element = target.axElement else { return false }

        // Some apps' focused AX elements (terminal emulators in
        // particular — observed with iTerm2) aren't real editable text
        // fields but still report success on kAXSelectedTextAttribute
        // without inserting anything. Two defenses: check settability
        // first, and verify the value actually changed after writing.
        guard isAttributeSettable(element, kAXSelectedTextAttribute) else { return false }

        let result = AXUIElementSetAttributeValue(
            element, kAXSelectedTextAttribute as CFString, text as CFTypeRef)
        guard result == .success else { return false }

        // If the field's value is readable, confirm our text actually
        // landed before trusting the "success" result. If it's unreadable
        // (some apps don't expose kAXValueAttribute even when insertion
        // worked), give the benefit of the doubt rather than risk a
        // false-negative double-paste.
        if let currentValue = attributeString(element, kAXValueAttribute), !currentValue.contains(text) {
            return false
        }

        return true
    }

    /// Temporary diagnostic for the "still auto-pastes into Settings/Photos"
    /// reports — logs exactly what `isTextCapable` sees so the next
    /// heuristic change (if any) is driven by data instead of another
    /// guess. Remove once the fallback behaves correctly everywhere it's
    /// been reported broken.
    private static func logFocusedElementDiagnostics(_ element: AXUIElement?) {
        guard let element else {
            voiceDropLog.log("Auto-paste diagnostics: no focused AX element.")
            return
        }
        let role = attributeString(element, kAXRoleAttribute) ?? "<none>"
        let subrole = attributeString(element, kAXSubroleAttribute) ?? "<none>"
        var rangeValue: AnyObject?
        let rangeReadable =
            AXUIElementCopyAttributeValue(element, kAXSelectedTextRangeAttribute as CFString, &rangeValue)
            == .success
        let rangeSettable = isAttributeSettable(element, kAXSelectedTextRangeAttribute)
        voiceDropLog.log(
            "Auto-paste diagnostics: role=\(role, privacy: .public) subrole=\(subrole, privacy: .public) selectedTextRange(readable=\(rangeReadable, privacy: .public), settable=\(rangeSettable, privacy: .public))"
        )
    }

    /// True if `element` is plausibly a text element worth pasting into.
    /// A known text-widget role is sufficient on its own. Otherwise, fall
    /// back to checking for `kAXSelectedTextRangeAttribute` — present on the
    /// custom-drawn text views terminals and Electron/Chromium apps use
    /// that don't report a proper text field role — but only for roles not
    /// already known to be non-editable: `kAXSelectedTextRangeAttribute` is
    /// also exposed by read-only elements like a Finder/Photos item's name
    /// label (`AXStaticText`, editable only via double-click-to-rename, not
    /// via focus+paste) purely so VoiceOver/accessibility clients can
    /// select their text for reading, which is what caused this check to
    /// wrongly treat Finder/Photos as paste targets before this exclusion
    /// list existed.
    private static func isTextCapable(_ element: AXUIElement) -> Bool {
        let textRoles: Set<String> = [
            kAXTextFieldRole as String,
            kAXTextAreaRole as String,
            kAXComboBoxRole as String,
        ]
        let nonEditableRoles: Set<String> = [
            kAXStaticTextRole as String,
            kAXImageRole as String,
            kAXButtonRole as String,
            kAXCellRole as String,
            kAXRowRole as String,
            kAXOutlineRole as String,
            kAXListRole as String,
            kAXScrollAreaRole as String,
            kAXApplicationRole as String,
            kAXWindowRole as String,
            kAXToolbarRole as String,
            kAXMenuRole as String,
            kAXMenuItemRole as String,
            kAXMenuBarRole as String,
            kAXMenuBarItemRole as String,
        ]
        let role = attributeString(element, kAXRoleAttribute)
        if let role, textRoles.contains(role) {
            return true
        }
        if let role, nonEditableRoles.contains(role) {
            return false
        }

        var value: AnyObject?
        return AXUIElementCopyAttributeValue(
            element, kAXSelectedTextRangeAttribute as CFString, &value) == .success
    }

    private static func isAttributeSettable(_ element: AXUIElement, _ attribute: String) -> Bool {
        var settable: DarwinBoolean = false
        let status = AXUIElementIsAttributeSettable(element, attribute as CFString, &settable)
        return status == .success && settable.boolValue
    }

    private static func attributeString(_ element: AXUIElement, _ attribute: String) -> String? {
        var value: AnyObject?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else {
            return nil
        }
        return value as? String
    }

    private static func copyToClipboard(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }

    /// Synthesizes a Cmd+V keystroke into whatever currently has focus —
    /// only called right after confirming focus hasn't changed since
    /// recording started, so this always lands in the intended app.
    private static func sendCommandV() {
        let vKeyCode: CGKeyCode = 0x09  // kVK_ANSI_V
        let source = CGEventSource(stateID: .combinedSessionState)
        guard
            let keyDown = CGEvent(keyboardEventSource: source, virtualKey: vKeyCode, keyDown: true),
            let keyUp = CGEvent(keyboardEventSource: source, virtualKey: vKeyCode, keyDown: false)
        else {
            return
        }
        keyDown.flags = .maskCommand
        keyUp.flags = .maskCommand
        keyDown.post(tap: .cghidEventTap)
        keyUp.post(tap: .cghidEventTap)
    }
}
