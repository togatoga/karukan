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

    // MARK: - Event handling

    override func recognizedEvents(_ sender: Any!) -> Int {
        Int(NSEvent.EventTypeMask.keyDown.rawValue)
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event, event.type == .keyDown else { return false }
        guard let client = sender as? (any IMKTextInput) else { return false }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        // Never swallow Command shortcuts.
        if flags.contains(.command) { return false }

        guard let key = KeyCodeMap.translate(event: event) else { return false }

        // Refresh the conversion context while no composition is active
        // (mirrors the fcitx5 addon, which captures surrounding text in the
        // Empty state). Queued before process_key on the same pipe, so the
        // engine sees it first.
        if !hasPreedit {
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

    // MARK: - Lifecycle

    override func activateServer(_ sender: Any!) {
        super.activateServer(sender)
        // Do not query the client here (client.attributes() during
        // activation can deadlock Chromium); surrounding text and window
        // positioning happen lazily on the first key event.
    }

    override func deactivateServer(_ sender: Any!) {
        // Mozc-style: commit the pending preedit on focus loss, then
        // persist what the user taught us.
        if let client = sender as? (any IMKTextInput),
            let result = engineClient.commitSync()
        {
            apply(actions: result.actions, client: client)
        }
        engineClient.saveLearningAsync()
        Self.candidateWindow.hide()
        super.deactivateServer(sender)
    }

    override func commitComposition(_ sender: Any!) {
        if let client = sender as? (any IMKTextInput),
            let result = engineClient.commitSync()
        {
            apply(actions: result.actions, client: client)
        }
        Self.candidateWindow.hide()
    }

    // MARK: - Applying engine actions

    private func apply(actions: [EngineAction], client: any IMKTextInput) {
        for action in actions {
            switch action {
            case .commit(let text):
                client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))

            case .updatePreedit(let text, let caret, let attributes):
                hasPreedit = !text.isEmpty
                setMarkedText(text: text, caret: caret, attributes: attributes, client: client)

            case .showCandidates(let candidates, let cursor, let page, let totalPages, _):
                var lineHeightRect = NSRect.zero
                client.attributes(forCharacterIndex: 0, lineHeightRectangle: &lineHeightRect)
                Self.candidateWindow.show(
                    candidates: candidates,
                    cursor: cursor,
                    page: page,
                    totalPages: totalPages,
                    cursorRect: lineHeightRect
                )

            case .hideCandidates:
                Self.candidateWindow.hide()

            case .updateAux(let text):
                Self.candidateWindow.setAux(text)

            case .hideAux:
                Self.candidateWindow.setAux(nil)
            }
        }
    }

    /// Send the text left of the cursor to the engine as conversion
    /// context. Conservative Mozc-style guards: skip clients that don't
    /// report a cursor and large documents (slow attributedSubstring IPC).
    private func sendSurroundingText(client: any IMKTextInput) {
        let documentLength = client.length()
        guard documentLength > 0, documentLength < 1000 else { return }
        let selected = client.selectedRange()
        guard selected.location != NSNotFound, selected.location > 0 else { return }

        let maxContextUTF16 = 40  // engine truncates further per its config
        let start = max(0, selected.location - maxContextUTF16)
        let range = NSRange(location: start, length: selected.location - start)
        guard let leftContext = client.attributedSubstring(from: range)?.string,
            !leftContext.isEmpty
        else { return }

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
            case "underline":
                style = .single
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
