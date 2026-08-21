#!/usr/bin/env bash
# 開発用の「安定したコード署名 identity（mojiroku-dev）」を login キーチェーンに一度だけ作る。
#
# なぜ: tauri dev の dev バイナリは未署名/アドホック署名で、再ビルド毎にバイナリ同一性が
#       変わる。macOS の TCC 許可（会議モードのマイク/画面収録など）は署名同一性に紐づくため、
#       再ビルドの度にリセットされ再許可ダイアログが出る。安定した自己署名 identity で署名すれば
#       designated requirement が固定され、一度与えた TCC 許可が再ビルドをまたいで永続する。
#       署名は scripts/dev-sign-run.sh が自動で行う。
#       （dev のキーチェーン回避＝BYOK/OAuth は src-tauri/src/secrets.rs の平文ストアで対応済み）
#
# 使い方: bash scripts/dev-codesign-setup.sh   （冪等。一度だけでよい）
# 削除  : security delete-identity -c "mojiroku-dev"
#
# 注: Apple Development 証明書がある場合はそちらを推奨（ADR-0022）。repo ルートに
#     .mojiroku-dev-sign.env（gitignore 済み）を置き
#       MOJIROKU_DEV_SIGN_IDENTITY="Apple Development: <名前> (XXXXXXXXXX)"
#     と書けば dev-sign-run.sh がそちらで署名する。本スクリプトの自己署名 mojiroku-dev は
#     証明書の無いマシン向けフォールバックとして残る。identity を切り替えた直後と
#     年次失効での証明書再作成時は TCC（マイク/画面収録）を一度だけ再許可すればよい。
set -euo pipefail

IDENTITY="mojiroku-dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$IDENTITY"; then
  echo "既に存在します: $IDENTITY（何もしません）"
  exit 0
fi

# macOS 同梱の /usr/bin/openssl は LibreSSL で -addext 非対応のことがある。
# 拡張は config ファイルで付与し、OpenSSL / LibreSSL の両方で通るようにする。
# brew の OpenSSL があれば優先。
OPENSSL=/usr/bin/openssl
[ -x /opt/homebrew/opt/openssl@3/bin/openssl ] && OPENSSL=/opt/homebrew/opt/openssl@3/bin/openssl

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/req.cnf" <<'CNF'
[req]
distinguished_name = dn
x509_extensions    = ext
prompt             = no
[dn]
CN = mojiroku-dev
[ext]
basicConstraints   = critical,CA:FALSE
keyUsage           = critical,digitalSignature
extendedKeyUsage   = critical,codeSigning
CNF

# 20 年有効の自己署名コード署名証明書（Apple Development 証明書のような年次失効で
# ACL が再び壊れるのを避けるため、長期の自己署名を使う）。
"$OPENSSL" req -x509 -newkey rsa:2048 -sha256 -days 7300 -nodes \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -config "$TMP/req.cnf" -extensions ext >/dev/null 2>&1

# OpenSSL 3+ はデフォルトで新しい PKCS12 アルゴリズムを使い、macOS の security import
# （旧 Security framework パーサ）が "MAC verification failed" で弾く。-legacy で
# 旧アルゴリズム（3DES/SHA1）に切り替えると Apple 側が取り込める。LibreSSL は -legacy
# 非対応かつ既定が旧形式なので付けない。
LEGACY=""
"$OPENSSL" version | grep -qE "^OpenSSL [3-9]" && LEGACY="-legacy"

"$OPENSSL" pkcs12 -export $LEGACY -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -out "$TMP/id.p12" -name "$IDENTITY" -passout pass:mojiroku >/dev/null 2>&1

# login キーチェーンへ秘密鍵ごとインポートし、codesign からの利用を許可する。
security import "$TMP/id.p12" -k "$KEYCHAIN" -P mojiroku \
  -T /usr/bin/codesign -T /usr/bin/security >/dev/null

echo "作成しました: $IDENTITY"
# 自己署名（未信頼）証明書は find-identity には出ないため find-certificate で確認する。
security find-certificate -c "$IDENTITY" -Z 2>/dev/null | sed -n 's/^SHA-1 hash: /  SHA-1: /p' || true
echo
echo "次の dev 起動（just dev）から、初回のみキーチェーン許可ダイアログが出ます。"
echo "「常に許可」を押せば、以降は再ビルドしてもパスワードを求められません。"
