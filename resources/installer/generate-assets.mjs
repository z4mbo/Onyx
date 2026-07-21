/**
 * Generates installer assets:
 *   - icon.ico  from logo.png (multi-size, PNG-encoded for NSIS compatibility)
 *   - sidebar.bmp  (164x314) zAI navy/cyan gradient for welcome/finish pages
 *   - header.bmp   (150x57)  zAI cyan accent for directory/progress pages
 *
 * Run: node resources/installer/generate-assets.mjs
 */

import { writeFileSync } from 'fs'
import { dirname, join } from 'path'
import { fileURLToPath } from 'url'
import sharp from 'sharp'

const __dirname = dirname(fileURLToPath(import.meta.url))
const resourcesDir = join(__dirname, '..')

// zAI brand colors
const C = {
  white: { r: 255, g: 255, b: 255 },
  ice50: { r: 244, g: 250, b: 252 },
  cyan100: { r: 218, g: 247, b: 251 },
  cyan200: { r: 168, g: 241, b: 250 },
  cyan400: { r: 40, g: 215, b: 234 },
  cyan500: { r: 0, g: 184, b: 212 },
  cyan600: { r: 0, g: 142, b: 170 },
  navy700: { r: 18, g: 48, b: 74 },
  navy900: { r: 7, g: 28, b: 47 }
}

function lerp(a, b, t) {
  return Math.round(a + (b - a) * t)
}
function lerpColor(c1, c2, t) {
  return { r: lerp(c1.r, c2.r, t), g: lerp(c1.g, c2.g, t), b: lerp(c1.b, c2.b, t) }
}
function multiGradient(stops, t) {
  if (t <= stops[0].pos) return stops[0].color
  if (t >= stops[stops.length - 1].pos) return stops[stops.length - 1].color
  for (let i = 0; i < stops.length - 1; i++) {
    if (t >= stops[i].pos && t <= stops[i + 1].pos) {
      const lt = (t - stops[i].pos) / (stops[i + 1].pos - stops[i].pos)
      return lerpColor(stops[i].color, stops[i + 1].color, lt)
    }
  }
  return stops[stops.length - 1].color
}

// ── ICO generation from logo.png ──────────────────────────────────
async function generateICO() {
  const logoPath = join(resourcesDir, 'logo.png')
  const sizes = [16, 24, 32, 48, 64, 128, 256]

  // Resize logo to each size and get PNG buffers
  const pngBuffers = await Promise.all(
    sizes.map((s) => sharp(logoPath).resize(s, s).png().toBuffer())
  )

  // ICO file: 6-byte header + 16-byte directory entries + PNG data
  const headerSize = 6
  const dirSize = sizes.length * 16
  let dataOffset = headerSize + dirSize

  // ICO header
  const header = Buffer.alloc(6)
  header.writeUInt16LE(0, 0) // reserved
  header.writeUInt16LE(1, 2) // type = ICO
  header.writeUInt16LE(sizes.length, 4) // image count

  // Directory entries
  const entries = Buffer.alloc(dirSize)
  for (let i = 0; i < sizes.length; i++) {
    const off = i * 16
    entries[off] = sizes[i] < 256 ? sizes[i] : 0 // width (0 = 256)
    entries[off + 1] = sizes[i] < 256 ? sizes[i] : 0 // height (0 = 256)
    entries[off + 2] = 0 // color palette
    entries[off + 3] = 0 // reserved
    entries.writeUInt16LE(1, off + 4) // color planes
    entries.writeUInt16LE(32, off + 6) // bits per pixel
    entries.writeUInt32LE(pngBuffers[i].length, off + 8) // data size
    entries.writeUInt32LE(dataOffset, off + 12) // data offset
    dataOffset += pngBuffers[i].length
  }

  const ico = Buffer.concat([header, entries, ...pngBuffers])
  writeFileSync(join(resourcesDir, 'icon.ico'), ico)
  console.log('  icon.ico     (' + sizes.join(', ') + 'px)')
}

// ── BMP generation ────────────────────────────────────────────────
function createBMP(width, height, getPixel) {
  const rowSize = Math.ceil((width * 3) / 4) * 4
  const pixelDataSize = rowSize * height
  const fileSize = 54 + pixelDataSize
  const buf = Buffer.alloc(fileSize)

  buf[0] = 0x42; buf[1] = 0x4d
  buf.writeUInt32LE(fileSize, 2)
  buf.writeUInt32LE(0, 6)
  buf.writeUInt32LE(54, 10)

  buf.writeUInt32LE(40, 14)
  buf.writeInt32LE(width, 18)
  buf.writeInt32LE(height, 22)
  buf.writeUInt16LE(1, 26)
  buf.writeUInt16LE(24, 28)
  buf.writeUInt32LE(0, 30)
  buf.writeUInt32LE(pixelDataSize, 34)
  buf.writeInt32LE(2835, 38)
  buf.writeInt32LE(2835, 42)
  buf.writeUInt32LE(0, 46)
  buf.writeUInt32LE(0, 50)

  for (let row = 0; row < height; row++) {
    const srcY = height - 1 - row
    for (let x = 0; x < width; x++) {
      const { r, g, b } = getPixel(x, srcY)
      const off = 54 + row * rowSize + x * 3
      buf[off] = b
      buf[off + 1] = g
      buf[off + 2] = r
    }
  }
  return buf
}

function generateSidebar() {
  const W = 164, H = 314
  const stops = [
    { pos: 0.0, color: C.white },
    { pos: 0.14, color: C.ice50 },
    { pos: 0.32, color: C.cyan100 },
    { pos: 0.5, color: C.cyan200 },
    { pos: 0.68, color: C.cyan400 },
    { pos: 0.84, color: C.cyan600 },
    { pos: 1.0, color: C.navy900 }
  ]
  return createBMP(W, H, (x, y) => {
    const t = y / (H - 1)
    if (x < 5) return lerpColor(C.cyan500, C.navy700, t)
    return multiGradient(stops, t)
  })
}

function generateHeader() {
  const W = 150, H = 57
  return createBMP(W, H, (x, y) => {
    if (y >= H - 3) {
      const t = x / (W - 1)
      return lerpColor(C.cyan400, C.navy700, t)
    }
    const t = y / (H - 4)
    return lerpColor(C.white, C.ice50, t * 0.7)
  })
}

// ── Main ──────────────────────────────────────────────────────────
await generateICO()
writeFileSync(join(__dirname, 'sidebar.bmp'), generateSidebar())
writeFileSync(join(__dirname, 'header.bmp'), generateHeader())

console.log('  sidebar.bmp  (164 x 314)')
console.log('  header.bmp   (150 x 57)')
console.log('\nAll installer assets generated successfully.')
