import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const windows = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "windows");
mkdirSync(windows, { recursive: true });

const BG = [0x1c, 0x1c, 0x1c];
const CARD = [0x2a, 0x2a, 0x2a];
const FG = [0xf4, 0xf4, 0xf5];
const MUTED = [0x8a, 0x8a, 0x8e];
const LINE = [0x3a, 0x3a, 0x3a];

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) {
    c ^= b;
    for (let i = 0; i < 8; i++) c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const t = Buffer.from(type);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([t, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function canvas(w, h, fill = BG) {
  const px = new Uint8Array(w * h * 3);
  for (let i = 0; i < w * h; i++) {
    px[i * 3] = fill[0];
    px[i * 3 + 1] = fill[1];
    px[i * 3 + 2] = fill[2];
  }
  return { w, h, px };
}

function set(c, x, y, col) {
  if (x < 0 || y < 0 || x >= c.w || y >= c.h) return;
  const i = (y * c.w + x) * 3;
  c.px[i] = col[0];
  c.px[i + 1] = col[1];
  c.px[i + 2] = col[2];
}

function fillRect(c, x, y, w, h, col, r = 0) {
  const x0 = Math.round(x);
  const y0 = Math.round(y);
  const x1 = Math.round(x + w);
  const y1 = Math.round(y + h);
  for (let yy = y0; yy < y1; yy++) {
    for (let xx = x0; xx < x1; xx++) {
      if (r > 0) {
        const dx = xx < x0 + r ? x0 + r - xx : xx >= x1 - r ? xx - (x1 - 1 - r) : 0;
        const dy = yy < y0 + r ? y0 + r - yy : yy >= y1 - r ? yy - (y1 - 1 - r) : 0;
        if (dx * dx + dy * dy > r * r) continue;
      }
      set(c, xx, yy, col);
    }
  }
}

function mix(a, b, t) {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}

/** BrandMark: four rounded bars in a 24×24 viewBox. */
function drawLogo(c, ox, oy, size) {
  const s = size / 24;
  const bars = [
    { x: 3, y: 10, w: 3.2, h: 8, r: 1 },
    { x: 8.4, y: 6, w: 3.2, h: 12, r: 1 },
    { x: 13.8, y: 8, w: 3.2, h: 10, r: 1 },
    { x: 19.2, y: 4, w: 1.8, h: 14, r: 0.8 },
  ];
  for (const bar of bars) {
    fillRect(c, ox + bar.x * s, oy + bar.y * s, bar.w * s, bar.h * s, FG, Math.max(1, bar.r * s));
  }
}

const FONT = {
  A: ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
  B: ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
  C: ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
  D: ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
  E: ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
  F: ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
  G: ["01110", "10001", "10000", "10111", "10001", "10001", "01110"],
  H: ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
  I: ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
  K: ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
  L: ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
  M: ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
  N: ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
  O: ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
  P: ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
  R: ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
  S: ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
  T: ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
  U: ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
  V: ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
  W: ["10001", "10001", "10001", "10101", "10101", "11011", "10001"],
  Y: ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
  0: ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
  1: ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
  2: ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
  3: ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
  4: ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
  5: ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
  6: ["01110", "10000", "11110", "10001", "10001", "10001", "01110"],
  7: ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
  8: ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
  9: ["01110", "10001", "10001", "01111", "00001", "00001", "01110"],
  " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
  ".": ["00000", "00000", "00000", "00000", "00000", "00100", "00100"],
  "-": ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
  "/": ["00001", "00010", "00100", "00100", "01000", "10000", "10000"],
};

function drawText(c, text, x, y, scale, col) {
  let cx = x;
  for (const ch of text.toUpperCase()) {
    const glyph = FONT[ch] ?? FONT[" "];
    for (let gy = 0; gy < 7; gy++) {
      for (let gx = 0; gx < 5; gx++) {
        if (glyph[gy][gx] === "1") {
          fillRect(c, cx + gx * scale, y + gy * scale, scale, scale, col);
        }
      }
    }
    cx += 6 * scale;
  }
  return cx;
}

function textWidth(text, scale) {
  return text.length * 6 * scale - scale;
}

function dots(c, step = 8, col = [0x32, 0x32, 0x32]) {
  for (let y = 3; y < c.h; y += step) {
    for (let x = 3; x < c.w; x += step) set(c, x, y, col);
  }
}

function writeBmp(c, path) {
  const rowBytes = Math.ceil((c.w * 3) / 4) * 4;
  const pixelBytes = rowBytes * c.h;
  const header = Buffer.alloc(54);
  header.write("BM", 0);
  header.writeUInt32LE(54 + pixelBytes, 2);
  header.writeUInt32LE(54, 10);
  header.writeUInt32LE(40, 14);
  header.writeInt32LE(c.w, 18);
  header.writeInt32LE(c.h, 22);
  header.writeUInt16LE(1, 26);
  header.writeUInt16LE(24, 28);
  header.writeUInt32LE(pixelBytes, 34);
  const pixels = Buffer.alloc(pixelBytes);
  for (let y = 0; y < c.h; y++) {
    const srcY = c.h - 1 - y;
    for (let x = 0; x < c.w; x++) {
      const si = (srcY * c.w + x) * 3;
      const di = y * rowBytes + x * 3;
      pixels[di] = c.px[si + 2];
      pixels[di + 1] = c.px[si + 1];
      pixels[di + 2] = c.px[si];
    }
  }
  writeFileSync(path, Buffer.concat([header, pixels]));
}

function writePng(c, path) {
  const raw = Buffer.alloc((c.w * 3 + 1) * c.h);
  for (let y = 0; y < c.h; y++) {
    raw[y * (c.w * 3 + 1)] = 0;
    for (let x = 0; x < c.w; x++) {
      const si = (y * c.w + x) * 3;
      const di = y * (c.w * 3 + 1) + 1 + x * 3;
      raw[di] = c.px[si];
      raw[di + 1] = c.px[si + 1];
      raw[di + 2] = c.px[si + 2];
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(c.w, 0);
  ihdr.writeUInt32BE(c.h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  writeFileSync(
    path,
    Buffer.concat([
      Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
      pngChunk("IHDR", ihdr),
      pngChunk("IDAT", deflateSync(raw)),
      pngChunk("IEND", Buffer.alloc(0)),
    ]),
  );
}

function sidebar() {
  const c = canvas(164, 314);
  for (let y = 0; y < c.h; y++) {
    const col = mix(BG, [0x12, 0x12, 0x12], y / c.h);
    for (let x = 0; x < c.w; x++) set(c, x, y, col);
  }
  dots(c, 10, [0x2c, 0x2c, 0x2c]);
  fillRect(c, 0, 0, 3, c.h, FG);
  const logo = 72;
  drawLogo(c, (c.w - logo) / 2, 48, logo);
  const title = "CUEBOARD";
  const ts = 2;
  drawText(c, title, Math.round((c.w - textWidth(title, ts)) / 2), 140, ts, FG);
  fillRect(c, 48, 168, c.w - 96, 1, LINE);
  const sub = "WINDOWS";
  drawText(c, sub, Math.round((c.w - textWidth(sub, 1)) / 2), 186, 1, MUTED);
  const sub2 = "SOUNDBOARD";
  drawText(c, sub2, Math.round((c.w - textWidth(sub2, 1)) / 2), 200, 1, MUTED);
  drawText(c, "V1.0", Math.round((c.w - textWidth("V1.0", 1)) / 2), 280, 1, MUTED);
  return c;
}

function header() {
  const c = canvas(150, 57);
  fillRect(c, 0, 0, c.w, c.h, BG);
  dots(c, 7, [0x2a, 0x2a, 0x2a]);
  fillRect(c, 0, 0, 3, c.h, FG);
  drawLogo(c, 14, 8, 40);
  drawText(c, "CUEBOARD", 62, 22, 1, FG);
  return c;
}

writeBmp(sidebar(), join(windows, "nsis-sidebar.bmp"));
writeBmp(header(), join(windows, "nsis-header.bmp"));
writePng(sidebar(), join(windows, "nsis-sidebar.png"));
console.log("installer art written to", windows);
