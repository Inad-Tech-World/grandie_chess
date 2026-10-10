// Generates the Grandie Chess app icon from the brand logo.
//
// Crops the head + crown region of the portrait logo and draws it into a
// 1024x1024 macOS-style rounded square (824px shape on the standard icon grid)
// with transparent margins and a soft drop shadow.
//
// Usage: swift scripts/make-icon.swift <logo.jpg> <out.png> [cropX cropY cropSize]

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: make-icon.swift <logo> <out.png> [cropX cropY cropSize]\n".data(using: .utf8)!)
    exit(2)
}
let inputURL = URL(fileURLWithPath: args[1])
let outputURL = URL(fileURLWithPath: args[2])

guard let source = CGImageSourceCreateWithURL(inputURL as CFURL, nil),
      let logo = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    FileHandle.standardError.write("cannot read \(inputURL.path)\n".data(using: .utf8)!)
    exit(1)
}

// Default crop: from the top of the king's crown down to the bottom of the beard,
// centred horizontally. Coordinates are in image pixels with the origin top-left.
let side = args.count >= 6 ? Int(args[5])! : 800
let cropX = args.count >= 6 ? Int(args[3])! : (logo.width - side) / 2
let cropY = args.count >= 6 ? Int(args[4])! : 40
guard let cropped = logo.cropping(to: CGRect(x: cropX, y: cropY, width: side, height: side)) else {
    FileHandle.standardError.write("crop rectangle is outside the image\n".data(using: .utf8)!)
    exit(1)
}

let canvas = 1024
let shape: CGFloat = 824
let inset = (CGFloat(canvas) - shape) / 2
let radius: CGFloat = 185.4

let space = CGColorSpace(name: CGColorSpace.sRGB)!
guard let ctx = CGContext(
    data: nil, width: canvas, height: canvas, bitsPerComponent: 8, bytesPerRow: 0,
    space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { exit(1) }

ctx.interpolationQuality = .high
let rect = CGRect(x: inset, y: inset + 6, width: shape, height: shape)
let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)

// Drop shadow under the rounded square.
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 24,
              color: CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.45))
ctx.addPath(path)
ctx.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 1))
ctx.fillPath()
ctx.restoreGState()

// Artwork clipped to the rounded square.
ctx.saveGState()
ctx.addPath(path)
ctx.clip()
ctx.draw(cropped, in: rect)
ctx.restoreGState()

// Hairline edge so the black icon stays visible on a dark Dock.
ctx.addPath(path)
ctx.setStrokeColor(CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.10))
ctx.setLineWidth(3)
ctx.strokePath()

guard let image = ctx.makeImage(),
      let dest = CGImageDestinationCreateWithURL(outputURL as CFURL, UTType.png.identifier as CFString, 1, nil) else {
    exit(1)
}
CGImageDestinationAddImage(dest, image, nil)
guard CGImageDestinationFinalize(dest) else { exit(1) }
print("wrote \(outputURL.path) (\(canvas)x\(canvas), crop \(cropX),\(cropY) \(side)px)")
