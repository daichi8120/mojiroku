import contextlib
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from check_consistency import main, score


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

    def test_main_rejects_unusable_baseline(self):
        for baseline in ['', 'new output format', '  20.00 -- 25.00 S1']:
            with self.subTest(baseline=baseline), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                audio = root / 'audio.wav'
                audio.write_bytes(b'fixture')
                reference = root / 'reference.json'
                reference.write_text(json.dumps({
                    'audio_sha256': hashlib.sha256(audio.read_bytes()).hexdigest(),
                    'intervals': [[0, 10, 'A']],
                }))
                binary = root / 'binary'
                binary.write_bytes(b'fake executable; subprocess output is mocked')
                argv = ['check_consistency.py', '--audio', str(audio), '--reference', str(reference),
                        '--binary', str(binary), '--baseline', str(binary), '--models', str(root),
                        '--output', str(root / 'output')]
                with patch('sys.argv', argv), patch('subprocess.check_output', side_effect=[baseline, '  0.00 -- 10.00 S1']), contextlib.redirect_stdout(io.StringIO()):
                    with self.assertRaises(SystemExit) as result:
                        main()
                self.assertNotEqual(result.exception.code, 0)

    def test_empty_output_cannot_pass(self):
        self.assertFalse(score([], [(0, 10, 'A')])['pass'])


if __name__ == '__main__':
    unittest.main()
