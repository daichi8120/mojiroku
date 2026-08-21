# 0028. 話者分離 segmentation を pyannote-segmentation-3.0 に差し替え（reverb-v1 のライセンス撤回）

- ステータス: 採用（2026-08-21 決定。[ADR-0009](./ADR-0009_話者分離スパイク結果.md) の「segmentation = reverb-diarization-v1」を **supersede**）。**出荷ゲートの purity A/B は 2026-08-21 に実施し PASS**
- 日付: 2026-08-21
- 関連: [ADR-0004](./ADR-0004_話者分離はsherpa-onnxで実現.md)（sherpa-onnx 路線）/ [ADR-0009](./ADR-0009_話者分離スパイク結果.md)（差し替え元の推奨構成・「MIT」という誤記の出どころ）/ [ADR-0018](./ADR-0018_話者ライブラリの声紋照合.md)（embedding 側。本 ADR では変更しない）/ [ADR-0027](./ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md)（オープンソース化でライセンス整合の点検が必要になった）

## Context

話者分離の segmentation モデルに使っている `reverb-diarization-v1` が、**Rev Model Non-Production License** であることが判明した。

- §3.2 が Models / Derivatives / **Outputs** の使用を非商用・Non-Production に限定する。
- §4.2 で、その制限が **Outputs にも及ぶ**。mojiroku の Outputs は**議事録そのもの**である。
- §1.1 が「入手元を問わず適用」と定めるため、**取得経路を変えても回避できない**（k2-fsa の ONNX 変換版を DL していても同じ）。
- 定義上、**会社員が業務の会議に使う用途は Personal にも Non-Production にも当たらない**。つまり mojiroku の想定利用そのものが許諾外になる。

[ADR-0009](./ADR-0009_話者分離スパイク結果.md) は推奨構成に「reverb-diarization-v1（9.5MB, **MIT**）」と書いていたが、**この MIT という記載が誤り**だった。ADR-0027 でオープンソース化を決め、`NOTICE` の第三者ライセンスを点検する過程で発覚した。

## Decision

**segmentation を `pyannote/segmentation-3.0` の ONNX 変換版（MIT License, Copyright (c) 2022 CNRS）に差し替える。**

- tarball: `https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2`
- 展開後の `model.onnx` は 5,992,913 bytes（f32。int8 は 1,540,506）。ローカル名は `sherpa-pyannote-segmentation-3-0.onnx`。
- ONNX メタデータの幾何は reverb-v1 と一致する（`window_size=160000` / `receptive_field_size=991` / `receptive_field_shift=270` / `model_type=pyannote-segmentation-3.0`）。したがって **`diarization/` 側のコード変更は不要**。
- **embedding（`nemo_en_titanet_large.onnx`）は変更しない。**

### 選択肢がこれしかない

sherpa-onnx の `OfflineSpeakerSegmentationModelConfig` は **`pyannote` フィールドしか持たない**（crate ソースで確認）。Sortformer / DiariZen / MSDD はローダーを自作しない限り載らない。
`speaker-segmentation-models` タグのアセットは 3 本で、うち reverb v1 / v2 は同じ Rev ライセンスである。**残るのは pyannote-segmentation-3.0 だけ**。

### `speaker-diarization-community-1` は無意味

`pyannote/speaker-diarization-community-1`（CC-BY-4.0）に乗り換える案もあったが、**その segmentation の重みは seg-3.0 とバイト一致する**（実測）。改善はクラスタリング側にあり、**それは sherpa-onnx が実装していない**。乗り換えても得るものが無い。

### 商用ライセンスの打診は行わない

Rev への商用ライセンス問い合わせは選ばない。**費用を払わない方針**（サーバー維持費 $0 の北極星と同じ理由）。

## 検証

出荷ゲートの purity A/B を **2026-08-21 に実施し、PASS した**（ADR-0009 と同一の日本語会議・同一の 600 秒抜粋）。
再現手順と実行スクリプトは [`eval/diarization/`](../../eval/diarization/) にある。

### まず harness が ADR-0009 を再現することを確かめた

pyannote を測る前に、同じ harness で ADR-0009 の既知値を再現した。ここが合わなければ以降の数字は読めない。

| | ADR-0009 の記載 | 今回の実測 |
|---|---|---|
| reverb th=0.80 baseline のセグメント / クラスタ | 34 / 11 | 34 / 11 |
| baseline purity A/B/C | 75 / 72 / 97% | 75 / 73 / 97% |
| baseline oracle 整合 | 77% | 77% |
| baseline 被覆 | 98–99% | 99 / 98 / 98% |
| consolidation 後 A/B/C | 94 / 79 / 97% | 94 / 80 / 97% |

