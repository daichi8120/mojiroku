"""Script 2: frame metrics + consolidation replica of crates/mojiroku-core/src/diarization/mod.rs"""
import json, glob, os, sys
from collections import defaultdict
import numpy as np

GT = [
    (0, 20, "A"), (20, 60, "B"), (60, 72, "A"), (72, 82, "B"),
    (96, 133, "C"), (135, 405, "B"), (410, 465, "C"), (465, 480, "A"),
    (480, 495, "B"), (495, 525, "A"), (525, 555, "B"), (560, 585, "A"),
]
FRAME = 0.1
AUDIO_S = 600.0
NF = int(round(AUDIO_S / FRAME))
ANCHOR_MIN_SECONDS, ANCHOR_MIN_FRACTION = 15.0, 0.06

def gt_frames():
    g = np.array([""] * NF, dtype=object)
    for a, b, spk in GT:
        g[int(round(a / FRAME)):int(round(b / FRAME))] = spk
    return g

def frame_labels(segs, key="cluster"):
    """assign each 100ms frame to the covering segment with max overlap (tie -> earlier start)."""
    best = np.full(NF, -1.0)          # overlap seconds
    lab = np.full(NF, -1, dtype=int)
    ncov = np.zeros(NF, dtype=int)
    for s in sorted(segs, key=lambda x: x["start"]):
        i0 = max(0, int(np.floor(s["start"] / FRAME)))
        i1 = min(NF, int(np.ceil(s["end"] / FRAME)))
        for i in range(i0, i1):
            ov = min(s["end"], (i + 1) * FRAME) - max(s["start"], i * FRAME)
            if ov <= 0:
                continue
            ncov[i] += 1
            if ov > best[i] + 1e-9:
                best[i] = ov; lab[i] = s[key]
    return lab, ncov

def consolidate(segs, embs, ok, anchor_floor_abs=ANCHOR_MIN_SECONDS,
                anchor_frac=ANCHOR_MIN_FRACTION):
    dim = embs.shape[1]
    dur = defaultdict(float); acc = defaultdict(lambda: np.zeros(dim, dtype=np.float64))
    for i, s in enumerate(segs):
        w = s["end"] - s["start"]
        dur[s["cluster"]] += w
        if ok[i]:
            acc[s["cluster"]] += embs[i].astype(np.float64) * w
    centroid = {}
    for c, v in acc.items():
        n = np.linalg.norm(v)
        centroid[c] = (v / n if n > 0 else v).astype(np.float32)
    total = sum(dur.values())
    floor = max(anchor_frac * total, anchor_floor_abs)
    anchors = sorted([c for c in sorted(centroid) if dur[c] >= floor],
                     key=lambda c: -dur[c])
    if not anchors and centroid:
        anchors = [max(sorted(centroid), key=lambda c: dur[c])]
    if not anchors:
        return [], {}, floor, total
    largest = anchors[0]
    def nearest(v):
        return max(anchors, key=lambda c: float(np.dot(v, centroid[c])))
    out = []
    for i, s in enumerate(segs):
        if ok[i]:
            lab = nearest(embs[i])
        elif s["cluster"] in centroid:
            lab = nearest(centroid[s["cluster"]])
        else:
            lab = largest
        out.append({"start": s["start"], "end": s["end"], "cluster": lab})
    return out, {"anchors": anchors, "dur": dict(dur)}, floor, total

