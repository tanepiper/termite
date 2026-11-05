"""Test one model on ZeroGPU. `fetch` downloads it with the internet on and
returns its path. `run` holds the GPU, in a separate process, so the path is
passed in, and everything inside it is confined by `confine`."""
import glob, json, os, pathlib, shutil, signal, site, subprocess
import gradio as gr
import spaces
from huggingface_hub import list_repo_files, snapshot_download

HERE = pathlib.Path(__file__).parent
SITE = site.getsitepackages()[0]
LIBS = ":".join(glob.glob(SITE + "/nvidia/*/lib"))
ALIAS = "model"
SOCKET = "/tmp/run/model.sock"

INNER = r"""
python3 /tmp/work/serve.py "$MODEL" model /tmp/run/model.sock >/tmp/run/server.log 2>&1 &
server=$!
tries=0
until [ -S /tmp/run/model.sock ]; do
  tries=$((tries + 1))
  if [ $tries -ge 120 ] || ! kill -0 $server 2>/dev/null; then echo SERVER FAILED; tail -15 /tmp/run/server.log | cut -c1-200; exit 1; fi
  sleep 1
done
grep -c 'assigned to device CUDA0' /tmp/run/server.log
/tmp/eval run --only "$NAME" --commit 74dd4fd-zerogpu $LIMIT
"""


def fetch(repo):
    files = [f for f in list_repo_files(repo) if "q4_k_m" in f.lower() and f.endswith(".gguf")]
    whole = [f for f in files if "-of-" not in f]
    chosen = whole[:1] or sorted(files)
    folder = snapshot_download(repo, allow_patterns=chosen)
    return str(pathlib.Path(folder) / sorted(chosen)[0])


@spaces.GPU(duration=300)
def run(weights, spec, limit):
    shutil.rmtree("/tmp/work", ignore_errors=True)
    shutil.rmtree("/tmp/run", ignore_errors=True)
    os.makedirs("/tmp/work/src")
    os.makedirs("/tmp/work/home")
    os.makedirs("/tmp/run")
    shutil.copytree(HERE / "bundle/eval", "/tmp/work/eval")
    shutil.copy(HERE / "eval-tool", "/tmp/eval")
    shutil.copy(HERE / "confine", "/tmp/confine")
    os.chmod("/tmp/confine", 0o755)
    shutil.copy(HERE / "serve.py", "/tmp/work/serve.py")
    os.chmod("/tmp/eval", 0o755)
    model = json.loads(spec)
    model.update(url="http://127.0.0.1:8080", socket=SOCKET, model=ALIAS, context_tokens=4096)
    pathlib.Path("/tmp/work/eval/models.jsonl").write_text(json.dumps(model) + "\n")
    pathlib.Path("/tmp/work/home/config.json").write_text(json.dumps({
        "model_url": "http://127.0.0.1:8080", "model": ALIAS,
        "decision_url": "http://127.0.0.1:8080", "decision_model": "none",
        "model_socket": SOCKET}))
    env = {**os.environ, "MODEL": weights, "NAME": model["name"], "TMPDIR": "/tmp/run",
           "HOME": "/tmp/run", "TERMITE_HOME": "/tmp/work/home", "LD_LIBRARY_PATH": LIBS,
           "LIMIT": f"--limit {int(limit)}" if int(limit) else ""}
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
        # The model server is in the group too: leave nothing behind.
        try:
            os.killpg(job.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    files = {pathlib.Path(p).name: open(p).read() for p in glob.glob("/tmp/work/eval/results/*.json")}
    return json.dumps({"code": code, "log": log[-3000:], "files": files})


with gr.Blocks() as demo:
    repo, spec, limit, out = gr.Text(), gr.Text(), gr.Number(value=0), gr.Text()
    gr.Button("fetch").click(fetch, repo, out, api_name="fetch")
    gr.Button("run").click(run, [repo, spec, limit], out, api_name="run")
demo.launch()
