"""Verify real sidecar parent-death cleanup; requires a model and Metal access."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

WRAPPER = """
import json, subprocess, sys
from pathlib import Path
with open(sys.argv[2], 'w') as log:
    child = subprocess.Popen(json.loads(sys.argv[3]), stdout=log, stderr=log)
    Path(sys.argv[1]).write_text(str(child.pid))
    child.wait()
"""


def check(sidecar: Path, model: Path, source: Path, output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    pidfile, log = output / "child.pid", output / "child.log"
    command = [str(sidecar.resolve()), "--translate", str(model.resolve()), str(source.resolve()), "ja", "--no-think"]
    parent = subprocess.Popen([sys.executable, "-c", WRAPPER, str(pidfile), str(log), json.dumps(command)],
                              start_new_session=True)
    try:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if parent.poll() is not None:
                raise AssertionError("sidecar completed before parent-death test")
            if pidfile.exists() and log.exists() and "llama_model_loader" in log.read_text(errors="replace"):
                break
            time.sleep(0.01)
        else:
            raise AssertionError("sidecar did not enter model loading")
        child = int(pidfile.read_text())
        started = time.monotonic()
        parent.kill()
        parent.wait(timeout=3)
        while time.monotonic() - started < 3:
            state = subprocess.run(["/bin/ps", "-p", str(child), "-o", "stat="],
                                   capture_output=True, text=True, timeout=2).stdout.strip()
            if not state or state.startswith("Z"):
                break
            time.sleep(0.01)
        else:
            raise AssertionError("sidecar survived parent exit")
        # Disappearance alone can be a crash. Require evidence that the watcher ran.
        if "translation: parent exited" not in log.read_text(errors="replace"):
            raise AssertionError("sidecar exited without the parent watcher marker")
        return {"cleanup_ms": round((time.monotonic() - started) * 1000), "child_state": state or "reaped"}
    finally:
        # The supervisor owns this process group even after the direct parent dies.
        try:
            os.killpg(parent.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        parent.wait(timeout=3)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("sidecar", "model", "source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(check(args.sidecar, args.model, args.source, args.output)))


if __name__ == "__main__":
    main()
