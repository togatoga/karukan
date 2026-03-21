import Cocoa
import InputMethodKit

class KarukanInputController: IMKInputController {
    private var engine: OpaquePointer?
    private var engineInitialized = false

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        engine = karukan_mac_engine_new()
        if engine != nil {
            NSLog("Karukan: engine created")
            // Initialize model in background to avoid blocking input
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                guard let self = self, let engine = self.engine else { return }
                let result = karukan_mac_engine_init(engine)
                if result == 0 {
                    self.engineInitialized = true
                    NSLog("Karukan: engine initialized successfully")
                } else {
                    NSLog("Karukan: engine initialization failed")
                }
            }
        }
    }

    deinit {
        if let engine = engine {
            karukan_mac_engine_free(engine)
        }
    }

    // MARK: - Key Event Handling

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event = event, let engine = engine else {
            return false
        }
        guard let client = sender as? (any IMKTextInput) else {
            return false
        }

        let keycode = event.keyCode
        let mods = UInt64(event.modifierFlags.rawValue)

        switch event.type {
        case .keyDown:
            let character = event.characters?.unicodeScalars.first.map { UInt32($0.value) } ?? 0
            let consumed = karukan_mac_process_key(engine, keycode, character, mods, 1)
            if consumed != 0 {
                updateUI(client: client)
                return true
            }
            return false

        case .keyUp:
            let character = event.characters?.unicodeScalars.first.map { UInt32($0.value) } ?? 0
            let consumed = karukan_mac_process_key(engine, keycode, character, mods, 0)
            if consumed != 0 {
                updateUI(client: client)
                return true
            }
            return false

        case .flagsChanged:
            // For modifier-only events, determine press/release by checking the bit
            let isPress = isModifierPress(event: event)
            let consumed = karukan_mac_process_key(engine, keycode, 0, mods, isPress ? 1 : 0)
            if consumed != 0 {
                updateUI(client: client)
                return true
            }
            return false

        default:
            return false
        }
    }

    // MARK: - IMK Lifecycle

    override func activateServer(_ sender: Any!) {
        super.activateServer(sender)
        NSLog("Karukan: activated")
    }

    override func deactivateServer(_ sender: Any!) {
        guard let engine = engine else {
            super.deactivateServer(sender)
            return
        }

        // Commit any pending input
        if karukan_mac_engine_is_empty(engine) == 0 {
            let committed = karukan_mac_engine_commit(engine)
            if committed != 0, let client = sender as? (any IMKTextInput) {
                if let commitPtr = karukan_mac_engine_get_commit(engine) {
                    let text = String(cString: commitPtr)
                    client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
                }
            }
        }

        // Save learning cache
        karukan_mac_engine_save_learning(engine)
        super.deactivateServer(sender)
    }

    // MARK: - UI Update

    private func updateUI(client: any IMKTextInput) {
        guard let engine = engine else { return }

        // Handle commit first
        if karukan_mac_engine_has_commit(engine) != 0 {
            if let commitPtr = karukan_mac_engine_get_commit(engine) {
                let text = String(cString: commitPtr)
                client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
            }
        }

        // Update preedit (marked text)
        if karukan_mac_engine_has_preedit(engine) != 0 {
            if let preeditPtr = karukan_mac_engine_get_preedit(engine) {
                let text = String(cString: preeditPtr)
                let caretChars = Int(karukan_mac_engine_get_preedit_caret(engine))

                if text.isEmpty {
                    // Clear marked text
                    client.setMarkedText(
                        "",
                        selectionRange: NSRange(location: 0, length: 0),
                        replacementRange: NSRange(location: NSNotFound, length: 0)
                    )
                } else {
                    // Create attributed string with underline for composition
                    let attrs: [NSAttributedString.Key: Any] = [
                        .underlineStyle: NSUnderlineStyle.single.rawValue,
                        .foregroundColor: NSColor.textColor,
                    ]
                    let attrStr = NSAttributedString(string: text, attributes: attrs)

                    client.setMarkedText(
                        attrStr,
                        selectionRange: NSRange(location: caretChars, length: 0),
                        replacementRange: NSRange(location: NSNotFound, length: 0)
                    )
                }
            }
        }
    }

    // MARK: - Helpers

    /// Determine if a flagsChanged event is a press or release.
    /// We check whether the modifier bit corresponding to the keycode is now set.
    private func isModifierPress(event: NSEvent) -> Bool {
        let flags = event.modifierFlags
        switch event.keyCode {
        case 0x38, 0x3C: // Shift L/R
            return flags.contains(.shift)
        case 0x3B, 0x3E: // Control L/R
            return flags.contains(.control)
        case 0x3A, 0x3D: // Option L/R
            return flags.contains(.option)
        case 0x37, 0x36: // Command L/R
            return flags.contains(.command)
        default:
            return false
        }
    }
}
