// Rasterise un SVG en PNG via NSImage (rendu système fidèle).
// Usage : swift render_svg.swift <entrée.svg> <largeur> <sortie.png> [hauteur]
// La hauteur vaut la largeur si omise (icône carrée).
import AppKit

let args = CommandLine.arguments
guard args.count >= 4, let w = Double(args[2]) else {
    FileHandle.standardError.write("usage: render_svg.swift <svg> <width> <out.png> [height]\n".data(using: .utf8)!)
    exit(64)
}
let h = args.count >= 5 ? (Double(args[4]) ?? w) : w

guard let image = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write("impossible de charger le SVG\n".data(using: .utf8)!)
    exit(1)
}

let pw = Int(w)
let ph = Int(h)
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: pw, pixelsHigh: ph,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
) else { exit(2) }
rep.size = NSSize(width: w, height: h)

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
image.draw(in: NSRect(x: 0, y: 0, width: w, height: h),
           from: .zero, operation: .copy, fraction: 1.0)
NSGraphicsContext.restoreGraphicsState()

guard let png = rep.representation(using: .png, properties: [:]) else { exit(4) }
try png.write(to: URL(fileURLWithPath: args[3]))
