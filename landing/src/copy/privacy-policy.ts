// プライバシーポリシー本文（ja / en）。LP のマーケコピー（ja.ts / en.ts）とは分けて持つ。
//
// なぜ独立したページが要るか:
//   Google の OAuth 審査は「ホームページと同一ドメインで公開されたプライバシーポリシー」を
//   必須要件にしている（sensitive scope verification）。加えて、配布アプリとして
//   「何が端末から出るのか」を一箇所で読めるようにする意味がある。
//
// ⚠️ ここに書く事実はコードと一致していなければ意味がない。通信先を増やしたら必ずこの表も足す。
//    2026-08-29 時点の宛先はソースから機械的に洗い出した（crates/ と src-tauri/src の https:// 全件）。
//
// ja を形の正とし、en は同じ型で構造一致を tsc に守らせる。

export interface PolicySection {
  heading: string;
  /** 段落。空配列可 */
  body: string[];
  /** 箇条書き。空配列可 */
  bullets: string[];
}

export interface EgressRow {
  /** 宛先ホスト */
  host: string;
  /** いつ通信するか */
  when: string;
  /** 何が送られるか */
  what: string;
  /** 既定でオンか、ユーザーが選ぶか */
  trigger: string;
}

export interface PrivacyPolicyCopy {
  meta: { title: string; description: string };
  title: string;
  updated: string;
  lead: string[];
  summary: { heading: string; bullets: string[] };
  stored: PolicySection;
  egress: {
    heading: string;
    body: string[];
    columns: { host: string; when: string; what: string; trigger: string };
    rows: EgressRow[];
    note: string;
  };
  /** 運営者が動かす唯一のサーバー（Slack/Notion の OAuth 中継）の説明 */
  broker: PolicySection;
  google: PolicySection;
  notCollected: PolicySection;
  deletion: PolicySection;
  website: PolicySection;
  changes: PolicySection;
  contact: {
    heading: string;
    body: string[];
    linkLabel: string;
    linkHref: string;
  };
  backLabel: string;
}

const APP_DIR = "~/Library/Application Support/com.daichi0812.mojiroku";
const ISSUES = "https://github.com/daichi8120/mojiroku/issues";

