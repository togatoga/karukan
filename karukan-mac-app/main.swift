import Cocoa
import InputMethodKit

class AppDelegate: NSObject, NSApplicationDelegate {
    // Global IMKServer instance — must stay alive for the lifetime of the process
    static var server: IMKServer!

    func applicationDidFinishLaunching(_ notification: Notification) {
        AppDelegate.server = IMKServer(
            name: "Karukan_Connection",
            bundleIdentifier: Bundle.main.bundleIdentifier!
        )
        NSLog("Karukan IME server started")
    }
}

// Entry point
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
