import XCTest

@testable import KarukanIME

final class KeyCodeMapTests: XCTestCase {
    func testPrintableAscii() {
        let event = KeyCodeMap.translate(keyCode: 0, charactersIgnoringModifiers: "a", flags: [])
        XCTAssertEqual(event?.keysym, 0x61)
        XCTAssertEqual(event?.modifiers.shift, false)
    }

    func testShiftedLetter() {
        let event = KeyCodeMap.translate(
            keyCode: 0, charactersIgnoringModifiers: "A", flags: [.shift])
        XCTAssertEqual(event?.keysym, 0x41)
        XCTAssertEqual(event?.modifiers.shift, true)
    }

    func testSpace() {
        let event = KeyCodeMap.translate(keyCode: 49, charactersIgnoringModifiers: " ", flags: [])
        XCTAssertEqual(event?.keysym, 0x20)
    }

    func testReturnKey() {
        let event = KeyCodeMap.translate(keyCode: 36, charactersIgnoringModifiers: "\r", flags: [])
        XCTAssertEqual(event?.keysym, 0xff0d)
    }

    func testEscape() {
        let event = KeyCodeMap.translate(
            keyCode: 53, charactersIgnoringModifiers: "\u{1b}", flags: [])
        XCTAssertEqual(event?.keysym, 0xff1b)
    }

    func testBackspace() {
        let event = KeyCodeMap.translate(
            keyCode: 51, charactersIgnoringModifiers: "\u{7f}", flags: [])
        XCTAssertEqual(event?.keysym, 0xff08)
    }

    func testArrowKeys() {
        XCTAssertEqual(
            KeyCodeMap.translate(keyCode: 123, charactersIgnoringModifiers: nil, flags: [])?.keysym,
            0xff51)
        XCTAssertEqual(
            KeyCodeMap.translate(keyCode: 124, charactersIgnoringModifiers: nil, flags: [])?.keysym,
            0xff53)
        XCTAssertEqual(
            KeyCodeMap.translate(keyCode: 125, charactersIgnoringModifiers: nil, flags: [])?.keysym,
            0xff54)
        XCTAssertEqual(
            KeyCodeMap.translate(keyCode: 126, charactersIgnoringModifiers: nil, flags: [])?.keysym,
            0xff52)
    }

    func testControlModifier() {
        let event = KeyCodeMap.translate(
            keyCode: 0, charactersIgnoringModifiers: "l", flags: [.control, .shift])
        XCTAssertEqual(event?.keysym, 0x6c)
        XCTAssertEqual(event?.modifiers.control, true)
        XCTAssertEqual(event?.modifiers.shift, true)
    }

    func testNonAsciiNotTranslated() {
        // Kana input layouts produce non-ASCII characters; unsupported.
        XCTAssertNil(KeyCodeMap.translate(keyCode: 0, charactersIgnoringModifiers: "あ", flags: []))
        XCTAssertNil(KeyCodeMap.translate(keyCode: 0, charactersIgnoringModifiers: nil, flags: []))
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
