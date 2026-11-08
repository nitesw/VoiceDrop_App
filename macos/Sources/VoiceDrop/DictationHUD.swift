import AppKit
import SwiftUI

/// Every state the Dictation HUD can show. Monochrome throughout — the
/// waveform is pure white too, not a system accent color (overrides
/// VISUAL_STYLE.md's original "reserve accent color for the waveform"
/// reading; the actual design call ended up all-white).
enum HUDState: Equatable {
    case hidden
    case downloadingModel(progress: Double)
    case recording(level: Float)
    case processing
    case noSpeech
    /// `reason` is for logging only, never shown to the user — the HUD
    /// text is always the generic "Copied to clipboard" regardless of
    /// *why* injection fell back.
    case fallback(reason: String)
    /// Secure field: the transcript was discarded, never injected or
    /// copied anywhere — see `TextInjector.InjectionOutcome.discarded`.
    case discarded
    case error(message: String)
    /// Brief confirmation after ⌥⇧Space switches the dictation language.
    case language(name: String)
}

private struct HUDContentView: View {
    let state: HUDState

    var body: some View {
        HStack(spacing: 10) {
            switch state {
            case .hidden:
                EmptyView()
            case .downloadingModel(let progress):
                ProgressView(value: progress)
                    .frame(width: 120)
                Text("Downloading model…")
                    .foregroundStyle(.secondary)
            case .recording(let level):
                WaveformTraceView(level: CGFloat(level))
                    .frame(width: 100, height: 36)
            case .processing:
                ProgressView()
                    .controlSize(.small)
                Text("Processing…")
                    .foregroundStyle(.secondary)
            case .noSpeech:
                Text("No speech detected")
                    .foregroundStyle(.secondary)
            case .fallback:
                Text("Copied to clipboard")
                    .foregroundStyle(.secondary)
            case .discarded:
                Text("Secure field — not saved")
                    .foregroundStyle(.secondary)
            case .error(let message):
                Text(message)
                    .foregroundStyle(.secondary)
            case .language(let name):
                Text(name)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .fixedSize()
        .background(Color.black.opacity(0.85), in: Capsule())
        .foregroundStyle(.white)
    }
}

/// Discrete vertical bars (the original style), each reacting to sound —
/// but instead of a fixed 5 bars all pulsing simultaneously in one spot,
/// this holds a rolling history of levels, one per bar slot: each tick, the
/// newest sample enters on the right and every older sample shifts one
/// slot left, falling off the left edge once it scrolls out. The bar
/// *positions* never move — only the data flowing through them does — but
/// because each position's value changes every frame, it reads as motion:
/// newest on the right, oldest sliding off the left, matching how a level
/// meter/heart-rate monitor scrolls.
///
/// Keeps its own rolling history in `@State`, which only works because
/// `DictationHUDController` updates one persistent `NSHostingView`'s
/// `rootView` in place (rather than replacing the view each call) — that's
/// what lets SwiftUI diff this view against its previous frame instead of
/// recreating it from scratch 30 times a second and losing the history on
/// every tick.
private struct WaveformTraceView: View {
    let level: CGFloat
    private static let barCount = 14
    private let maxBarHeight: CGFloat = 32
    @State private var history: [CGFloat] = Array(repeating: 0, count: WaveformTraceView.barCount)

    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<history.count, id: \.self) { i in
                Capsule()
                    .fill(Color.white)
                    .frame(width: 4, height: max(4, maxBarHeight * history[i]))
            }
        }
        .animation(.easeOut(duration: 0.08), value: history)
        .onChange(of: level) { newLevel in
            history.removeFirst()
            history.append(newLevel)
        }
    }
}

