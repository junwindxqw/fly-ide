/** LSP 诊断与大纲状态（按 URI 索引）。
 *  诊断由 Transport 层在收到 publishDiagnostics 时写入；
 *  大纲由 Outline 面板按需请求 documentSymbol。 */

export interface DiagnosticEntry {
  message: string;
  severity: number; // 1=Error 2=Warning 3=Info 4=Hint
  line: number; // 0 起
  col: number;
  endLine: number;
  endCol: number;
  source: string;
}

/** URI → 诊断列表。 */
let diagnostics = $state(new Map<string, DiagnosticEntry[]>());

/** URI → 大纲符号。 */
let outline = $state(new Map<string, OutlineSymbol[]>());

export interface OutlineSymbol {
  name: string;
  kind: number; // LSP SymbolKind
  line: number; // 0 起
  col: number;
  detail: string;
}

export function getDiagnostics(uri: string): DiagnosticEntry[] {
  return diagnostics.get(uri) ?? [];
}

export function allDiagnostics(): { uri: string; entries: DiagnosticEntry[] }[] {
  return [...diagnostics.entries()].map(([uri, entries]) => ({ uri, entries }));
}

export function totalProblems(): number {
  let count = 0;
  for (const entries of diagnostics.values()) {
    count += entries.length;
  }
  return count;
}

export function setDiagnostics(uri: string, entries: DiagnosticEntry[]) {
  diagnostics = new Map(diagnostics).set(uri, entries);
}

export function clearDiagnostics(uri: string) {
  const next = new Map(diagnostics);
  next.delete(uri);
  diagnostics = next;
}

export function getOutline(uri: string): OutlineSymbol[] {
  return outline.get(uri) ?? [];
}

export function setOutline(uri: string, symbols: OutlineSymbol[]) {
  outline = new Map(outline).set(uri, symbols);
}

/** 在 Transport 层拦截 publishDiagnostics 并写入状态。 */
export function interceptLspMessage(message: string) {
  try {
    const parsed = JSON.parse(message);
    if (parsed.method === "textDocument/publishDiagnostics") {
      const uri: string = parsed.params?.uri ?? "";
      if (!uri) {
        return;
      }
      const entries: DiagnosticEntry[] = (parsed.params?.diagnostics ?? []).map(
        (d: {
          message: string;
          severity?: number;
          range?: {
            start: { line: number; character: number };
            end: { line: number; character: number };
          };
          source?: string;
        }) => ({
          message: d.message ?? "",
          severity: d.severity ?? 1,
          line: d.range?.start?.line ?? 0,
          col: d.range?.start?.character ?? 0,
          endLine: d.range?.end?.line ?? 0,
          endCol: d.range?.end?.character ?? 0,
          source: d.source ?? "",
        }),
      );
      setDiagnostics(uri, entries);
    }
  } catch {
    // 非 JSON（不太可能）忽略
  }
}

/** 请求大纲符号（client 由调用方传入，避免循环依赖）。 */
export async function requestOutline(
  client: { request<P, R>(method: string, params: P): Promise<R> },
  uri: string,
): Promise<void> {
  const symbols = await client.request<
    { textDocument: { uri: string } },
    Array<{
      name: string;
      kind: number;
      range: { start: { line: number; character: number }; end: unknown };
      selectionRange: { start: { line: number; character: number } };
      detail?: string;
    }>
  >("textDocument/documentSymbol", { textDocument: { uri } });
  setOutline(
    uri,
    symbols.map((s) => ({
      name: s.name,
      kind: s.kind,
      line: s.selectionRange?.start?.line ?? 0,
      col: s.selectionRange?.start?.character ?? 0,
      detail: s.detail ?? "",
    })),
  );
}

export const symbolKindNames: Record<number, string> = {
  5: "class",
  6: "method",
  12: "function",
  13: "variable",
  8: "field",
  23: "enum",
  10: "property",
};
