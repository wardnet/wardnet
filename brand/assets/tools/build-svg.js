#!/usr/bin/env node
/**
 * outline-svg.js
 *
 * Reads every SVG in brand/assets/src/, replaces <text> elements with
 * outlined <path> elements (font-independent), and writes the results to
 * brand/assets/dist/ — preserving all other markup exactly as-is.
 *
 * Usage (from brand/assets/tools/):
 *   yarn install          # one-time
 *   node outline-svg.js
 *
 * On first run the two font TTFs are auto-downloaded into tools/fonts/.
 */

import fs    from 'node:fs';
import path  from 'node:path';
import https from 'node:https';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

let opentype;
try {
  const mod = await import('opentype.js');
  opentype = mod.default ?? mod;
} catch { console.error('\n  Missing dep — run: yarn install\n'); process.exit(1); }

let DOMParser, XMLSerializer;
try {
  ({ DOMParser, XMLSerializer } = await import('@xmldom/xmldom'));
} catch {
  console.error('\n  Missing dep — run: yarn install\n');
  process.exit(1);
}

const SRC_DIR  = path.resolve(__dirname, '..', 'src');
const DIST_DIR = path.resolve(__dirname, '..', 'dist');
const FONT_DIR = path.resolve(__dirname, '..', 'fonts');

// Maps "family-fragment|weight" → downloadable TTF spec.
const FONT_SPECS = {
  'archivo|800': {
    file: 'Archivo_800ExtraBold.ttf',
    url:  'https://cdn.jsdelivr.net/npm/@expo-google-fonts/archivo/Archivo_800ExtraBold.ttf',
  },
  'hanken grotesk|500': {
    file: 'HankenGrotesk_500Medium.ttf',
    url:  'https://cdn.jsdelivr.net/npm/@expo-google-fonts/hanken-grotesk/HankenGrotesk_500Medium.ttf',
  },
};

// ---------------------------------------------------------------------------
// Font loading
// ---------------------------------------------------------------------------

function fontKey(family, weight) {
  return `${family.split(',')[0].toLowerCase().trim()}|${weight || '400'}`;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const go = (u) => https.get(u, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        go(res.headers.location); return;
      }
      if (res.statusCode !== 200) { reject(new Error('HTTP ' + res.statusCode + ' – ' + u)); return; }
      const chunks = [];
      res.on('data', c => chunks.push(c));
      res.on('end', () => { fs.writeFileSync(dest, Buffer.concat(chunks)); resolve(); });
    }).on('error', reject);
    go(url);
  });
}

async function ensureFont(key) {
  const spec = FONT_SPECS[key];
  if (!spec) throw new Error(`No font registered for key "${key}". Add it to FONT_SPECS.`);
  const dest = path.join(FONT_DIR, spec.file);
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(FONT_DIR, { recursive: true });
    process.stdout.write(`  downloading ${spec.file} … `);
    await download(spec.url, dest);
    console.log('ok');
  }
  return opentype.loadSync(dest);
}

// ---------------------------------------------------------------------------
// Text layout helpers
// ---------------------------------------------------------------------------

// Lay out `text` glyph-by-glyph; returns {d, endX}.
// endX is the position after the last glyph (including trailing letterSpacing),
// i.e. where the next adjacent tspan should start.
function layoutRun(font, text, fontSize, x0, baseline, letterSpacing) {
  const scale  = fontSize / font.unitsPerEm;
  const glyphs = font.stringToGlyphs(text);
  let x = x0;
  const parts = [];
  for (let i = 0; i < glyphs.length; i++) {
    const g = glyphs[i];
    if (i > 0) x += (font.getKerningValue(glyphs[i - 1], g) || 0) * scale;
    parts.push(g.getPath(x, baseline, fontSize).toPathData(2));
    x += g.advanceWidth * scale + letterSpacing;
  }
  return { d: parts.join(' '), endX: x };
}

// Natural rendered width (without trailing letterSpacing).
function naturalWidth(font, text, fontSize, letterSpacing) {
  const scale  = fontSize / font.unitsPerEm;
  const glyphs = font.stringToGlyphs(text);
  let w = 0;
  for (let i = 0; i < glyphs.length; i++) {
    if (i > 0) w += (font.getKerningValue(glyphs[i - 1], glyphs[i]) || 0) * scale;
    w += glyphs[i].advanceWidth * scale;
  }
  return w + (glyphs.length - 1) * letterSpacing;
}

// Find letter-spacing that makes `text` span exactly `targetWidth` at `fontSize`.
function solveSpacing(font, text, fontSize, targetWidth) {
  const n = font.stringToGlyphs(text).length;
  if (n <= 1) return 0;
  const nw = naturalWidth(font, text, fontSize, 0);
  let ls = (targetWidth - nw) / (n - 1);
  // one correction pass
  const got = naturalWidth(font, text, fontSize, ls);
  ls += (targetWidth - got) / (n - 1);
  return ls;
}

