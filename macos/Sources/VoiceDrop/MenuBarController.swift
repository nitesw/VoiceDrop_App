import AppKit

/// The persistent Menu Bar Icon (`NSStatusItem`). Single click opens a
/// dropdown: Enable/Disable toggle, "Settings…" (disabled — Phase 5 hasn't
/// built the Settings Window yet), and Quit. Enabled/disabled state is
/// reflected via the button's opacity (`appearsDisabled`), never by
/// introducing color, per VISUAL_STYLE.md.
final class MenuBarController: NSObject {
    private let statusItem: NSStatusItem
    private let enabledMenuItem = NSMenuItem()

    /// Called whenever the user toggles Enabled/Disabled from the dropdown.
    /// The caller (main.swift) is responsible for actually suspending the
    /// Push-to-Talk Hotkey listener — this controller only owns the menu
    /// and its own displayed state.
    var onToggleEnabled: ((Bool) -> Void)?

    private(set) var isEnabled = true

    override init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        super.init()
        configureButton()
        configureMenu()
        updateAppearance()
    }

    private func configureButton() {
        guard let button = statusItem.button else { return }
        if let image = MenuBarIconLoader.load() {
            button.image = image
        } else {
            // Fallback so the app is still reachable if the icon fails to
            // load for some reason — never worse than an invisible item.
            voiceDropLog.log("Menu bar icon failed to load; falling back to text.")
            button.title = "VD"
        }
    }

    private func configureMenu() {
        let menu = NSMenu()

        enabledMenuItem.title = "Enabled"
        enabledMenuItem.target = self
        enabledMenuItem.action = #selector(toggleEnabled)
        menu.addItem(enabledMenuItem)

        menu.addItem(.separator())

        let settingsItem = NSMenuItem(title: "Settings…", action: nil, keyEquivalent: "")
        settingsItem.isEnabled = false  // Phase 5 — Settings Window doesn't exist yet.
        menu.addItem(settingsItem)

        menu.addItem(.separator())

        let quitItem = NSMenuItem(title: "Quit VoiceDrop", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu
    }

    @objc private func toggleEnabled() {
        isEnabled.toggle()
        updateAppearance()
        onToggleEnabled?(isEnabled)
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func updateAppearance() {
        enabledMenuItem.state = isEnabled ? .on : .off
        statusItem.button?.appearsDisabled = !isEnabled
    }
}
