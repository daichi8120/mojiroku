// sitemap.xml をビルド時に生成する。
//
// 以前は landing/public/sitemap.xml に手書きで、`lastmod` が 2026-07-04 で固定されていた。
// 静的ファイルなので以後どれだけ更新しても変わらず、クローラに古い日付を出し続けていた。
//
// 出力の形（URL・hreflang・priority・changefreq）は手書き版と同一に保つ。
// @astrojs/sitemap を使わないのは、出力名が sitemap-index.xml になり、
// robots.txt と Search Console に登録済みの /sitemap.xml を壊すため。
import type { APIRoute } from "astro";

/** ビルド日。landing は内容が変わったときだけデプロイするので、ビルド日 ≒ 最終更新日。 */
const LASTMOD = new Date().toISOString().slice(0, 10);

/** i18n は astro.config.mjs のとおり ja=`/`（prefix なし）・en=`/en/`。 */
const PAGES = [
  { path: "/", priority: "1.0" },
  { path: "/en/", priority: "0.9" },
] as const;

const FALLBACK_ORIGIN = "https://mojiroku.com";

export const GET: APIRoute = ({ site }) => {
  const origin = (site ?? new URL(FALLBACK_ORIGIN)).origin;

  // hreflang は全ページで同じ集合を指す（ja / en / x-default=ja）。
  const alternates = [
    `<xhtml:link rel="alternate" hreflang="ja" href="${origin}/"/>`,
    `<xhtml:link rel="alternate" hreflang="en" href="${origin}/en/"/>`,
    `<xhtml:link rel="alternate" hreflang="x-default" href="${origin}/"/>`,
  ]
    .map((l) => `    ${l}`)
    .join("\n");

  const urls = PAGES.map(
    (p) => `  <url>
    <loc>${origin}${p.path}</loc>
    <lastmod>${LASTMOD}</lastmod>
    <changefreq>weekly</changefreq>
    <priority>${p.priority}</priority>
${alternates}
  </url>`,
  ).join("\n");

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls}
</urlset>
`;

  return new Response(xml, {
    headers: { "Content-Type": "application/xml; charset=utf-8" },
  });
};
