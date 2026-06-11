import Carbon
import Cocoa
import InputMethodKit

/// Thin InputMethodKit adapter for the karukan engine.
///
/// All IME state (Empty → Composing → Conversion, romaji conversion,
/// candidates, learning) lives in karukan-imserver; this controller only
/// translates key events and applies the resulting UI actions, mirroring
/// the fcitx5 addon (karukan.cpp).
@objc(KarukanInputController)
class KarukanInputController: IMKInputController {
    static let candidateWindow = CandidateWindowController()

    /// Mirrors whether the engine currently shows a preedit (updated from
    /// engine actions). Used to decide when to refresh surrounding text.
    private var hasPreedit = false

    /// True while the Roman (direct input) mode from ComponentInputModeDict
    /// is selected; every key passes through to the application.
    private var isRomanMode = false

    /// Right Command tap = return to Japanese input (Mozc-style; mirrors
    /// the right-Super mode toggle of the Linux frontend).
    private var rightCommandTap = RightCommandTapDetector()

    private static let japaneseModeID = "dev.togatoga.inputmethod.Karukan.Japanese"
    private static let romanModeID = "dev.togatoga.inputmethod.Karukan.Roman"

    // MARK: - Event handling

    override func recognizedEvents(_ sender: Any!) -> Int {
        Int(NSEvent.EventTypeMask.keyDown.union(.flagsChanged).rawValue)
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event else { return false }
        guard let client = sender as? (any IMKTextInput) else { return false }

        // Modifier events only matter for the right-Command tap; never
        // consume them so the system keeps tracking modifier state.
        if event.type == .flagsChanged {
            if rightCommandTap.handleFlagsChanged(
                keyCode: event.keyCode,
                rawModifierFlags: event.modifierFlags.rawValue
            ) {
                returnToJapaneseInput(client: client)
            }
            return false
        }
        guard event.type == .keyDown else { return false }
        rightCommandTap.handleKeyDown()

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        // Never swallow Command shortcuts.
        if flags.contains(.command) { return false }

        // JIS keyboard かな/英数 keys switch input modes (Mozc-style).
        switch event.keyCode {
        case KeyCodeMap.kanaKeyCode:
            // Full return to Japanese input, not just selectMode: かな must
            // also leave the engine-internal alphabet/katakana mode, both on
            // real JIS keyboards and when a Karabiner-style "command tap →
            // 英数/かな" rule turns a right-Command tap into this key (the
            // rule's lazy modifier means RightCommandTapDetector never sees
            // the tap).
            returnToJapaneseInput(client: client)
            return true
        case KeyCodeMap.eisuKeyCode:
            // Commit any pending composition before going direct.
            flushComposition(client: client)
            client.selectMode(Self.romanModeID)
            return true
        default:
            break
        }

        // Direct input: everything passes through to the application.
        if isRomanMode { return false }

        guard let key = KeyCodeMap.translate(event: event) else { return false }

        // Refresh the conversion context while no composition is active
        // (mirrors the fcitx5 addon, which captures surrounding text in the
        // Empty state). Queued before process_key on the same pipe, so the
        // engine sees it first. Skipped for function/navigation keysyms
        // (0xff00 range): they can't start a composition, and the three
        // synchronous client IPCs in sendSurroundingText would otherwise
        // fire on every arrow-key repeat.
        if !hasPreedit && key.keysym < 0xff00 {
            sendSurroundingText(client: client)
        }

        guard let result = engineClient.processKeySync(key) else {
            // Engine busy or dead: let the key pass through rather than
            // freezing input.
            return false
        }
        apply(actions: result.actions, client: client)
        return result.consumed
    }

    // MARK: - Input mode switching

    /// Right Command tap: one-way return to Japanese input, from either
    /// level of "half-width mode" — the Roman input mode (英数) or the
    /// engine-internal alphabet/katakana mode (entered via Shift+letter,
    /// which previously had no way back on macOS).
    private func returnToJapaneseInput(client: any IMKTextInput) {
        // Always selectMode (no-op when already selected) and clear
        // isRomanMode directly instead of waiting for setValue: when the
        // system mode is already Japanese but this session's cached
        // isRomanMode is stale-true (see activateServer), selectMode
        // changes nothing and setValue never fires — without the direct
        // reset the session would stay in pass-through forever.
        client.selectMode(Self.japaneseModeID)
        isRomanMode = false
        // Forward Super_R so the engine's mode toggle (alphabet/katakana →
        // hiragana) runs; a no-op when already in hiragana mode. Sent even
        // when leaving the Roman input mode, so a stale engine-internal
        // alphabet mode doesn't survive the round trip.
        let key = EngineKeyEvent(keysym: KeyCodeMap.superRKeysym, modifiers: KeyModifiers())
        if let result = engineClient.processKeySync(key) {
            apply(actions: result.actions, client: client)
        }
    }

