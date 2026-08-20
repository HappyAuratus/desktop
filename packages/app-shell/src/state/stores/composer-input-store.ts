import { create } from "zustand";
import { persist } from "zustand/middleware";
import type * as acp from "@agentclientprotocol/sdk";
import { createDebouncedJSONStorage } from "./debounced-json-storage";

/** One image parked in an unsent composer draft (memory only; not written to disk). */
export interface ParkedComposerImage {
  id: string;
  name: string;
  size: number;
  content: acp.ImageContent;
}

export interface ParkedComposerInput {
  text: string;
  images: ParkedComposerImage[];
  /**
   * Remembers that attachments existed in this process after image bytes are
   * stripped for localStorage. Image-only parks still do not survive restart.
   */
  retainedAttachments?: boolean;
}

interface ComposerInputState {
  /**
   * Unsent composer text/images keyed by `conversationKeyFor`. The composer
   * component is reused across switches, so parking here is what restores a
   * half-typed message when the user comes back to that session or draft.
   * Text survives restarts via localStorage; images stay in-process only.
   */
  byKey: Record<string, ParkedComposerInput>;
  /** Replaces the parked payload for one conversation. */
  setInput: (key: string, input: ParkedComposerInput) => void;
  /** Drops parked input when the conversation is sent or discarded. */
  clear: (key: string) => void;
  /** Drops every parked entry whose key is in the given set. */
  clearKeys: (keys: Iterable<string>) => void;
  /** Moves parked input onto a newly minted session id after first send. */
  rekey: (fromKey: string, toKey: string) => void;
  /** Test helper so cases cannot leak parked text into each other. */
  reset: () => void;
}

const EMPTY: ParkedComposerInput = { text: "", images: [] };

export const COMPOSER_INPUT_STORAGE_KEY = "ora.composer-input.v1";

/** True when leaving this conversation should keep the parked payload. */
export function composerInputHasContent(input: ParkedComposerInput): boolean {
  return (
    input.text.trim().length > 0 ||
    input.images.length > 0 ||
    input.retainedAttachments === true
  );
}

/**
 * Disk shape for one parked conversation: typed text only. Image-only parks
 * stay in memory for the current process; restoring an empty retained stub
 * after restart looked like a blank composer with nothing to recover.
 */
function textOnlyPark(input: ParkedComposerInput): ParkedComposerInput | null {
  if (input.text.trim().length === 0) return null;
  return { text: input.text, images: [] };
}

/** Keeps only entries that still have typed text or retained attachments. */
function sanitizeParkedByKey(
  byKey: Record<string, ParkedComposerInput> | undefined,
): Record<string, ParkedComposerInput> {
  if (byKey === undefined) return {};
  const next: Record<string, ParkedComposerInput> = {};
  for (const [key, input] of Object.entries(byKey)) {
    const parked = textOnlyPark(input);
    if (parked !== null) next[key] = parked;
  }
  return next;
}

/**
 * Parks unsent composer contents per conversation so switching sessions does
 * not throw away a half-written message. Typed text is mirrored to
 * localStorage (frontend only); attached images remain in memory for the
 * current process.
 */
export const useComposerInputStore = create<ComposerInputState>()(
  persist(
    (set, get) => ({
      byKey: {},
      setInput: (key, input) =>
        set((state) => {
          const previous = state.byKey[key] ?? EMPTY;
          const next: ParkedComposerInput = {
            text: input.text,
            images: input.images,
            ...(input.images.length > 0 || input.retainedAttachments === true
              ? { retainedAttachments: true }
              : {}),
          };
          if (
            previous.text === next.text &&
            previous.images === next.images &&
            previous.retainedAttachments === next.retainedAttachments
          ) {
            return state;
          }
          if (!composerInputHasContent(next)) {
            if (!(key in state.byKey)) return state;
            const byKey = { ...state.byKey };
            delete byKey[key];
            return { byKey };
          }
          return {
            byKey: {
              ...state.byKey,
              [key]: next,
            },
          };
        }),
      clear: (key) =>
        set((state) => {
          if (!(key in state.byKey)) return state;
          const byKey = { ...state.byKey };
          delete byKey[key];
          return { byKey };
        }),
      clearKeys: (keys) => {
        const removing = new Set(keys);
        if (removing.size === 0) return;
        set((state) => {
          let changed = false;
          const byKey = { ...state.byKey };
          for (const key of removing) {
            if (key in byKey) {
              delete byKey[key];
              changed = true;
            }
          }
          return changed ? { byKey } : state;
        });
      },
      rekey: (fromKey, toKey) => {
        if (fromKey === toKey) return;
        const parked = get().byKey[fromKey];
        if (parked === undefined) return;
        set((state) => {
          const byKey = { ...state.byKey };
          delete byKey[fromKey];
          byKey[toKey] = parked;
          return { byKey };
        });
      },
      reset: () => set({ byKey: {} }),
    }),
    {
      name: COMPOSER_INPUT_STORAGE_KEY,
      // Keystroke parks coalesce; pagehide / visibility flush for durability.
      storage: createDebouncedJSONStorage(),
      // Never write image payloads; restart restores text and drops attachments.
      partialize: (state) => ({
        byKey: sanitizeParkedByKey(state.byKey),
      }),
      merge: (persisted, current) => {
        const slice = persisted as Partial<ComposerInputState> | undefined;
        return {
          ...current,
          byKey: sanitizeParkedByKey(slice?.byKey),
        };
      },
    },
  ),
);