**全項目 1pt 以内で再現。** なお consolidation は ADR-0009 のスパイク（絶対 30s floor）ではなく
**本番実装の `max(15s, 6%×塗り総尺)`** を使った。reverb では floor=35.6s となり、ADR-0009 と同じ 3 話者に着地する。

### 表①: ADR-0009 の規約そのまま（purity の母数 = GT の全フレーム）

| 手法 | クラスタ数 | A | B | C | 分離 | oracle 整合 | 被覆 A/B/C |
|---|---|---|---|---|---|---|---|
| reverb 0.80 baseline | 11 | 75% | 73% | 97% | ○ | 77% | 99/98/98% |
| reverb 0.80 consolidation 後 | 3 | 94% | 80% | 97% | ○ | 85% | 99/98/98% |
| pyannote 0.80 baseline | 11 | 58% | 57% | 79% | ○ | 61% | 72/64/79% |
| pyannote 0.80 consolidation 後 | 3 | 69% | 62% | 79% | ○ | 66% | 72/64/79% |

**この規約では pyannote が大きく劣って見える。だが差の正体は purity ではなく被覆である。**
GT は「大雑把な手動区間」なので、区間の中に沈黙が入っている。Silero VAD で測ると
**GT 559s のうち 181s が非発話**だった。reverb は 600s 中 **593s を塗る**ため、その沈黙まで被覆として得点していた。

Silero VAD を参照にした recall / precision は次のとおり。

| 条件 | recall | precision | 塗り総尺 |
|---|---|---|---|
| reverb 0.80 | 99.7% | 67.3% | 589s |
| pyannote 0.80 | 92.0% | 91.9% | 398s |

pyannote の塗り 398s は、VAD が発話と認めた 398s とほぼ一致する。

### 表②: 実発話フレームのみ（GT ∩ VAD 発話、母数 378s）

| 手法 | クラスタ数 | A | B | C | 分離 | oracle 整合 | 被覆 A/B/C |
|---|---|---|---|---|---|---|---|
| reverb 0.80 baseline | 11 | 83% | 88% | 99% | ○ | 89% | 100/100/100% |
| reverb 0.80 consolidation 後 | 3 | 100% | 93% | 99% | ○ | 95% | 100/100/100% |
| pyannote 0.80 baseline | 11 | 79% | 81% | 97% | ○ | 84% | 96/89/97% |
| pyannote 0.80 consolidation 後 | 3 | 93% | 88% | 97% | ○ | 90% | 96/89/97% |

表①と表②を両方載せるのは、どちらか片方だけでは判断を誤るからである。
表①だけなら「大幅劣化」に見え、表②だけなら「指標を都合よく変えた」に見える。

### threshold スイープ（consolidation 後・表②の規約）

| threshold | reverb A/B/C（oracle） | pyannote A/B/C（oracle） |
|---|---|---|
| 0.70 | 100/97/99（98%） | 93/87/97（90%） |
| 0.75 | 100/96/99（97%） | 93/88/97（90%） |
| 0.80 | 100/93/99（95%） | 93/88/97（90%） |
| 0.85 | 100/93/99（95%） | 93/88/97（90%） |
| 0.90 | 100/93/99（95%） | 93/88/97（90%） |

**pyannote は consolidation 後、threshold にほぼ不感である**（0.70–0.90 で 93/88/97 が動かない）。
分離は全条件で ○、クラスタ数は全条件で 3、anchor floor は 23.5s。**retune の利益はゼロ。**

### 誤りの内訳（判定の核心）

| consolidation 後 0.80 | 他話者へ誤帰属 | 未割当 |
|---|---|---|
| reverb | 15.9s（4.2%） | 1.3s（0.3%） |
| pyannote | 6.8s（1.8%） | 30.2s（8.0%） |

内訳を見ると、reverb は B の 15.6s が誤帰属、pyannote は B の 25.5s が未割当である。

**「誰の発言かを間違える」量は pyannote が半分以下。** 代わりに未割当が増える。
議事録という製品では、**発言者の取り違えのほうが空欄より害が大きい。**

### 表③: 製品粒度のプロキシ（`merge.rs` 相当）

`merge.rs` はフレームではなく**発話セグメント**に時間重なりで話者を付ける。
VAD セグメントを STT セグメントの代理として同じ割当をすると、次のようになる。

