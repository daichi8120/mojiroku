import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from mixed import evaluate, passes, prepare, read_float_wav, write_float_wav


def passage(name, language, text, start, end):
    return {"id": name, "language": language, "reference": text,
            "start_ms": start, "end_ms": end}


class MixedFixturesTests(unittest.TestCase):
    def test_composition_preserves_samples_and_one_second_gaps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = []
            originals = {}
            for lang in ("ja", "en"):
                for index in range(3):
                    name = f"{lang}-{index}.wav"
                    pcm = struct.pack("<16f", *([0.1 * (index + 1)] * 16))
                    originals[lang, index] = pcm
                    write_float_wav(root / name, pcm)
                    records.append({"id": name, "language": lang, "audio": name,
                                    "reference": "example", "duration_seconds": 0.001,
                                    "audio_sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()})
            source = root / "sources.json"
            source.write_text(json.dumps({"records": records}))
            output = root / "prepared"
            data = json.loads(prepare(source, output).read_text())
            fixture = data["fixtures"][0]
            expected = originals["ja", 0] + bytes(64000) + originals["en", 0] + bytes(64000) + originals["ja", 1]
            self.assertEqual(read_float_wav(output / fixture["audio"]), expected)
            self.assertEqual([(p["start_ms"], p["end_ms"]) for p in fixture["passages"]],
                             [(0, 1), (1001, 1002), (2002, 2003)])
            self.assertEqual(fixture["duration_ms"], 2003)
            self.assertEqual(len(data["sources"]), 6)
            with self.assertRaises(FileExistsError):
                prepare(source, output)
            records[0]["audio_sha256"] = "invalid"
            source.write_text(json.dumps({"records": records}))
            with self.assertRaisesRegex(ValueError, "checksum"):
                prepare(source, root / "invalid")
            self.assertFalse((root / "invalid").exists())

    def test_same_language_blocks_allow_a_merged_segment(self):
        fixture = {"duration_ms": 3000, "passages": [
            passage("a", "en", "hello", 0, 1000), passage("b", "en", "world", 2000, 3000)]}
        result = evaluate(fixture, [{"start_ms": 0, "end_ms": 3000, "text": "hello world"}])
        self.assertEqual(result["passages"][0]["errors"], 0)
        self.assertTrue(passes(result, {"a": {"errors": 0}, "b": {"errors": 0}}, 0, 0))

    def test_cross_language_timestamps_are_not_hidden_by_text_matching(self):
        fixture = {"duration_ms": 3000, "passages": [
            passage("a", "en", "hello", 0, 1000), passage("b", "ja", "ab", 2000, 3000)]}
        result = evaluate(fixture, [{"start_ms": 0, "end_ms": 3000, "text": "hello ab"}])
        self.assertEqual(result["cross_language_segments"], 1)
        self.assertFalse(passes(result, {"a": {"errors": 0}, "b": {"errors": 0}}, 1, 99))

    def test_missing_short_passage_cannot_pass_with_tolerance(self):
        fixture = {"duration_ms": 1000, "passages": [passage("a", "en", "yes", 0, 1000)]}
        result = evaluate(fixture, [])
        self.assertFalse(passes(result, {"a": {"errors": 0}}, 0.05, 2))
        wrong = evaluate(fixture, [{"start_ms": 0, "end_ms": 1000, "text": "no"}])
        self.assertFalse(passes(wrong, {"a": {"errors": 0}}, 0.05, 2))

    def test_timestamp_failures_are_reported(self):
        fixture = {"duration_ms": 1000, "passages": [passage("a", "en", "hello", 0, 1000)]}
        for start, end in [(-1, 500), (0, 1001), (500, 400), (500, 500)]:
            with self.subTest(start=start, end=end):
                result = evaluate(fixture, [{"start_ms": start, "end_ms": end, "text": "hello"}])
                self.assertGreater(result["invalid_timestamps"], 0)

    def test_gate_compares_against_isolated_errors_without_changing_scores(self):
        result = {"invalid_timestamps": 0, "cross_language_segments": 0,
                  "passages": [{"source_ids": ["a"], "hypothesis": "heard speech",
                                "errors": 6, "reference_units": 22, "rate": 6 / 22}]}
        self.assertTrue(passes(result, {"a": {"errors": 5}}, 0.05, 2))
        self.assertFalse(passes(result, {"a": {"errors": 0}}, 0.05, 2))
        self.assertEqual(result["passages"][0]["errors"], 6)


if __name__ == "__main__":
    unittest.main()
