import type { Extension } from "@codemirror/state";

/** 按扩展名的语法高亮懒加载（Lezer 官方语言包）。
 *
 * 只允许动态 import——若在此文件顶部静态 import 语言包，
 * 它们会被打进首屏 bundle（Vite 会报 INEFFECTIVE_DYNAMIC_IMPORT）；
 * 加载结果按扩展名缓存，失败降级为无高亮（纯文本仍可编辑）。
 */

type SyntaxLoader = () => Promise<Extension[]>;

const loaders: Record<string, SyntaxLoader> = {
  js: async () => [(await import("@codemirror/lang-javascript")).javascript()],
  mjs: async () => [(await import("@codemirror/lang-javascript")).javascript()],
  jsx: async () => [(await import("@codemirror/lang-javascript")).javascript({ jsx: true })],
  ts: async () => [(await import("@codemirror/lang-javascript")).javascript({ typescript: true })],
  tsx: async () => [
    (await import("@codemirror/lang-javascript")).javascript({ typescript: true, jsx: true }),
  ],
  py: async () => [(await import("@codemirror/lang-python")).python()],
  rs: async () => [(await import("@codemirror/lang-rust")).rust()],
  php: async () => [await (await import("@codemirror/lang-php")).php()],
  go: async () => [(await import("@codemirror/lang-go")).go()],
  html: async () => [(await import("@codemirror/lang-html")).html()],
  htm: async () => [(await import("@codemirror/lang-html")).html()],
  css: async () => [(await import("@codemirror/lang-css")).css()],
  json: async () => [(await import("@codemirror/lang-json")).json()],
  md: async () => [(await import("@codemirror/lang-markdown")).markdown()],
  c: async () => [(await import("@codemirror/lang-cpp")).cpp()],
  h: async () => [(await import("@codemirror/lang-cpp")).cpp()],
  cpp: async () => [(await import("@codemirror/lang-cpp")).cpp()],
  hpp: async () => [(await import("@codemirror/lang-cpp")).cpp()],
  xml: async () => [(await import("@codemirror/lang-xml")).xml()],
  sql: async () => [(await import("@codemirror/lang-sql")).sql()],
};

const cache = new Map<string, Promise<Extension[]>>();

export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** 按路径加载语法扩展；无对应语言/加载失败时返回空数组（降级为纯文本）。 */
export async function loadSyntaxFor(path: string): Promise<Extension[]> {
  const ext = extensionOf(path);
  const loader = loaders[ext];
  if (!loader) {
    return [];
  }
  let pending = cache.get(ext);
  if (!pending) {
    pending = loader().catch((err) => {
      console.error(`语法包加载失败（${ext}），降级为纯文本`, err);
      cache.delete(ext);
      return [] as Extension[];
    });
    cache.set(ext, pending);
  }
  return pending;
}

/** 是否存在对应语法包（用于测试/状态展示）。 */
export function hasSyntaxFor(path: string): boolean {
  return extensionOf(path) in loaders;
}
