import unittest

from download import select_rows
from metrics import edit_distance, score, tokens
from run import aggregate, combined_text


class MetricTests(unittest.TestCase):
    def test_japanese_width_spacing_and_punctuation(self):
        # Escapes keep source prose in English; these represent Japanese text.
        reference = "\u30ab\u30bf\u30ab\u30ca\u3001\uff21\uff11\u3002"
        hypothesis = "\uff76\uff80\uff76\uff85 A1"
        self.assertEqual(score(reference, hypothesis, "ja")["errors"], 0)
        self.assertEqual(tokens("\u30b3\u30fc\u30d2\u30fc", "ja"), list("\u30b3\u30fc\u30d2\u30fc"))

    def test_english_word_boundaries_and_apostrophes(self):
        self.assertEqual(tokens("It's HIGH-quality\u2014really!", "en"),
                         ["its", "high", "quality", "really"])
        self.assertEqual(score("one two three", "one four", "en")["errors"], 2)

    def test_insertions_can_exceed_one_hundred_percent(self):
        self.assertEqual(score("one", "one two three", "en")["rate"], 2)
        self.assertEqual(score("one two", "", "en")["rate"], 1)
        with self.assertRaises(ValueError):
            score("...", "words", "en")

    def test_edit_distance_known_examples(self):
        self.assertEqual(edit_distance(list("kitten"), list("sitting")), 3)
        self.assertEqual(edit_distance([], list("abc")), 3)
        self.assertEqual(edit_distance(list("abc"), []), 3)

    def test_segments_preserve_english_word_boundaries(self):
        self.assertEqual(combined_text([{"text": "hello"}, {"text": "world"}], "en"), "hello world")

    def test_corpus_rate_is_weighted_by_reference_length(self):
        rows = [dict(language=lang, decoding=decoder, errors=errors, reference_units=units,
                     duration_seconds=10, pipeline_seconds=1, process_seconds=2)
                for lang in ("ja", "en") for decoder in ("greedy", "beam5")
                for errors, units in ((1, 1), (0, 9))]
        self.assertTrue(all(row["rate"] == 0.1 for row in aggregate(rows)))
        with self.assertRaises(ValueError):
            aggregate(rows[:2])

    def test_selection_uses_recording_ids_and_is_order_independent(self):
        # A sentence may have multiple recordings. Do not deduplicate sentence IDs.
        lines = [f"1\t{name}.wav\tText\ttext\tt e x t\t16000\tMALE" for name in ("a", "b", "c")]
        expected = select_rows("\n".join(lines), 2)
        self.assertEqual(expected, select_rows("\n".join(reversed(lines)), 2))
        self.assertEqual(len(select_rows("\n".join(lines), 3)), 3)
        with self.assertRaises(ValueError):
            select_rows("\n".join(lines), 4)

    def test_model_comparison_does_not_merge_two_greedy_models(self):
        rows = [dict(language=language, variant=variant, decoding="greedy", errors=errors,
                     reference_units=10, duration_seconds=10, pipeline_seconds=seconds,
                     process_seconds=seconds + 1)
                for language in ("ja", "en")
                for variant, errors, seconds in (("turbo", 2, 1), ("large-v3", 1, 3))]
        summary = aggregate(rows, ("turbo", "large-v3"))
        self.assertEqual([row["rate"] for row in summary], [0.2, 0.1, 0.2, 0.1])
        self.assertEqual([row["pipeline_seconds"] for row in summary], [1, 3, 1, 3])


if __name__ == "__main__":
    unittest.main()
