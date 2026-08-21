# 話者分離の評価ハーネス

[ADR-0028](../../docs/decisions/ADR-0028_話者分離segmentationをpyannoteに差し替え.md) の出荷ゲート
（segmentation を reverb-diarization-v1 → pyannote-segmentation-3.0 に差し替える purity A/B）を
2026-08-21 に実行したときの資材。**次に同じ測定をするときの再現手順**を残す。

正解ラベル（GT）は [ADR-0009](../../docs/decisions/ADR-0009_話者分離スパイク結果.md) のスパイクで
手作業で付けたもので、当時セッションの記録にしか残っていなかった。2026-08-21 に復元して
`gt_adr0009.py` に固定した。**数値は当時のまま**である。

## 前提: 対象音声はリポジトリに含まれない

評価に使う音声は**非公開の実会議録音**で、リポジトリには入っていない。会議名・参加者名も残さない
（GT の話者は A / B / C のまま）。作者の手元では次の手順で同じ音声を再取得できる。

1. mojiroku の履歴 DB（`~/Library/Application Support/com.daichi0812.mojiroku/mojiroku.sqlite3`）の
   `recordings` テーブルから、`duration_ms` が `3209000` のレコードを引く。
   録音 ID は `f5a6cb30-8bec-4f91-ab3f-03a4a696a764`。
2. その `.wav` の **3:00〜13:00 の 600 秒**を 16kHz mono に切り出す（GT の時刻はこの抜粋の相対秒）。

```bash
mkdir -p /tmp/purity-ab && cd /tmp/purity-ab
SRC="$HOME/Library/Application Support/com.daichi0812.mojiroku/recordings/f5a6cb30-8bec-4f91-ab3f-03a4a696a764.wav"
ffmpeg -y -ss 180 -t 600 -i "$SRC" -ac 1 -ar 16000 jp_3to13.wav -loglevel error
```

## 環境とモデル

`sherpa-onnx` は **1.13.3 にピンする**。バージョンを指定しないと 1.13.6 が入る。

```bash
cd /tmp/purity-ab
python3 -m venv venv
./venv/bin/pip install "sherpa-onnx==1.13.3" numpy soundfile
```

モデルは 3 本。segmentation の 2 本目（reverb）は**比較対象としてのみ**使う。

```bash
cd /tmp/purity-ab
# pyannote-segmentation-3.0（MIT, CNRS）
curl -sL -O https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2
shasum -a 256 sherpa-onnx-pyannote-segmentation-3-0.tar.bz2
# 24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488
tar xjf sherpa-onnx-pyannote-segmentation-3-0.tar.bz2   # model.onnx = 5,992,913 bytes (f32)

# 参照 VAD（Silero）
curl -sL -O https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx
```

embedding（`nemo_titanet_large.onnx`）と旧 segmentation（`sherpa-reverb-diarization-v1.onnx`）は、
アプリのモデルキャッシュ `~/Library/Application Support/com.daichi0812.mojiroku/models/` を直接読む。

## 実行

スクリプトは `/tmp/purity-ab` を作業ディレクトリとして**パスを直書き**している。
2026-08-21 に実際に走ったものをそのまま置いてあるので、編集せずコピーして使う。

```bash
mkdir -p /tmp/purity-ab && cp eval/diarization/*.py /tmp/purity-ab/   # リポジトリルートで実行
cd /tmp/purity-ab
./venv/bin/python run_infer.py       # 推論 → results/*.json + *.npz
./venv/bin/python make_vad.py        # 参照 VAD → vad.json
./venv/bin/python metrics.py         # 表①（ADR-0009 の規約。母数 = GT 全フレーム）
./venv/bin/python vad_analysis.py    # 表②（母数 = GT∩VAD発話）+ recall/precision
./venv/bin/python detail.py          # 区間別の得失・誤り内訳（誤帰属 / 未割当）
./venv/bin/python product_proxy.py   # 表③（merge.rs 相当の発話粒度）
```

`gt_adr0009.py` は復元した GT の原本で、実行はしない。同じ配列が `metrics.py` の先頭に埋め込まれており、
`vad_analysis.py` / `detail.py` / `product_proxy.py` はそれを読み込んで使う。

## 各スクリプトが出すもの

| スクリプト | 出力 |
|---|---|
| `run_infer.py` | sherpa の推論。segmentation × threshold の全条件で raw セグメントと turn ごとの TitaNet 埋め込みを `results/` へ |
| `make_vad.py` | Silero VAD の発話区間 `vad.json`（表②以降の母数） |
| `metrics.py` | 表①。ADR-0009 と同じ規約の purity / 被覆 / oracle 整合。consolidation の複製もここ |
| `vad_analysis.py` | 表②。実発話フレームだけを母数にした purity と、segmentation の recall / precision |
| `detail.py` | GT 区間ごとの得失と、誤りの内訳（他話者へ誤帰属 / 未割当） |
| `product_proxy.py` | 表③。発話セグメント単位に話者を付けてから測る製品粒度のプロキシ |

## 固定パラメータ

測定条件を変えるとき、ここを変えたかどうかを必ず記録する。

- クラスタリング: `num_clusters=-1`（人数を教えない）、threshold は 0.70 / 0.75 / 0.80 / 0.85 / 0.90 をスイープ
- segmentation: `min_duration_on=0.3`, `min_duration_off=0.5`, `num_threads=4`
- embedding: `nemo_titanet_large.onnx`（dim 192, `num_threads=4`）。両条件で共通
- consolidation は本番実装（`crates/mojiroku-core/src/diarization/`）どおり
  - 埋め込み窓は `EMBED_MIN=0.3s` / `EMBED_MAX=120s`（中心から拡張・切詰め）
  - anchor floor = `max(15s, 6%×塗り総尺)`
  - 埋め込みと centroid は L2 正規化。turn は最近接 anchor centroid（cosine）へ再割当
- フレーム割当は 100ms 粒度・重なり最大（同点は開始が早い方）

## 生成物は残していない

`results/`（推論結果）・`vad.json`・`metrics.json`・音声・モデルは**リポジトリに置かない**。
測定結果そのものは ADR-0028 の「検証」節が正本である。
