import type { BlameLine } from "../git";

/** 编辑器行首 blame 装饰。
 *  blameExtension(data) 按 data 构建装饰字段；文档编辑时装饰随 changes 映射。 */
import { Decoration, type DecorationSet, EditorView, WidgetType } from "@codemirror/view";
import { StateField, type Extension } from "@codemirror/state";

export interface BlameEntry {
  author: string;
  date: string;
  summary: string;
}

class BlameWidget extends WidgetType {
  constructor(readonly entry: BlameEntry) {
    super();
  }

  override eq(other: BlameWidget) {
    return (
      this.entry.author === other.entry.author &&
      this.entry.date === other.entry.date &&
      this.entry.summary === other.entry.summary
    );
  }

  override toDOM() {
    const span = document.createElement("span");
    span.className = "blame-widget";
    span.textContent = `${this.entry.author} · ${this.entry.date} · ${this.entry.summary}`;
    return span;
  }

  override ignoreEvent() {
    return false;
  }
}

function buildDecorations(data: Map<number, BlameEntry>, doc: { lines: number; line(n: number): { from: number } }): ReturnType<typeof Decoration.set> {
  const specs: { pos: number; deco: Decoration }[] = [];
  for (const [line, entry] of data) {
    if (line < 1 || line > doc.lines) {
      continue;
    }
    const docLine = doc.line(line);
    specs.push({
      pos: docLine.from,
      deco: Decoration.widget({ widget: new BlameWidget(entry), side: -1 }),
    });
  }
  specs.sort((a, b) => a.pos - b.pos);
  return Decoration.set(specs.map((s) => s.deco.range(s.pos)));
}

const blameTheme = EditorView.theme({
  ".blame-widget": {
    color: "var(--text-dim, #8b96a3)",
    fontSize: "11px",
    marginRight: "16px",
    fontFamily: "'Segoe UI', system-ui, sans-serif",
  },
});

/** blame 装饰扩展：装饰在 state 创建时按数据构建，编辑时随 changes 映射。
 *  关闭 blame 时由调用方重建不含此扩展的 state。 */
export function blameExtension(data: Map<number, BlameEntry>): Extension[] {
  if (data.size === 0) {
    return [];
  }
  const field = StateField.define<DecorationSet>({
    create: (state) => buildDecorations(data, state.doc),
    update(deco, tr) {
      return tr.docChanged ? deco.map(tr.changes) : deco;
    },
    provide: (field) => EditorView.decorations.from(field),
  });
  return [field, blameTheme];
}

/** BlameLine[] → 行号 → 条目 映射（日期 ISO 截断为 YYYY-MM-DD）。 */
export function blameMapOf(lines: BlameLine[]): Map<number, BlameEntry> {
  const map = new Map<number, BlameEntry>();
  for (const line of lines) {
    map.set(line.line, {
      author: line.author,
      date: line.time ? new Date(line.time * 1000).toISOString().slice(0, 10) : "",
      summary: line.summary.slice(0, 40),
    });
  }
  return map;
}
