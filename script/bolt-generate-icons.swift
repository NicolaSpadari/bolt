// BOLT: renders the app icon PNGs from crates/zed/resources/bolt-icon.svg (Lucide, ISC).
// Black macOS-grid tile (824 of 1024, radius 185) with the glyph stroked in the theme's editor foreground (#acb2be).
// Usage: swift script/bolt-generate-icons.swift
import AppKit

let resources = URL(fileURLWithPath: "crates/zed/resources")
let glyphColor = "#acb2be"
var svg = try String(contentsOf: resources.appendingPathComponent("bolt-icon.svg"), encoding: .utf8)
svg = svg.replacingOccurrences(of: "currentColor", with: glyphColor)
guard let glyph = NSImage(data: svg.data(using: .utf8)!) else { fatalError("cannot load SVG") }

func render(size: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let scale = CGFloat(size) / 1024
    let tile = NSRect(x: 100 * scale, y: 100 * scale, width: 824 * scale, height: 824 * scale)
    NSColor.black.setFill()
    NSBezierPath(roundedRect: tile, xRadius: 185 * scale, yRadius: 185 * scale).fill()
    let glyphSide = 560 * scale
    let glyphRect = NSRect(x: (CGFloat(size) - glyphSide) / 2, y: (CGFloat(size) - glyphSide) / 2,
                           width: glyphSide, height: glyphSide)
    glyph.draw(in: glyphRect)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for prefix in ["app-icon", "app-icon-dev", "app-icon-preview", "app-icon-nightly"] {
    try render(size: 512).write(to: resources.appendingPathComponent("\(prefix).png"))
    try render(size: 1024).write(to: resources.appendingPathComponent("\(prefix)@2x.png"))
}
print("icons written")
