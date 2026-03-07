import Cocoa
import InputMethodKit

// IMKServer manages the connection between the input method and client apps
let bundleId = Bundle.main.bundleIdentifier ?? "com.karukan.inputmethod.Karukan"
let connectionName = "Karukan_Connection"

guard let server = IMKServer(name: connectionName, bundleIdentifier: bundleId) else {
    NSLog("Karukan: Failed to create IMKServer")
    exit(1)
}

NSLog("Karukan: IMKServer started (connection: \(connectionName))")

// Keep a reference to prevent deallocation
_ = server

NSApplication.shared.run()
