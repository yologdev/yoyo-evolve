#!/usr/bin/env python3
"""scripts/run_probes.py — re-run yoyo's repertoire and say what still works.

A probe is a small script under probes/ that yoyo wrote to show one thing he
can do (skills/repertoire describes them). This runner is the half he does not
own: it re-runs every probe against the built binary and records the result,
so the reading cannot be bent by whoever wrote the probe.

    run_probes.py [YOYO_BIN]    run probes/*.sh, write .yoyo/probes_last_run.json
    run_probes.py --render      print the "WHAT YOU CAN DO" lines for the prompt

It is a mirror, not a gate. Nothing is reverted, blocked or scored on what it
prints; scripts/yoyo_context.sh shows the lines as facts and no planner rule
reads them.

How a probe runs:
  - in a fresh empty directory, with HOME pointing at another empty one;
  - with an environment of PATH and YOYO_BIN only, so no model key and no
    credential reaches it (a probe that needs a model cannot pass here; that is
    a stated limit of this version, not a verdict on the ability);
  - for at most PROBE_SECS seconds, and the whole run for at most TOTAL_SECS;
  - exit status 0 means "still works". Anything else, or a timeout, is "broke".

What it counts besides pass and fail, because a repertoire can be made to look
healthy by rewriting or pruning it:
  - edited: the probe's text differs from what it was the first time this
    runner saw it. The first-seen hash is carried forward in the result file,
    not read from git history: CI clones 50 commits deep, where the oldest
    commit looks like it added every file, so a git-based answer went quiet
    after about 50 commits;
  - retired: files under probes/retired/.
The result file lives under .yoyo/, which a task can write, so these counts
keep an honest author honest and are not a control.

Always exits 0. With no probes it writes no file and removes a stale one.
"""
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time

PROBE_DIR = "probes"
RESULT = os.path.join(".yoyo", "probes_last_run.json")
def _secs(name, default):
    try:
        return max(1, int(os.environ.get(name, "")))
    except ValueError:
        return default


PROBE_SECS = _secs("YOYO_PROBE_SECS", 30)     # overridable so a test need not wait 30 s
TOTAL_SECS = _secs("YOYO_PROBES_TOTAL_SECS", 180)
CAN_MAX = 160


def git(*args):
    try:
        r = subprocess.run(["git", *args], capture_output=True, text=True, timeout=20)
        return r.stdout.strip() if r.returncode == 0 else ""
    except Exception:
        return ""


def probe_files():
    try:
        names = sorted(n for n in os.listdir(PROBE_DIR) if n.endswith(".sh"))
    except OSError:
        return []
    return [os.path.join(PROBE_DIR, n) for n in names if os.path.isfile(os.path.join(PROBE_DIR, n))]


def retired_count():
    try:
        return sum(1 for n in os.listdir(os.path.join(PROBE_DIR, "retired")) if n.endswith(".sh"))
    except OSError:
        return 0


def can_line(path):
    """The probe's own one-line description: the first `# can: ...` line."""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                s = line.strip()
                if s.lower().startswith("# can:"):
                    text = " ".join(s[6:].split())
                    return text[:CAN_MAX] + ("…" if len(text) > CAN_MAX else "")
    except OSError:
        pass
    return ""


def content_hash(path):
    try:
        with open(path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()[:16]
    except OSError:
        return ""


def previous_first_hashes():
    try:
        with open(RESULT, encoding="utf-8") as f:
            rows = json.load(f).get("probes", [])
        return {r["name"]: r["first_hash"] for r in rows if r.get("name") and r.get("first_hash")}
    except Exception:
        return {}


def run_one(path, yoyo_bin):
    """Run one probe in its own process group, so a timeout takes its children too."""
    work, home = tempfile.mkdtemp(prefix="probe-work-"), tempfile.mkdtemp(prefix="probe-home-")
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "YOYO_BIN": yoyo_bin, "HOME": home}
    try:
        proc = subprocess.Popen(
            ["bash", os.path.abspath(path)], cwd=work, env=env, start_new_session=True,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        try:
            return "works" if proc.wait(timeout=PROBE_SECS) == 0 else "broke"
        except subprocess.TimeoutExpired:
            return "broke"
        finally:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except OSError:
                pass
            try:
                proc.wait(timeout=5)
            except Exception:
                pass
    except Exception:
        return "not run"
    finally:
        shutil.rmtree(work, ignore_errors=True)
        shutil.rmtree(home, ignore_errors=True)


def run(yoyo_bin):
    files = probe_files()
    if not files:
        if os.path.exists(RESULT):
            os.remove(RESULT)
        print("  repertoire: no probes.")
        return
    yoyo_bin = os.path.abspath(yoyo_bin)
    first = previous_first_hashes()
    start = time.time()
    rows = []
    for path in files:
        if not os.path.exists(yoyo_bin):
            status = "not run"
        elif time.time() - start > TOTAL_SECS:
            status = "not run"
        else:
            status = run_one(path, yoyo_bin)
        name, now = os.path.basename(path)[:-3], content_hash(path)
        rows.append({
            "name": name,
            "can": can_line(path),
            "status": status,
            "first_hash": first.get(name) or now,
            "edited": bool(first.get(name)) and first[name] != now,
        })
    out = {
        "sha": git("rev-parse", "--short", "HEAD"),
        "date": time.strftime("%Y-%m-%d", time.gmtime()),
        "probes": rows,
    }
    os.makedirs(os.path.dirname(RESULT), exist_ok=True)
    with open(RESULT, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
        f.write("\n")
    works = sum(1 for r in rows if r["status"] == "works")
    print("  repertoire: %d probe(s), %d still work." % (len(rows), works))


def render():
    files = probe_files()
    retired = retired_count()
    if not files:
        line = ("Repertoire: empty. A probe is a small script under probes/ that shows one thing "
                "you can do; the repertoire skill describes them. Having none is fine.")
        if retired:
            line += " Retired: %d." % retired
        print(line)
        return
    try:
        with open(RESULT, encoding="utf-8") as f:
            data = json.load(f)
        rows = {r["name"]: r for r in data.get("probes", [])}
    except Exception:
        data, rows = {}, {}
    names = [os.path.basename(p)[:-3] for p in files]
    ran = [rows[n] for n in names if n in rows and rows[n].get("status") in ("works", "broke")]
    if not ran:
        print("Repertoire: %d thing(s) you have written a probe for. None has been run yet." % len(names))
    else:
        works = sum(1 for r in ran if r["status"] == "works")
        print("Repertoire: %d thing(s) you have shown you can do · %d still worked when last run (%s, %s)"
              % (len(names), works, data.get("sha") or "unknown commit", data.get("date") or "unknown date"))
        for r in ran:
            if r["status"] == "broke":
                print("No longer works: %s%s" % (r["name"], " — " + r["can"] if r.get("can") else ""))
        waiting = len(names) - len(ran)
        if waiting:
            print("Written since that run, not run yet: %d" % waiting)
    edited = sum(1 for n in names if rows.get(n, {}).get("edited") is True)
    if edited or retired:
        print("Edited after being written: %d · Retired: %d" % (edited, retired))
    print("(Probes run with no model key, so they cover only what the binary does on its own.)")


def main():
    try:
        if len(sys.argv) > 1 and sys.argv[1] == "--render":
            render()
        else:
            run(sys.argv[1] if len(sys.argv) > 1 else "./target/debug/yoyo")
    except Exception as e:  # a mirror must never stop a session
        print("  repertoire: runner error (%s); nothing recorded." % type(e).__name__, file=sys.stderr)
    sys.exit(0)


if __name__ == "__main__":
    main()
