import type { JSONContent } from "@tiptap/core";
import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

type MarkSpec = { type: string; attrs?: Record<string, string | null> };

const HEADING = /^(#{1,6})(?:\s+|$)(.*)$/;
const TASK = /^(\s*)[-*+]\s+\[([ xX])\]\s+(.*)$/;
const BULLET = /^(\s*)[-*+]\s+(.*)$/;
const ORDERED = /^(\s*)(\d+)\.\s+(.*)$/;
const FENCE = /^```([^\s`]*)$/;
const RULE = /^(?:---|\*\*\*|___)\s*$/;
const HTML_CLIPBOARD =
  /<(p|div|h[1-6]|ul|ol|li|pre|blockquote|strong|em|a|code|hr)\b/i;
const INLINE_MARKDOWN =
  /(\*\*\*(?!\s)[^*]+(?<!\s)\*\*\*|\*\*(?!\s)[^*]+(?<!\s)\*\*|__(?!\s)[^_]+(?<!\s)__|(?<!\*)\*(?![*\s])[^*]+(?<!\s)\*(?!\*)|(?<![A-Za-z0-9_])_(?![_\s])[^_]+(?<!\s)_(?![A-Za-z0-9_])|~~(?!\s)[^~]+(?<!\s)~~|==(?!\s)[^=]+(?<!\s)==|`[^`]+`|\[[^\]]+\]\([^)]+\))/;
/** Prompt pastes are small; cap quote recursion so `>>>>>>>>>>…` cannot blow the stack. */
const MAX_QUOTE_DEPTH = 32;

/**
 * True when clipboard text uses the composer's Markdown surface, so paste
 * should build nodes instead of dumping literal `#` / `**` into a paragraph.
 */
export function looksLikeComposerMarkdown(text: string): boolean {
  if (text.length === 0) {
    return false;
  }
  return (
    /^(#{1,6}\s|\s*[-*+]\s|\s*\d+\.\s|>\s|```|---|___|\*\*\*)/m.test(text) ||
    INLINE_MARKDOWN.test(text)
  );
}

/**
 * Turns Markdown that the prompt box can represent into Tiptap JSON.
 * Inverse of `documentPlainText` for paste and HITL draft restore.
 * HTML tags stay text so `<script>` cannot become markup.
 */
export function markdownToComposerContent(text: string): JSONContent {
  const blocks = parseBlocks(text.replace(/\r\n/g, "\n").split("\n"));
  return {
    type: "doc",
    content: blocks.length === 0 ? [{ type: "paragraph" }] : blocks,
  };
}

/**
 * Pastes Markdown as composer nodes when the clipboard is plain text.
 * HTML copies (browser/editor) keep ProseMirror's default path.
 */
export const ComposerMarkdownPaste = Extension.create({
  name: "composerMarkdownPaste",
  priority: 50,

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("composerMarkdownPaste"),
        props: {
          handlePaste: (_view, event) => {
            if (event.clipboardData === null) {
              return false;
            }
            if (event.clipboardData.files.length > 0) {
              return false;
            }
            const html = event.clipboardData.getData("text/html");
            if (html.length > 0 && HTML_CLIPBOARD.test(html)) {
              return false;
            }
            const text = event.clipboardData.getData("text/plain");
            if (!looksLikeComposerMarkdown(text)) {
              return false;
            }
            const doc = markdownToComposerContent(text);
            return this.editor.commands.insertContent(doc.content ?? []);
          },
        },
      }),
    ];
  },
});

function parseBlocks(lines: string[], quoteDepth = 0): JSONContent[] {
  const blocks: JSONContent[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (line === undefined) {
      break;
    }

    const fence = FENCE.exec(line);
    if (fence !== null) {
      const language = fence[1] === "" ? null : fence[1];
      const body: string[] = [];
      index += 1;
      while (index < lines.length && lines[index] !== "```") {
        body.push(lines[index] ?? "");
        index += 1;
      }
      if (index < lines.length && lines[index] === "```") {
        index += 1;
      }
      blocks.push(codeBlock(language, body.join("\n")));
      continue;
    }

    if (RULE.test(line) && line.trim().length > 0) {
      blocks.push({ type: "horizontalRule" });
      index += 1;
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading !== null) {
      blocks.push({
        type: "heading",
        attrs: { level: heading[1].length },
        content: parseInline(heading[2] ?? ""),
      });
      index += 1;
      continue;
    }

    if (line.startsWith(">")) {
      if (quoteDepth >= MAX_QUOTE_DEPTH) {
        blocks.push(paragraph(line));
        index += 1;
        continue;
      }
      const quoted = parseQuoteRun(lines, index, quoteDepth);
      blocks.push({
        type: "blockquote",
        content:
          quoted.inner.length === 0 ? [{ type: "paragraph" }] : quoted.inner,
      });
      index = quoted.next;
      continue;
    }

    if (TASK.test(line) || BULLET.test(line) || ORDERED.test(line)) {
      const { node, next } = parseList(lines, index);
      blocks.push(node);
      index = next;
      continue;
    }

    if (line.length === 0) {
      index += 1;
      continue;
    }

    blocks.push(paragraph(line));
    index += 1;
  }

  return blocks;
}

