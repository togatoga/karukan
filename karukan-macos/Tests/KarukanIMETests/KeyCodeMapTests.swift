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
