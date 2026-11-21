import Cocoa
import XCTest

@testable import KarukanIME

final class CandidatePanelSideTests: XCTestCase {
    private let visible = NSRect(x: 0, y: 0, width: 1400, height: 800)

    func testShortPanelNearTheBottomStaysBelow() {
        let caret = NSRect(x: 10, y: 100, width: 2, height: 20)
        XCTAssertEqual(
            CandidatePanelSide.choose(caret: caret, panelHeight: 50, visibleFrame: visible),
            .below
        )
        XCTAssertEqual(
            CandidatePanelSide.below.originY(caret: caret, panelHeight: 50),
            50
        )
    }

    func testTallPanelNearTheBottomGoesAbove() {
        // The same caret fits a short panel below, but not a full-page one.
        // Placement has to use the full height, or the window flips upward
        // the moment the list fills in.
        let caret = NSRect(x: 10, y: 100, width: 2, height: 20)
        XCTAssertEqual(
            CandidatePanelSide.choose(caret: caret, panelHeight: 200, visibleFrame: visible),
            .above
        )
        XCTAssertEqual(
            CandidatePanelSide.above.originY(caret: caret, panelHeight: 200),
            caret.maxY
        )
    }

    func testPanelBelowHangsFromTheCaretBottom() {
        let caret = NSRect(x: 40, y: 500, width: 4, height: 18)
        XCTAssertEqual(
            CandidatePanelSide.choose(caret: caret, panelHeight: 240, visibleFrame: visible),
            .below
        )
        XCTAssertEqual(
            CandidatePanelSide.below.originY(caret: caret, panelHeight: 240),
            caret.minY - 240
        )
    }
}

final class CandidatePanelFrameTests: XCTestCase {
    private let visible = NSRect(x: 0, y: 0, width: 1400, height: 800)

    func testWidePanelNearTheRightEdgeShiftsLeft() {
        let frame = NSRect(x: 1300, y: 400, width: 200, height: 100)
        let fitted = CandidatePanelFrame.fitted(frame, in: visible)
        XCTAssertEqual(fitted.maxX, visible.maxX, accuracy: 0.5)
        XCTAssertLessThan(fitted.minX, frame.minX)
        XCTAssertEqual(fitted.width, frame.width, accuracy: 0.5)
    }

    func testPanelPastTheBottomSlidesUp() {
        let frame = NSRect(x: 40, y: -30, width: 180, height: 120)
        let fitted = CandidatePanelFrame.fitted(frame, in: visible)
        XCTAssertEqual(fitted.minY, visible.minY, accuracy: 0.5)
        XCTAssertEqual(fitted.height, frame.height, accuracy: 0.5)
    }

    func testPanelThatAlreadyFitsIsLeftAlone() {
        let frame = NSRect(x: 40, y: 200, width: 180, height: 120)
        let fitted = CandidatePanelFrame.fitted(frame, in: visible)
        XCTAssertEqual(fitted, frame)
    }
}

final class CandidateWindowControllerTests: XCTestCase {
    func testHeightAndOriginStayPutAsCandidatesChange() throws {
        try skipWithoutScreen()
        let window = CandidateWindowController()
        let caret = NSRect(x: 200, y: 500, width: 4, height: 18)
        window.show(
            candidates: [CandidateItem(text: "変換", description: nil)],
            cursor: 0,
            page: 0,
            totalPages: 1,
            cursorRect: caret
        )
        let first = window.testingFrame
        window.show(
            candidates: (1...CandidateWindowController.rowCapacity).map {
                CandidateItem(text: "候補\($0)", description: nil)
            },
            cursor: 3,
            page: 0,
            totalPages: 1,
            cursorRect: nil
        )
        let second = window.testingFrame
        XCTAssertEqual(first.height, second.height, accuracy: 0.5)
        XCTAssertEqual(first.origin.y, second.origin.y, accuracy: 0.5)
        window.show(
            candidates: (1...CandidateWindowController.rowCapacity).map {
                CandidateItem(text: "候補\($0)", description: nil)
            },
            cursor: 0,
            page: 0,
            totalPages: 4,
            cursorRect: nil
        )
        XCTAssertEqual(window.testingFrame.height, first.height, accuracy: 0.5)
        window.hide()
    }

    func testLongCandidateNearTheRightEdgeStaysOnScreen() throws {
        try skipWithoutScreen()
        let screen = try XCTUnwrap(NSScreen.main)
        let window = CandidateWindowController()
        let caret = NSRect(x: screen.visibleFrame.maxX - 30, y: 500, width: 4, height: 18)
        window.show(
            candidates: [CandidateItem(text: String(repeating: "あ", count: 30), description: nil)],
            cursor: 0,
            page: 0,
            totalPages: 1,
            cursorRect: caret
        )
        let frame = window.testingFrame
        XCTAssertLessThanOrEqual(frame.maxX, screen.visibleFrame.maxX + 0.5)
        XCTAssertLessThan(frame.minX, caret.minX)
        window.hide()
    }

    func testLongCandidateWidensThePanel() throws {
        try skipWithoutScreen()
        let window = CandidateWindowController()
        let caret = NSRect(x: 200, y: 500, width: 4, height: 18)
        window.show(
            candidates: [CandidateItem(text: String(repeating: "あ", count: 24), description: "[全]ひらがな")],
            cursor: 0,
            page: 0,
            totalPages: 1,
            cursorRect: caret
        )
        XCTAssertGreaterThan(window.testingFrame.width, 200)
        window.hide()
    }

    func testShortListNearTheScreenBottomOpensAbove() throws {
        try skipWithoutScreen()
        let screen = try XCTUnwrap(NSScreen.main)
        let window = CandidateWindowController()
        let caret = NSRect(x: screen.visibleFrame.midX, y: screen.visibleFrame.minY + 30, width: 4, height: 18)
        window.show(
            candidates: [CandidateItem(text: "あ", description: nil)],
            cursor: 0,
            page: 0,
            totalPages: 1,
            cursorRect: caret
        )
        XCTAssertEqual(window.testingFrame.origin.y, caret.maxY, accuracy: 0.5)
        window.hide()
    }

    private func skipWithoutScreen() throws {
        try XCTSkipIf(NSScreen.main == nil, "no screen to place the panel on")
    }
}