/**
 * Consecutive `>` lines are one quote. Peel a single marker and parse the
 * rest as blocks so `> >` nests and `> - item` becomes a list inside.
 */
function parseQuoteRun(
  lines: string[],
  start: number,
  quoteDepth: number,
): { inner: JSONContent[]; next: number } {
  const peeled: string[] = [];
  let index = start;
  while (index < lines.length) {
    const line = lines[index];
    if (line === undefined || !line.startsWith(">")) {
      break;
    }
    peeled.push(line.startsWith("> ") ? line.slice(2) : line.slice(1));
    index += 1;
  }
  return { inner: parseBlocks(peeled, quoteDepth + 1), next: index };
}

function paragraph(text: string): JSONContent {
  const content = parseInline(text);
  return content.length === 0
    ? { type: "paragraph" }
    : { type: "paragraph", content };
}

function codeBlock(language: string | null, text: string): JSONContent {
  const node: JSONContent = {
    type: "codeBlock",
    attrs: { language },
  };
  if (text.length > 0) {
    node.content = [{ type: "text", text }];
  }
  return node;
}

function parseList(
  lines: string[],
  start: number,
): { node: JSONContent; next: number } {
  const first = lines[start] ?? "";
  const isTask = TASK.test(first);
  const isOrdered = !isTask && ORDERED.test(first);
  const items: JSONContent[] = [];
  let index = start;

  while (index < lines.length) {
    const line = lines[index] ?? "";
    const task = TASK.exec(line);
    const bullet = BULLET.exec(line);
    const ordered = ORDERED.exec(line);
    if (isTask) {
      if (task === null) {
        break;
      }
      items.push({
        type: "taskItem",
        attrs: { checked: task[2].toLowerCase() === "x" },
        content: [paragraph(task[3] ?? "")],
      });
      index += 1;
      continue;
    }
    if (isOrdered) {
      if (ordered === null) {
        break;
      }
      items.push({
        type: "listItem",
        content: [paragraph(ordered[3] ?? "")],
      });
      index += 1;
      continue;
    }
    if (bullet === null || TASK.test(line)) {
      break;
    }
    items.push({
      type: "listItem",
      content: [paragraph(bullet[2] ?? "")],
    });
    index += 1;
  }

  return {
    node: {
      type: isTask ? "taskList" : isOrdered ? "orderedList" : "bulletList",
      content: items,
    },
    next: index,
  };
}

/**
 * Parses leftover source from the start of each remainder so a shared `***`
 * run can close bold and then open italic (`**a***b*`) without seeing the
 * already-consumed stars as part of the next opener.
 */
function parseInline(text: string): JSONContent[] {
  const nodes: JSONContent[] = [];
  let rest = text;

  while (rest.length > 0) {
    if (rest[0] === "`") {
      const close = rest.indexOf("`", 1);
      if (close !== -1) {
        pushText(nodes, rest.slice(1, close), [{ type: "code" }]);
        rest = rest.slice(close + 1);
        continue;
      }
    }

    if (rest.startsWith("[")) {
      const last = nodes.at(-1);
      // `![alt](url)` is split so `[` is at the start of `rest`; keep it text.
      const afterImageBang =
        last?.type === "text" &&
        typeof last.text === "string" &&
        last.text.endsWith("!");
      const link = afterImageBang ? null : parseLink(rest, 0);
      if (link !== null) {
        const attrs: Record<string, string | null> = { href: link.href };
        if (link.title !== undefined) {
          attrs.title = link.title;
        }
        let inner = parseInline(link.label);
        inner = withMark(inner, { type: "link", attrs });
        nodes.push(...inner);
        rest = rest.slice(link.end);
        continue;
      }
    }

    const wrapped = takeWrapped(rest, 0);
    if (wrapped !== null) {
      let inner = parseInline(wrapped.inner);
      for (const mark of wrapped.marks) {
        inner = withMark(inner, mark);
      }
      nodes.push(...inner);
      rest = rest.slice(wrapped.end);
      continue;
    }

    const next = nextSpecial(rest, 1);
    pushText(nodes, rest.slice(0, next), []);
    rest = rest.slice(next);
  }

  return nodes;
}

/**
 * GFM inline link, including an optional quoted title. Images (`![...](...)`)
 * stay text; the prompt box does not own image nodes.
 */
function parseLink(
  text: string,
  index: number,
): { label: string; href: string; title?: string; end: number } | null {
  if (index > 0 && text[index - 1] === "!") {
    return null;
  }
  const closeLabel = text.indexOf("](", index);
  if (closeLabel === -1 || closeLabel === index + 1) {
    return null;
  }
  const label = text.slice(index + 1, closeLabel);
  if (label.includes("[")) {
    return null;
  }
  const taken = takeLinkDestination(text, closeLabel + 2);
  if (taken === null) {
    return null;
  }
  let cursor = taken.end;
  while (cursor < text.length && /\s/.test(text[cursor] ?? "")) {
    cursor += 1;
  }
  let title: string | undefined;
  const quote = text[cursor];
  if (quote === '"' || quote === "'") {
    const closeQuote = text.indexOf(quote, cursor + 1);
    if (closeQuote === -1) {
      return null;
    }
    const quoted = text.slice(cursor + 1, closeQuote);
    title = quoted.length === 0 ? undefined : quoted;
    cursor = closeQuote + 1;
    while (cursor < text.length && /\s/.test(text[cursor] ?? "")) {
      cursor += 1;
    }
  }
  if (text[cursor] !== ")") {
    return null;
  }
  return { label, href: taken.href, title, end: cursor + 1 };
}

