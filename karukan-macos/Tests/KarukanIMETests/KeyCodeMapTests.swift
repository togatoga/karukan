import Cocoa
import XCTest

@testable import KarukanIME

final class KeyCodeMapTests: XCTestCase {
    func testPrintableAscii() {
        let event = KeyCodeMap.translate(
            keyCode: 0, characters: "a", charactersIgnoringModifiers: "a", flags: [])
        XCTAssertEqual(event?.keysym, 0x61)
        XCTAssertEqual(event?.modifiers.shift, false)
    }

    func testShiftedLetter() {
        let event = KeyCodeMap.translate(
            keyCode: 0, characters: "A", charactersIgnoringModifiers: "A", flags: [.shift])
        XCTAssertEqual(event?.keysym, 0x41)
        XCTAssertEqual(event?.modifiers.shift, true)
    }

    func testShiftedPunctuation() {
        // IMK key events resolve Shift only in `characters`: Shift+/ comes
        // in as characters="?" but charactersIgnoringModifiers="/". The
        // shifted form must win or ？ becomes ・.
        let event = KeyCodeMap.translate(
            keyCode: 44, characters: "?", charactersIgnoringModifiers: "/", flags: [.shift])
        XCTAssertEqual(event?.keysym, 0x3f)
        XCTAssertEqual(event?.modifiers.shift, true)
    }

    func testControlKeyFallsBackToIgnoringModifiers() {
        // Ctrl+A: `characters` is the control character U+0001; the engine
        // wants the plain key plus the control flag (like fcitx5 sends).
        let event = KeyCodeMap.translate(
            keyCode: 0, characters: "\u{01}", charactersIgnoringModifiers: "a",
            flags: [.control])
        XCTAssertEqual(event?.keysym, 0x61)
        XCTAssertEqual(event?.modifiers.control, true)
    }

    func testOptionGlyphFallsBackToIgnoringModifiers() {
        // Option+a: `characters` is "å"; fall back to the plain key.
        let event = KeyCodeMap.translate(
            keyCode: 0, characters: "å", charactersIgnoringModifiers: "a", flags: [.option])
        XCTAssertEqual(event?.keysym, 0x61)
        XCTAssertEqual(event?.modifiers.alt, true)
    }

    func testSpace() {
        let event = KeyCodeMap.translate(
            keyCode: 49, characters: " ", charactersIgnoringModifiers: " ", flags: [])
        XCTAssertEqual(event?.keysym, 0x20)
    }

    func testReturnKey() {
        let event = KeyCodeMap.translate(
            keyCode: 36, characters: "\r", charactersIgnoringModifiers: "\r", flags: [])
        XCTAssertEqual(event?.keysym, 0xff0d)
    }

    func testEscape() {
        let event = KeyCodeMap.translate(
            keyCode: 53, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", flags: [])
        XCTAssertEqual(event?.keysym, 0xff1b)
    }

    func testBackspace() {
        let event = KeyCodeMap.translate(
            keyCode: 51, characters: "\u{7f}", charactersIgnoringModifiers: "\u{7f}", flags: [])
        XCTAssertEqual(event?.keysym, 0xff08)
    }

    func testArrowKeys() {
        for (keyCode, keysym) in [(123, 0xff51), (124, 0xff53), (125, 0xff54), (126, 0xff52)] {
            XCTAssertEqual(
                KeyCodeMap.translate(
                    keyCode: UInt16(keyCode), characters: nil,
                    charactersIgnoringModifiers: nil, flags: []
                )?.keysym,
                UInt32(keysym))
        }
    }

    func testControlModifier() {
        let event = KeyCodeMap.translate(
            keyCode: 0, characters: "\u{0c}", charactersIgnoringModifiers: "l",
            flags: [.control, .shift])
        XCTAssertEqual(event?.keysym, 0x6c)
        XCTAssertEqual(event?.modifiers.control, true)
        XCTAssertEqual(event?.modifiers.shift, true)
    }

    func testNonAsciiNotTranslated() {
        // Kana input layouts produce non-ASCII characters; unsupported.
        XCTAssertNil(
            KeyCodeMap.translate(
                keyCode: 0, characters: "あ", charactersIgnoringModifiers: "あ", flags: []))
        XCTAssertNil(
            KeyCodeMap.translate(
                keyCode: 0, characters: nil, charactersIgnoringModifiers: nil, flags: []))
    }
}

final class Utf16ConversionTests: XCTestCase {
    func testAsciiOffsets() {
        XCTAssertEqual(utf16Offset(ofScalarOffset: 2, in: "abc"), 2)
    }

    func testJapaneseOffsets() {
        XCTAssertEqual(utf16Offset(ofScalarOffset: 2, in: "かきく"), 2)
    }

