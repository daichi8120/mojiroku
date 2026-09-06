"""Network-free process ownership and evaluation provenance checks."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

RUN_PATH = Path(__file__).with_name("run.py")
spec = importlib.util.spec_from_file_location("translation_eval", RUN_PATH)
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)


def process_is_running(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    if sys.platform.startswith("linux"):
        try:
            stat = Path(f"/proc/{pid}/stat").read_text()
        except FileNotFoundError:
            # The child may have been reaped between the signal and state checks.
            # If /proc is unavailable, keep treating a present PID as running.
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return False
            return True
        # The command name can contain spaces and parentheses; state follows its
        # final closing parenthesis. PID 1 may retain a terminated zombie in CI.
        return stat.rpartition(") ")[2].split()[0] != "Z"
    return True


@unittest.skipUnless(os.name == "posix", "evaluation uses POSIX process groups")
class ProcessTests(unittest.TestCase):
    def test_linux_zombie_is_terminated_but_live_states_are_not(self):
        # A zombie still accepts signal 0; process existence alone is insufficient.
        with mock.patch.object(sys, "platform", "linux"), mock.patch.object(os, "kill", return_value=None):
            for state, running in [("Z", False), ("S", True), ("R", True)]:
                with self.subTest(state=state), mock.patch.object(Path, "read_text", return_value=f"123 (worker (child)) {state} 1 2 3"):
                    self.assertEqual(process_is_running(123), running)

    def test_reaped_descendant_needs_no_process_state_read(self):
        with mock.patch.object(os, "kill", side_effect=ProcessLookupError), mock.patch.object(Path, "read_text") as read:
            self.assertFalse(process_is_running(123))
            read.assert_not_called()

    def test_success_and_nonzero_exit(self):
        result = evaluation.run_checked([sys.executable, "-c", "print('caption')"], subprocess.DEVNULL, 3)
        self.assertEqual(result.stdout, "caption\n")
        with self.assertRaises(subprocess.CalledProcessError):
            evaluation.run_checked([sys.executable, "-c", "raise SystemExit(7)"], subprocess.DEVNULL, 3)

    def test_timeout_kills_wrapper_and_child_with_inherited_stdout(self):
        # An outer process supervises the regression and always cleans up the
        # orphaned child left by an implementation that kills only the wrapper.
        with tempfile.TemporaryDirectory() as folder:
            marker = Path(folder) / "processes.json"
            wrapper = """
import json, os, subprocess, sys
from pathlib import Path
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
Path(sys.argv[1]).write_text(json.dumps([os.getpid(), child.pid]))
child.wait()
"""
            driver = """
import importlib.util, subprocess, sys
spec = importlib.util.spec_from_file_location('translation_eval', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
try:
    module.run_checked([sys.executable, '-c', sys.argv[2], sys.argv[3]], subprocess.DEVNULL, 1)
except subprocess.TimeoutExpired:
    print('timeout handled', flush=True)
else:
    raise AssertionError('expected timeout')
"""
            process = subprocess.Popen(
                [sys.executable, "-c", driver, str(RUN_PATH), wrapper, str(marker)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True,
            )
            pids = []
            try:
                try:
                    stdout, stderr = process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    self.fail("timeout left the inherited output pipe open")
                self.assertEqual(process.returncode, 0, stderr)
                self.assertEqual(stdout.strip(), "timeout handled")
                pids = json.loads(marker.read_text())
                for pid in pids:
                    deadline = time.monotonic() + 2
                    while process_is_running(pid):
                        if time.monotonic() >= deadline:
                            self.fail(f"process {pid} survived timeout")
                        time.sleep(0.01)
            finally:
                if marker.exists():
                    pids = json.loads(marker.read_text())
                # Also cleans up the intentionally broken implementation in a red run.
                for pid in pids:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.communicate(timeout=3)


class ProvenanceTests(unittest.TestCase):
    def test_model_integrity_rejects_modified_and_missing_cache_files(self):
        with tempfile.TemporaryDirectory() as folder:
            models = Path(folder)
            for name in (evaluation.TURBO, evaluation.VAD):
                (models / name).write_bytes(b"abc")
            expected = {name: evaluation.sha256(models / name) for name in (evaluation.TURBO, evaluation.VAD)}
            with mock.patch.object(evaluation, "MODEL_HASHES", expected):
                self.assertEqual(evaluation.verify_stt_models(models), expected)
                (models / evaluation.TURBO).write_bytes(b"bad")
                with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                    evaluation.verify_stt_models(models)
                (models / evaluation.VAD).unlink()
                with self.assertRaises(FileNotFoundError):
                    evaluation.verify_stt_models(models)

    def test_cli_pipeline_and_vad_fallback_are_verified(self):
        result = {"whisper_model": evaluation.TURBO, "vad_model": evaluation.VAD, "decoding": "greedy"}
        self.assertEqual(evaluation.stt_metadata(result, ""), {**result, "language_hint": "auto"})
        for changed in ({"decoding": "beam5"}, {"whisper_model": "other.bin"}, {"vad_model": "other.bin"}):
            with self.assertRaisesRegex(ValueError, "requested turbo/VAD/greedy"):
                evaluation.stt_metadata({**result, **changed}, "")
        with self.assertRaisesRegex(ValueError, "fell back"):
            evaluation.stt_metadata(result, "stt vad: filtering failed: model unavailable")

    def test_duplicate_basenames_cannot_pool_distinct_models(self):
        evaluation.validate_model_names([Path("4B.gguf"), Path("9B.gguf")])
        with self.assertRaisesRegex(ValueError, "basenames must be unique"):
            evaluation.validate_model_names([Path("first/model.gguf"), Path("second/model.gguf")])

    def test_device_metadata_records_cpu_and_ram_or_unknown(self):
        values = {"machdep.cpu.brand_string": "Test CPU", "hw.model": "Test Mac", "hw.memsize": "17179869184"}
        def sysctl(command, stderr, timeout):
            return subprocess.CompletedProcess(command, 0, values[command[-1]] + "\n")
        with mock.patch.object(evaluation.platform, "system", return_value="Darwin"):
            with mock.patch.object(evaluation, "run_checked", side_effect=sysctl):
                device = evaluation.device_metadata()
                self.assertEqual(device["cpu"], "Test CPU")
                self.assertEqual(device["hardware_model"], "Test Mac")
                self.assertEqual(device["physical_memory_bytes"], 17179869184)
            with mock.patch.object(evaluation, "run_checked", side_effect=OSError("unavailable")):
                self.assertIsNone(evaluation.device_metadata()["physical_memory_bytes"])


if __name__ == "__main__":
    unittest.main()
