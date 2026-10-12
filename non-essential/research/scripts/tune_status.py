"""One-shot status of the tuning jobs: running, finished or crashed, with the latest score.
Usage: tune_status.py [stage]   (stage: structure, hyper, colour; default hyper)"""
import glob, os, subprocess, sys, time
LOGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "satellite", "match", "logs")
stage = sys.argv[1] if len(sys.argv) > 1 else "hyper"
alive = subprocess.run(["powershell", "-c", "(Get-CimInstance Win32_Process -Filter \"Name='python.exe'\").CommandLine"], capture_output=True, text=True).stdout
for log in sorted(glob.glob(os.path.join(LOGS, f"*_{stage}.log"))):
    key = os.path.basename(log).split("_")[0]
    text = open(log, errors="replace").read()
    err = log[:-4] + ".err"
    etext = open(err, errors="replace").read() if os.path.exists(err) else ""
    lines = [l for l in text.splitlines() if l.strip()]
    last = lines[-1] if lines else ""
    running = f"biome_run.py {key} {stage}" in alive
    if "Traceback" in etext or "Traceback" in text:
        state = "CRASHED"
        last = (etext or text).strip().splitlines()[-1]
    elif any(l.startswith("best") for l in lines):
        state = "finished"
    elif running:
        state = "running"
    else:
        state = "stopped"
    age = int(time.time() - os.path.getmtime(log))
    print(f"{key:11s} {state:9s} last change {age:4d}s ago | {last[:110]}")
