import AppKit
import ServiceManagement

/// The persistent Menu Bar Icon (`NSStatusItem`). Single click opens a
/// dropdown: Enable/Disable toggle, tap-to-toggle recording, Launch at
/// Login, and Quit. There is no Settings window by
/// design (ADR-0009) — everything else is hotkey-driven. Enabled/disabled state is
/// reflected via the button's opacity (`appearsDisabled`), never by
/// introducing color, per VISUAL_STYLE.md.
final class MenuBarController: NSObject {
    private let statusItem: NSStatusItem
    private let enabledMenuItem = NSMenuItem()
    private let launchAtLoginItem = NSMenuItem()
    private let tapToToggleItem = NSMenuItem()

    /// Called when the user switches between holding ⌥Space and tapping it
    /// to start/stop. main.swift forwards it to `HotkeyMonitor`.
    var onToggleTapMode: ((Bool) -> Void)?

    /// Whether ⌥Space taps start/stop recording instead of being held.
    /// Set by main.swift at launch from the saved mode.
    var isTapToToggle = false {
        didSet { updateAppearance() }
    }

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

        tapToToggleItem.title = "Tap ⌥Space to Start/Stop"
        tapToToggleItem.target = self
        tapToToggleItem.action = #selector(toggleTapMode)
        menu.addItem(tapToToggleItem)

        launchAtLoginItem.title = "Launch at Login"
        launchAtLoginItem.target = self
        launchAtLoginItem.action = #selector(toggleLaunchAtLogin)
        menu.addItem(launchAtLoginItem)

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

    @objc private func toggleTapMode() {
        isTapToToggle.toggle()
        onToggleTapMode?(isTapToToggle)
    }

    /// Registers/unregisters the app itself as a login item via
    /// `SMAppService` (macOS 13+). macOS remembers this per app location, so
    /// it's meant for the copy installed in /Applications.
    @objc private func toggleLaunchAtLogin() {
        let service = SMAppService.mainApp
        do {
            if service.status == .enabled {
                try service.unregister()
            } else {
                try service.register()
            }
        } catch {
            voiceDropLog.log("Launch at Login change failed: \(error.localizedDescription, privacy: .public)")
        }
        if service.status == .requiresApproval {
            // The user turned VoiceDrop off in System Settings → Login Items
            // before; only they can turn it back on there.
            SMAppService.openSystemSettingsLoginItems()
        }
        updateAppearance()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func updateAppearance() {
        enabledMenuItem.state = isEnabled ? .on : .off
        tapToToggleItem.state = isTapToToggle ? .on : .off
        launchAtLoginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off
        statusItem.button?.appearsDisabled = !isEnabled
    }
}
