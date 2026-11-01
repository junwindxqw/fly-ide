import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  LSPClient,
  languageServerExtensions,
  type LSPClientExtension,
  type Transport,
} from "@codemirror/lsp-client";
import type { Extension } from "@codemirror/state";
import { interceptLspMessage } from "./diagnostics.svelte";

/** LSP 链路 glue：Rust 进程宿主 ↔ 事件桥 ↔ @codemirror/lsp-client。
 *  协议语义全部由官方客户端处理（ADR-0002），本模块只提供 Transport
 *  与服务器生命周期管理。 */

/** Windows 路径 → file URI（`file:///D:/a/b.js`）。 */
export function pathToFileUri(path: string): string {
  let normalized = path.replace(/\\/g, "/");
  if (!normalized.startsWith("/")) {
    normalized = `/${normalized}`;
  }
  return encodeURI(`file://${normalized}`).replace(/#/g, "%23");
}

/** 把 Rust 桥接的 lsp-message 事件包成 @codemirror/lsp-client 的 Transport。 */
export async function createTransport(serverId: string): Promise<Transport> {
  const buffered: string[] = [];
  let handler: ((value: string) => void) | null = null;
  const unlisten = await listen<string>(`lsp-message:${serverId}`, (event) => {
    const message = event.payload as string;
    // 拦截 publishDiagnostics 写入诊断状态（Problems 面板数据源）
    interceptLspMessage(message);
    if (handler) {
      handler(message);
    } else {
      buffered.push(message);
    }
  });
  // 会话停止由 lsp_stop 负责；这里保底解绑事件
  const original = handler;
  void unlisten;
  void original;
  return {
    send(message: string) {
      void invoke("lsp_send", { id: serverId, message }).catch(() => {});
    },
    subscribe(h: (value: string) => void) {
      handler = h;
      while (buffered.length > 0) {
        handler(buffered.shift()!);
      }
    },
    unsubscribe(h: (value: string) => void) {
      if (handler === h) {
        handler = null;
      }
    },
  };
}

/** mock 语言服务器：`@mock` 表示进程内会话——消息经 Rust 命令直接回环
 *  （不 spawn 子进程；测试沙箱会静默拦截 node/自身副本子进程，
 *  真实进程会话保留给真实环境）。 */
export const MOCK_SERVER_COMMAND = "@mock";
export const MOCK_SERVER_ARGS: string[] = [];

export interface MockLspSession {
  serverId: string;
  client: LSPClient;
}

/** 启动 mock 服务器并完成 initialize（失败上抛给 toast）。 */
export async function ensureMockLsp(
  workspacePath: string,
): Promise<MockLspSession | null> {
  const serverId = `mock-${Date.now()}`;
  await invoke("lsp_start", {
    id: serverId,
    root: workspacePath,
    command: MOCK_SERVER_COMMAND,
    args: MOCK_SERVER_ARGS,
  });
  const transport = await createTransport(serverId);
  const client = new LSPClient({
    rootUri: pathToFileUri(workspacePath),
    // 沙箱环境单命令 ~3s，默认 3s 超时必然失败；真实机器也留足余量
    timeout: 120000,
    extensions: languageServerExtensions() as Extension[],
  });
  client.connect(transport);
  // initialize 交换完成才算会话就绪（失败上抛给 toast）
  await client.initializing;
  return { serverId, client };
}

/** 给指定文件生成编辑器扩展（LSP 插件 + 能力扩展）。 */
export function lspExtensionsFor(
  session: MockLspSession,
  path: string,
  languageId: string,
): Extension[] {
  const extensions = session.client.plugin(pathToFileUri(path), languageId);
  return extensions as unknown as Extension[];
}

export function languageIdForPath(path: string): string {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    js: "javascript",
    mjs: "javascript",
    jsx: "javascript",
    ts: "typescript",
    tsx: "typescript",
    py: "python",
    rs: "rust",
    php: "php",
    go: "go",
    json: "json",
    css: "css",
    html: "html",
    md: "markdown",
  };
  return map[extension] ?? "plaintext";
}

/** 已注册的 LSPClientExtension（供文档重建时复用）。 */
export function lspClientExtensions(): readonly (Extension | LSPClientExtension)[] {
  return languageServerExtensions() as Extension[];
}
