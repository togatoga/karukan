import Cocoa

/// Which side of the caret the candidate panel sits on.
///
/// Chosen once per appearance from the full panel height, then kept until
/// the panel hides. Re-deciding from however many candidates are filled in
/// flips the panel above and below the caret as the list grows and shrinks.
enum CandidatePanelSide: Equatable {
    case above
    case below

    /// Below the caret unless a panel of `panelHeight` would extend past the
    /// bottom of `visibleFrame`.
    static func choose(caret: NSRect, panelHeight: CGFloat, visibleFrame: NSRect) -> CandidatePanelSide {
        if caret.origin.y - panelHeight < visibleFrame.origin.y {
            return .above
        }
        return .below
    }

    /// Panel origin, in screen coordinates (y grows upward). Below hangs the
    /// panel from the caret's bottom edge; above sits it on the caret's top.
    func originY(caret: NSRect, panelHeight: CGFloat) -> CGFloat {
        switch self {
        case .above:
            return caret.origin.y + caret.height
        case .below:
            return caret.origin.y - panelHeight
        }
    }
}

/// Shift a panel frame so it stays inside the screen.
///
/// Width still follows the longest candidate. Near the right edge that growth
/// moves the left edge leftward, the way the system Japanese IME does, instead
/// of letting the panel run off the screen. The same for the top and bottom.
enum CandidatePanelFrame {
    static func fitted(_ frame: NSRect, in visibleFrame: NSRect) -> NSRect {
        var frame = frame
        if frame.maxX > visibleFrame.maxX {
            frame.origin.x -= frame.maxX - visibleFrame.maxX
        }
        if frame.minX < visibleFrame.minX {
            frame.origin.x = visibleFrame.minX
        }
        if frame.minY < visibleFrame.minY {
            frame.origin.y = visibleFrame.minY
        }
        if frame.maxY > visibleFrame.maxY {
            frame.origin.y -= frame.maxY - visibleFrame.maxY
        }
        return frame
    }
}

/// Custom candidate window (borderless non-activating NSPanel).
///
/// The engine pre-paginates: `show` receives only the visible page plus
/// page metadata, so this controller just renders rows. An optional aux
/// line (reading hint / model info from the engine) is shown as a footer.
///
/// The panel always lays out `rowCapacity` candidate rows — the engine's
/// page size — plus a page-indicator row and an aux row, whether or not
/// those footers have text. Height therefore does not follow the candidate
/// count or whether `[1/4]` is showing. The above/below choice uses that
/// full height and stays put until the panel hides.
class CandidateWindowController {
    // Visual scale of the panel. Candidate rows use a larger type size
    // than the footers (page indicator / aux line), matching the system
    // Japanese IME's proportions.
    private static let candidateFontSize: CGFloat = 18
    private static let footerFontSize: CGFloat = 13
    private static let minPanelWidth: CGFloat = 160

    /// Rows reserved on every appearance. Matches
    /// `CandidateList::DEFAULT_PAGE_SIZE` in karukan-im: the engine never
    /// sends more than this many candidates in one `show`.
    static let rowCapacity = 9

    private let panel: NSPanel
    private let stackView: NSStackView
    private var rowViews: [NSView] = []
    private var auxText: String?
    /// Side chosen for the current appearance. Nil while the panel is hidden.
    private var side: CandidatePanelSide?

    private struct PageState {
        let candidates: [CandidateItem]
        let cursor: Int
        let page: Int
        let totalPages: Int
    }
    private var pageState: PageState?

    /// Height of one candidate row, measured from the font the row uses so
    /// an empty slot occupies the same space as a filled one.
    private static let candidateRowHeight: CGFloat = {
        let label = NSTextField(labelWithString: "あ")
        label.font = NSFont.systemFont(ofSize: candidateFontSize)
        return ceil(label.intrinsicContentSize.height)
    }()

    /// Height of the page-indicator and aux rows. Both are always reserved,
    /// so `[1/4]` appearing does not make the panel taller.
    private static let footerRowHeight: CGFloat = {
        let label = NSTextField(labelWithString: "[1/4]")
        label.font = NSFont.systemFont(ofSize: footerFontSize)
        return ceil(label.intrinsicContentSize.height)
    }()