/// Borderless, always-on-top panel hosting the HUD. Never becomes key or
/// main, and ignores mouse events entirely — per "HUD never steals focus
/// from the Injection Target app," it must not be clickable at all, since
/// even an accidental click could shift focus away from the target.
///
/// The panel is not kept for the app's lifetime: `hide()` throws it away
/// and the next `show` builds a fresh one. A long-lived panel broke after
/// the display slept: macOS rebuilt its Spaces on wake and left the panel
/// in none of them, so `orderFrontRegardless` silently did nothing and the
/// HUD never appeared again until relaunch. A new window always lands in
/// the current Space. Waking and display changes also drop the panel
/// (rebuilding it at once if it's on screen), for a HUD that stays up
/// across a sleep, such as the "Allow Accessibility" prompt.
final class DictationHUDController {
    private var panel: NSPanel?
    private var hostingView: NSHostingView<HUDContentView>?
    private var dismissTimer: Timer?
    /// What's on screen now (`.hidden` when nothing is), so a panel dropped
    /// while visible can be rebuilt showing the same thing.
    private var currentState: HUDState = .hidden
    private var observers: [(NotificationCenter, NSObjectProtocol)] = []

    init() {
        let workspace = NSWorkspace.shared.notificationCenter
        for (center, name) in [
            (workspace, NSWorkspace.screensDidWakeNotification),
            (workspace, NSWorkspace.didWakeNotification),
            (NotificationCenter.default, NSApplication.didChangeScreenParametersNotification),
        ] {
            let token = center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.rebuildPanel()
            }
            observers.append((center, token))
        }
    }

    deinit {
        for (center, token) in observers { center.removeObserver(token) }
    }

    func show(_ state: HUDState, autoDismissAfter: TimeInterval? = nil) {
        dismissTimer?.invalidate()
        dismissTimer = nil
        currentState = state

        let panel = ensurePanel()
        // Update the existing hosting view's rootView rather than
        // replacing it — replacing it would tear down and recreate the
        // whole SwiftUI view tree on every call, which for
        // WaveformTraceView (updated ~30x/second while recording) would
        // reset its rolling history back to empty on every single frame.
        if let hostingView {
            hostingView.rootView = HUDContentView(state: state)
        } else {
            let hosting = NSHostingView(rootView: HUDContentView(state: state))
            panel.contentView = hosting
            hostingView = hosting
        }
        hostingView?.layoutSubtreeIfNeeded()
        let fitting = hostingView?.fittingSize ?? NSSize(width: 200, height: 44)
        positionPanel(panel, size: fitting)
        panel.orderFrontRegardless()

        if let autoDismissAfter {
            dismissTimer = Timer.scheduledTimer(withTimeInterval: autoDismissAfter, repeats: false) { [weak self] _ in
                self?.hide()
            }
        }
    }

    func hide() {
        dismissTimer?.invalidate()
        dismissTimer = nil
        currentState = .hidden
        discardPanel()
    }

    /// Replaces the on-screen panel after a wake or display change, with
    /// the same content and any pending auto-dismiss kept. A hidden HUD has
    /// no panel (see `hide`), so there's nothing to do then.
    private func rebuildPanel() {
        guard panel != nil, currentState != .hidden else { return }
        discardPanel()
        voiceDropLog.log("Display woke or changed — rebuilt the HUD window.")
        let pendingDismiss = dismissTimer
        let state = currentState
        dismissTimer = nil
        show(state)
        dismissTimer = pendingDismiss
    }

    private func discardPanel() {
        panel?.orderOut(nil)
        panel?.close()
        panel = nil
        hostingView = nil
    }

    private func ensurePanel() -> NSPanel {
        if let panel { return panel }
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 44),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        // Only `self.panel` owns it, so `close()` must not also release it.
        panel.isReleasedWhenClosed = false
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        // Never intercepts clicks — see the type doc above.
        panel.ignoresMouseEvents = true
        self.panel = panel
        return panel
    }

    /// Always bottom-center of the main screen (ADR-0009: no position
    /// picker).
    private func positionPanel(_ panel: NSPanel, size: NSSize) {
        guard let screen = NSScreen.main else { return }
        let screenFrame = screen.visibleFrame
        let x = screenFrame.midX - size.width / 2
        let y = screenFrame.minY + 60
        panel.setFrame(NSRect(x: x, y: y, width: size.width, height: size.height), display: true)
    }
}