    func testSurrogatePairOffsets() {
        // 𛀗 (hentaigana) is a surrogate pair in UTF-16: 1 scalar == 2 units.
        XCTAssertEqual(utf16Offset(ofScalarOffset: 1, in: "𛀗か"), 2)
        XCTAssertEqual(utf16Offset(ofScalarOffset: 2, in: "𛀗か"), 3)
    }

    func testOffsetClamping() {
        XCTAssertEqual(utf16Offset(ofScalarOffset: 100, in: "かき"), 2)
        XCTAssertEqual(utf16Offset(ofScalarOffset: -1, in: "かき"), 0)
    }

    func testRange() {
        let range = utf16Range(of: 1..<3, in: "𛀗かき")
        XCTAssertEqual(range, NSRange(location: 2, length: 2))
    }
}

final class RightCommandTapDetectorTests: XCTestCase {
    private let rcmd = KeyCodeMap.rightCommandKeyCode
    private let lcmd: UInt16 = 55  // kVK_Command (left)
    private let shiftKey: UInt16 = 56  // kVK_Shift

    /// Press/release with injected time and session press counter so the
    /// tests are deterministic (no real CGEventSource dependency).
    private func down(
        _ d: inout RightCommandTapDetector, keyCode: UInt16? = nil,
        flags: NSEvent.ModifierFlags = [.command], at t: TimeInterval = 0, count: UInt32 = 0
    ) -> Bool {
        d.handleFlagsChanged(
            keyCode: keyCode ?? rcmd, flags: flags, now: t, pressCount: { count })
    }

    private func up(
        _ d: inout RightCommandTapDetector, keyCode: UInt16? = nil,
        flags: NSEvent.ModifierFlags = [], at t: TimeInterval = 0.1, count: UInt32 = 0
    ) -> Bool {
        d.handleFlagsChanged(
            keyCode: keyCode ?? rcmd, flags: flags, now: t, pressCount: { count })
    }

    func testLoneTapFires() {
        var detector = RightCommandTapDetector()
        XCTAssertFalse(down(&detector))
        XCTAssertTrue(up(&detector))
    }

    func testSecondTapFiresAgain() {
        var detector = RightCommandTapDetector()
        _ = down(&detector)
        XCTAssertTrue(up(&detector))
        _ = down(&detector, at: 1.0)
        XCTAssertTrue(up(&detector, at: 1.1))
    }

    func testSessionPressDuringHoldCancels() {
        // ⌘V where the V keyDown is claimed by the menu and never reaches
        // handle(): the session press counter still ticks, so no fire.
        var detector = RightCommandTapDetector()
        _ = down(&detector, count: 10)
        XCTAssertFalse(up(&detector, count: 11))
    }

    func testMouseClickDuringHoldCancels() {
        // Right-⌘-click: mouse events never reach an IMKInputController,
        // but they tick the session press counter.
        var detector = RightCommandTapDetector()
        _ = down(&detector, count: 5)
        XCTAssertFalse(up(&detector, count: 6))
    }

    func testLongHoldDoesNotFire() {
        var detector = RightCommandTapDetector()
        _ = down(&detector, at: 0)
        XCTAssertFalse(up(&detector, at: RightCommandTapDetector.maxTapDuration + 0.01))
    }

    func testKeyDownCancelsPendingTap() {
        // A key that does reach handle() cancels via the fast path.
        var detector = RightCommandTapDetector()
        _ = down(&detector)
        detector.cancel()
        XCTAssertFalse(up(&detector))
    }

    func testOtherModifierDuringHoldCancels() {
        // Right ⌘ down, Shift down (chord), Shift up, right ⌘ up → no fire.
        var detector = RightCommandTapDetector()
        _ = down(&detector)
        XCTAssertFalse(down(&detector, keyCode: shiftKey, flags: [.command, .shift]))
        XCTAssertFalse(down(&detector, keyCode: shiftKey, flags: [.command]))
        XCTAssertFalse(up(&detector))
    }

    func testPressWhileOtherModifierHeldDoesNotArm() {
        // Shift held, then right ⌘ tapped: a ⇧⌘ chord, not a tap.
        var detector = RightCommandTapDetector()
        _ = down(&detector, keyCode: shiftKey, flags: [.shift])
        XCTAssertFalse(down(&detector, flags: [.command, .shift]))
        XCTAssertFalse(up(&detector, flags: [.shift]))
    }

    func testRightTapWhileLeftCommandHeldDoesNotFire() {
        // Left ⌘ held the whole time: right ⌘'s release still reports
        // .command, so the tap never completes; releasing left ⌘ later
        // must not fire either.
        var detector = RightCommandTapDetector()
        _ = down(&detector, keyCode: lcmd)
        _ = down(&detector)
        XCTAssertFalse(up(&detector, flags: [.command]))
        XCTAssertFalse(up(&detector, keyCode: lcmd))
    }
}
