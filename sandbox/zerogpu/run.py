"""Run the core set on the private ZeroGPU Space, one model at a time.

The reference model goes first, then the core models that ran longest ago, then
candidates. It stops before the free daily quota can run out, because time past
it is billed. Every GPU call goes in the ledger, and a failed call counts as a
full window. Nothing is retried.

Usage: run.py SPACE_FOLDER|- MODELS OUT_DIR LEDGER [--limit N] [--only NAME]
"""
import argparse, json, os, pathlib, sys, time
from huggingface_hub import HfApi
from gradio_client import Client

SPACE = "tanepiper/termite-zerogpu-gate"
BUDGET = 35 * 60  # of the 40 free minutes
WINDOW = 300  # what one `run` call can use
DAY = 24 * 60 * 60
DECISION = ("ggml-org/Kev-4B-GGUF", "Kev-4B-Q8_0.gguf", "f4624b6bb7072f1bab726a622c1cad3f96030b9e")

parser = argparse.ArgumentParser()
parser.add_argument("space")
parser.add_argument("models", type=pathlib.Path)
parser.add_argument("out", type=pathlib.Path)
parser.add_argument("ledger", type=pathlib.Path)
parser.add_argument("--limit", type=int, default=0)
parser.add_argument("--only")
args = parser.parse_args()


def read_ledger():
    if not args.ledger.exists():
        return []
    return [json.loads(line) for line in args.ledger.read_text().splitlines()]


def record(name, seconds, outcome):
    with args.ledger.open("a") as ledger:
        ledger.write(json.dumps({"at": int(time.time()), "name": name,
                                 "seconds": round(seconds), "outcome": outcome}) + "\n")
    args.ledger.chmod(0o600)


def used_today():
    since = time.time() - DAY
    return sum(row["seconds"] for row in read_ledger() if row["at"] > since)


def in_order(rows):
    """Reference first, then whatever ran longest ago, core before candidates."""
    last = {row["name"]: row["at"] for row in read_ledger()}
    return sorted(rows, key=lambda m: (not m.get("reference"), m["status"] != "core",
                                       last.get(m["name"], 0)))


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
    if used_today() + WINDOW > BUDGET:
        print("stopped: quota limit", int(used_today()), "s used", flush=True)
        break
    weights = client.predict(model["repo"], model["file"], model["revision"], api_name="/fetch")
    started = time.time()
    spec = {k: model[k] for k in ("name", "size_b", "refusals_removed")}
    try:
        reply = json.loads(client.predict(weights, decision, json.dumps(spec), args.limit, api_name="/run"))
    except Exception as error:
        record(model["name"], WINDOW, "error")
        print(model["name"], "ERROR", str(error)[:300], flush=True)
        break
    seconds = time.time() - started
    record(model["name"], seconds, str(reply["code"]))
    print(model["name"], "code", reply["code"], "took", int(seconds), "s", flush=True)
    print(reply["log"][-600:] if reply["code"] else "", flush=True)
    for name, text in reply["files"].items():
        path = args.out / name
        path.write_text(text)
        path.chmod(0o600)
