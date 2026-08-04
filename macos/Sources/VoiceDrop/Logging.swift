import os

/// Shared logger for the whole app. `NSLog`'s `%{public}@` hint is silently
/// ignored — legacy NSLog is routed through the unified log with everything
/// private by default, and that syntax only works with `os.Logger`'s
/// compile-time-parsed format strings. Every dynamic value logged below must
/// be explicitly marked `privacy: .public` or it will show as `<private>`
/// in Console.app.
let voiceDropLog = Logger(subsystem: "com.voicedrop.app", category: "app")
