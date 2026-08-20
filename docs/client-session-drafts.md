# Client Session Drafts

## Purpose

Starting a chat must give immediate sidebar and composer feedback without creating an unused backend Task or Session. The app therefore represents a new chat as a client-only session draft until the user sends the first message. This keeps abandoned chats out of backend persistence while preserving a visible, selectable row for work that has actually started.

## Ownership and state

`draft-sessions-store` owns client-only draft metadata and content. A draft belongs either to a project root (`taskId: null`) or to an existing worktree task. `workspace-selection-store` adds a mutually exclusive `draftId` leg, so a selection cannot point at both a draft and a persisted session. Leaving an empty, idle draft removes it; typed drafts remain available in the sidebar.

`composer-input-store` parks unsent input by conversation key. Persisted sessions use their session id, drafts use `draft:<id>`, and task-only surfaces use `task:<id>`. Moving to another conversation hydrates the composer from that key before the browser paints the new surface, preventing input from leaking between sessions, drafts, or direct-chat tasks.

## Persistence boundary

Typed draft text is mirrored to `localStorage` through a debounced storage adapter. Image bytes remain memory-only because persisting base64 attachments would consume browser storage quickly; after a restart, text is restored and attachments are dropped. Empty drafts and in-flight warm-session ids are also excluded from disk so restart cannot resurrect unusable rows.

Persisted payloads are treated as untrusted runtime data. Rehydration validates containers, entries, identifiers, text, timestamps, and return destinations before constructing store state. A malformed entry is discarded instead of preventing the application from starting. Storage flushes isolate failures per store and per key: a quota or security error cannot block other queued writes, and the failed value remains pending for a later retry.

## First-send lifecycle

The first send follows these stages:

1. Mark the selected draft as `sendInFlight` and show an optimistic user turn immediately.
2. Warm the provider session without moving the user away from the draft.
3. Confirm that the user is still on the same selection, then bind the captured draft id to the warm session id.
4. Rekey composer, plugin, and workflow state onto the final session id and select that session.
5. For a project-root chat, create its Task from the first message; worktree chats reuse their existing Task.
6. Attach the warm Session to the Task. Successful backend attachment is the commit boundary: the client removes the muted draft row and keeps the real session selected.

Every synchronous setup step and asynchronous provider step is inside the same recovery boundary. Before attachment succeeds, failure clears `sendInFlight`, unbinds the warm id, restores text and images to the original draft, and makes that draft dismissible and retryable. After attachment succeeds, failures belong to the real session; rolling back to a draft would risk duplicating an already persisted session.

Stopping or navigating away during warming abandons the handshake without pulling the user back. The original payload is re-parked on the captured draft id, and returning to that row restores the composer. Draft/session reconciliation moves selection onto a newly persisted session before deleting its draft, so `selection.draftId` never points at a removed row.

## Cleanup rules

Deleting a session clears its parked composer input and any bound draft. Deleting a task additionally clears its `task:<id>` park and all drafts in that task. Deleting a project performs the same cleanup for every cached child task and session. Project/task deletion is an explicit destructive action, so its child drafts are removed even if a send was in progress; retaining them would leave rows whose parent no longer exists.

## Verification

The frontend tests cover draft creation and dismissal, structured sidebar search, keyboard focus after dismissal, conversation hydration, corrupt persistence recovery, storage-write isolation and retry, deletion cascades, navigation during warming, synchronous setup failures, attach failures, prompt failures after attachment, and restored attachment byte accounting.
