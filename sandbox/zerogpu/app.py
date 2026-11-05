"""Test one model on ZeroGPU. `fetch` downloads it with the internet on and
returns its path. `run` holds the GPU, in a separate process, so the path is
passed in, and everything inside it is confined by `confine`. The servers are
the pinned llama-server that T4 jobs use, copied by `hosted.sh server`."""
import glob, json, os, pathlib, shutil, signal, site, subprocess
import gradio as gr
import spaces
from huggingface_hub import hf_hub_download, snapshot_download

HERE = pathlib.Path(__file__).parent
COMMIT = (HERE / "commit").read_text().strip()
LLAMA = "/tmp/llama"
LIBS = ":".join([LLAMA] + glob.glob(site.getsitepackages()[0] + "/nvidia/*/lib"))
ALIAS = "model"
SOCKET = "/tmp/run/model.sock"
DECISION = "ggml-org/Kev-4B-GGUF:Q8_0"
DECISION_SOCKET = "/tmp/run/decision.sock"

shutil.copytree(HERE / "llama", LLAMA, dirs_exist_ok=True)
os.chmod(LLAMA + "/llama-server", 0o755)

INNER = r"""
serve() {
  /tmp/llama/llama-server --model "$1" --alias "$2" --host "$3" --ctx-size 4096 --n-gpu-layers 99 >"$3.log" 2>&1 &
  server=$!
  tries=0
  until grep -q 'listening on' "$3.log" 2>/dev/null; do
    tries=$((tries + 1))
    if [ $tries -ge 120 ] || ! kill -0 $server 2>/dev/null; then echo SERVER FAILED; tail -15 "$3.log" | cut -c1-200; exit 1; fi
    sleep 1
  done
}
serve "$DECISION_WEIGHTS" "$DECISION" /tmp/run/decision.sock
serve "$MODEL" model /tmp/run/model.sock
grep -h offloaded /tmp/run/*.log || true
/tmp/eval run --only "$NAME" --commit "$COMMIT-zerogpu" $LIMIT
"""


def fetch(repo, file, revision):
    """Download pinned weights. A split file needs all of its parts."""
    if "-00001-of-" in file:
        pattern = file.replace("-00001-of-", "-*-of-")
        folder = snapshot_download(repo, revision=revision, allow_patterns=[pattern])
        return str(pathlib.Path(folder) / file)
    return hf_hub_download(repo, file, revision=revision)


@spaces.GPU(duration=300)
def run(weights, decision_weights, spec, limit):
    shutil.rmtree("/tmp/work", ignore_errors=True)
    shutil.rmtree("/tmp/run", ignore_errors=True)
    os.makedirs("/tmp/work/src")
    os.makedirs("/tmp/work/home")
    os.makedirs("/tmp/run")
    shutil.copytree(HERE / "bundle/eval", "/tmp/work/eval")
    shutil.copy(HERE / "eval-tool", "/tmp/eval")
    shutil.copy(HERE / "confine", "/tmp/confine")
    os.chmod("/tmp/confine", 0o755)
    os.chmod("/tmp/eval", 0o755)
    model = json.loads(spec)
    model.update(url="http://127.0.0.1:8080", socket=SOCKET, model=ALIAS, context_tokens=4096)
    pathlib.Path("/tmp/work/eval/models.jsonl").write_text(json.dumps(model) + "\n")
    pathlib.Path("/tmp/work/home/config.json").write_text(json.dumps({
        "model_url": "http://127.0.0.1:8080", "model": ALIAS,
        "decision_url": "http://127.0.0.1:9931", "decision_model": DECISION,
        "model_socket": SOCKET, "decision_socket": DECISION_SOCKET}))
    env = {**os.environ, "MODEL": weights, "DECISION_WEIGHTS": decision_weights,
           "DECISION": DECISION, "NAME": model["name"], "COMMIT": COMMIT,
           "TMPDIR": "/tmp/run", "HOME": "/tmp/run", "TERMITE_HOME": "/tmp/work/home",
           "LD_LIBRARY_PATH": LIBS, "LIMIT": f"--limit {int(limit)}" if int(limit) else ""}
    job = subprocess.Popen(
        ["/tmp/confine", "--write", "/tmp/work/eval", "--write", "/tmp/run",
         "--write", "/dev", "--write", "/proc", "--", "/bin/sh", "-c", INNER],
        cwd="/tmp/work", env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, start_new_session=True)
    try:
        log, _ = job.communicate(timeout=285)
        code = job.returncode
    except subprocess.TimeoutExpired:
        os.killpg(job.pid, signal.SIGKILL)
        log, _ = job.communicate()
        code = "timeout"
    finally:
        # The model servers are in the group too: leave nothing behind.
        try:
            os.killpg(job.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    files = {pathlib.Path(p).name: open(p).read() for p in glob.glob("/tmp/work/eval/results/*.json")}
    return json.dumps({"code": code, "log": log[-3000:], "files": files})


with gr.Blocks() as demo:
    repo, file, revision, out = gr.Text(), gr.Text(), gr.Text(), gr.Text()
    weights, decision_weights, spec, limit = gr.Text(), gr.Text(), gr.Text(), gr.Number(value=0)
    gr.Button("fetch").click(fetch, [repo, file, revision], out, api_name="fetch")
    gr.Button("run").click(run, [weights, decision_weights, spec, limit], out, api_name="run")
demo.launch()