export const ja: PrivacyPolicyCopy = {
  meta: {
    title: "プライバシーポリシー | mojiroku",
    description:
      "mojiroku が扱うデータと、端末の外へ出る場面をすべて明記します。録音・文字起こし・要約は既定でこの Mac の中だけに保存されます。",
  },
  title: "プライバシーポリシー",
  updated: "最終更新: 2026年8月29日",
  lead: [
    "mojiroku は、録音・文字起こし・要約をあなたの Mac の中だけで完結させるデスクトップアプリです。会議の音声・文字起こし・要約が運営者に届くことはありません。受け取る仕組みを作っていないためです。",
    "運営者が動かしているサーバーは 1 つだけで、Slack と Notion の連携を成立させるための小さな中継（OAuth ブローカー）です。何がそこを通り、何を保存しないのかは下に書きました。",
    "このページは、mojiroku が何を保存し、どういうときに何が端末の外へ出るのかを、実装に即して書いたものです。アプリのソースは AGPL-3.0 で公開しているので、ここに書いてあることは自分で確かめられます。",
  ],
  summary: {
    heading: "先に要点",
    bullets: [
      "アカウント登録もログインもありません。運営者はあなたが誰かを知りません。",
      "録音・文字起こし・要約・話者情報は、あなたの Mac の中にだけ保存されます。",
      "利用状況の計測（テレメトリ）とクラッシュレポートの送信は行っていません。",
      "端末の外へ出るのは、下の表にある場面だけです。ほとんどはあなたが選んだときだけ動きます。",
      "運営者があなたの会議データを見ることはできません。受け取る仕組みが存在しないためです。",
      "Slack と Notion の連携だけは、運営者が動かす中継サーバーを経由します（何も保存しません）。Google カレンダーは経由しません。",
    ],
  },
  stored: {
    heading: "端末に保存されるもの",
    body: [
      `録音した音声、文字起こし、要約、議事録、話者、アクションアイテム、処理ジョブは、${APP_DIR} の中の SQLite データベースと音声ファイルとして保存されます。文字起こしと要約に使う AI モデルも同じ場所に置かれます。`,
      "API キーと連携トークンは、設定ファイルではなく macOS のキーチェーンに保管します。",
    ],
    bullets: [
      "保存先はすべてあなたの Mac の中で、運営者は参照できません。",
      "バックアップやクラウド同期は行いません（Time Machine や iCloud の対象になるかは、あなたの macOS の設定次第です）。",
    ],
  },
  egress: {
    heading: "端末の外へ出る場面",
    body: [
      "mojiroku が外部と通信するのは、次の場面だけです。2026年8月29日時点のソースコードから機械的に洗い出したものです。",
    ],
    columns: {
      host: "宛先",
      when: "いつ",
      what: "何が送られるか",
      trigger: "きっかけ",
    },
    rows: [
      {
        host: "huggingface.co",
        when: "初回起動時のモデル取得",
        what: "何も送りません（ダウンロードのみ）",
        trigger: "自動",
      },
      {
        host: "mojiroku.com / github.com",
        when: "アプリの更新確認とダウンロード",
        what: "何も送りません（更新情報の取得のみ。版の比較は端末内で行います）",
        trigger: "自動",
      },
      {
        host: "mojiroku.com（OAuth ブローカー）",
        when: "Slack または Notion の連携を有効にしたとき",
        what: "認可コードと、その交換で得た Slack の Webhook URL / Notion のアクセストークン。**議事録や会議の内容は通りません**",
        trigger: "あなたが選んだとき",
      },
      {
        host: "accounts.google.com / oauth2.googleapis.com / www.googleapis.com",
        when: "Google カレンダー連携を有効にしたとき",
        what: "認可の要求とトークンの更新。予定の取得（受信のみ）",
        trigger: "あなたが選んだとき",
      },
      {
        host: "calendar.google.com",
        when: "旧方式の限定公開 iCal URL を設定しているとき",
        what: "その URL への取得要求（受信のみ）",
        trigger: "あなたが選んだとき",
      },
      {
        host: "api.anthropic.com / api.openai.com",
        when: "クラウド要約（BYOK）を選んで要約を実行したとき",
        what: "要約の対象になる文字起こしの本文",
        trigger: "既定はオフ。あなたが自分の API キーを設定したときだけ",
      },
      {
        host: "api.notion.com / www.notion.so",
        when: "Notion へ書き出すボタンを押したとき",
        what: "書き出す議事録（要約と文字起こし）",
        trigger: "その都度の操作",
      },
      {
        host: "hooks.slack.com",
        when: "Slack へ送るボタンを押したとき",
        what: "送信する議事録",
        trigger: "その都度の操作",
      },
      {
        host: "chatgpt.com / claude.ai",
        when: "「ChatGPT で開く」「Claude で開く」を押したとき",
        what: "**文字起こしと要約を含むプロンプト**。1500 文字以内なら URL に載って渡り、それを超える場合はクリップボードへのコピーだけになります（どちらの場合もクリップボードにはコピーされます）",
        trigger: "その都度の操作",
      },
      {
        host: "docs.google.com",
        when: "フィードバックフォームを開いたとき",
        what: "アプリと macOS のバージョン、Mac の種別（URL の事前入力として載る）",
        trigger: "その都度の操作。フォームの送信もあなたが押したときだけ",
      },
    ],
    note: "この表にない宛先へは通信しません。広告配信事業者・データブローカー・解析事業者へデータを渡すことはありません。なお、どのような HTTP 要求でも、接続する側の IP アドレスは接続先のサーバーから見えます（これは mojiroku に限らずインターネット通信一般の性質です）。それぞれの宛先での取り扱いは、各社のプライバシーポリシーに従います。",
  },
  broker: {
    heading: "Slack・Notion 連携の中継サーバーについて",
    body: [
      "Slack と Notion は、認可コードをトークンに交換する際に「アプリの秘密鍵」を必要とする方式（confidential client）しか認めていません。配布するアプリに秘密鍵を埋め込むと誰でも取り出せてしまうため、mojiroku.com 上の小さな中継（Cloudflare Worker）が交換だけを代行しています。",
      "この中継を通るのは、認可コードと、その交換で得た Slack の Webhook URL または Notion のアクセストークンだけです。**議事録・要約・文字起こし・音声は一切通りません。**書き出しの本文は、あなたの Mac から Slack / Notion へ直接送られます。",
      "中継はデータベースもファイルストレージも持たず、通過した値を保存しません。受け取った結果はその場であなたの端末（127.0.0.1）へ転送され、以後は端末の macOS キーチェーンに保管されます。",
    ],
    bullets: [
      "**Google カレンダーはこの中継を経由しません。** 端末が Google と直接やり取りします（PKCE + loopback）。",
      "中継のソースコードも同じリポジトリで公開しています（`landing/worker/index.ts`）。",
      "連携を使わなければ、この中継に接続することはありません。",
    ],
  },
  google: {
    heading: "Google カレンダーのデータについて",
    body: [
      "mojiroku が Google に求める権限は `calendar.events.readonly`（予定の閲覧）ひとつだけです。予定の作成・変更・削除はできません。",
      "取得するのは、直近の予定のタイトルと開始・終了時刻です。用途は 2 つで、会議の開始時刻に「録音しますか」と通知を出すことと、録音のタイトルに予定名を補完することです。どちらもアプリの画面上ではっきり見える機能です。",
      "取得した予定そのものはデータベースに保存しません。通知とタイトル補完のためにメモリ上で使うだけです。あなたが録音を作った場合に限り、その録音のタイトルとして予定名があなたの Mac の中に残ることがあります。",
      "mojiroku は Google から取得したデータを、いかなるサーバーへも送信しません。認可も予定の取得も、あなたの端末が Google と直接行います（PKCE + loopback リダイレクト）。**上に書いた Slack・Notion 用の中継サーバーも、Google のデータには一切関与しません。**運営者を含め、誰かがあなたの予定を読むことはできません。",
    ],
    bullets: [
      "アクセストークンとリフレッシュトークンは macOS のキーチェーンに保管します。",
      "アプリの「連携を解除」を押すと、保存されたトークンは削除されます。Google アカウント側からも取り消せます。",
      "mojiroku による Google ユーザーデータの取り扱いは、Limited Use requirements を含む Google API Services User Data Policy に準拠します。取得したデータは、上に挙げたアプリ内機能の提供のためだけに使い、第三者へ移転せず、広告には一切使いません。",
    ],
  },
  notCollected: {
    heading: "収集していないもの",
    body: [],
    bullets: [
      "アカウント情報（登録もログインもありません）",
      "利用状況の計測・行動ログ（テレメトリを送っていません）",
      "クラッシュレポートの自動送信",
      "位置情報、連絡先、広告 ID",
      "録音や文字起こしの内容（運営者が受け取る仕組みがありません）",
    ],
  },
  deletion: {
    heading: "データの削除",
    body: [
      "すべてあなたの端末にあるので、削除もあなたの手元で完結します。",
    ],
    bullets: [
      "個別の録音・議事録は、アプリの履歴から削除できます。",
      "連携トークンと API キーは、アプリの「連携を解除」または設定から削除できます。",
      `すべて消す場合は、アプリを削除したうえで ${APP_DIR} を削除してください。`,
    ],
  },
  website: {
    heading: "このウェブサイトについて",
    body: [
      "mojiroku.com では、閲覧数の把握のために Cloudflare Web Analytics を使っています。Cookie を使わず、個人を識別する情報も、閲覧者を横断して追跡する識別子も収集しません。",
    ],
    bullets: [
      "広告は掲載していません。広告目的のトラッカーも入れていません。",
      "ダウンロードは GitHub Releases から配信されます。その際は GitHub のプライバシーポリシーが適用されます。",
    ],
  },
  changes: {
    heading: "このポリシーの変更",
    body: [
      "内容を変更した場合は、このページの最終更新日を改めます。通信先が増える変更を行うときは、上の表を必ず更新します。",
      "このページの変更履歴は、ソースリポジトリの Git 履歴として公開されています。",
    ],
    bullets: [],
  },
  contact: {
    heading: "問い合わせ",
    body: [
      "このポリシーやデータの扱いについての質問は、GitHub の Issue で受け付けています。",
    ],
    linkLabel: "GitHub Issues で質問する",
    linkHref: ISSUES,
  },
  backLabel: "トップへ戻る",
};