function takeLinkDestination(
  text: string,
  start: number,
): { href: string; end: number } | null {
  if (text[start] === "<") {
    const close = text.indexOf(">", start + 1);
    if (close === -1) {
      return null;
    }
    const href = text.slice(start + 1, close);
    return href.length === 0 ? null : { href, end: close + 1 };
  }
  let cursor = start;
  let depth = 0;
  while (cursor < text.length) {
    const char = text[cursor];
    if (
      char === undefined ||
      (depth === 0 && (char === ")" || /\s/.test(char)))
    ) {
      break;
    }
    if (char === "(") {
      depth += 1;
    } else if (char === ")") {
      depth -= 1;
    }
    cursor += 1;
  }
  const href = text.slice(start, cursor);
  return href.length === 0 ? null : { href, end: cursor };
}

/**
 * Longest delimiter first, then the nearest closer. Extra stars after a
 * closer stay on the remainder (`**bold***em*`) which parseInline slices off.
 */
function takeWrapped(
  text: string,
  index: number,
): { inner: string; marks: MarkSpec[]; end: number } | null {
  const delimiters: Array<{ token: string; marks: MarkSpec[] }> = [
    { token: "***", marks: [{ type: "italic" }, { type: "bold" }] },
    { token: "**", marks: [{ type: "bold" }] },
    { token: "__", marks: [{ type: "bold" }] },
    { token: "~~", marks: [{ type: "strike" }] },
    { token: "==", marks: [{ type: "highlight" }] },
    { token: "*", marks: [{ type: "italic" }] },
    { token: "_", marks: [{ type: "italic" }] },
  ];
  for (const delimiter of delimiters) {
    if (!gfmCanOpen(text, index, delimiter.token)) {
      continue;
    }
    const start = index + delimiter.token.length;
    const close = text.indexOf(delimiter.token, start);
    if (close > start && gfmCanClose(text, close, delimiter.token)) {
      return {
        inner: text.slice(start, close),
        marks: delimiter.marks,
        end: close + delimiter.token.length,
      };
    }
  }
  return null;
}

/**
 * Same flanking as ComposerBold / ComposerItalic / ComposerStrike: no space
 * after the opener or before the closer, and `_` does not fire inside snake_case.
 */
function gfmCanOpen(text: string, index: number, token: string): boolean {
  if (!text.startsWith(token, index)) {
    return false;
  }
  const next = text[index + token.length];
  if (next !== undefined && /\s/.test(next)) {
    return false;
  }
  if (token === "***" || token === "**" || token === "*") {
    if (index > 0 && text[index - 1] === "*") {
      return false;
    }
    if (next === "*") {
      return false;
    }
  }
  if (token === "_" || token === "__") {
    if (index > 0 && text[index - 1] === "_") {
      return false;
    }
    if (
      token === "_" &&
      index > 0 &&
      /[A-Za-z0-9]/.test(text[index - 1] ?? "")
    ) {
      return false;
    }
    if (token === "_" && next === "_") {
      return false;
    }
  }
  return true;
}

function gfmCanClose(text: string, close: number, token: string): boolean {
  const prev = text[close - 1];
  if (prev !== undefined && /\s/.test(prev)) {
    return false;
  }
  // Extra `*` / `_` after the closer stay in the run so `**bold***em*` can
  // close bold with two stars and open italic with the leftover one. Openers
  // already refuse to start in the middle of a longer token.
  const after = text[close + token.length];
  if (token === "_" && after !== undefined && /[A-Za-z0-9]/.test(after)) {
    return false;
  }
  return true;
}

function nextSpecial(text: string, from: number): number {
  for (let index = from; index < text.length; index += 1) {
    const char = text[index];
    if (
      char === "`" ||
      char === "[" ||
      char === "*" ||
      char === "_" ||
      char === "~" ||
      char === "="
    ) {
      return index;
    }
  }
  return text.length;
}

function withMark(nodes: JSONContent[], mark: MarkSpec): JSONContent[] {
  return nodes.map((node) => {
    if (node.type !== "text" || typeof node.text !== "string") {
      return node;
    }
    return {
      ...node,
      marks: [...(node.marks ?? []), mark],
    };
  });
}

function pushText(nodes: JSONContent[], text: string, marks: MarkSpec[]): void {
  if (text.length === 0) {
    return;
  }
  const node: JSONContent = { type: "text", text };
  if (marks.length > 0) {
    node.marks = marks;
  }
  nodes.push(node);
}
