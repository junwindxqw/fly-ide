// 最小 LSP 服务器 fixture（测试与冒烟共用）：stdio + Content-Length 帧。
//
// 能力：initialize/initialized、didOpen（回 publishDiagnostics 并写日志）、
// completion（固定 mock 项）、hover、shutdown/exit。
// 每收到一条请求/通知都追加到 cwd 下 fly-mock-lsp.log，供冒烟断言链路。

import fs from "node:fs";

try {
  fs.appendFileSync(
    "fly-mock-lsp.log",
    `${JSON.stringify({ started: true, cwd: process.cwd(), pid: process.pid })}\n`,
  );
} catch {
  // 日志失败不影响协议
}

let buffer = Buffer.alloc(0);

function send(obj) {
  const body = JSON.stringify(obj);
  process.stdout.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
}

function log(message) {
  try {
    fs.appendFileSync("fly-mock-lsp.log", `${JSON.stringify({ method: message.method ?? "*", id: message.id ?? null })}\n`);
  } catch {
    // 日志失败不影响协议
  }
}

function onMessage(message) {
  log(message);
  if (message.method === "initialize") {
    send({
      id: message.id,
      result: {
        capabilities: {
          positionEncoding: "utf-16",
          textDocumentSync: { openClose: true, change: 1 },
          completionProvider: { triggerCharacters: ["."] },
          hoverProvider: true,
        },
        serverInfo: { name: "mock-lsp" },
      },
    });
    return;
  }
  if (message.method === "initialized") {
    return;
  }
  if (message.method === "textDocument/didOpen") {
    const uri = message.params?.textDocument?.uri;
    send({
      method: "textDocument/publishDiagnostics",
      params: {
        uri,
        diagnostics: [
          {
            range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } },
            message: "mock diagnostic from server",
            severity: 1,
            source: "mock-lsp",
          },
        ],
      },
    });
    return;
  }
  if (message.method === "textDocument/completion") {
    send({
      id: message.id,
      result: [
        { label: "MOCK_COMPLETION_ALPHA", kind: 2 },
        { label: "MOCK_COMPLETION_BETA", kind: 2 },
      ],
    });
    return;
  }
  if (message.method === "textDocument/hover") {
    send({ id: message.id, result: { contents: { kind: "markdown", value: "**mock** hover" } } });
    return;
  }
  if (message.method === "shutdown") {
    send({ id: message.id, result: null });
    return;
  }
  if (message.method === "exit") {
    process.exit(0);
  }
  if (message.id !== undefined && message.method) {
    send({ id: message.id, error: { code: -32601, message: "MethodNotFound" } });
  }
}

process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd < 0) {
      return;
    }
    const header = buffer.slice(0, headerEnd).toString();
    const match = /Content-Length: (\d+)/i.exec(header);
    if (!match) {
      buffer = Buffer.alloc(0);
      return;
    }
    const length = Number(match[1]);
    const bodyStart = headerEnd + 4;
    if (buffer.length < bodyStart + length) {
      return;
    }
    const body = buffer.slice(bodyStart, bodyStart + length).toString();
    buffer = buffer.slice(bodyStart + length);
    try {
      onMessage(JSON.parse(body));
    } catch {
      // 非法消息忽略
    }
  }
});
