// Rasterise un SVG en PNG carré via NSImage (rendu système fidèle).
// Usage : swift render_svg.swift <entrée.svg> <taille> <sortie.png>
import AppKit

let args = CommandLine.arguments
guard args.count == 4, let size = Double(args[2]) else {
    FileHandle.standardError.write("usage: render_svg.swift <svg> <size> <out.png>\n".data(using: .utf8)!)
    exit(64)
}

guard let image = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write("impossible de charger le SVG\n".data(using: .utf8)!)
    exit(1)
}

let px = Int(size)
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
) else { exit(2) }
rep.size = NSSize(width: size, height: size)

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size),
           from: .zero, operation: .copy, fraction: 1.0)
NSGraphicsContext.restoreGraphicsState()

guard let png = rep.representation(using: .png, properties: [:]) else { exit(4) }
try png.write(to: URL(fileURLWithPath: args[3]))
