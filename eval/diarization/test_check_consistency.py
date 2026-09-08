import unittest
from check_consistency import score


class ConsistencyTest(unittest.TestCase):
    def test_rejects_one_voice_split_across_labels(self):
        result = score([(0, 5, 'S1'), (5, 10, 'S2')], [(0, 10, 'A')])
        self.assertFalse(result['pass'])
        self.assertEqual(result['speakers']['A']['purity'], 0.5)

    def test_rejects_different_voices_merged_under_one_label(self):
        result = score([(0, 10, 'S1')], [(0, 5, 'A'), (5, 10, 'B')])
        self.assertFalse(result['pass'])
        self.assertFalse(result['separate'])

    def test_accepts_consistent_separate_voices_with_arbitrary_ids(self):
        self.assertTrue(score([(0, 5, 'S8'), (5, 10, 'S2')], [(0, 5, 'A'), (5, 10, 'B')])['pass'])

    def test_empty_output_cannot_pass(self):
        self.assertFalse(score([], [(0, 10, 'A')])['pass'])


if __name__ == '__main__':
    unittest.main()