| 条件 | A | B | C | 加重 | 話者が付かない発話 |
|---|---|---|---|---|---|
| reverb 0.80 | 100% | 96% | 100% | 97% | 0.0s |
| pyannote 0.80 | 97% | 98% | 100% | 98% | 1.3s |

**製品が実際に出す粒度では差が消える**（むしろ pyannote が +1pt）。
フレーム単位の未割当 30.2s は、発話単位に上げると 1.3s まで回収される。全 threshold で同じ結果だった。

### 区間別の得失（GT ∩ VAD 発話）

- 劣る: 135–405s（B, 167s）98% → 86%（未割当 26s がここに集中）／0–20s（A, 実発話 11s）100% → 82%／465–480s（A, 実発話 9s）98% → 83%
- 勝つ: 72–82s（B, 実発話 4s）0% → 95%（reverb はこの区間を丸ごと別話者に誤帰属している）／525–555s（B, 実発話 28s）73% → 87%

### 性能

0.048×RT（reverb は 0.069×RT）。600s の音声で 29.1s vs 43.0s。
consolidation の埋め込みは +2.8s / +3.6s。f32 model.onnx は 6.0MB vs 9.5MB。

### 判定: PASS

- 分離（A/B/C が別クラスタ）は全 threshold で維持。ADR-0009 が最重要とした性質は無傷。
- consolidation は人数を教えずに同じく 3 クラスタへ収束する。`DEFAULT_THRESHOLD=0.80` のままで動く。
- 誤帰属は reverb の半分以下（1.8% vs 4.2%）。
- 製品粒度では 98% vs 97% で同等以上。話者が付かない発話は 1.3s。
- 速く、小さい。

**ただし「pyannote が全面的に優れる」わけではない。**
フレーム単位の oracle 整合は **reverb 95% に対し pyannote 90% で、reverb が 5pt 上**である。
入れ替わったのは**誤りの種類**で、pyannote は取り違えを減らす代わりに未割当を増やす。
製品の粒度でその未割当がほぼ回収されること、そして取り違えのほうが害が大きいことが、PASS の根拠である。

## 影響・制約

- **出荷ゲート（purity の再スパイク）は実施済みで PASS**（2026-08-21）。結果は「検証」節、再現資材は [`eval/diarization/`](../../eval/diarization/)。
- **多人数会議では約 3 割のセグメントで割当先が変わる**（決定時に別途比較した実録音 2 本で、最良 1:1 対応のセグメント一致率は 99.4% と 69.6%）。**どちらが正しいかは正解ラベルなしでは判定できない。**
- 公開されている唯一の日本語ヘッドツーヘッド（arXiv 2509.26177, CALLHOME-Japanese）で pyannote 3.1 パイプラインは DER 28.8% と振るわない。ただしこれは**パイプラインの数字**で、mojiroku の構成（sherpa のしきい値クラスタリング + 自前 consolidation）とは別物である。**reverb 側には日本語の公開数値が一切ない**ため、日本語での優劣は**どちら向きにも公開証拠がない**。
- 既存ユーザーのキャッシュに旧 `sherpa-reverb-diarization-v1.onnx`（9.5MB）が孤児として残る（実害なし）。
- 既存録音の話者分離結果は DB に残る。**再分離しない限り reverb 産**である。
- `th=0.80` と consolidation の anchor 定数は reverb 前提のチューニング値だった。**再スパイクで確認した結果、変更不要**（0.70–0.90 のスイープで pyannote は不感。anchor floor は 23.5s で 3 話者に収束する）。
- **embedding（TitaNet）は変更しないため、話者ライブラリの声紋照合（[ADR-0018](./ADR-0018_話者ライブラリの声紋照合.md)）は無傷。**

### ゲートを通したうえで残る懸念

- **GT が大雑把**（[ADR-0009](./ADR-0009_話者分離スパイク結果.md) 自身の注記）。今回それが表①の解釈を歪める直接の原因になった。**厳密な DER は依然として未測定。**
- **音声は 1 本のみ。** ADR-0009 と同一条件で比べることを優先した。
- **Silero VAD 自体が完全な参照ではない。** 表②の未割当 30.2s は「VAD が発話と認めたうえで pyannote が落とした分」であり、フレーム単位では実在の取りこぼしである。
- **表③は代理である。** VAD セグメント ≒ STT セグメントであって、whisper の実出力ではない。実 transcript での再確認は残る。
- **135–405s の未割当 26s は単一の長い独演区間に集中している。** 長い独演で相槌や小声が落ちる傾向がないかは、別の音声で見る価値がある。
- 重なり発話のフレームは pyannote 41 / reverb 71（100ms フレーム）。
