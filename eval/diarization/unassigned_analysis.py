"""未割当（GT では発話、VAD でも発話、なのに話者が付かない）フレームの原因を分類する。

ADR-0028 の purity A/B で pyannote に「未割当 30.2s（8.0%）」が残った。Issue #5 は
「しきい値ではなく、**未割当セグメントの原因を分類してから**手を打つ」としている。
本スクリプトはその分類だけを行う（改善案は出さない）。

母数は **GT ∩ VAD**（`vad_analysis.py` の表②と同じ）。VAD が沈黙と言う区間は最初から
除くので、「GT が大雑把に沈黙を含んでいた」ぶんは未割当に数えない。

**生セグメントと consolidate 後の両方**を出す。ADR-0028 の 8.0% は consolidate 後の値で、
生では数字が違う。どちらか一方だけを見ると、統合が何を吸収しているかが分からない。

    RESULT=results/pyannote_th0.80.json ./venv/bin/python unassigned_analysis.py
"""

import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from metrics import FRAME, GT, NF, consolidate, frame_labels, gt_frames  # noqa: E402

RESULT = os.environ.get("RESULT", "results/pyannote_th0.80.json")
VAD = os.environ.get("VAD", "vad.json")
SHORT_S = 0.5      # 「短い」の境目。短い発話ほど埋め込みが不安定になる
NEAR_TURN_S = 0.3  # 話者交替の近傍とみなす距離。重なり・立ち上がりはここに集中するはず


def runs(flags):
    """連続する True を (開始フレーム, 終了フレーム) の列にまとめる。"""
    out, start = [], None
    for i, v in enumerate(flags):
        if v and start is None:
            start = i
        elif not v and start is not None:
            out.append((start, i))
            start = None
    if start is not None:
        out.append((start, len(flags)))
    return out


def report(name, labels, g, speech, turns, gt_edges):
    denom = (g != "") & speech
    unassigned = denom & (labels == -1)
    n_d, n_u = int(denom.sum()), int(unassigned.sum())
    print(f"── {name} ──")
    print(f"  母数（GT ∩ VAD）: {n_d * FRAME:.1f} s")
    print(f"  未割当          : {n_u * FRAME:.1f} s（{100 * n_u / max(1, n_d):.1f}%）")

    rs = runs(unassigned.tolist())
    print(f"  連続区間        : {len(rs)} 本")

    by_len = defaultdict(lambda: [0, 0.0])
    near_turn, at_edge = [0, 0.0], [0, 0.0]
    by_spk = defaultdict(float)
    for a, b in rs:
        dur = (b - a) * FRAME
        k = (f"< {SHORT_S} s" if dur < SHORT_S else
             "0.5–1 s" if dur < 1.0 else "1–3 s" if dur < 3.0 else "3 s 以上")
        by_len[k][0] += 1
        by_len[k][1] += dur
        if turns and min(abs(a - t) for t in turns) * FRAME <= NEAR_TURN_S:
            near_turn[0] += 1
            near_turn[1] += dur
        if a in gt_edges or (b - 1) in gt_edges:
            at_edge[0] += 1
            at_edge[1] += dur
        by_spk[g[a]] += dur

    print("  長さ別:", "  ".join(
        f"{k}={by_len[k][0]}本/{by_len[k][1]:.1f}s" for k in
        ["< 0.5 s", "0.5–1 s", "1–3 s", "3 s 以上"] if by_len[k][0]))
    print(f"  話者交替の近傍（±{NEAR_TURN_S}s）: {near_turn[0]} 本 / {near_turn[1]:.1f} s")
    print(f"  GT 区間の端に接する          : {at_edge[0]} 本 / {at_edge[1]:.1f} s")
    print("  GT 話者別:", "  ".join(f"{s}={by_spk[s]:.1f}s" for s in sorted(by_spk) if s))

    # 話者ごとの「自分の発話のうち何%が未割当か」。総量だけ見ると発話量の多い話者が
    # 常に大きく見えるので、割合で正規化する。
    print("  話者別の未割当率:", "  ".join(
        f"{s}={100 * int((unassigned & (g == s)).sum()) / max(1, int((denom & (g == s)).sum())):.1f}%"
        for s in sorted({x for x in g if x})))

    longest = sorted(rs, key=lambda r: r[1] - r[0], reverse=True)[:5]
    print("  長い順:", "  ".join(
        f"{a * FRAME:.1f}–{b * FRAME:.1f}s({(b - a) * FRAME:.1f}s,{g[a]})" for a, b in longest))
    print()


def main() -> int:
    g = gt_frames()
    speech = np.zeros(NF, dtype=bool)
    for a, b in json.load(open(VAD)):
        speech[int(round(a / FRAME)):int(round(b / FRAME))] = True

    turns = [i for i in range(1, NF) if g[i] != g[i - 1] and g[i] and g[i - 1]]
    gt_edges = set()
    for a, b, _ in GT:
        gt_edges.add(int(round(a / FRAME)))
        gt_edges.add(int(round(b / FRAME)) - 1)

    d = json.load(open(RESULT))
    z = np.load(RESULT[:-5] + ".npz")
    print(f"モデル {d['model']} / threshold {d['threshold']} / クラスタ {d['n_clusters']}\n")

    report("生セグメント", frame_labels(d["segments"])[0], g, speech, turns, gt_edges)
    cons = consolidate(d["segments"], z["embs"], z["ok"])[0]
    report("consolidate 後（ADR-0028 の表と同じ）", frame_labels(cons)[0], g, speech, turns, gt_edges)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