def analyse(segs, tag):
    g = gt_frames()
    lab, ncov = frame_labels(segs)
    rows = {}
    painted = float(sum(s["end"] - s["start"] for s in segs))
    for spk in ["A", "B", "C"]:
        m = (g == spk)
        tot = int(m.sum())
        sub = lab[m]
        cnt = defaultdict(int)
        for x in sub:
            cnt[x] += 1
        assigned = tot - cnt.get(-1, 0)
        dom = max((c for c in cnt if c != -1), key=lambda c: cnt[c], default=None)
        rows[spk] = {
            "gt_frames": tot, "gt_s": tot * FRAME,
            "dominant": (int(dom) if dom is not None else None),
            "purity_all": (cnt[dom] / tot if dom is not None and tot else 0.0),
            "purity_assigned": (cnt[dom] / assigned if dom is not None and assigned else 0.0),
            "coverage": assigned / tot if tot else 0.0,
            "dist": {int(c): cnt[c] / tot for c in sorted(cnt, key=lambda c: -cnt[c])},
        }
    doms = [rows[s]["dominant"] for s in "ABC"]
    sep = len(set(doms)) == 3 and None not in doms
    tw = sum(rows[s]["gt_frames"] for s in "ABC")
    oracle = sum(rows[s]["purity_all"] * rows[s]["gt_frames"] for s in "ABC") / tw
    return {"tag": tag, "n_clusters": len(set(s["cluster"] for s in segs)),
            "rows": rows, "sep": sep, "oracle": oracle,
            "painted_s": painted, "overlap_frames": int((ncov >= 2).sum()),
            "gt_total_s": tw * FRAME}

def main():
    results = []
    for jf in sorted(glob.glob("/tmp/purity-ab/results/*.json")):
        d = json.load(open(jf))
        tag = os.path.basename(jf)[:-5]
        z = np.load(jf[:-5] + ".npz")
        embs, ok = z["embs"], z["ok"]
        base = analyse(d["segments"], tag + " baseline")
        base.update(model=d["model"], threshold=d["threshold"], mode="baseline",
                    process_s=d["process_s"], embed_s=d["embed_s"], audio_s=d["audio_s"])
        cons_segs, info, floor, total = consolidate(d["segments"], embs, ok)
        cons = analyse(cons_segs, tag + " consolidated")
        cons.update(model=d["model"], threshold=d["threshold"], mode="consolidated",
                    process_s=d["process_s"], embed_s=d["embed_s"], audio_s=d["audio_s"],
                    anchor_floor=floor, painted_total=total)
        results.extend([base, cons])
    json.dump(results, open("/tmp/purity-ab/metrics.json", "w"), indent=1, default=lambda o: int(o) if hasattr(o,"item") else str(o))

    hdr = f"{'条件':<26}{'mode':<14}{'clus':>5}{'A':>7}{'B':>7}{'C':>7}{'分離':>6}{'oracle':>8}{'covA':>7}{'covB':>7}{'covC':>7}{'塗り秒':>8}{'重複f':>6}"
    print(hdr); print("-" * len(hdr))
    for r in results:
        R = r["rows"]
        print(f"{r['model']+' th='+format(r['threshold'],'.2f'):<26}{r['mode']:<14}{r['n_clusters']:>5}"
              f"{R['A']['purity_all']*100:>6.0f}%{R['B']['purity_all']*100:>6.0f}%{R['C']['purity_all']*100:>6.0f}%"
              f"{('○' if r['sep'] else '×'):>6}{r['oracle']*100:>7.0f}%"
              f"{R['A']['coverage']*100:>6.0f}%{R['B']['coverage']*100:>6.0f}%{R['C']['coverage']*100:>6.0f}%"
              f"{r['painted_s']:>8.0f}{r['overlap_frames']:>6}")

    print("\n=== leak 詳細 (GT フレーム比。-1 = 未割当) ===")
    for r in results:
        print(f"\n[{r['model']} th={r['threshold']:.2f} {r['mode']}] clusters={r['n_clusters']}"
              + (f" anchor_floor={r['anchor_floor']:.1f}s" if r['mode'] == 'consolidated' else ""))
        for spk in "ABC":
            R = r["rows"][spk]
            items = [(c, v) for c, v in R["dist"].items()]
            s = "  ".join(f"{'未割当' if c==-1 else 'spk%02d'%c}={v*100:.0f}%" for c, v in items[:6])
            print(f"  {spk} ({R['gt_s']:.0f}s) dom={R['dominant']} purity_all={R['purity_all']*100:.0f}% "
                  f"purity_assigned={R['purity_assigned']*100:.0f}% | {s}")

if __name__ == "__main__":
    main()
