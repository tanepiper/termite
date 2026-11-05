import json, sys, uvicorn
from llama_cpp.server.app import create_app
from llama_cpp.server.settings import ModelSettings, ServerSettings

path, alias, sock = sys.argv[1:4]
inner = create_app(
    server_settings=ServerSettings(),
    model_settings=[ModelSettings(model=path, model_alias=alias, n_gpu_layers=-1, n_ctx=4096, verbose=True)],
)


async def app(scope, receive, send):
    """This server spells a schema request differently from llama-server."""
    if scope["type"] != "http" or scope["path"] != "/v1/chat/completions":
        return await inner(scope, receive, send)
    body = b""
    while True:
        message = await receive()
        body += message.get("body", b"")
        if not message.get("more_body"):
            break
    request = json.loads(body)
    wanted = request.get("response_format") or {}
    if wanted.get("type") == "json_schema":
        request["response_format"] = {"type": "json_object", "schema": wanted["json_schema"]["schema"]}
    request.pop("chat_template_kwargs", None)
    request["stream"] = False
    # A runaway reply would use up the GPU window.
    request.setdefault("max_tokens", 512)
    fixed = json.dumps(request).encode()
    scope = {**scope, "headers": [(k, v) for k, v in scope["headers"] if k != b"content-length"]
             + [(b"content-length", str(len(fixed)).encode())]}
    sent = False

    async def replay():
        nonlocal sent
        if sent:
            return await receive()
        sent = True
        return {"type": "http.request", "body": fixed, "more_body": False}

    await inner(scope, replay, send)


uvicorn.run(app, uds=sock, log_level="warning")
