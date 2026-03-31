/**
 * rinfra persistent Node.js worker.
 *
 * Protocol (line-delimited JSON over stdin/stdout):
 *   Request:  {"s": "<base64 script>", "i": "<base64 input>"}
 *   Response: {"r": "<base64 result>", "o": "<stderr text>", "c": <exit_code>}
 *
 * Convention:
 *   - Script's console.log / process.stdout.write -> captured as result
 *   - Script's console.error / process.stderr.write -> captured as diagnostic
 *   - INPUT parameter (Buffer) -> raw input bytes
 *   - Script body supports top-level await
 */
"use strict";

const readline = require("readline");

const _stdoutWrite = process.stdout.write.bind(process.stdout);
const _stderrWrite = process.stderr.write.bind(process.stderr);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

let _capturing = false;
let _outChunks = [];
let _errChunks = [];

process.stdout.write = function (chunk, encoding, cb) {
  if (_capturing) {
    _outChunks.push(
      Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk))
    );
    if (typeof cb === "function") cb();
    return true;
  }
  return _stdoutWrite(chunk, encoding, cb);
};

process.stderr.write = function (chunk, encoding, cb) {
  if (_capturing) {
    _errChunks.push(
      Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk))
    );
    if (typeof cb === "function") cb();
    return true;
  }
  return _stderrWrite(chunk, encoding, cb);
};

function respond(resp) {
  _stdoutWrite(JSON.stringify(resp) + "\n");
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on("line", async (line) => {
  let req;
  try {
    req = JSON.parse(line);
  } catch (e) {
    respond({ r: "", o: "invalid request: " + e.message, c: 1 });
    return;
  }

  const scriptCode = Buffer.from(req.s, "base64").toString("utf-8");
  const inputBuffer = Buffer.from(req.i || "", "base64");

  _outChunks = [];
  _errChunks = [];
  _capturing = true;
  let exitCode = 0;

  try {
    const fn = new AsyncFunction("require", "INPUT", scriptCode);
    await fn(require, inputBuffer);
  } catch (e) {
    exitCode = 1;
    _errChunks.push(Buffer.from((e.stack || String(e)) + "\n"));
  } finally {
    _capturing = false;
  }

  const resultBuf = Buffer.concat(_outChunks);
  const errStr = Buffer.concat(_errChunks).toString("utf-8");

  respond({
    r: resultBuf.toString("base64"),
    o: errStr,
    c: exitCode,
  });
});
