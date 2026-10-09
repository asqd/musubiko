// Draws the Musubiko app icon: two branches tied by an orange cord on an ink tile.
// The shapes match the in-app logo (components/Icons.tsx), drawn on a 24-unit grid.
// Usage: swift design/make-icon.swift <output.png>   (1024 × 1024, macOS icon grid)
import AppKit

let size: CGFloat = 1024
let output = CommandLine.arguments.dropFirst().first ?? "icon_1024.png"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(
        srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
        green: CGFloat((hex >> 8) & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255,
        alpha: alpha
    )
}

let space = CGColorSpace(name: CGColorSpace.sRGB)!
let context = CGContext(
    data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
    space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!

// Tile: 824 × 824 centred, as in Apple's macOS icon template, with a soft shadow.
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)
context.saveGState()
context.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: color(0x000000, 0.35))
context.addPath(tilePath)
context.setFillColor(color(0x17202C))
context.fillPath()
context.restoreGState()

context.saveGState()
context.addPath(tilePath)
context.clip()
let gradient = CGGradient(colorsSpace: space, colors: [color(0x34425A), color(0x141B26)] as CFArray, locations: [0, 1])!
context.drawLinearGradient(gradient, start: CGPoint(x: 0, y: tile.maxY), end: CGPoint(x: 0, y: tile.minY), options: [])
context.restoreGState()

// Logo grid: 24 units, y down, mapped onto a 600-px square in the middle of the tile.
let unit: CGFloat = 600 / 24
let origin = CGPoint(x: 212, y: 812)
func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: origin.x + x * unit, y: origin.y - y * unit) }

func stroke(_ hex: UInt32, _ build: (CGMutablePath) -> Void) {
    let path = CGMutablePath()
    build(path)
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -6), blur: 12, color: color(0x000000, 0.3))
    context.addPath(path)
    context.setStrokeColor(color(hex))
    context.setLineWidth(2.2 * unit)
    context.setLineCap(.round)
    context.setLineJoin(.round)
    context.strokePath()
    context.restoreGState()
}

// Trunk and the branch leaving it.
stroke(0xF3F5F8) { path in
    path.move(to: p(7, 19))
    path.addLine(to: p(7, 5))
}
stroke(0xF3F5F8) { path in
    path.move(to: p(7, 9))
    path.addCurve(to: p(17, 16), control1: p(7, 13), control2: p(17, 11))
    path.addLine(to: p(17, 19))
}
// The cord that ties them.
stroke(0xFF7A3D) { path in
    path.move(to: p(17, 5))
    path.addLine(to: p(17, 7.5))
    path.addCurve(to: p(7, 13.5), control1: p(17, 10.5), control2: p(7, 9.5))
}

let image = context.makeImage()!
let rep = NSBitmapImageRep(cgImage: image)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
print("wrote \(output)")
