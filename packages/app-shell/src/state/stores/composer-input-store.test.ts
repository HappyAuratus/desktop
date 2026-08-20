import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  COMPOSER_INPUT_STORAGE_KEY,
  composerInputHasContent,
  useComposerInputStore,
} from "./composer-input-store";
import { flushDebouncedPersistStorage } from "./debounced-json-storage";

beforeEach(() => {
  flushDebouncedPersistStorage();
  window.localStorage.clear();
  useComposerInputStore.getState().reset();
  flushDebouncedPersistStorage();
  window.localStorage.clear();
});

afterEach(() => {
  useComposerInputStore.getState().reset();
  flushDebouncedPersistStorage();
  window.localStorage.clear();
});

describe("composerInputHasContent", () => {
  it("treats whitespace-only text as empty unless images are parked", () => {
    expect(composerInputHasContent({ text: "  ", images: [] })).toBe(false);
    expect(
      composerInputHasContent({
        text: "",
        images: [
          {
            id: "i1",
            name: "a.png",
            size: 1,
            content: { mimeType: "image/png", data: "aa" },
          },
        ],
      }),
    ).toBe(true);
  });
});

describe("useComposerInputStore", () => {
  it("parks and clears input per conversation key", () => {
    useComposerInputStore.getState().setInput("s1", {
      text: "hello",
      images: [],
    });
    useComposerInputStore.getState().setInput("s2", {
      text: "other",
      images: [],
    });

    expect(useComposerInputStore.getState().byKey.s1).toEqual({
      text: "hello",
      images: [],
    });

    useComposerInputStore.getState().clear("s1");
    expect(useComposerInputStore.getState().byKey.s1).toBeUndefined();
    expect(useComposerInputStore.getState().byKey.s2?.text).toBe("other");
  });

  it("drops empty payloads instead of storing blanks", () => {
    useComposerInputStore.getState().setInput("s1", {
      text: "keep",
      images: [],
    });
    useComposerInputStore.getState().setInput("s1", { text: "  ", images: [] });
    expect(useComposerInputStore.getState().byKey.s1).toBeUndefined();
  });

  it("rekeys parked input onto a new conversation id", () => {
    useComposerInputStore.getState().setInput("draft:d1", {
      text: "first send",
      images: [],
    });
    useComposerInputStore.getState().rekey("draft:d1", "session-1");

    expect(useComposerInputStore.getState().byKey["draft:d1"]).toBeUndefined();
    expect(useComposerInputStore.getState().byKey["session-1"]?.text).toBe(
      "first send",
    );
  });

  it("persists typed text to localStorage without image payloads", () => {
    useComposerInputStore.getState().setInput("s1", {
      text: "parked",
      images: [
        {
          id: "i1",
          name: "a.png",
          size: 1,
          content: { mimeType: "image/png", data: "aa" },
        },
      ],
    });
    flushDebouncedPersistStorage();

    const raw = window.localStorage.getItem(COMPOSER_INPUT_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as {
      state: { byKey: Record<string, { text: string; images: unknown[] }> };
    };
    expect(parsed.state.byKey.s1).toEqual({
      text: "parked",
      images: [],
    });
    // In-process memory still keeps the attachment for the current session.
    expect(useComposerInputStore.getState().byKey.s1?.images).toHaveLength(1);
  });

  it("rehydrates text-only parks from localStorage", async () => {
    useComposerInputStore.getState().reset();
    flushDebouncedPersistStorage();
    window.localStorage.setItem(
      COMPOSER_INPUT_STORAGE_KEY,
      JSON.stringify({
        state: {
          byKey: { s1: { text: "survive restart", images: [] } },
        },
        version: 0,
      }),
    );
    await useComposerInputStore.persist.rehydrate();
    expect(useComposerInputStore.getState().byKey.s1).toEqual({
      text: "survive restart",
      images: [],
    });
  });

  it("does not persist image-only parks", () => {
    useComposerInputStore.getState().setInput("s1", {
      text: "",
      images: [
        {
          id: "i1",
          name: "a.png",
          size: 1,
          content: { mimeType: "image/png", data: "aa" },
        },
      ],
    });
    expect(useComposerInputStore.getState().byKey.s1?.images).toHaveLength(1);
    flushDebouncedPersistStorage();

    const raw = window.localStorage.getItem(COMPOSER_INPUT_STORAGE_KEY);
    const parsed = JSON.parse(raw ?? '{"state":{"byKey":{}}}') as {
      state: { byKey: Record<string, unknown> };
    };
    expect(parsed.state.byKey.s1).toBeUndefined();
  });
});
