"""Which popular local models should termite test next, and where can they run?

Reads the public Hugging Face API with no token, and writes eval/watch.jsonl.
Nothing is downloaded and no model is contacted. Prints the models that weren't
in the last list.

Usage: python3 -I scripts/model-watch.py
"""
import json, pathlib, re, urllib.parse, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
WATCH = ROOT / "eval/watch.jsonl"
MODELS = ROOT / "eval/models.jsonl"
API = "https://huggingface.co/api"

# A Q4_K_M file is about 0.6 GB per billion parameters. Add a fifth for context.
GB_PER_BILLION = 0.72
# What this Mac can give a model out of its 96 GB.
LOCAL_GB = 80
# The compute included with PRO each month.
CREDIT_USD = 2
PIPELINES = ("text-generation", "image-text-to-text")
SORTS = ("trendingScore", "downloads")
PER_LIST = 40
NO_REFUSALS = re.compile(r"uncensored|abliterat|heretic|obliterat", re.IGNORECASE)
NAMED_SIZE = re.compile(r"(?<![\d.A-Za-z])(\d+(?:\.\d+)?)B(?![A-Za-z])")


def get(path):
    request = urllib.request.Request(API + path, headers={"User-Agent": "termite-eval"})
    with urllib.request.urlopen(request, timeout=60) as reply:
        return json.load(reply)


def popular():
    """Every GGUF model on the lists, once each, in the order first seen."""
    found = {}
    for pipeline in PIPELINES:
        for sort in SORTS:
            query = urllib.parse.urlencode([
                ("filter", "gguf"), ("pipeline_tag", pipeline), ("sort", sort),
                ("limit", PER_LIST), ("expand[]", "gguf"), ("expand[]", "downloads"),
                ("expand[]", "likes"), ("expand[]", "trendingScore"), ("expand[]", "gated"),
            ])
            for model in get("/models?" + query):
                found.setdefault(model["id"], model)
    return found.values()


def machines():
    """Hosted GPUs, cheapest first, as (name, GB of GPU memory, dollars an hour)."""
    rows = []
    for flavor in get("/jobs/hardware"):
        gpu = flavor.get("accelerator") or {}
        if gpu.get("type") == "gpu" and flavor.get("unitLabel") == "minute":
            memory = int(gpu["vram"].split()[0])
            rows.append((flavor["name"], memory, round(flavor["unitCostUSD"] * 60, 2)))
    return sorted(rows, key=lambda row: row[2])


def place(needs_gb, gpus):
    """Where a model of this size fits. The Mac is free, so it comes first."""
    if needs_gb <= LOCAL_GB:
        return {"where": "mac", "flavor": None, "usd_per_hour": None, "minutes_in_credit": None}
    for name, memory, price in gpus:
        if memory >= needs_gb:
            return {"where": "hosted", "flavor": name, "usd_per_hour": price,
                    "minutes_in_credit": int(CREDIT_USD / price * 60)}
    return {"where": "too_big", "flavor": None, "usd_per_hour": None, "minutes_in_credit": None}


def row(model, gpus, tested):
    params = (model.get("gguf") or {}).get("total")
    if not params:
        return None
    # The Hub reads one file, which may be a small helper. A size in the name
    # is the better guide when it says more.
    named = [float(size) for size in NAMED_SIZE.findall(model["id"].split("/")[-1])]
    size_b = round(max([params / 1e9, *named]), 1)
    needs_gb = round(size_b * GB_PER_BILLION, 1)
    return {
        "repo": model["id"],
        "size_b": size_b,
        "needs_gb": needs_gb,
        **place(needs_gb, gpus),
        "refusals_removed": bool(NO_REFUSALS.search(model["id"])),
        "gated": bool(model.get("gated")),
        "downloads": model.get("downloads", 0),
        "likes": model.get("likes", 0),
        "trending": model.get("trendingScore", 0),
        "tested": any(model["id"].lower() in name for name in tested),
    }


def main():
    tested = [json.loads(line)["model"].lower() for line in MODELS.read_text().splitlines()]
    seen = set()
    if WATCH.exists():
        seen = {json.loads(line)["repo"] for line in WATCH.read_text().splitlines()}
    gpus = machines()
    rows = [r for r in (row(m, gpus, tested) for m in popular()) if r]
    rows.sort(key=lambda r: (-r["trending"], r["repo"]))
    WATCH.write_text("".join(json.dumps(r) + "\n" for r in rows))

    counts = {w: sum(r["where"] == w for r in rows) for w in ("mac", "hosted", "too_big")}
    print(f"model-watch: {len(rows)} models: {counts['mac']} fit this Mac, "
          f"{counts['hosted']} need hosted GPUs, {counts['too_big']} too big")
    for r in rows:
        if r["repo"] not in seen:
            place_text = r["flavor"] and f"{r['flavor']} ${r['usd_per_hour']}/h" or r["where"]
            removed = ", refusals removed" if r["refusals_removed"] else ""
            print(f"new: {r['repo']} ({r['size_b']}B, {place_text}{removed})")


main()
