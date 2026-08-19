# @ora/editor

Tiptap 3 kit used by Ora surfaces that need rich text. The package is a set of
extensions, nodes, and toolbar primitives — it does not own persistence, i18n,
or product chrome.

## Public boundaries

- Diff helpers and the editor node/mark kit.
- `createComposerExtensions()` plus plain-text conversion for prompt boxes
  (`@ora/editor/composer`, no dashboard SCSS theme). `documentPlainText` is the
  payload/draft form; `markdownToComposerContent` is the inverse for paste and
  restore. `COMPOSER_CAPABILITIES` is the prompt-box minimum set; slots can be
  omitted or replaced.
- Toolbar primitives for full-page editing; the chat composer uses `@ora/ui`
  instead so it does not pull the kit's SCSS theme.

## Non-responsibilities

- Chat send / HITL submit behavior (lives in `@ora/app-shell`).
- Spec file editing.
