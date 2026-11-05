"""Run the core set on the private ZeroGPU Space, one model at a time.

The reference model goes first, then the core models that ran longest ago, then
candidates, then models that keep failing, each alone in its own window. It stops before the free daily quota can run out, because time past
it is billed. Every GPU call goes in the ledger, and a failed call counts as a
full window. Nothing is retried.

A model that runs out of time keeps what it finished and carries on in the next
window, up to PARTS windows, and the parts become one run file.

Usage: run.py SPACE_FOLDER|- MODELS OUT_DIR LEDGER [--limit N] [--only NAME] [--pay] [--decision]

--pay carries on past the free quota, at $1 for 10 minutes. Time is still logged.
--decision tests decision models on the safety ladder, from
eval/hosted-decision-models.jsonl.
"""
import argparse, json, os, pathlib, sys, time
from huggingface_hub import HfApi
from gradio_client import Client

SPACE = "tanepiper/termite-zerogpu-gate"
BUDGET = 35 * 60  # of the 40 free minutes
WINDOW = 480  # what one `run` call can use
DAY = 24 * 60 * 60
PARTS = 5  # windows one model can use
DECISION = ("ggml-org/Kev-4B-GGUF", "Kev-4B-Q8_0.gguf", "f4624b6bb7072f1bab726a622c1cad3f96030b9e")

parser = argparse.ArgumentParser()
parser.add_argument("space")
parser.add_argument("models", type=pathlib.Path)
parser.add_argument("out", type=pathlib.Path)
parser.add_argument("ledger", type=pathlib.Path)
parser.add_argument("--limit", type=int, default=0)
parser.add_argument("--only")
parser.add_argument("--pay", action="store_true")
parser.add_argument("--decision", action="store_true")
args = parser.parse_args()


def read_ledger():
    if not args.ledger.exists():
        return []
    return [json.loads(line) for line in args.ledger.read_text().splitlines()]


def record(name, seconds, outcome, reason=""):
    row = {"at": int(time.time()), "name": name, "seconds": round(seconds), "outcome": outcome}
    if reason:
        row["reason"] = reason
    with args.ledger.open("a") as ledger:
        ledger.write(json.dumps(row) + "\n")
    args.ledger.chmod(0o600)


def used_today():
    since = time.time() - DAY
    return sum(row["seconds"] for row in read_ledger() if row["at"] > since)


def failures():
    """Failed calls per model since its last finished run."""
    count = {}
    for row in read_ledger():
        count[row["name"]] = 0 if row["outcome"] == "0" else count.get(row["name"], 0) + 1
    return count


def in_order(rows):
    """Reference first, then whatever ran longest ago, core before candidates.
    Models that keep failing go last, so the rest are not held up behind them."""
    last = {row["name"]: row["at"] for row in read_ledger()}
    failed = failures()
    return sorted(rows, key=lambda m: (not m.get("reference"), failed.get(m["name"], 0),
                                       m["status"] != "core", last.get(m["name"], 0)))


def reason(reply):
    """Why a call failed: the server's first complaint, for the ledger."""
    if not reply["code"]:
        return ""
    lines = [line for line in reply["log"].splitlines() if " E " in line or "error" in line.lower()]
    return lines[0][:200] if lines else str(reply["code"])


def window(model, weights, skip, limit):
    """One GPU call, written to the ledger. None if the call failed."""
    started = time.time()
    try:
        if args.decision:
            spec = {k: model[k] for k in ("name", "size_b", "url", "socket", "model")}
            spec["letters"] = model.get("letters", "")
            reply = json.loads(client.predict(weights, json.dumps(spec), api_name="/judge"))
        else:
            spec = {k: model[k] for k in ("name", "size_b", "refusals_removed")}
            reply = json.loads(client.predict(weights, decision, json.dumps(spec), limit, skip, api_name="/run"))
    except Exception as error:
        record(model["name"], WINDOW, "error")
        print(model["name"], "ERROR", str(error)[:300], flush=True)
        return None
    seconds = time.time() - started
    record(model["name"], seconds, str(reply["code"]), reason(reply))
    print(model["name"], "code", reply["code"], "took", int(seconds), "s", flush=True)
    print(reply["log"][-600:] if reply["code"] else "", flush=True)
    return reply


def save(path, text):
    path.write_text(text)
    path.chmod(0o600)


def test(model, weights):
    """Run a model over as many windows as it needs. False if a call failed."""
    done, path, kept = 0, None, None
    for _ in range(PARTS):
        reply = window(model, weights, done, args.limit - done if args.limit else 0)
        if reply is None:
            return False
        finished = 0
        for name, text in reply["files"].items():
            part = json.loads(text)
            if part.get("models") and "cases" in part["models"][0]:
                finished = len(part["models"][0]["cases"])
            if kept is None:
                path, kept = args.out / name, part
            elif finished:
                kept["models"][0]["cases"] += part["models"][0]["cases"]
                kept["models"][0]["seconds"] += part["models"][0]["seconds"]
            save(path, json.dumps(kept, indent=2))
        done += finished
        if "out of time" not in reply["log"] or not finished:
            return True
        print(model["name"], "carrying on after", done, flush=True)
    print(model["name"], "unfinished after", done, flush=True)
    return True


token = os.environ["HF_TOKEN"]
api = HfApi(token=token)
assert api.repo_info(SPACE, repo_type="space").private is True, "not private"
if args.space != "-":
    api.upload_folder(repo_id=SPACE, repo_type="space", folder_path=args.space,
                      delete_patterns=["serve.py", "__pycache__/*"])
    time.sleep(20)
for _ in range(70):
    stage = api.get_space_runtime(SPACE).stage
    if stage == "RUNNING":
        break
    if "ERROR" in str(stage):
        sys.exit("space failed")
    time.sleep(15)
client = Client(SPACE, token=token, verbose=False)
args.out.mkdir(exist_ok=True, mode=0o700)
rows = [json.loads(line) for line in args.models.read_text().splitlines()]
if args.only:
    rows = [row for row in rows if row["name"] == args.only]
decision = client.predict(*DECISION, api_name="/fetch")
for model in in_order(rows):
    if not args.pay and used_today() + WINDOW > BUDGET:
        print("stopped: quota limit", int(used_today()), "s used", flush=True)
        break
    weights = client.predict(model["repo"], model["file"], model["revision"], api_name="/fetch")
    if not test(model, weights):
        break
