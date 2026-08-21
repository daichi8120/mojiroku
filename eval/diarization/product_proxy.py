"""merge.rs 相当の粒度で評価: 発話単位(VADセグメント=STTセグメントの代理)に
最大重なりのクラスタを割り当ててから purity を測る。"""
import json
from collections import defaultdict
import numpy as np
src = open("/tmp/purity-ab/metrics.py").read().replace("\nmain()\n", "\n")
ns = {}; exec(compile(src, "m", "exec"), ns)
GT, FRAME, NF = ns["GT"], ns["FRAME"], ns["NF"]
gt_frames, frame_labels, consolidate = ns["gt_frames"], ns["frame_labels"], ns["consolidate"]
vad = json.load(open("/tmp/purity-ab/vad.json"))
speech = np.zeros(NF, bool)
for a, b in vad: speech[int(round(a/FRAME)):int(round(b/FRAME))] = True
g = gt_frames()

def load(model, th):
    jf = f"/tmp/purity-ab/results/{model}_th{th:.2f}.json"
    d = json.load(open(jf)); z = np.load(jf[:-5]+".npz")
    return consolidate(d["segments"], z["embs"], z["ok"])[0]

print(f"{'条件':<16}{'A':>8}{'B':>8}{'C':>8}{'加重':>8}{'話者無し発話':>12}")
for model in ("reverb", "pyannote"):
    for th in (0.70, 0.75, 0.80, 0.85, 0.90):
        segs = load(model, th)
        lab = np.full(NF, -1, int)
        # 発話(VADセグメント)ごとに、重なり最大のクラスタを発話全体へ付与（merge.rs 相当）
        nolabel = 0.0
        for a, b in vad:
            ov = defaultdict(float)
            for s in segs:
                o = min(s["end"], b) - max(s["start"], a)
                if o > 0: ov[s["cluster"]] += o
            i0, i1 = int(round(a/FRAME)), int(round(b/FRAME))
            if ov: lab[i0:i1] = max(ov, key=ov.get)
            else: nolabel += b - a
        dm = {}
        for spk in "ABC":
            m = (g == spk) & speech
            cnt = defaultdict(int)
            for x in lab[m]: cnt[x] += 1
            dm[spk] = max((c for c in cnt if c != -1), key=lambda c: cnt[c])
        ps, tots = [], []
        for spk in "ABC":
            m = (g == spk) & speech
            ps.append((lab[m] == dm[spk]).sum()/m.sum()); tots.append(int(m.sum()))
        w = sum(p*t for p, t in zip(ps, tots))/sum(tots)
        sep = len(set(dm.values())) == 3
        print(f"{model+' th='+format(th,'.2f'):<16}{ps[0]*100:>7.0f}%{ps[1]*100:>7.0f}%{ps[2]*100:>7.0f}%"
              f"{w*100:>7.0f}%{nolabel:>11.1f}s   分離{'○' if sep else '×'}")
