"""The agent evaluation suite (spec 13.7).

    python eval/run.py --dedalo target/release/dedalo --agent "claude -p {prompt_file}"
    python eval/run.py --dedalo target/release/dedalo --dry-run

Each task runs in a fresh folder holding its seed file. The agent command
is run in that folder with `{prompt}` (the prompt text) or `{prompt_file}`
(a file holding it) substituted; it should drive dedalo through the CLI or
the MCP server. The checks then score the folder with the dedalo binary.

`--dry-run` runs no agent: it checks the tasks themselves, that every repair
seed fails and every edit and render seed is valid.

The release gate (spec 13.7) is 85% of tasks solved on the first attempt and
95% within three rounds. `--rounds 3` gives the agent up to three tries, each
with the previous diagnostics appended to the prompt.
"""

import argparse
import json
import pathlib
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def run(cmd, cwd, stdin=None):
    return subprocess.run(cmd, cwd=cwd, input=stdin, capture_output=True, text=True, encoding="utf-8")


class Dedalo:
    def __init__(self, exe):
        self.exe = str(pathlib.Path(exe).resolve())

    def validate(self, path, cwd):
        out = run([self.exe, "validate", path, "--json"], cwd)
        try:
            return json.loads(out.stdout)
        except json.JSONDecodeError:
            return {"status": "error", "diagnostics": [{"code": "NO_OUTPUT", "message": out.stderr}]}

    def document(self, path, cwd):
        out = run([self.exe, "convert", path, "--to", "json"], cwd)
        return json.loads(out.stdout) if out.returncode == 0 else None


def ids_of(doc):
    d = doc["dungeon"]
    ids = [s["id"] for s in d.get("spaces", [])]
    for s in d.get("spaces", []):
        ids += [e["id"] for e in s.get("elements", [])]
    ids += [e["id"] for e in d.get("elements", [])]
    ids += [c["id"] for c in d.get("connections", [])]
    ids += [n["id"] for n in d.get("notes", [])]
    return set(ids)


