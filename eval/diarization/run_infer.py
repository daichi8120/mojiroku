"""Script 1: inference only. Dumps raw sherpa segments + per-turn TitaNet embeddings."""
import json, os, sys, time
import numpy as np, soundfile as sf
import sherpa_onnx as so

MODELS = os.path.expanduser("~/Library/Application Support/com.daichi0812.mojiroku/models")
EMB = os.path.join(MODELS, "nemo_titanet_large.onnx")
SEG = {
    "reverb": os.path.join(MODELS, "sherpa-reverb-diarization-v1.onnx"),
    "pyannote": "/tmp/purity-ab/sherpa-onnx-pyannote-segmentation-3-0/model.onnx",
}
WAV = "/tmp/purity-ab/jp_3to13.wav"
OUT = "/tmp/purity-ab/results"
NTHREADS = 4
EMBED_MIN_S, EMBED_MAX_S = 0.3, 120.0

def embed_window(start_s, end_s, sr, n):
    a = int(start_s * sr); b = int(end_s * sr)
    min_len = int(EMBED_MIN_S * sr); max_len = int(EMBED_MAX_S * sr)
    if b - a < min_len:
        c = (a + b) // 2; a = c - min_len // 2; b = a + min_len
    elif b - a > max_len:
        c = (a + b) // 2; a = c - max_len // 2; b = a + max_len
    a = max(0, min(a, n)); b = max(0, min(b, n))
    return (a, b) if b > a else None

def l2(v):
    nrm = float(np.sqrt(np.sum(v * v)))
    return v / nrm if nrm > 0 else v

def main():
    os.makedirs(OUT, exist_ok=True)
    pcm, sr = sf.read(WAV, dtype="float32")
    assert sr == 16000 and pcm.ndim == 1, (sr, pcm.shape)
    audio_s = len(pcm) / sr
    print(f"audio {audio_s:.1f}s sr={sr}", flush=True)

    ext = so.SpeakerEmbeddingExtractor(
        so.SpeakerEmbeddingExtractorConfig(model=EMB, num_threads=NTHREADS))
    dim = ext.dim
    print("emb dim", dim, flush=True)

    conds = [("reverb", 0.70), ("reverb", 0.75), ("reverb", 0.80), ("reverb", 0.85), ("reverb", 0.90),
             ("pyannote", 0.70), ("pyannote", 0.75), ("pyannote", 0.80), ("pyannote", 0.85), ("pyannote", 0.90)]
    for name, th in conds:
        tag = f"{name}_th{th:.2f}"
        cfg = so.OfflineSpeakerDiarizationConfig(
            segmentation=so.OfflineSpeakerSegmentationModelConfig(
                pyannote=so.OfflineSpeakerSegmentationPyannoteModelConfig(model=SEG[name]),
                num_threads=NTHREADS),
            embedding=so.SpeakerEmbeddingExtractorConfig(model=EMB, num_threads=NTHREADS),
            clustering=so.FastClusteringConfig(num_clusters=-1, threshold=th),
            min_duration_on=0.3, min_duration_off=0.5)
        sd = so.OfflineSpeakerDiarization(cfg)
        assert sd.sample_rate == sr
        t0 = time.time()
        res = sd.process(pcm).sort_by_start_time()
        proc = time.time() - t0
        segs = [{"start": float(s.start), "end": float(s.end), "cluster": int(s.speaker)} for s in res]
        del sd

        # per-turn embeddings (same rules as Rust embed_segment)
        t1 = time.time()
        embs = np.zeros((len(segs), dim), dtype=np.float32)
        ok = np.zeros(len(segs), dtype=bool)
        for i, s in enumerate(segs):
            w = embed_window(s["start"], s["end"], sr, len(pcm))
            if w is None:
                continue
            st = ext.create_stream()
            st.accept_waveform(sr, pcm[w[0]:w[1]])
            st.input_finished()
            if not ext.is_ready(st):
                continue
            e = np.asarray(ext.compute(st), dtype=np.float32)
            embs[i] = l2(e); ok[i] = True
        emb_s = time.time() - t1
        np.savez(os.path.join(OUT, tag + ".npz"), embs=embs, ok=ok)
        with open(os.path.join(OUT, tag + ".json"), "w") as f:
            json.dump({"model": name, "threshold": th, "segments": segs,
                       "n_clusters": len(set(x["cluster"] for x in segs)),
                       "process_s": proc, "embed_s": emb_s, "audio_s": audio_s}, f)
        print(f"{tag}: segs={len(segs)} clusters={len(set(x['cluster'] for x in segs))} "
              f"process={proc:.1f}s ({proc/audio_s:.3f}xRT) embed={emb_s:.1f}s "
              f"emb_ok={int(ok.sum())}/{len(segs)}", flush=True)

main()
