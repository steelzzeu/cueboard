import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(root, { recursive: true });

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) {
    c ^= b;
    for (let i = 0; i < 8; i++) c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const t = Buffer.from(type);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([t, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function makePng(size) {
  const raw = Buffer.alloc((size + 1) * size);
  const bg = [0x22, 0x22, 0x22];
  const fg = [0xe8, 0xe8, 0xe8];
  for (let y = 0; y < size; y++) {
    raw[y * (size + 1)] = 0;
    for (let x = 0; x < size; x++) {
      const i = y * (size + 1) + 1 + x;
      let [r, g, b] = bg;
      const u = size / 16;
      const bars = [
        { x: 4 * u, w: 2 * u, h: 8 * u },
        { x: 7 * u, w: 2 * u, h: 11 * u },
        { x: 10 * u, w: 2 * u, h: 6 * u },
      ];
      for (const bar of bars) {
        const top = (size - bar.h) / 2;
        if (x >= bar.x && x < bar.x + bar.w && y >= top && y < top + bar.h) {
          [r, g, b] = fg;
        }
      }
      raw[i] = (r << 16) | (g << 8) | b;
    }
  }
  // Store true RGB bytes, not packed ints.
  const rgb = Buffer.alloc((size * 3 + 1) * size);
  for (let y = 0; y < size; y++) {
    rgb[y * (size * 3 + 1)] = 0;
    for (let x = 0; x < size; x++) {
      const u = size / 16;
      const bars = [
        { x: 4 * u, w: 2 * u, h: 8 * u },
        { x: 7 * u, w: 2 * u, h: 11 * u },
        { x: 10 * u, w: 2 * u, h: 6 * u },
      ];
      let [r, g, b] = bg;
      for (const bar of bars) {
        const top = (size - bar.h) / 2;
        if (x >= bar.x && x < bar.x + bar.w && y >= top && y < top + bar.h) {
          [r, g, b] = fg;
        }
      }
      const o = y * (size * 3 + 1) + 1 + x * 3;
      rgb[o] = r;
      rgb[o + 1] = g;
      rgb[o + 2] = b;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const png = Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(rgb)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
  return png;
}

function makeIco(pngs) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(pngs.length, 4);
  const entries = [];
  let offset = 6 + 16 * pngs.length;
  for (const { size, data } of pngs) {
    const entry = Buffer.alloc(16);
    entry[0] = size === 256 ? 0 : size;
    entry[1] = size === 256 ? 0 : size;
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(data.length, 8);
    entry.writeUInt32LE(offset, 12);
    entries.push(entry);
    offset += data.length;
  }
  return Buffer.concat([header, ...entries, ...pngs.map((p) => p.data)]);
}

const png32 = makePng(32);
const png128 = makePng(128);
const png256 = makePng(256);
writeFileSync(join(root, "32x32.png"), png32);
writeFileSync(join(root, "128x128.png"), png128);
writeFileSync(join(root, "icon.png"), png256);
writeFileSync(
  join(root, "icon.ico"),
  makeIco([
    { size: 32, data: png32 },
    { size: 128, data: png128 },
    { size: 256, data: png256 },
  ]),
);
console.log("icons written to", root);
