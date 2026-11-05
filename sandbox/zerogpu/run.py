"""Run models from models.jsonl on the private ZeroGPU Space, one at a time.
Stops before the free daily quota can run out, because time past it is billed.

Usage: run.py SPACE_FOLDER|- MODELS OUT_DIR MINUTES_LEFT
"""
import json, os, pathlib, sys, time
from huggingface_hub import HfApi
from gradio_client import Client

token = os.environ["HF_TOKEN"]
api = HfApi(token=token)
repo = "tanepiper/termite-zerogpu-gate"
assert api.repo_info(repo, repo_type="space").private is True, "not private"
if sys.argv[1] != "-":
    api.upload_folder(repo_id=repo, repo_type="space", folder_path=sys.argv[1])
    time.sleep(20)
for _ in range(70):
    stage = api.get_space_runtime(repo).stage
    if stage == "RUNNING":
        break
    if "ERROR" in str(stage):
        sys.exit("space failed")
    time.sleep(15)
client = Client(repo, token=token, verbose=False)
out = pathlib.Path(sys.argv[3])
out.mkdir(exist_ok=True, mode=0o700)
used = 0.0
limit_seconds = float(sys.argv[4]) * 60
for line in pathlib.Path(sys.argv[2]).read_text().splitlines():
    model = json.loads(line)
    if used >= limit_seconds:
        print("stopped: quota limit", flush=True)
        break
    weights = client.predict(model["repo"], api_name="/fetch")
    started = time.time()
    spec = {k: model[k] for k in ("name", "size_b", "refusals_removed")}
    try:
        reply = json.loads(client.predict(weights, json.dumps(spec), 0, api_name="/run"))
    except Exception as error:
        print(model["name"], "ERROR", str(error)[:300], flush=True)
        break
    used += time.time() - started
    print(model["name"], "code", reply["code"], "used", int(used), "s", flush=True)
    progress = [l for l in reply["log"].splitlines() if "/48" in l]
    print("reached", progress[-1][:60] if progress else "no case", flush=True)
    for name, text in reply["files"].items():
        path = out / name
        path.write_text(text)
        path.chmod(0o600)
