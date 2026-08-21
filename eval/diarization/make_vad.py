"""Script 0: 参照 VAD。Silero VAD で発話区間を出して /tmp/purity-ab/vad.json に落とす。
表② / 区間別 / 表③ の 3 本がこの vad.json を読む。"""
import sherpa_onnx as so, numpy as np, soundfile as sf, json
pcm, sr = sf.read("/tmp/purity-ab/jp_3to13.wav", dtype="float32")
cfg = so.VadModelConfig()
cfg.silero_vad.model = "/tmp/purity-ab/silero_vad.onnx"
cfg.silero_vad.threshold = 0.5
cfg.silero_vad.min_silence_duration = 0.25
cfg.silero_vad.min_speech_duration = 0.25
cfg.sample_rate = sr
vad = so.VoiceActivityDetector(cfg, buffer_size_in_seconds=100)
segs = []; win = 512; i = 0
while i + win <= len(pcm):
    vad.accept_waveform(pcm[i:i+win]); i += win
    while not vad.empty():
        s = vad.front; segs.append((s.start/sr, (s.start+len(s.samples))/sr)); vad.pop()
vad.flush()
while not vad.empty():
    s = vad.front; segs.append((s.start/sr, (s.start+len(s.samples))/sr)); vad.pop()
print("vad segs", len(segs), "speech_s", sum(b-a for a, b in segs))
json.dump(segs, open("/tmp/purity-ab/vad.json", "w"))
