// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';

// 静的サイト（ゼロJS既定）。独自ドメイン mojiroku.com（Cloudflare Pages, apex）。
// i18n: ja は従来どおり `/`（prefix なし）、en は `/en/`。
// `/download`・`/updater/latest.json`・`/oauth/*` は出荷済みアプリに焼き付いた URL のため
// ja に prefix を付けない構成が必須（変更禁止）。
export default defineConfig({
  site: 'https://mojiroku.com',
  i18n: {
    locales: ['ja', 'en'],
    defaultLocale: 'ja',
    routing: { prefixDefaultLocale: false },
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
