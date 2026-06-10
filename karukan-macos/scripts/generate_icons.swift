#!/usr/bin/env swift
// Generates the input-mode menu icons (resources/*.tiff).
//
// The icons are template images — a black glyph on transparency — the
// macOS convention for text-style input source icons (あ, A, 拼, …).
// Combined with TISIconIsTemplate=true in Info.plist, the system re-tints
// the glyph for the light/dark menu bar and the selected state, which is
// what keeps it legible everywhere (the previous white-glyph TIFF was
// invisible on a light menu bar).
//
// Usage: swift scripts/generate_icons.swift [output-dir]   (default: resources)

import AppKit

func renderRep(glyph: String, points: CGFloat, scale: CGFloat) -> NSBitmapImageRep {
    let pixels = Int(points * scale)
    guard
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    else {
        fatalError("failed to create bitmap rep")
    }
    // Same point size at different pixel densities = a 1x/2x multi-rep TIFF.
    rep.size = NSSize(width: points, height: points)

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    defer { NSGraphicsContext.restoreGraphicsState() }

    let attributed = NSAttributedString(
        string: glyph,
        attributes: [
            .font: NSFont.systemFont(ofSize: points * 0.85, weight: .bold),
            .foregroundColor: NSColor.black,
        ]
    )
    let size = attributed.size()
    attributed.draw(
        at: NSPoint(x: (points - size.width) / 2, y: (points - size.height) / 2))
    return rep
}

func writeIcon(glyph: String, to url: URL) {
    let reps = [
        renderRep(glyph: glyph, points: 16, scale: 1),
        renderRep(glyph: glyph, points: 16, scale: 2),
    ]
    guard let data = NSBitmapImageRep.representationOfImageReps(in: reps, using: .tiff, properties: [:])
    else {
        fatalError("failed to encode TIFF for \(glyph)")
    }
    do {
        try data.write(to: url)
        print("wrote \(url.path)")
    } catch {
        fatalError("failed to write \(url.path): \(error)")
    }
}

let outputDir = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "resources")
writeIcon(glyph: "か", to: outputDir.appendingPathComponent("karukan.tiff"))
writeIcon(glyph: "A", to: outputDir.appendingPathComponent("karukan-roman.tiff"))
