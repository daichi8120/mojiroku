// mojiroku リモート MCP の OAuth 2.1 ゲートウェイ（ADR-0025）。
//
// - OAuth まわり（/authorize の検証・/token・/register(DCR)・/.well-known/*）は
//   @cloudflare/workers-oauth-provider が処理する。トークン類は OAUTH_KV に
//   ハッシュのみ保存される（ライブラリの設計）。
// - アクセストークン検証済みの /mcp リクエストだけが apiHandler に届き、
//   Cloudflare Tunnel の origin へ ORIGIN_TOKEN を付けてプロキシされる。
// - /authorize の「ユーザー認証」は自分専用パスフレーズ（GATEWAY_PASSPHRASE）。
//   議事録 DB は単一ユーザーのローカルデータなので、ID 基盤は持たない。

import { OAuthProvider, type OAuthHelpers } from '@cloudflare/workers-oauth-provider';

interface Env {
  OAUTH_KV: KVNamespace;
  OAUTH_PROVIDER: OAuthHelpers;
  /** /authorize 同意画面のパスフレーズ（wrangler secret） */
  GATEWAY_PASSPHRASE: string;
  /** origin（mojiroku-mcp --http）の Bearer トークン（wrangler secret） */
  ORIGIN_TOKEN: string;
}

/** Tunnel の origin。ここより先（Mac 側）は mojiroku-mcp 自身の Bearer 検証が守る。 */
const ORIGIN = 'https://mcp-origin.mojiroku.com';

/** タイミング攻撃でパスフレーズを先頭から確定されないよう、全バイトを必ず比較する。 */
function constantTimeEq(a: string, b: string): boolean {
  const ab = new TextEncoder().encode(a);
  const bb = new TextEncoder().encode(b);
  if (ab.length !== bb.length) return false;
  let diff = 0;
  for (let i = 0; i < ab.length; i++) diff |= ab[i] ^ bb[i];
  return diff === 0;
}

function html(body: string, status = 200): Response {
  return new Response(body, {
    status,
    headers: { 'content-type': 'text/html; charset=utf-8' },
  });
}

/** 同意画面。フォームは同じ /authorize（クエリ付き）へ POST し、OAuth パラメータを引き継ぐ。 */
function consentPage(clientName: string, action: string, error?: string): string {
  return `<!doctype html>
<html lang="ja"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>mojiroku MCP 接続の承認</title>
<style>
  body { font-family: -apple-system, sans-serif; background: #111; color: #eee;
         display: grid; place-items: center; min-height: 100dvh; margin: 0; }
  form { background: #1c1c1e; padding: 2rem; border-radius: 12px; max-width: 22rem; width: 90%; }
  h1 { font-size: 1.1rem; margin: 0 0 .5rem; }
  p { font-size: .85rem; color: #aaa; margin: 0 0 1rem; }
  input { width: 100%; box-sizing: border-box; padding: .6rem; border-radius: 8px;
          border: 1px solid #444; background: #111; color: #eee; margin-bottom: 1rem; }
  button { width: 100%; padding: .6rem; border: 0; border-radius: 8px;
           background: #e8590c; color: #fff; font-weight: 600; cursor: pointer; }
  .err { color: #ff6b6b; font-size: .85rem; margin-bottom: 1rem; }
</style></head><body>
<form method="post" action="${action}">
  <h1>mojiroku の議事録への接続を承認</h1>
  <p>クライアント: <strong>${clientName}</strong><br>
     承認すると、このクライアントはローカル Mac の議事録を検索・閲覧できます（読み取り専用）。</p>
  ${error ? `<div class="err">${error}</div>` : ''}
  <input type="password" name="passphrase" placeholder="パスフレーズ" autofocus required>
  <button type="submit">承認する</button>
</form></body></html>`;
}

/** HTML 属性・テキストに埋め込む値のエスケープ（クライアント名は DCR で外部入力になる）。 */
function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!,
  );
}

const defaultHandler = {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/authorize') {
      const oauthReqInfo = await env.OAUTH_PROVIDER.parseAuthRequest(request);
      const client = await env.OAUTH_PROVIDER.lookupClient(oauthReqInfo.clientId);
      const clientName = escapeHtml(client?.clientName ?? oauthReqInfo.clientId);
      // フォームの POST 先に元のクエリをそのまま残す（OAuth パラメータの持ち回り）。
      const action = escapeHtml(url.pathname + url.search);

      if (request.method === 'GET') {
        return html(consentPage(clientName, action));
      }
      if (request.method === 'POST') {
        const form = await request.formData();
        const passphrase = String(form.get('passphrase') ?? '');
        if (!constantTimeEq(passphrase, env.GATEWAY_PASSPHRASE)) {
          // ブルートフォース抑止に一律 1 秒待たせてから同じ画面に戻す。
          await new Promise((r) => setTimeout(r, 1000));
          return html(consentPage(clientName, action, 'パスフレーズが違います'), 401);
        }
        const { redirectTo } = await env.OAUTH_PROVIDER.completeAuthorization({
          request: oauthReqInfo,
          userId: 'daichi',
          metadata: { approvedAt: new Date().toISOString() },
          scope: oauthReqInfo.scope,
          props: {},
        });
        return Response.redirect(redirectTo, 302);
      }
      return new Response('Method not allowed', { status: 405 });
    }

    return new Response('mojiroku MCP gateway', { status: 404 });
  },
};

const apiHandler = {
  async fetch(request: Request, env: Env): Promise<Response> {
    // ここに届く時点でアクセストークン検証は済んでいる。origin へは ORIGIN_TOKEN に
    // 差し替えて転送する（クライアントのトークンを origin に見せない）。
    const url = new URL(request.url);
    const target = new URL(url.pathname + url.search, ORIGIN);
    const headers = new Headers(request.headers);
    headers.set('authorization', `Bearer ${env.ORIGIN_TOKEN}`);
    headers.delete('cookie');
    // Response をそのまま返すことで SSE ストリーミングと Mcp-Session-Id を素通しする。
    return fetch(target, {
      method: request.method,
      headers,
      body: request.body,
      redirect: 'manual',
    });
  },
};

export default new OAuthProvider({
  apiRoute: '/mcp',
  // ライブラリの型は WorkerEntrypoint 前提の総称型だが、README 記載の通り
  // 「fetch を持つオブジェクト」も受け付ける。
  apiHandler: apiHandler as any,
  defaultHandler: defaultHandler as any,
  authorizeEndpoint: '/authorize',
  tokenEndpoint: '/token',
  clientRegistrationEndpoint: '/register',
  scopesSupported: ['meetings.read'],
  // OAuth 2.1: S256 のみ許可（plain PKCE を無効化）。
  allowPlainPKCE: false,
});
