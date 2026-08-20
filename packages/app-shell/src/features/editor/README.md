# editor

App-shell wrapper around `@ora/editor` for prompt boxes.

## Responsibilities

- Own the Tiptap instance for chat and HITL (`ComposerEditor`).
- Seed and `replaceText` from `documentPlainText` via `markdownToComposerContent`
  so HITL drafts remount as the same nodes rather than leftover `**` / `#`.
  Typing an opener in front of existing closers (`**`, `==`, `~~`, and the
  rest of the prompt Markdown surface) stays source until a trailing space or
  newline, which then renders that line only. Backspace against contiguous
  converted mark runs restores their Markdown source so deletes edit real characters.
- Map Enter to submit only in a body paragraph. Inside a quote, list, heading,
  or fenced code block, Enter returns to body text in one step. Shift+Enter is
  the newline inside those structures, and also opens a fence from an opener
  line.
- Surface slash/`@` query state from the text immediately before the caret so
  `/` still opens skills/commands after existing prompt text, and `@` drives
  workspace file mentions (chips) owned by the chat composer palette.
- Style kit nodes with Ora CSS variables. Links match the dashboard underline
  and open in the host browser on click. File `@` mentions render as inline
  type-icon + basename refs (Tabler via `WorkspaceFileIcon` / React node view),
  not bordered pills: soft teal basename ink with type-colored icons.
  `==highlight==` is a Typora yellow (`rgb(255, 255, 0)`; delimiters hidden).
  `/` skills and `$` commands are mint-wash pills with forest green ink
  (Cursor-style; no neon glow).
- Sent user messages stay `documentPlainText` in the store and render read-only
  via chat `MarkdownDocument` (`density="compact"`). Compact mode expands
  TipTap single newlines outside fences and maps `==highlight==` to `<mark>`.
  Future edit remounts `ComposerEditor` on that same string; history rows do
  not keep a TipTap instance.

## Non-responsibilities

- Chat chrome (attachments, model picker, plus menu).
- HITL gate / draft store ownership.
- Spec document editing.
- Read-only rendering of sent user/assistant history (owned by chat
  `MarkdownDocument` / `MarkdownMessage`).

## Performance

The editor is uncontrolled (`shouldRerenderOnTransaction: false`). Parents
re-render on slash/`@`/blankness changes, not on each character of a normal
sentence. HITL drafts subscribe via `onTextChange` and reload through
`markdownToComposerContent` so overlay/embedded remounts keep formatted nodes.