    init() {
        panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 100),
            styleMask: [.nonactivatingPanel, .borderless],
            backing: .buffered,
            defer: true
        )
        panel.level = .popUpMenu
        panel.hidesOnDeactivate = false
        panel.isOpaque = false
        panel.backgroundColor = NSColor.windowBackgroundColor
        panel.ignoresMouseEvents = true

        stackView = NSStackView()
        stackView.orientation = .vertical
        stackView.alignment = .leading
        stackView.spacing = 4
        stackView.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        stackView.translatesAutoresizingMaskIntoConstraints = false

        // Top and leading only. Pinning every edge makes the stack take the
        // panel's current size, so fittingSize comes back short and the
        // footer draws outside the panel (or gets clipped at the screen edge).
        panel.contentView?.addSubview(stackView)
        if let contentView = panel.contentView {
            NSLayoutConstraint.activate([
                stackView.topAnchor.constraint(equalTo: contentView.topAnchor),
                stackView.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            ])
        }
    }

    var isVisible: Bool { panel.isVisible }

    /// Frame after the latest layout. Tests use this to check that the
    /// panel does not move when the candidate count changes.
    var testingFrame: NSRect { panel.frame }

    /// `cursorRect: nil` reuses the rect from the previous `show` — the
    /// caller can skip its (synchronous, per-keystroke) client IPC while
    /// the panel is already on screen, since the composition anchor
    /// doesn't move mid-composition.
    func show(
        candidates: [CandidateItem], cursor: Int, page: Int, totalPages: Int, cursorRect: NSRect?
    ) {
        pageState = PageState(
            candidates: candidates, cursor: cursor, page: page, totalPages: totalPages)
        render(cursorRect: cursorRect)
    }

    /// Update the aux footer; re-renders in place if the window is visible.
    /// Pass `deferRender: true` when a `show`/`hide` follows in the same
    /// action batch, so the panel is rendered once per batch instead of
    /// once for the aux change and again for the candidates.
    func setAux(_ text: String?, deferRender: Bool = false) {
        auxText = text
        if !deferRender, panel.isVisible, pageState != nil {
            render(cursorRect: nil)
        }
    }

    func hide() {
        pageState = nil
        side = nil
        panel.orderOut(nil)
    }

    private func render(cursorRect: NSRect?) {
        clearRows()
        guard let state = pageState else {
            hide()
            return
        }
        // An empty list still renders while the aux footer has text: a
        // source-filtered view narrowed to an empty source must show its
        // 「候補なし」 footer, not a silently vanishing panel.
        if state.candidates.isEmpty && (auxText ?? "").isEmpty {
            hide()
            return
        }

        let slots = max(state.candidates.count, Self.rowCapacity)
        for index in 0..<slots {
            if index < state.candidates.count {
                addCandidateRow(
                    state.candidates[index], number: index + 1, selected: index == state.cursor)
            } else {
                addBlankRow()
            }
        }
        let pageText = state.totalPages > 1 ? "[\(state.page + 1)/\(state.totalPages)]" : ""
        addFooterLabel(pageText)
        addFooterLabel(auxText ?? "")

        positionPanel(cursorRect: cursorRect)
    }

    private func clearRows() {
        for view in rowViews {
            stackView.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        rowViews.removeAll()
    }

    private func addCandidateRow(_ candidate: CandidateItem, number: Int, selected: Bool) {
        let text = NSMutableAttributedString(
            string: "\(number). \(candidate.text)",
            attributes: [
                .font: NSFont.systemFont(ofSize: Self.candidateFontSize),
                .foregroundColor: selected ? NSColor.white : NSColor.labelColor,
            ]
        )
        if let description = candidate.description {
            text.append(
                NSAttributedString(
                    string: "  \(description)",
                    attributes: [
                        .font: NSFont.systemFont(ofSize: Self.footerFontSize),
                        .foregroundColor: selected
                            ? NSColor.white.withAlphaComponent(0.8)
                            : NSColor.secondaryLabelColor,
                    ]
                ))
        }

        let label = NSTextField(labelWithAttributedString: text)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.maximumNumberOfLines = 1
        label.lineBreakMode = .byTruncatingTail
        if selected {
            label.backgroundColor = NSColor.selectedContentBackgroundColor
            label.drawsBackground = true
        } else {
            label.backgroundColor = .clear
            label.drawsBackground = false
        }

        let row = fixedHeightRow()
        row.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            label.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            label.centerYAnchor.constraint(equalTo: row.centerYAnchor),
        ])
        stackView.addArrangedSubview(row)
        rowViews.append(row)
    }

    private func addBlankRow() {
        let row = fixedHeightRow()
        stackView.addArrangedSubview(row)
        rowViews.append(row)
    }

    /// A row that occupies `candidateRowHeight` whether or not it has text,
    /// so a short page and a full page produce the same panel height.
    private func fixedHeightRow() -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.heightAnchor.constraint(equalToConstant: Self.candidateRowHeight).isActive = true
        return row
    }

    private func addFooterLabel(_ text: String) {
        let label = NSTextField(labelWithString: text)
        label.font = NSFont.systemFont(ofSize: Self.footerFontSize)
        label.textColor = NSColor.secondaryLabelColor
        label.maximumNumberOfLines = 1
        label.lineBreakMode = .byTruncatingTail
        label.translatesAutoresizingMaskIntoConstraints = false
        label.setContentCompressionResistancePriority(.required, for: .vertical)
        label.heightAnchor.constraint(equalToConstant: Self.footerRowHeight).isActive = true
        stackView.addArrangedSubview(label)
        rowViews.append(label)
    }

    private var lastCursorRect: NSRect = .zero

    private func positionPanel(cursorRect: NSRect?) {
        if let rect = cursorRect {
            lastCursorRect = rect
        }
        let caret = lastCursorRect

        stackView.layoutSubtreeIfNeeded()
        let contentSize = stackView.fittingSize
        let panelWidth = max(contentSize.width, Self.minPanelWidth)
        let panelHeight = contentSize.height

        guard caret != .zero else {
            panel.setFrame(
                NSRect(x: 100, y: 100, width: panelWidth, height: panelHeight), display: true)
            panel.orderFront(nil)
            return
        }

        let screen = Self.screen(containing: caret)
        if side == nil {
            if let screen {
                side = CandidatePanelSide.choose(
                    caret: caret, panelHeight: panelHeight, visibleFrame: screen.visibleFrame)
            } else {
                side = .below
            }
        }
        let originY = (side ?? .below).originY(caret: caret, panelHeight: panelHeight)
        var frame = NSRect(x: caret.origin.x, y: originY, width: panelWidth, height: panelHeight)
        if let screen {
            frame = CandidatePanelFrame.fitted(frame, in: screen.visibleFrame)
        }

        panel.setFrame(frame, display: true)
        panel.orderFront(nil)
    }

    /// The screen the caret is on. `NSScreen.main` is the screen with the
    /// keyboard focus, which is not necessarily the one under the caret.
    private static func screen(containing caret: NSRect) -> NSScreen? {
        NSScreen.screens.first { $0.frame.intersects(caret) } ?? NSScreen.main
    }
}
