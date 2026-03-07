import Cocoa

/// Custom candidate window displayed near the cursor during conversion.
class KarukanCandidateWindow {
    private var panel: NSPanel?
    private var contentView: NSView?

    init() {
        // Create a borderless, non-activating panel
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 100),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: true
        )
        panel.level = .popUpMenu
        panel.isOpaque = false
        panel.backgroundColor = NSColor.clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        self.panel = panel
    }

    /// Show the candidate window with the given candidates near the specified rect.
    func show(candidates: [(String, String)], cursor: Int, nearRect: NSRect) {
        guard let panel = panel, !candidates.isEmpty else {
            hide()
            return
        }

        let contentView = buildContentView(candidates: candidates, cursor: cursor)
        panel.contentView = contentView

        // Position the window below the cursor
        let origin = NSPoint(
            x: nearRect.origin.x,
            y: nearRect.origin.y - contentView.frame.height
        )
        panel.setFrameOrigin(origin)
        panel.setContentSize(contentView.frame.size)
        panel.orderFront(nil)
    }

    /// Hide the candidate window.
    func hide() {
        panel?.orderOut(nil)
    }

    // MARK: - Content Building

    private func buildContentView(candidates: [(String, String)], cursor: Int) -> NSView {
        let itemHeight: CGFloat = 24
        let padding: CGFloat = 4
        let width: CGFloat = 280
        let totalHeight = CGFloat(candidates.count) * itemHeight + padding * 2

        let container = NSView(frame: NSRect(x: 0, y: 0, width: width, height: totalHeight))

        // Background
        let bg = NSVisualEffectView(frame: container.bounds)
        bg.material = .menu
        bg.state = .active
        bg.wantsLayer = true
        bg.layer?.cornerRadius = 6
        container.addSubview(bg)

        // Candidate items (numbered 1-9)
        for (i, (text, annotation)) in candidates.enumerated() {
            let y = totalHeight - padding - CGFloat(i + 1) * itemHeight
            let itemView = NSView(frame: NSRect(x: 0, y: y, width: width, height: itemHeight))

            // Highlight selected item
            if i == cursor {
                let highlight = NSView(frame: itemView.bounds)
                highlight.wantsLayer = true
                highlight.layer?.backgroundColor = NSColor.selectedContentBackgroundColor.cgColor
                highlight.layer?.cornerRadius = 3
                itemView.addSubview(highlight)
            }

            // Number label
            let numLabel = NSTextField(labelWithString: "\(i + 1).")
            numLabel.frame = NSRect(x: 8, y: 2, width: 20, height: 20)
            numLabel.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
            numLabel.textColor = i == cursor ? .white : .secondaryLabelColor
            itemView.addSubview(numLabel)

            // Candidate text
            let textLabel = NSTextField(labelWithString: text)
            textLabel.frame = NSRect(x: 32, y: 2, width: width - 100, height: 20)
            textLabel.font = NSFont.systemFont(ofSize: 14)
            textLabel.textColor = i == cursor ? .white : .labelColor
            itemView.addSubview(textLabel)

            // Annotation (if present)
            if !annotation.isEmpty {
                let annLabel = NSTextField(labelWithString: annotation)
                annLabel.frame = NSRect(x: width - 70, y: 2, width: 62, height: 20)
                annLabel.font = NSFont.systemFont(ofSize: 11)
                annLabel.textColor = i == cursor ? .white.withAlphaComponent(0.7) : .tertiaryLabelColor
                annLabel.alignment = .right
                itemView.addSubview(annLabel)
            }

            container.addSubview(itemView)
        }

        return container
    }
}