// Extract a numeric letter-spacing from an attribute value or inline style string.
function parseLetterSpacing(attrVal, styleStr) {
  if (attrVal != null && attrVal !== '') {
    const v = parseFloat(attrVal);
    if (!isNaN(v)) return v;
  }
  if (styleStr) {
    const m = styleStr.match(/letter-spacing\s*:\s*(-?[\d.]+)/);
    if (m) return parseFloat(m[1]);
  }
  return 0;
}

// ---------------------------------------------------------------------------
// SVG DOM helpers
// ---------------------------------------------------------------------------

function collectTextNodes(node, out = []) {
  if (node.nodeName === 'text') { out.push(node); return out; }
  const kids = node.childNodes || [];
  for (let i = 0; i < kids.length; i++) collectTextNodes(kids[i], out);
  return out;
}

// ---------------------------------------------------------------------------
// Replace one <text> element with <path> elements in the live DOM.
// ---------------------------------------------------------------------------

async function outlineTextElement(doc, textEl, fontCache) {
  const family   = textEl.getAttribute('font-family') || 'Archivo';
  const weight   = textEl.getAttribute('font-weight')  || '400';
  const fontSize = parseFloat(textEl.getAttribute('font-size') || '16');
  const x0       = parseFloat(textEl.getAttribute('x') || '0');
  const baseline = parseFloat(textEl.getAttribute('y') || '0');
  const fillAttr = textEl.getAttribute('fill') || '#000000';
  const textLengthAttr = textEl.getAttribute('textLength');
  const lengthAdjust   = textEl.getAttribute('lengthAdjust');
  const parentLS = parseLetterSpacing(
    textEl.getAttribute('letter-spacing'),
    textEl.getAttribute('style'),
  );

  const key = fontKey(family, weight);
  if (!fontCache[key]) fontCache[key] = await ensureFont(key);
  const font = fontCache[key];

  // Collect tspan children.
  const tspans = [];
  for (let i = 0; i < textEl.childNodes.length; i++) {
    if (textEl.childNodes[i].nodeName === 'tspan') tspans.push(textEl.childNodes[i]);
  }

  const SVG_NS = 'http://www.w3.org/2000/svg';
  const pathEls = [];

  if (tspans.length > 0) {
    // Multi-color wordmark: each tspan is a run continuing from the previous endX.
    let currentX = x0;
    for (const tspan of tspans) {
      const text = tspan.textContent;
      if (!text) continue;
      const fill = tspan.getAttribute('fill') || fillAttr;
      // Letter-spacing from tspan inline style, falling back to parent text element.
      const ls = parseLetterSpacing(null, tspan.getAttribute('style')) || parentLS;
      const { d, endX } = layoutRun(font, text, fontSize, currentX, baseline, ls);
      currentX = endX;
      const el = doc.createElementNS(SVG_NS, 'path');
      el.setAttribute('fill', fill);
      el.setAttribute('d', d);
      pathEls.push(el);
    }
  } else {
    // Plain text, possibly with textLength / lengthAdjust="spacing".
    const text = textEl.textContent;
    if (!text.trim()) return;
    let ls = parentLS;
    if (textLengthAttr && lengthAdjust === 'spacing') {
      ls = solveSpacing(font, text, fontSize, parseFloat(textLengthAttr));
    }
    const { d } = layoutRun(font, text, fontSize, x0, baseline, ls);
    const el = doc.createElementNS(SVG_NS, 'path');
    el.setAttribute('fill', fillAttr);
    el.setAttribute('d', d);
    pathEls.push(el);
  }

  // Replace the <text> node with the generated <path> elements.
  const parent = textEl.parentNode;
  for (const el of pathEls) parent.insertBefore(el, textEl);
  parent.removeChild(textEl);
}

// ---------------------------------------------------------------------------
// Process one file.
// ---------------------------------------------------------------------------

async function processFile(srcPath, fontCache) {
  const xml = fs.readFileSync(srcPath, 'utf8');
  const doc  = new DOMParser().parseFromString(xml, 'image/svg+xml');
  const textEls = collectTextNodes(doc.documentElement);

  if (textEls.length === 0) return xml; // no text — pass through unchanged

  // Process in reverse order so earlier indices aren't invalidated by DOM mutation.
  for (let i = textEls.length - 1; i >= 0; i--) {
    await outlineTextElement(doc, textEls[i], fontCache);
  }

  return new XMLSerializer().serializeToString(doc);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const fontCache = {};
const files = fs.readdirSync(SRC_DIR).filter(f => f.endsWith('.svg')).sort();
fs.mkdirSync(DIST_DIR, { recursive: true });

console.log('\nWardnet · outlining text → vector paths\n');

try {
  for (const file of files) {
    const result = await processFile(path.join(SRC_DIR, file), fontCache);
    fs.writeFileSync(path.join(DIST_DIR, file), result, 'utf8');
    console.log(`  ✓ ${file}`);
  }
  console.log(`\nDone. ${files.length} file(s) written to dist/\n`);
} catch (e) {
  console.error('\n  ERROR: ' + (e?.message || e) + '\n');
  process.exit(1);
}
