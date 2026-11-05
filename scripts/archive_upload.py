"""Upload runs and observations to a private Hugging Face dataset. Called by
archive-runs.sh.

Creates the dataset private if it is missing, refuses to continue unless it
reports itself private, and adds only files it does not already hold. The card
is the one file it replaces.
"""
import base64, json, os, pathlib, sys, urllib.error, urllib.request

API = "https://huggingface.co/api"
token = os.environ["HF_TOKEN_VALUE"]


def call(path, body=None, content="application/json"):
    data = body if isinstance(body, bytes) else (json.dumps(body).encode() if body is not None else None)
    request = urllib.request.Request(API + path, data=data, headers={
        "Authorization": "Bearer " + token, "Content-Type": content, "User-Agent": "termite-eval"})
    with urllib.request.urlopen(request, timeout=120) as reply:
        return json.load(reply)


name, home, card = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
owner = call("/whoami-v2")["name"]
repo = f"{owner}/{name}"
try:
    info = call(f"/datasets/{repo}")
except urllib.error.HTTPError as error:
    if error.code not in (401, 404):
        raise
    call("/repos/create", {"type": "dataset", "name": name, "private": True})
    info = call(f"/datasets/{repo}")
if info.get("private") is not True:
    sys.exit(f"archive-runs.sh: {repo} is not private; nothing uploaded")

held = {f["path"] for f in call(f"/datasets/{repo}/tree/main?recursive=true")}
files = {path.name: path for path in (home / "runs").glob("*.json")}
files |= {f"observations/{path.name}": path for path in (home / "observations").glob("*.jsonl")}
new = {target: path for target, path in sorted(files.items()) if target not in held}
new["README.md"] = card
targets = list(new)
for start in range(0, len(targets), 20):
    lines = [{"key": "header", "value": {"summary": "Add runs"}}]
    for target in targets[start:start + 20]:
        lines.append({"key": "file", "value": {"path": target, "encoding": "base64",
                      "content": base64.b64encode(new[target].read_bytes()).decode()}})
    body = "\n".join(json.dumps(line) for line in lines).encode()
    call(f"/datasets/{repo}/commit/main", body, "application/x-ndjson")
print(f"archive-runs.sh: {len(new) - 1} new files in private dataset {repo}", file=sys.stderr)