    /// Called by the system when the user changes the input mode (IME menu,
    /// かな/英数 keys via selectMode, or System Settings).
    override func setValue(_ value: Any!, forTag tag: Int, client sender: Any!) {
        guard tag == kTextServiceInputModePropertyTag, let modeID = value as? String else {
            super.setValue(value, forTag: tag, client: sender)
            return
        }
        let wasRomanMode = isRomanMode
        isRomanMode = (modeID == Self.romanModeID)
        if isRomanMode && !wasRomanMode {
            // Leaving Japanese mode: flush the composition into the app.
            if let client = sender as? (any IMKTextInput) {
                flushComposition(client: client)
            } else {
                Self.candidateWindow.hide()
            }
        }
    }

    // MARK: - Lifecycle

    override func activateServer(_ sender: Any!) {
        super.activateServer(sender)
        // Do not query the client here (client.attributes() during
        // activation can deadlock Chromium); surrounding text and window
        // positioning happen lazily on the first key event.
        //
        // Re-sync isRomanMode with the system's actual input source: this
        // flag is per-session, and a mode switch made while another app's
        // session was active doesn't reach us via setValue. A stale true
        // silently passes every key through (typed romaji stays alphabet)
        // even though the menu bar shows か. TIS is a local API, not a
        // client query, so it is safe during activation.
        if let source = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
            let idPtr = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)
        {
            let id = Unmanaged<CFString>.fromOpaque(idPtr).takeUnretainedValue() as String
            if id == Self.japaneseModeID || id == Self.romanModeID {
                isRomanMode = (id == Self.romanModeID)
            }
        }
    }

    override func deactivateServer(_ sender: Any!) {
        // Mozc-style: commit the pending preedit on focus loss, then
        // persist what the user taught us.
        if let client = sender as? (any IMKTextInput) {
            flushComposition(client: client)
        } else {
            Self.candidateWindow.hide()
        }
        engineClient.saveLearningAsync()
        super.deactivateServer(sender)
    }

    override func commitComposition(_ sender: Any!) {
        if let client = sender as? (any IMKTextInput) {
            flushComposition(client: client)
        } else {
            Self.candidateWindow.hide()
        }
    }

    /// Commit any pending composition via the engine and apply the cleanup
    /// actions it emits (clear preedit, hide candidates/aux).
    private func flushComposition(client: any IMKTextInput) {
        if let result = engineClient.commitSync() {
            apply(actions: result.actions, client: client)
        } else {
            // Engine unavailable: still drop any stale candidate panel.
            Self.candidateWindow.hide()
        }
    }

    // MARK: - Applying engine actions

    private func apply(actions: [EngineAction], client: any IMKTextInput) {
        // The engine emits ShowCandidates before UpdateAux. Fold aux changes
        // in first (deferring their render when a candidate update follows)
        // so the panel is rendered once per batch, not once for the
        // candidates and again for the aux footer.
        let updatesCandidates = actions.contains {
            switch $0 {
            case .showCandidates, .hideCandidates: return true
            default: return false
            }
        }
        for action in actions {
            switch action {
            case .updateAux(let text):
                Self.candidateWindow.setAux(text, deferRender: updatesCandidates)
            case .hideAux:
                Self.candidateWindow.setAux(nil, deferRender: updatesCandidates)
            default:
                break
            }
        }

        for action in actions {
            switch action {
            case .commit(let text):
                client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))

            case .updatePreedit(let text, let caret, let attributes):
                hasPreedit = !text.isEmpty
                setMarkedText(text: text, caret: caret, attributes: attributes, client: client)

            case .showCandidates(let candidates, let cursor, let page, let totalPages):
                // Query the composition anchor (a synchronous IPC into the
                // focused app) only when the panel comes on screen; it
                // doesn't move while the panel stays visible.
                var cursorRect: NSRect?
                if !Self.candidateWindow.isVisible {
                    var lineHeightRect = NSRect.zero
                    client.attributes(forCharacterIndex: 0, lineHeightRectangle: &lineHeightRect)
                    cursorRect = lineHeightRect
                }
                Self.candidateWindow.show(
                    candidates: candidates,
                    cursor: cursor,
                    page: page,
                    totalPages: totalPages,
                    cursorRect: cursorRect
                )

            case .hideCandidates:
                Self.candidateWindow.hide()

            case .updateAux, .hideAux:
                break  // applied above
            }
        }
    }

    /// Send the text left of the cursor to the engine as conversion
    /// context. Gated on `selectedRange` only: `client.length()` is the
    /// least-implemented part of IMKTextInput (it returns 0 even in apps
    /// whose `attributedSubstring` works fine), and the request below is
    /// capped to 40 UTF-16 units anyway, so document size doesn't matter.
    /// Whether a client supports this at all is app-dependent (Cocoa text
    /// views do; Electron/Chromium/terminals mostly don't), so the skip
    /// reasons are logged for dogfooding visibility.
    private func sendSurroundingText(client: any IMKTextInput) {
        // When capture isn't possible, CLEAR the engine's context rather
        // than skipping: leaving the context from a previous cursor
        // position in place makes the engine condition on (and display)
        // text that is no longer left of the cursor. No context beats a
        // wrong one. selectedRange flakiness is per-keystroke in some
        // apps, so this also self-heals on the next successful capture.
        let selected = client.selectedRange()
        guard selected.location != NSNotFound, selected.location > 0 else {
            NSLog("KarukanIME: surrounding text cleared (no usable selection)")
            engineClient.setSurroundingTextAsync(text: "", cursorPos: 0)
            return
        }

        let maxContextUTF16 = 40  // engine truncates further per its config
        let start = max(0, selected.location - maxContextUTF16)
        let range = NSRange(location: start, length: selected.location - start)
        // string(from:actualRange:) rather than attributedSubstring(from:):
        // it's the IMKTextInput document-access method clients actually
        // implement (azooKey-Desktop settled on the same call).
        var actualRange = NSRange()
        guard let leftContext = client.string(from: range, actualRange: &actualRange),
            !leftContext.isEmpty
        else {
            NSLog("KarukanIME: surrounding text cleared (string(from:) unavailable)")
            engineClient.setSurroundingTextAsync(text: "", cursorPos: 0)
            return
        }

        NSLog("KarukanIME: surrounding text captured (\(leftContext.count) chars)")
        engineClient.setSurroundingTextAsync(
            text: leftContext,
            cursorPos: leftContext.unicodeScalars.count
        )
    }

    private func setMarkedText(
        text: String, caret: Int, attributes: [PreeditAttr], client: any IMKTextInput
    ) {
        guard !text.isEmpty else {
            client.setMarkedText(
                NSAttributedString(string: ""),
                selectionRange: NSRange(location: 0, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0)
            )
            return
        }

        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [.underlineStyle: NSUnderlineStyle.single.rawValue]
        )
        for attr in attributes {
            guard let range = utf16Range(of: attr.start..<attr.end, in: text) else { continue }
            let style: NSUnderlineStyle
            switch attr.style {
            // The focused/highlighted segment is drawn with a thick
            // underline (the convention azooKey/mac-akaza use for marked
            // text, since background colors are unreliable across apps).
            case "underline_double", "highlight", "reverse":
                style = .thick
            default:
                style = .single
            }
            attributed.addAttribute(.underlineStyle, value: style.rawValue, range: range)
        }

        let caretUTF16 = utf16Offset(ofScalarOffset: caret, in: text)
        client.setMarkedText(
            attributed,
            selectionRange: NSRange(location: caretUTF16, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
    }
}

// MARK: - Unicode scalar → UTF-16 offset conversion

/// The engine reports positions in Unicode scalar values; IMK APIs take
/// UTF-16 offsets.
func utf16Offset(ofScalarOffset offset: Int, in text: String) -> Int {
    let scalars = text.unicodeScalars
    let clamped = min(max(offset, 0), scalars.count)
    let index = scalars.index(scalars.startIndex, offsetBy: clamped)
    return text.utf16.distance(from: text.utf16.startIndex, to: index)
}

func utf16Range(of scalarRange: Range<Int>, in text: String) -> NSRange? {
    guard scalarRange.lowerBound >= 0, scalarRange.lowerBound <= scalarRange.upperBound else {
        return nil
    }
    let start = utf16Offset(ofScalarOffset: scalarRange.lowerBound, in: text)
    let end = utf16Offset(ofScalarOffset: scalarRange.upperBound, in: text)
    return NSRange(location: start, length: end - start)
}
