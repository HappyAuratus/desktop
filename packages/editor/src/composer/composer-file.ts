import { Node, mergeAttributes, type JSONContent } from "@tiptap/core";

export interface ComposerFileAttrs {
  path: string;
  startLine?: number;
  endLine?: number;
  /** When `directory`, the chip renders a folder glyph; payload stays a path. */
  kind?: "file" | "directory";
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    composerFile: {
      /** Inserts workspace file chips without round-tripping through plain text. */
      insertComposerFiles: (files: ComposerFileAttrs[]) => ReturnType;
    };
  }
}

/**
 * Visible label for a file chip: basename plus an optional line range.
 */
export function composerFileLabel(attrs: ComposerFileAttrs): string {
  const name = fileName(attrs.path);
  const range = lineRange(attrs);
  return range === null ? name : `${name}:${range}`;
}

/**
 * Wire format matching the previous backtick path:line payload the agent sees.
 */
export function composerFilePlainText(attrs: ComposerFileAttrs): string {
  const range = lineRange(attrs);
  const target = range === null ? attrs.path : `${attrs.path}:${range}`;
  return `\`${target}\``;
}

function fileName(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function lineRange(attrs: ComposerFileAttrs): string | null {
  if (attrs.startLine === undefined) {
    return null;
  }
  if (attrs.endLine === undefined || attrs.endLine === attrs.startLine) {
    return String(attrs.startLine);
  }
  return `${attrs.startLine}-${attrs.endLine}`;
}

function fileContent(files: ComposerFileAttrs[]): JSONContent[] {
  return files.flatMap((file) => [
    {
      type: "composerFile",
      attrs: {
        path: file.path,
        startLine: file.startLine ?? null,
        endLine: file.endLine ?? null,
        kind: file.kind ?? "file",
      },
    },
    { type: "text", text: " " },
  ]);
}

/**
 * Inline file-range chip for explorer selections. Atom so typing after it
 * stays body text, same exclusive model as link chips.
 */
export const ComposerFile = Node.create({
  name: "composerFile",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: true,

  addAttributes() {
    return {
      path: { default: "" },
      startLine: { default: null },
      endLine: { default: null },
      kind: { default: "file" },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-composer-file]",
        getAttrs: (element) => {
          if (!(element instanceof HTMLElement)) return false;
          const kindAttr = element.getAttribute("data-kind");
          const kind = kindAttr === "directory" ? "directory" : "file";
          const startLine = element.getAttribute("data-start-line");
          const endLine = element.getAttribute("data-end-line");
          return {
            path: element.getAttribute("data-composer-file") ?? "",
            kind,
            startLine: startLine === null ? null : Number(startLine),
            endLine: endLine === null ? null : Number(endLine),
          };
        },
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    const attrs: ComposerFileAttrs = {
      path: String(node.attrs.path),
      startLine:
        node.attrs.startLine === null
          ? undefined
          : Number(node.attrs.startLine),
      endLine:
        node.attrs.endLine === null ? undefined : Number(node.attrs.endLine),
      kind: node.attrs.kind === "directory" ? "directory" : "file",
    };
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-composer-file": attrs.path,
        "data-kind": attrs.kind ?? "file",
        ...(attrs.startLine === undefined
          ? {}
          : { "data-start-line": String(attrs.startLine) }),
        ...(attrs.endLine === undefined
          ? {}
          : { "data-end-line": String(attrs.endLine) }),
        class: "composer-chip composer-chip-file",
        contenteditable: "false",
        title: composerFilePlainText(attrs).replace(/`/g, ""),
      }),
      ["span", { class: "composer-chip-glyph", "aria-hidden": "true" }],
      ["span", { class: "composer-chip-label" }, composerFileLabel(attrs)],
    ];
  },

  renderText({ node }) {
    return composerFilePlainText({
      path: String(node.attrs.path),
      startLine:
        node.attrs.startLine === null
          ? undefined
          : Number(node.attrs.startLine),
      endLine:
        node.attrs.endLine === null ? undefined : Number(node.attrs.endLine),
    });
  },

  addCommands() {
    return {
      insertComposerFiles:
        (files) =>
        ({ editor, commands }) => {
          if (files.length === 0) {
            return false;
          }
          const content = fileContent(files);
          if (editor.isEmpty) {
            return commands.setContent({
              type: "doc",
              content: [{ type: "paragraph", content }],
            });
          }
          return commands.insertContent(content);
        },
    };
  },
});
