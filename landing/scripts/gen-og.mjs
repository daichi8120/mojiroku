// OG 画像生成（1200×630、ja/en の2枚）。SVG を sharp で PNG にラスタライズ（ブラウザ非依存・決定論的）。
// 実行: node scripts/gen-og.mjs  → public/og.png（ja）+ public/og.en.png（en）
// フォントはシステム（Hiragino=日本語 / Helvetica=ラテン / Menlo=ワードマーク）。
import sharp from "sharp";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));

const W = 1200;
const H = 630;

const JP_FONT = "Hiragino Sans, 'Hiragino Kaku Gothic ProN', sans-serif";
const EN_FONT = "Helvetica Neue, Helvetica, Arial, sans-serif";

// 言語別の描画内容。headline は複数行対応（英語は1行に収まらないため2行）。
// pill の width / x は目視で調整した値（librsvg はテキスト幅で自動レイアウトしないため）。
const LOCALES = {
  ja: {
    out: "og.png",
    font: JP_FONT,
    headline: { lines: ["会議を、Mac の中で議事録に。"], size: 74, startY: 312, lineHeight: 88 },
    sub: { text: "録音 → 文字起こし → 要約。すべて端末内で完結する、基本無料の議事録アプリ。", size: 30, y: 382 },
    pills: [
      { text: "ローカル完結・基本無料", width: 310 },
      { text: "MCP で Claude から議事録を検索", width: 410 },
    ],
  },
  en: {
    out: "og.en.png",
    font: EN_FONT,
    headline: { lines: ["Turn meetings into notes,", "entirely on your Mac."], size: 68, startY: 258, lineHeight: 84 },
    sub: { text: "Record → transcribe → summarize. All on-device. Free to use.", size: 30, y: 402 },
    pills: [
      { text: "On-device · Free to use", width: 300 },
      { text: "Ask Claude about your meetings (MCP)", width: 470 },
    ],
  },
};

function renderSvg(cfg) {
  const headline = cfg.headline.lines
    .map(
      (line, i) =>
        `<text x="90" y="${cfg.headline.startY + i * cfg.headline.lineHeight}" font-family="${cfg.font}" font-size="${cfg.headline.size}" font-weight="700" fill="#f8fafc" letter-spacing="-1">${line}</text>`,
    )
    .join("\n  ");

  const pill2X = cfg.pills[0].width + 20;

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0.6" y2="1">
      <stop offset="0" stop-color="#0a0e1c"/>
      <stop offset="1" stop-color="#05070f"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.28" cy="0.12" r="0.7">
      <stop offset="0" stop-color="#6366f1" stop-opacity="0.40"/>
      <stop offset="0.55" stop-color="#4f46e5" stop-opacity="0.08"/>
      <stop offset="1" stop-color="#4f46e5" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="glow2" cx="0.95" cy="1" r="0.6">
      <stop offset="0" stop-color="#7c3aed" stop-opacity="0.22"/>
      <stop offset="1" stop-color="#7c3aed" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <rect width="${W}" height="${H}" fill="url(#bg)"/>
  <rect width="${W}" height="${H}" fill="url(#glow)"/>
  <rect width="${W}" height="${H}" fill="url(#glow2)"/>
  <rect x="0.5" y="0.5" width="${W - 1}" height="${H - 1}" fill="none" stroke="#1e293b" stroke-width="1"/>

  <!-- ブランドロックアップ -->
  <g transform="translate(90, 92)">
    <rect x="0" y="-6" width="46" height="46" rx="11" fill="#4f46e5"/>
    <g transform="translate(11, 5) scale(0.20)">
      <rect x="29" y="48" width="8" height="24" rx="4" fill="#ffffff"/>
      <rect x="42" y="40" width="8" height="40" rx="4" fill="#ffffff"/>
      <rect x="55" y="46" width="8" height="28" rx="4" fill="#ffffff"/>
      <rect x="70" y="44.5" width="21" height="7" rx="3.5" fill="#ffffff"/>
      <rect x="70" y="56.5" width="21" height="7" rx="3.5" fill="#ffffff"/>
      <rect x="70" y="68.5" width="13" height="7" rx="3.5" fill="#c7d2fe"/>
    </g>
    <text x="62" y="28" font-family="Menlo, monospace" font-size="30" font-weight="500" fill="#e2e8f0" letter-spacing="-0.5">mojiroku</text>
  </g>

  <!-- 見出し -->
  ${headline}

  <!-- サブ -->
  <text x="92" y="${cfg.sub.y}" font-family="${cfg.font}" font-size="${cfg.sub.size}" font-weight="400" fill="#94a3b8">${cfg.sub.text}</text>

  <!-- フッターのピル -->
  <g transform="translate(90, 470)" font-family="${cfg.font}" font-size="22">
    <rect x="0" y="0" width="${cfg.pills[0].width}" height="48" rx="24" fill="#1e1b4b" stroke="#4338ca" stroke-width="1"/>
    <circle cx="28" cy="24" r="4" fill="#818cf8"/>
    <text x="44" y="31" fill="#c7d2fe">${cfg.pills[0].text}</text>

    <rect x="${pill2X}" y="0" width="${cfg.pills[1].width}" height="48" rx="24" fill="#0f172a" stroke="#334155" stroke-width="1"/>
    <text x="${pill2X + 24}" y="31" fill="#cbd5e1">${cfg.pills[1].text}</text>
  </g>

  <text x="${W - 90}" y="${H - 56}" text-anchor="end" font-family="Menlo, monospace" font-size="24" fill="#64748b">mojiroku.com</text>
</svg>`;
}

for (const cfg of Object.values(LOCALES)) {
  const out = join(__dirname, "..", "public", cfg.out);
  await sharp(Buffer.from(renderSvg(cfg))).png().toFile(out);
  console.log("wrote", out);
}
