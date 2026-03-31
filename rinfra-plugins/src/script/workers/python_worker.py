"""
rinfra persistent Python worker.

Protocol (line-delimited JSON over stdin/stdout):
  Request:  {"s": "<base64 script>", "i": "<base64 input>"}
  Response: {"r": "<base64 result>", "o": "<stderr text>", "c": <exit_code>}

Convention:
  - Script's print() / sys.stdout  -> captured as result (ScriptOutput.result)
  - Script's stderr                -> captured as diagnostic (ScriptOutput.stdout)
  - INPUT global variable          -> raw input bytes
"""
import sys
import json
import base64
import traceback
import io

_real_stdin = sys.stdin
_real_stdout = sys.stdout
_real_stderr = sys.stderr


def _respond(resp):
    _real_stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
    _real_stdout.flush()


while True:
    try:
        line = _real_stdin.readline()
        if not line:
            break
        req = json.loads(line)
    except Exception as e:
        _respond({"r": "", "o": str(e), "c": 1})
        continue

    script_code = base64.b64decode(req["s"]).decode("utf-8")
    input_bytes = base64.b64decode(req.get("i", ""))

    out_buf = io.BytesIO()
    out_text = io.TextIOWrapper(out_buf, encoding="utf-8", line_buffering=True)
    err_capture = io.StringIO()
    exit_code = 0

    try:
        sys.stdout = out_text
        sys.stderr = err_capture
        scope = {"__builtins__": __builtins__, "INPUT": input_bytes}
        exec(compile(script_code, "<rinfra-script>", "exec"), scope)
        out_text.flush()
    except SystemExit as e:
        out_text.flush()
        exit_code = e.code if isinstance(e.code, int) else 1
    except Exception:
        out_text.flush()
        exit_code = 1
        err_capture.write(traceback.format_exc())
    finally:
        sys.stdout = _real_stdout
        sys.stderr = _real_stderr
        out_text.detach()

    _respond({
        "r": base64.b64encode(out_buf.getvalue()).decode(),
        "o": err_capture.getvalue(),
        "c": exit_code,
    })