def check(task, folder, dedalo, seed_doc, seed_text):
    """Returns a list of failed checks; empty means solved."""
    c = task["checks"]
    failed = []
    for f in c.get("files", []):
        if not (folder / f).exists() or (folder / f).stat().st_size == 0:
            failed.append(f"missing output {f}")
    for f, needle in c.get("file_contains", {}).items():
        if (folder / f).exists() and needle not in (folder / f).read_text(encoding="utf-8", errors="replace"):
            failed.append(f"{f} lacks {needle!r}")
    for f, needle in c.get("file_lacks", {}).items():
        if (folder / f).exists() and needle in (folder / f).read_text(encoding="utf-8", errors="replace"):
            failed.append(f"{f} contains {needle!r}")
    if "file" not in c:
        return failed
    path = c["file"]
    if not (folder / path).exists():
        return failed + [f"{path} was not written"]
    report = dedalo.validate(path, folder)
    if c.get("valid") and report.get("status") == "error":
        codes = [d["code"] for d in report.get("diagnostics", [])]
        failed.append(f"{path} has errors: {', '.join(codes)}")
        return failed
    for code in c.get("no_warnings", []):
        if any(d["code"] == code for d in report.get("diagnostics", [])):
            failed.append(f"{path} still warns {code}")
    doc = dedalo.document(path, folder)
    if doc is None:
        return failed + [f"{path} does not convert to JSON"]
    d = doc["dungeon"]
    spaces = d.get("spaces", [])
    rooms = [s for s in spaces if s["kind"] == "room"]
    elements = [e for s in spaces for e in s.get("elements", [])] + d.get("elements", [])
    conns = d.get("connections", [])
    if len(rooms) < c.get("min_rooms", 0):
        failed.append(f"{len(rooms)} rooms; at least {c['min_rooms']} wanted")
    if len(conns) < c.get("min_connections", 0):
        failed.append(f"{len(conns)} connections; at least {c['min_connections']} wanted")
    if sum(s["kind"] == "corridor" for s in spaces) < c.get("corridors", 0):
        failed.append("no corridor")
    for t in c.get("types", []):
        if not any(e["type"] == t for e in elements):
            failed.append(f"no element of type {t}")
    for t in c.get("connection_types", []):
        if not any(x["type"] == t for x in conns):
            failed.append(f"no connection of type {t}")
    for shape in c.get("shapes", []):
        if not any((s.get("shape") or {}).get("fn") == shape for s in rooms):
            failed.append(f"no {shape} room")
    if c.get("secret_door") and not any(x.get("secret") for x in conns):
        failed.append("no secret door")
    if c.get("locked_door") and not any((x.get("properties") or {}).get("locked") for x in conns):
        failed.append("no locked door")
    if c.get("hidden_room") and not any(s.get("visibility") in ("dm", "hidden") for s in rooms):
        failed.append("no room hidden from players")
    names = [e["name"] for e in d.get("exports", [])]
    for e in c.get("exports", []):
        if e not in names:
            failed.append(f"no export named {e}")
    if seed_doc is not None:
        before, after = ids_of(seed_doc), ids_of(doc)
        if c.get("keep_ids") and not before <= after:
            failed.append(f"elements removed: {', '.join(sorted(before - after))}")
        if c.get("keep_rooms"):
            lost = {s["id"] for s in seed_doc["dungeon"].get("spaces", []) if s["kind"] == "room"} - after
            if lost:
                failed.append(f"rooms removed: {', '.join(sorted(lost))}")
        for gone in c.get("missing_ids", []):
            if gone in after:
                failed.append(f"{gone} is still there")
    if c.get("new_room_adjacent"):
        target = c["new_room_adjacent"]
        if not any((s.get("layout") or {}).get("adjacent_to") == target and s["id"] not in ids_of(seed_doc) for s in rooms):
            failed.append(f"no new room placed next to {target}")
    if c.get("element_in"):
        eid, space = c["element_in"]
        if not any(e["id"] == eid for s in spaces if s["id"] == space for e in s.get("elements", [])):
            failed.append(f"{eid} is not in {space}")
    if c.get("keep_comments") and seed_text:
        text = (folder / path).read_text(encoding="utf-8")
        lost = [l.strip() for l in seed_text.splitlines() if l.strip().startswith("#") and l.strip() not in text]
        if lost:
            failed.append(f"comments lost: {lost[0]}")
    return failed


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--dedalo", default=str(ROOT / "target" / "release" / "dedalo"))
    p.add_argument("--agent", help="command template with {prompt} or {prompt_file}")
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--only", help="run one task by ID")
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()
    exe = args.dedalo + (".exe" if sys.platform == "win32" and not args.dedalo.endswith(".exe") else "")
    dedalo = Dedalo(exe)
    tasks = json.loads((ROOT / "eval" / "tasks.json").read_text(encoding="utf-8"))["tasks"]
    if args.only:
        tasks = [t for t in tasks if t["id"] == args.only]
    first, within, problems = 0, 0, []
    for task in tasks:
        folder = pathlib.Path(tempfile.mkdtemp(prefix=f"dedalo-eval-{task['id']}-"))
        seed_doc = seed_text = None
        if "seed" in task:
            seed_text = (ROOT / "tests" / "corpus" / task["seed"] / "input.dedalo").read_text(encoding="utf-8").replace("\r\n", "\n")
            (folder / "dungeon.dedalo").write_text(seed_text, encoding="utf-8", newline="\n")
            seed_doc = dedalo.document("dungeon.dedalo", folder)
            seed_report = dedalo.validate("dungeon.dedalo", folder)
        if args.dry_run:
            if task["kind"] == "repair" and seed_report["status"] != "error":
                problems.append(f"{task['id']}: the seed already validates")
            if task["kind"] in ("edit", "render") and seed_report["status"] == "error":
                problems.append(f"{task['id']}: the seed has errors")
            shutil.rmtree(folder)
            continue
        if not args.agent:
            sys.exit("pass --agent, or --dry-run")
        prompt = task["prompt"] + " Use the dedalo command (run `dedalo guide` for the language) to validate your work."
        failed = ["not run"]
        for round_no in range(1, args.rounds + 1):
            (folder / "PROMPT.txt").write_text(prompt, encoding="utf-8")
            cmd = args.agent.replace("{prompt_file}", shlex.quote(str(folder / "PROMPT.txt"))).replace("{prompt}", shlex.quote(prompt))
            subprocess.run(cmd, cwd=folder, shell=True, timeout=900)
            failed = check(task, folder, dedalo, seed_doc, seed_text)
            if not failed:
                first += round_no == 1
                within += 1
                break
            prompt = task["prompt"] + " A previous attempt was checked and failed: " + "; ".join(failed) + ". Fix it."
        print(f"{'PASS' if not failed else 'FAIL'} {task['id']}" + ("" if not failed else f": {'; '.join(failed)}"))
        shutil.rmtree(folder, ignore_errors=True)
    if args.dry_run:
        for x in problems:
            print("PROBLEM", x)
        print(f"{len(tasks)} tasks checked, {len(problems)} problems")
        sys.exit(1 if problems else 0)
    n = len(tasks)
    print(f"\nfirst attempt: {first}/{n} ({first / n:.0%}); within {args.rounds} rounds: {within}/{n} ({within / n:.0%})")
    gate = first / n >= 0.85 and (args.rounds < 3 or within / n >= 0.95)
    print("gate:", "met" if gate else "not met (85% first attempt, 95% within three rounds)")
    sys.exit(0 if gate else 1)


if __name__ == "__main__":
    main()
