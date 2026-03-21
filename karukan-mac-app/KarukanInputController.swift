import Cocoa
import InputMethodKit
import os.log

private let logger = OSLog(subsystem: "com.togatoga.inputmethod.Karukan", category: "IME")

/// Log to ~/Library/Logs/Karukan.log (macOS filters NSLog/os_log from IME processes)
private func imeLog(_ message: String) {
    os_log("%{public}@", log: logger, type: .default, message)
    let path = NSHomeDirectory() + "/Library/Logs/Karukan.log"
    let line = "\(Date()): \(message)\n"
    if let fh = FileHandle(forWritingAtPath: path) {
        fh.seekToEndOfFile()
        fh.write(line.data(using: .utf8)!)
        fh.closeFile()
    } else {
        FileManager.default.createFile(atPath: path, contents: line.data(using: .utf8))
    }
}

@objc(KarukanInputController)
class KarukanInputController: IMKInputController {
    private var engine: OpaquePointer?
    private var engineInitialized = false

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        super.init(server: server, delegate: delegate, client: inputClient)
        engine = karukan_mac_engine_new()
        if engine != nil {
            imeLog("engine created")
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                guard let self = self, let engine = self.engine else { return }
                let result = karukan_mac_engine_init(engine)
                if result == 0 {
                    self.engineInitialized = true
                    imeLog("engine initialized successfully")
                } else {
                    imeLog("engine initialization failed")
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
            // Capture surrounding text on first keypress (Empty → Composing transition)
            if karukan_mac_engine_is_empty(engine) != 0 {
                captureSurroundingText(client: client)
            }

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
        if let client = sender as? (any IMKTextInput) {
            captureSurroundingText(client: client)
        }
    }

    override func deactivateServer(_ sender: Any!) {
        guard let engine = engine else {
            super.deactivateServer(sender)
            return
        }

        if karukan_mac_engine_is_empty(engine) == 0 {
            let committed = karukan_mac_engine_commit(engine)
            if committed != 0, let client = sender as? (any IMKTextInput) {
                if let commitPtr = karukan_mac_engine_get_commit(engine) {
                    let text = String(cString: commitPtr)
                    client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
                }
            }
        }

        karukan_mac_engine_save_learning(engine)
        super.deactivateServer(sender)
    }

    // MARK: - UI Update

    private func updateUI(client: any IMKTextInput) {
        guard let engine = engine else { return }

        if karukan_mac_engine_has_commit(engine) != 0 {
            if let commitPtr = karukan_mac_engine_get_commit(engine) {
                let text = String(cString: commitPtr)
                client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
            }
            // Delay so the client reflects the inserted text before we read it back
            DispatchQueue.main.async { [weak self] in
                self?.captureSurroundingText(client: client)
            }
        }

        if karukan_mac_engine_has_preedit(engine) != 0 {
            if let preeditPtr = karukan_mac_engine_get_preedit(engine) {
                let text = String(cString: preeditPtr)
                let caretChars = Int(karukan_mac_engine_get_preedit_caret(engine))

                if text.isEmpty {
                    client.setMarkedText(
                        "",
                        selectionRange: NSRange(location: 0, length: 0),
                        replacementRange: NSRange(location: NSNotFound, length: 0)
                    )
                } else {
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

    // MARK: - Surrounding Text

    private func captureSurroundingText(client: any IMKTextInput) {
        guard let engine = engine else { return }

        let selectedRange = client.selectedRange()
        if selectedRange.location == NSNotFound { return }

        let cursorPos = selectedRange.location
        if cursorPos == 0 { return }

        // Request only text before cursor (left context).
        // Requesting beyond document length causes nil returns in many apps.
        let contextLen = min(cursorPos, 40)
        let leftRange = NSRange(location: cursorPos - contextLen, length: contextLen)

        // attributedSubstring(from:) is more widely supported than string(from:actualRange:)
        if let attrStr = client.attributedSubstring(from: leftRange) {
            let text = attrStr.string
            if !text.isEmpty {
                let cursorInText = UInt32(text.count)
                text.withCString { cstr in
                    karukan_mac_engine_set_surrounding_text(engine, cstr, cursorInText)
                }
                return
            }
        }

        // Fallback
        var actualRange = NSRange(location: NSNotFound, length: 0)
        if let text = client.string(from: leftRange, actualRange: &actualRange), !text.isEmpty {
            let cursorInText = UInt32(text.count)
            text.withCString { cstr in
                karukan_mac_engine_set_surrounding_text(engine, cstr, cursorInText)
            }
        }
    }

    // MARK: - Helpers

    private func isModifierPress(event: NSEvent) -> Bool {
        let flags = event.modifierFlags
        switch event.keyCode {
        case 0x38, 0x3C: return flags.contains(.shift)
        case 0x3B, 0x3E: return flags.contains(.control)
        case 0x3A, 0x3D: return flags.contains(.option)
        case 0x37, 0x36: return flags.contains(.command)
        default: return false
        }
    }
}
