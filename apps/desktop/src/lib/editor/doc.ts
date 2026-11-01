import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  lineNumbers,
} from "@codemirror/view";
import type { TextFile } from "../fs";

/** 文档模型：一个打开的文本文件对应一个 DocEntry。
 *
 * 编辑器内部文本统一规范为 `\n`；文件原始 EOL/编码/BOM 记录在 entry 上，
 * 保存时由 Rust 侧还原。大文件（readOnlyPlain 层级）以只读方式打开。
 */

export interface DocEntry {
  path: string;
  /** 标签页显示名 */
  name: string;
  state: EditorState;
  /** 打开时的文本（已规范 \n），用于脏标记 */
  originalText: string;
  encoding: string;
  bom: boolean;
  eol: string;
  tier: TextFile["tier"];
  readOnly: boolean;
}

const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "12px" },
  ".cm-scroller": {
    overflow: "auto",
    fontFamily: "'Cascadia Code', Consolas, monospace",
    lineHeight: "1.5",
  },
  ".cm-content": { padding: "4px 0" },
  ".cm-gutters": {
    backgroundColor: "var(--bg-side)",
    borderRight: "1px solid var(--border)",
    color: "var(--text-dim)",
  },
  ".cm-activeLine": { backgroundColor: "rgba(255,255,255,0.04)" },
  ".cm-activeLineGutter": { backgroundColor: "rgba(255,255,255,0.04)" },
});

export function createDoc(
  path: string,
  file: TextFile,
  onDocChanged: (path: string, state: EditorState) => void,
  targetLine?: number,
  lspExtensions: import("@codemirror/state").Extension[] = [],
  syntaxExtensions: import("@codemirror/state").Extension[] = [],
): DocEntry {
  const readOnly = file.tier === "readOnlyPlain";
  const lines = file.text.split("\n");
  let selection: ReturnType<typeof EditorSelection.cursor> | undefined;
  if (targetLine !== undefined && targetLine > 1) {
    const index = Math.min(Math.max(targetLine, 1), lines.length) - 1;
    let offset = 0;
    for (let i = 0; i < index; i++) {
      offset += lines[i].length + 1;
    }
    selection = EditorSelection.cursor(offset);
  }
  const state = EditorState.create({
    doc: file.text,
    selection,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightActiveLine(),
      drawSelection(),
      // 无此扩展时 Lezer parser 照常运行但不渲染 token span（无高亮样式回退）
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      editorTheme,
      EditorView.editable.of(!readOnly),
      EditorState.readOnly.of(readOnly),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.selectionSet) {
          onDocChanged(path, update.state);
        }
      }),
      ...syntaxExtensions,
      ...lspExtensions,
    ],
  });
  return {
    path,
    name: path.split(/[\\/]/).pop() ?? path,
    state,
    originalText: file.text,
    encoding: file.encoding,
    bom: file.bom,
    eol: file.eol,
    tier: file.tier,
    readOnly,
  };
}

/** 脏标记：当前文本与打开时不一致。 */
export function isDirty(doc: DocEntry): boolean {
  return doc.state.doc.toString() !== doc.originalText;
}

export function cursorInfo(state: EditorState): { line: number; col: number } {
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  return { line: line.number, col: head - line.from + 1 };
}
