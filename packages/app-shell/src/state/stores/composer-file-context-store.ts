import { create } from "zustand";

export interface ComposerFileSelection {
  path: string;
  startLine: number;
  endLine: number;
  /** Line text captured at quote time for eager agent context. */
  snippet?: string;
  origin?: "diff";
  /** Present when every quoted line is the same side; mixed add/delete omits it. */
  diffSide?: "old" | "new";
}

export interface PendingFileContext {
  id: number;
  selections: ComposerFileSelection[];
}

type ComposerFileDelivery = (selections: ComposerFileSelection[]) => void;

interface ComposerFileContextState {
  /**
   * Quotes waiting because that conversation's composer is not mounted.
   * Bound composers receive quotes via `bindDelivery` and never re-read this
   * list, so a session switch cannot replay chips the user already deleted.
   */
  pendingByConversation: Record<string, PendingFileContext[] | undefined>;
  /**
   * Queues a workspace-relative line range for one conversation's composer.
   * Returns false when the same path/range/origin is already pending.
   */
  addSelection: (
    conversationKey: string,
    selection: ComposerFileSelection,
  ) => boolean;
  /**
   * Delivers quotes to the bound composer, or queues them until it binds.
   * Returns false when every item was already pending.
   */
  addSelections: (
    conversationKey: string,
    selections: ComposerFileSelection[],
  ) => boolean;
  /**
   * Registers the active composer's insert handler for `conversationKey`.
   * Drains any queued batches once, then every later quote goes to the
   * handler (not back into this list). Unbind on unmount / key change so a
   * stale composer cannot steal another session's quotes.
   */
  bindDelivery: (
    conversationKey: string,
    deliver: ComposerFileDelivery,
  ) => () => void;
  /** Test helper: drop a queued conversation without delivering. */
  consumeSelections: (conversationKey: string, requestId: number) => void;
}

let nextRequestId = 0;

/** Live insert handlers, keyed like `pendingByConversation`. Not in Zustand so binding does not re-render. */
const deliveries = new Map<string, ComposerFileDelivery>();

/** Drops live handlers so tests cannot leak a bound composer into the next case. */
export function resetComposerFileDeliveriesForTests(): void {
  deliveries.clear();
}

function sameSelection(
  left: ComposerFileSelection,
  right: ComposerFileSelection,
): boolean {
  return (
    left.path === right.path &&
    left.startLine === right.startLine &&
    left.endLine === right.endLine &&
    left.origin === right.origin &&
    left.diffSide === right.diffSide
  );
}

function queuedSelections(
  queue: readonly PendingFileContext[] | undefined,
): ComposerFileSelection[] {
  return queue?.flatMap((batch) => batch.selections) ?? [];
}

function takeQueuedSelections(
  pendingByConversation: ComposerFileContextState["pendingByConversation"],
  conversationKey: string,
): ComposerFileSelection[] {
  const queue = pendingByConversation[conversationKey];
  if (queue === undefined || queue.length === 0) return [];
  return queue.flatMap((batch) => batch.selections);
}

/** Bridges file-explorer actions to the conversation composer without coupling the two views. */
export const useComposerFileContextStore = create<ComposerFileContextState>(
  (set, get) => ({
    pendingByConversation: {},
    addSelection: (conversationKey, selection) =>
      get().addSelections(conversationKey, [selection]),
    addSelections: (conversationKey, selections) => {
      if (selections.length === 0) return false;
      const deliver = deliveries.get(conversationKey);
      if (deliver !== undefined) {
        // TipTap insert must not run inside the quote event / Zustand stack.
        // Do not write `pendingByConversation`: rebinding the composer would
        // replay chips the user already had (or had deleted).
        queueMicrotask(() => {
          const current = deliveries.get(conversationKey);
          if (current !== deliver) {
            get().addSelections(conversationKey, selections);
            return;
          }
          try {
            current(selections);
          } catch {
            // TipTap may already be destroyed if the composer unmounted
            // between quote and this microtask.
          }
        });
        return true;
      }
      const queue = get().pendingByConversation[conversationKey];
      const existing = queuedSelections(queue);
      const fresh = selections.filter(
        (selection) =>
          !existing.some((candidate) => sameSelection(candidate, selection)),
      );
      if (fresh.length === 0) return false;
      set({
        pendingByConversation: {
          ...get().pendingByConversation,
          [conversationKey]: [
            ...(queue ?? []),
            { id: ++nextRequestId, selections: fresh },
          ],
        },
      });
      return true;
    },
    bindDelivery: (conversationKey, deliver) => {
      deliveries.set(conversationKey, deliver);
      // Drain after hydrate's microtask (bind is declared later in Composer).
      let cancelled = false;
      queueMicrotask(() => {
        if (cancelled) return;
        if (deliveries.get(conversationKey) !== deliver) return;
        const queued = takeQueuedSelections(
          get().pendingByConversation,
          conversationKey,
        );
        if (queued.length === 0) return;
        set((state) => {
          if (state.pendingByConversation[conversationKey] === undefined) {
            return state;
          }
          const pendingByConversation = { ...state.pendingByConversation };
          delete pendingByConversation[conversationKey];
          return { pendingByConversation };
        });
        deliver(queued);
      });
      return () => {
        cancelled = true;
        if (deliveries.get(conversationKey) === deliver) {
          deliveries.delete(conversationKey);
        }
      };
    },
    consumeSelections: (conversationKey, requestId) => {
      set((state) => {
        const queue = state.pendingByConversation[conversationKey];
        if (queue === undefined) return state;
        const next = queue.filter((batch) => batch.id !== requestId);
        if (next.length === queue.length) return state;
        const pendingByConversation = { ...state.pendingByConversation };
        if (next.length === 0) {
          delete pendingByConversation[conversationKey];
        } else {
          pendingByConversation[conversationKey] = next;
        }
        return { pendingByConversation };
      });
    },
  }),
);
