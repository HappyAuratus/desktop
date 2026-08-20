import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createDebouncedStateStorage,
  DEBOUNCED_PERSIST_MS,
  flushDebouncedPersistStorage,
} from "./debounced-json-storage";

describe("createDebouncedStateStorage", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    flushDebouncedPersistStorage();
    vi.useRealTimers();
    window.localStorage.clear();
  });

  it("coalesces rapid setItem calls into one disk write", () => {
    const storage = createDebouncedStateStorage(DEBOUNCED_PERSIST_MS);
    storage.setItem("k", "a");
    storage.setItem("k", "b");
    storage.setItem("k", "c");

    expect(window.localStorage.getItem("k")).toBeNull();
    expect(storage.getItem("k")).toBe("c");

    vi.advanceTimersByTime(DEBOUNCED_PERSIST_MS);
    expect(window.localStorage.getItem("k")).toBe("c");
  });

  it("flushes pending writes on demand and on pagehide", () => {
    const storage = createDebouncedStateStorage(DEBOUNCED_PERSIST_MS);
    storage.setItem("k", "pending");
    expect(window.localStorage.getItem("k")).toBeNull();

    storage.flush();
    expect(window.localStorage.getItem("k")).toBe("pending");

    storage.setItem("k", "again");
    expect(window.localStorage.getItem("k")).toBe("pending");
    window.dispatchEvent(new Event("pagehide"));
    expect(window.localStorage.getItem("k")).toBe("again");
  });

  it("flushes when the document becomes hidden", () => {
    const storage = createDebouncedStateStorage(DEBOUNCED_PERSIST_MS);
    storage.setItem("k", "hidden");
    const visibility = vi
      .spyOn(document, "visibilityState", "get")
      .mockReturnValue("hidden");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(window.localStorage.getItem("k")).toBe("hidden");
    visibility.mockRestore();
  });

  it("removeItem drops a pending write and clears disk immediately", () => {
    const storage = createDebouncedStateStorage(DEBOUNCED_PERSIST_MS);
    storage.setItem("k", "gone");
    storage.removeItem("k");
    expect(storage.getItem("k")).toBeNull();
    expect(window.localStorage.getItem("k")).toBeNull();
    vi.advanceTimersByTime(DEBOUNCED_PERSIST_MS);
    expect(window.localStorage.getItem("k")).toBeNull();
  });
});