export const en: PrivacyPolicyCopy = {
  meta: {
    title: "Privacy Policy | mojiroku",
    description:
      "Everything mojiroku stores, and every case where data leaves your Mac. Recordings, transcripts and summaries stay on your device by default.",
  },
  title: "Privacy Policy",
  updated: "Last updated: 29 August 2026",
  lead: [
    "mojiroku is a desktop app that records, transcribes and summarises meetings entirely on your Mac. Your meeting audio, transcripts and summaries never reach us, because we built no mechanism to receive them.",
    "We operate exactly one server: a small relay (an OAuth broker) that exists only to make the Slack and Notion integrations work. What passes through it, and what it does not store, is described below.",
    "This page describes what mojiroku stores and exactly when data leaves your device, based on the actual implementation. The source is published under AGPL-3.0, so you can verify every statement here yourself.",
  ],
  summary: {
    heading: "In short",
    bullets: [
      "There is no account and no sign-in. We do not know who you are.",
      "Recordings, transcripts, summaries and speaker data are stored only on your Mac.",
      "We collect no usage analytics and send no crash reports.",
      "Data leaves your device only in the cases listed in the table below, and most of them only when you choose to.",
      "We cannot read your meeting data, because no mechanism exists for us to receive it.",
      "Only the Slack and Notion integrations pass through a relay we operate (it stores nothing). Google Calendar does not.",
    ],
  },
  stored: {
    heading: "What is stored on your device",
    body: [
      `Recorded audio, transcripts, summaries, minutes, speakers, action items and processing jobs are stored as a SQLite database and audio files under ${APP_DIR}. The AI models used for transcription and summarisation are kept in the same place.`,
      "API keys and integration tokens are stored in the macOS Keychain rather than in a settings file.",
    ],
    bullets: [
      "All of it stays on your Mac; we cannot access any of it.",
      "We perform no backup and no cloud sync (whether Time Machine or iCloud picks these files up depends on your own macOS settings).",
    ],
  },
  egress: {
    heading: "When data leaves your device",
    body: [
      "These are the only cases in which mojiroku talks to the network. The list was derived mechanically from the source code as of 29 August 2026.",
    ],
    columns: {
      host: "Destination",
      when: "When",
      what: "What is sent",
      trigger: "Trigger",
    },
    rows: [
      {
        host: "huggingface.co",
        when: "Downloading models on first launch",
        what: "Nothing is sent (download only)",
        trigger: "Automatic",
      },
      {
        host: "mojiroku.com / github.com",
        when: "Checking for and downloading app updates",
        what: "Nothing is sent (the update manifest is fetched; the version comparison happens on your Mac)",
        trigger: "Automatic",
      },
      {
        host: "mojiroku.com (OAuth broker)",
        when: "When you enable the Slack or Notion integration",
        what: "The authorisation code, and the Slack webhook URL / Notion access token obtained by exchanging it. **No minutes or meeting content passes through**",
        trigger: "Only when you choose to",
      },
      {
        host: "accounts.google.com / oauth2.googleapis.com / www.googleapis.com",
        when: "When you enable Google Calendar",
        what: "Authorisation requests and token refreshes; fetching events (receive only)",
        trigger: "Only when you choose to",
      },
      {
        host: "calendar.google.com",
        when: "If you use the older secret iCal URL method",
        what: "A fetch request to that URL (receive only)",
        trigger: "Only when you choose to",
      },
      {
        host: "api.anthropic.com / api.openai.com",
        when: "When you run a cloud summary (BYOK)",
        what: "The transcript text being summarised",
        trigger: "Off by default; only after you set your own API key",
      },
      {
        host: "api.notion.com / www.notion.so",
        when: "When you press the export-to-Notion button",
        what: "The minutes being exported (summary and transcript)",
        trigger: "Each time you act",
      },
      {
        host: "hooks.slack.com",
        when: "When you press the send-to-Slack button",
        what: "The minutes being sent",
        trigger: "Each time you act",
      },
      {
        host: "chatgpt.com / claude.ai",
        when: "When you press Open in ChatGPT / Open in Claude",
        what: "**A prompt containing the transcript and summary.** If it is 1500 characters or shorter it is carried in the URL; longer prompts are only placed on your clipboard (the clipboard copy happens either way)",
        trigger: "Each time you act",
      },
      {
        host: "docs.google.com",
        when: "When you open the feedback form",
        what: "App and macOS version and Mac type, pre-filled into the form URL",
        trigger: "Each time you act; the form is only submitted if you submit it",
      },
    ],
    note: "We contact no destinations other than those listed. We never pass data to advertising networks, data brokers or analytics vendors. Note that with any HTTP request the connecting IP address is visible to the destination server — this is a property of internet communication generally, not something specific to mojiroku. Each destination handles that under its own privacy policy.",
  },
  broker: {
    heading: "The relay used for Slack and Notion",
    body: [
      "Slack and Notion only support an exchange flow that requires an application secret (a confidential client). Embedding such a secret in a distributed app would let anyone extract it, so a small relay (a Cloudflare Worker) on mojiroku.com performs the exchange instead.",
      "Only two things pass through the relay: the authorisation code, and the Slack webhook URL or Notion access token obtained from it. **No minutes, summaries, transcripts or audio ever pass through it.** Exported content goes directly from your Mac to Slack or Notion.",
      "The relay has no database and no file storage, and it retains nothing that passes through. The result is redirected straight back to your device (127.0.0.1) and is then kept in your macOS Keychain.",
    ],
    bullets: [
      "**Google Calendar does not use this relay.** Your device talks to Google directly (PKCE with a loopback redirect).",
      "The relay's source is published in the same repository (`landing/worker/index.ts`).",
      "If you never use those integrations, your device never contacts the relay.",
    ],
  },
  google: {
    heading: "Google Calendar data",
    body: [
      "mojiroku requests a single Google permission: `calendar.events.readonly` (view events). It cannot create, modify or delete anything.",
      "It reads the titles and start and end times of your upcoming events. There are two uses, both prominent in the app's interface: showing a notification asking whether to start recording when a meeting begins, and pre-filling the recording's title with the event name.",
      "The events themselves are never written to the database. They are held in memory only, for the notification and the title. If you do start a recording, the event name may remain on your Mac as that recording's title.",
      "mojiroku never transmits data obtained from Google to any server. Both the authorisation and the event fetch happen directly between your device and Google (PKCE with a loopback redirect). **The relay described above, used for Slack and Notion, plays no part in handling Google data.** Nobody, including the developer, can read your events.",
    ],
    bullets: [
      "Access and refresh tokens are stored in the macOS Keychain.",
      "Pressing Disconnect in the app deletes the stored tokens. You can also revoke access from your Google Account.",
      "mojiroku's use of information received from Google APIs adheres to the Google API Services User Data Policy, including the Limited Use requirements. Data obtained is used solely to provide the in-app features described above, is never transferred to third parties, and is never used for advertising.",
    ],
  },
  notCollected: {
    heading: "What we do not collect",
    body: [],
    bullets: [
      "Account details (there is no sign-up and no sign-in)",
      "Usage analytics or behavioural logs (no telemetry is sent)",
      "Automatic crash reports",
      "Location, contacts or advertising identifiers",
      "The contents of your recordings or transcripts (no mechanism exists for us to receive them)",
    ],
  },
  deletion: {
    heading: "Deleting your data",
    body: ["Everything lives on your device, so deletion happens entirely in your hands."],
    bullets: [
      "Individual recordings and minutes can be deleted from the app's history.",
      "Integration tokens and API keys can be removed with Disconnect, or from Settings.",
      `To remove everything, delete the app and then delete ${APP_DIR}.`,
    ],
  },
  website: {
    heading: "About this website",
    body: [
      "mojiroku.com uses Cloudflare Web Analytics to count page views. It sets no cookies and collects no personally identifying information and no identifier that follows visitors across sites.",
    ],
    bullets: [
      "There is no advertising on this site, and no advertising trackers.",
      "Downloads are served from GitHub Releases; GitHub's privacy policy applies to those requests.",
    ],
  },
  changes: {
    heading: "Changes to this policy",
    body: [
      "If this policy changes, the last-updated date above changes with it. Any change that adds a network destination will also update the table above.",
      "The revision history of this page is public as the Git history of the source repository.",
    ],
    bullets: [],
  },
  contact: {
    heading: "Contact",
    body: [
      "Questions about this policy or about how data is handled are welcome as GitHub issues.",
    ],
    linkLabel: "Ask on GitHub Issues",
    linkHref: ISSUES,
  },
  backLabel: "Back to home",
};

const dict: Record<string, PrivacyPolicyCopy> = { ja, en };

/** Astro.currentLocale を安全に解決する。未知値は ja。 */
export function getPrivacyCopy(locale: string | undefined): PrivacyPolicyCopy {
  return dict[locale ?? "ja"] ?? ja;
}
