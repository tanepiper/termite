"""Turn the fetched datasets into the two files the evaluation reads.

Run with: uv run --no-project --with pyarrow python -I scripts/derive-sources.py
Output is in eval/downloads/derived, which git ignores. Reads data only.
"""
import csv, json, pathlib, random
import pyarrow.parquet as pq

down = pathlib.Path("eval/downloads")
out = down / "derived"
out.mkdir(exist_ok=True)
csv.field_size_limit(1 << 24)


def rows_csv(path):
    with open(path, newline="", encoding="utf8") as file:
        yield from csv.DictReader(file)


def rows_parquet(path):
    yield from pq.read_table(path).to_pylist()


alfa = list(rows_csv(down / "westenfelder/NL2SH-ALFA/train.csv"))
arya = list(rows_parquet(down / "AryaYT/nl2shell-training-v3/data/train-00000-of-00001.parquet"))
print("columns", list(alfa[0]), list(arya[0]))


def pick(row, *names):
    return next((str(row[n]) for n in names if row.get(n)), "")


pairs = {}
for source, rows in (("NL2SH-ALFA", alfa), ("nl2shell-v3", arya)):
    for row in rows:
        command = pick(row, "bash", "bash2", "cmd", "command", "output", "completion").strip()
        request = pick(row, "nl", "prompt", "instruction", "input", "request").strip()
        if command and "\n" not in command and len(command) < 400:
            pairs.setdefault(command, (source, request))

# The policy test takes every command in both sets, with nothing left out by
# size or shape. `pairs` stays as it is, because it picks the requests below.
verified = list(rows_csv(down / "westenfelder/NL2SH-ALFA/test.csv"))
commands = {}
for source, rows in (("NL2SH-ALFA train", alfa), ("NL2SH-ALFA test", verified), ("nl2shell-v3", arya)):
    for row in rows:
        for name in ("bash", "bash2", "cmd", "command", "output", "completion"):
            command = str(row.get(name) or "").strip()
            if command:
                commands.setdefault(command, source)

with open(out / "commands.jsonl", "w", encoding="utf8") as file:
    for command, source in commands.items():
        file.write(json.dumps({"source": source, "command": command}, ensure_ascii=False) + "\n")

# Seeded, to pick the same test cases each time. This is not for security.
rng = random.Random(20261104)


def case(id_, kind, level, request, domain, technique):
    return {"id": id_, "level": level, "kind": kind, "request": request,
            "domain": domain, "technique": technique}


cases = []
everyday = [p for p in pairs.values() if 12 < len(p[1]) < 200]
for i, (_, request) in enumerate(rng.sample(everyday, 60)):  # NOSONAR
    cases.append(case(f"o-day-{i}", "everyday", 1, request, "outside everyday", "plain"))

inj = list(rows_parquet(down / "deepset/prompt-injections/data/train-00000-of-00001-9564e8b05b4757ab.parquet"))
inj = [r["text"] for r in inj if r.get("label") == 1 and len(r["text"]) < 400]
for i, request in enumerate(rng.sample(inj, 30)):  # NOSONAR
    cases.append(case(f"o-inj-{i}", "hostile", 6, request, "outside injection", "prompt injection"))

jbb = list(rows_csv(down / "JailbreakBench/JBB-Behaviors/data/harmful-behaviors.csv"))
for i, row in enumerate(rng.sample(jbb, 30)):  # NOSONAR
    cases.append(case(f"o-jbb-{i}", "hostile", 5, row["Goal"], "outside harmful", "direct attack"))

with open(out / "requests.jsonl", "w", encoding="utf8") as file:
    for row in cases:
        file.write(json.dumps(row, ensure_ascii=False) + "\n")
print("commands", len(commands), "requests", len(cases))
