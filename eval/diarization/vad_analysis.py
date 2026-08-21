import json, glob, os
from collections import defaultdict
import numpy as np
import importlib.util
spec = importlib.util.spec_from_file_location("m", "/tmp/purity-ab/metrics.py")
# reuse constants/functions without running main
src = open("/tmp/purity-ab/metrics.py").read().replace("\nmain()\n", "\n")
ns = {}
exec(compile(src, "metrics.py", "exec"), ns)
GT, FRAME, NF = ns["GT"], ns["FRAME"], ns["NF"]
gt_frames, frame_labels, consolidate = ns["gt_frames"], ns["frame_labels"], ns["consolidate"]

vad = json.load(open("/tmp/purity-ab/vad.json"))
speech = np.zeros(NF, dtype=bool)
for a, b in vad:
    speech[int(round(a/FRAME)):int(round(b/FRAME))] = True
g = gt_frames()
gt_mask = (g != "")
print(f"VAD speech frames: {speech.sum()*FRAME:.0f}s / 600s")
print(f"GT frames: {gt_mask.sum()*FRAME:.0f}s ; GT∩speech: {(gt_mask&speech).sum()*FRAME:.0f}s ; "
      f"GT内の非発話: {(gt_mask&~speech).sum()*FRAME:.0f}s")
for spk in "ABC":
    m = g == spk
    print(f"  {spk}: GT {m.sum()*FRAME:.0f}s / うち VAD発話 {(m&speech).sum()*FRAME:.0f}s "
          f"({(m&speech).sum()/m.sum()*100:.0f}%)")

print("\n=== seg モデルの recall/precision（Silero VAD 参照, 600s 全体）===")
print(f"{'条件':<20}{'recall':>8}{'prec':>8}{'塗り秒':>8}")
rows_out = []
for jf in sorted(glob.glob("/tmp/purity-ab/results/*.json")):
    d = json.load(open(jf))
    if abs(d["threshold"] - 0.80) > 1e-6: continue
    lab, _ = frame_labels(d["segments"])
    pred = lab >= 0
    rec = (pred & speech).sum() / speech.sum()
    prec = (pred & speech).sum() / max(pred.sum(), 1)
    print(f"{d['model']+' th=0.80':<20}{rec*100:>7.1f}%{prec*100:>7.1f}%{pred.sum()*FRAME:>8.0f}")

print("\n=== purity（GT∩VAD発話 フレームのみを母数にした第3の規約）===")
hdr = f"{'条件':<24}{'mode':<14}{'clus':>5}{'A':>7}{'B':>7}{'C':>7}{'分離':>6}{'oracle':>8}{'covA':>7}{'covB':>7}{'covC':>7}"
print(hdr); print("-"*len(hdr))
res = []
for jf in sorted(glob.glob("/tmp/purity-ab/results/*.json")):
    d = json.load(open(jf)); z = np.load(jf[:-5]+".npz")
    for mode in ("baseline", "consolidated"):
        segs = d["segments"] if mode == "baseline" else consolidate(d["segments"], z["embs"], z["ok"])[0]
        lab, _ = frame_labels(segs)
        pur = {}; ncl = len(set(s["cluster"] for s in segs))
        for spk in "ABC":
            m = (g == spk) & speech
            tot = int(m.sum()); sub = lab[m]
            cnt = defaultdict(int)
            for x in sub: cnt[x] += 1
            assigned = tot - cnt.get(-1, 0)
            dom = max((c for c in cnt if c != -1), key=lambda c: cnt[c], default=None)
            pur[spk] = dict(tot=tot, dom=dom, p=cnt[dom]/tot if dom is not None else 0.0,
                            cov=assigned/tot, dist={int(c): cnt[c]/tot for c in sorted(cnt, key=lambda c:-cnt[c])})
        doms = [pur[s]["dom"] for s in "ABC"]
        sep = len(set(doms)) == 3 and None not in doms
        tw = sum(pur[s]["tot"] for s in "ABC")
        oracle = sum(pur[s]["p"]*pur[s]["tot"] for s in "ABC")/tw
        print(f"{d['model']+' th='+format(d['threshold'],'.2f'):<24}{mode:<14}{ncl:>5}"
              f"{pur['A']['p']*100:>6.0f}%{pur['B']['p']*100:>6.0f}%{pur['C']['p']*100:>6.0f}%"
              f"{('○' if sep else '×'):>6}{oracle*100:>7.0f}%"
              f"{pur['A']['cov']*100:>6.0f}%{pur['B']['cov']*100:>6.0f}%{pur['C']['cov']*100:>6.0f}%")
        res.append((d['model'], d['threshold'], mode, pur))

print("\n=== leak 詳細（GT∩VAD発話 母数, consolidated のみ）===")
for model, th, mode, pur in res:
    if mode != "consolidated": continue
    print(f"[{model} th={th:.2f}]")
    for spk in "ABC":
        P = pur[spk]
        s = "  ".join(f"{'未割当' if c==-1 else 'spk%02d'%c}={v*100:.0f}%" for c, v in list(P["dist"].items())[:5])
        print(f"  {spk} ({P['tot']*FRAME:.0f}s) dom={P['dom']} purity={P['p']*100:.0f}% | {s}")
