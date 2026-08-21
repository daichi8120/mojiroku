import json, glob
from collections import defaultdict
import numpy as np
src = open("/tmp/purity-ab/metrics.py").read().replace("\nmain()\n", "\n")
ns = {}; exec(compile(src, "m", "exec"), ns)
GT, FRAME, NF = ns["GT"], ns["FRAME"], ns["NF"]
gt_frames, frame_labels, consolidate = ns["gt_frames"], ns["frame_labels"], ns["consolidate"]
vad = json.load(open("/tmp/purity-ab/vad.json"))
speech = np.zeros(NF, dtype=bool)
for a, b in vad: speech[int(round(a/FRAME)):int(round(b/FRAME))] = True
g = gt_frames()

def load(model, th):
    jf = f"/tmp/purity-ab/results/{model}_th{th:.2f}.json"
    d = json.load(open(jf)); z = np.load(jf[:-5]+".npz")
    return consolidate(d["segments"], z["embs"], z["ok"])[0]

conds = {"reverb@0.80": load("reverb", 0.80), "pyannote@0.80": load("pyannote", 0.80)}
labs = {k: frame_labels(v)[0] for k, v in conds.items()}
dom = {}
for k, lab in labs.items():
    dom[k] = {}
    for spk in "ABC":
        m = (g == spk) & speech
        cnt = defaultdict(int)
        for x in lab[m]: cnt[x] += 1
        dom[k][spk] = max((c for c in cnt if c != -1), key=lambda c: cnt[c])
inv = {k: {c: s for s, c in v.items()} for k, v in dom[list(dom)[0]].items()} if False else None

print("=== 割当済みフレーム限定の purity（GT∩VAD発話, consolidated）===")
print(f"{'条件':<16}{'A':>8}{'B':>8}{'C':>8}{'加重':>8}   | 未割当秒(A/B/C)")
for k, lab in labs.items():
    ps, tots, miss = [], [], []
    for spk in "ABC":
        m = (g == spk) & speech; tot = int(m.sum())
        cnt = defaultdict(int)
        for x in lab[m]: cnt[x] += 1
        asg = tot - cnt.get(-1, 0)
        ps.append(cnt[dom[k][spk]]/asg); tots.append(tot); miss.append(cnt.get(-1,0)*FRAME)
    w = sum(p*t for p, t in zip(ps, tots))/sum(tots)
    print(f"{k:<16}{ps[0]*100:>7.0f}%{ps[1]*100:>7.0f}%{ps[2]*100:>7.0f}%{w*100:>7.0f}%   | "
          f"{miss[0]:.0f}s/{miss[1]:.0f}s/{miss[2]:.0f}s (計 {sum(miss):.0f}s)")

print("\n=== GT 区間ごとの内訳（GT∩VAD発話フレーム。値は正解率=主クラスタ一致率）===")
print(f"{'区間':<16}{'GT':<4}{'発話s':>7}{'reverb':>9}{'pyannote':>10}   差")
for a, b, spk in GT:
    m = np.zeros(NF, bool); m[int(round(a/FRAME)):int(round(b/FRAME))] = True
    m &= speech
    if m.sum() == 0:
        print(f"{f'{a}-{b}':<16}{spk:<4}{0:>7.0f}  (VAD発話なし)"); continue
    accs = {}
    for k, lab in labs.items():
        accs[k] = (lab[m] == dom[k][spk]).sum()/m.sum()
    d = accs['pyannote@0.80'] - accs['reverb@0.80']
    flag = "  <<<" if d < -0.10 else ("  >>>" if d > 0.10 else "")
    print(f"{f'{a}-{b}':<16}{spk:<4}{m.sum()*FRAME:>7.0f}{accs['reverb@0.80']*100:>8.0f}%"
          f"{accs['pyannote@0.80']*100:>9.0f}%{d*100:>+7.0f}pt{flag}")

print("\n=== 誤り内訳（GT∩VAD発話, 秒）===")
for k, lab in labs.items():
    tot_conf = tot_un = 0.0
    for spk in "ABC":
        m = (g == spk) & speech
        cnt = defaultdict(int)
        for x in lab[m]: cnt[x] += 1
        un = cnt.get(-1, 0)*FRAME
        conf = sum(v for c, v in cnt.items() if c != -1 and c != dom[k][spk])*FRAME
        tot_un += un; tot_conf += conf
        print(f"  {k:<16}{spk}: 他話者へ誤帰属 {conf:>5.1f}s / 未割当 {un:>5.1f}s")
    print(f"  {k:<16}計: 誤帰属 {tot_conf:.1f}s ({tot_conf/377.7*100:.1f}%) / 未割当 {tot_un:.1f}s ({tot_un/377.7*100:.1f}%)\n")
