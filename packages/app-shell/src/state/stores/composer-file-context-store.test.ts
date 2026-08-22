import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "@ora/ui";
import { addComposerFileSelections } from "../../features/chat/add-composer-file-selection";
import {
  resetComposerFileDeliveriesForTests,
  useComposerFileContextStore,
} from "./composer-file-context-store";
import { useWorkspaceSelectionStore } from "./workspace-selection-store";

beforeEach(() => {
  resetComposerFileDeliveriesForTests();
  useComposerFileContextStore.setState({ pendingByConversation: {} });
  useWorkspaceSelectionStore.setState({
    selection: {
      projectId: null,
      taskId: null,
      sessionId: "session-a",
      workflowRunId: null,
      draftId: null,
    },
    pendingRestore: null,
    createFocus: null,
  });
});

function pendingSelections(conversationKey: string) {
  return (
    useComposerFileContextStore.getState().pendingByConversation[
      conversationKey
    ] ?? []
  ).flatMap((batch) => batch.selections);
}

describe("useComposerFileContextStore", () => {
  it("queues and consumes selections per conversation key", () => {
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 1,
      }),
    ).toBe(true);
    expect(
      useComposerFileContextStore.getState().addSelection("session-b", {
        path: "b.ts",
        startLine: 2,
        endLine: 2,
      }),
    ).toBe(true);

    const pendingA =
      useComposerFileContextStore.getState().pendingByConversation["session-a"];
    const pendingB =
      useComposerFileContextStore.getState().pendingByConversation["session-b"];
    expect(pendingA?.map((batch) => batch.selections)).toEqual([
      [{ path: "a.ts", startLine: 1, endLine: 1 }],
    ]);
    expect(pendingB?.map((batch) => batch.selections)).toEqual([
      [{ path: "b.ts", startLine: 2, endLine: 2 }],
    ]);
    expect(pendingA?.[0]?.id).not.toBe(pendingB?.[0]?.id);

    useComposerFileContextStore
      .getState()
      .consumeSelections("session-a", pendingA![0]!.id);
    expect(
      useComposerFileContextStore.getState().pendingByConversation["session-a"],
    ).toBeUndefined();
    expect(pendingSelections("session-b")).toEqual([
      { path: "b.ts", startLine: 2, endLine: 2 },
    ]);
  });

  it("keeps a later batch when an earlier request id is consumed", () => {
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "a.ts",
      startLine: 1,
      endLine: 1,
    });
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "b.ts",
      startLine: 2,
      endLine: 2,
    });
    const [first, second] =
      useComposerFileContextStore.getState().pendingByConversation[
        "session-a"
      ] ?? [];
    expect(first).toBeDefined();
    expect(second).toBeDefined();

    useComposerFileContextStore
      .getState()
      .consumeSelections("session-a", first!.id);
    expect(pendingSelections("session-a")).toEqual([
      { path: "b.ts", startLine: 2, endLine: 2 },
    ]);
    expect(
      useComposerFileContextStore.getState().pendingByConversation["session-a"],
    ).toEqual([second]);
  });

  it("returns false when the same range is already pending", () => {
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 3,
        snippet: "x",
      }),
    ).toBe(true);
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 3,
        snippet: "y",
      }),
    ).toBe(false);

    expect(
      addComposerFileSelections([
        { path: "a.ts", startLine: 1, endLine: 3, snippet: "z" },
      ]),
    ).toBe(false);
  });

  it("queues the same range twice when one quote is a file citation and one is a diff review", () => {
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 2,
        snippet: "keep\nnew line",
      }),
    ).toBe(true);
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 2,
        snippet: " keep\n+new line",
        origin: "diff",
        diffSide: "new",
      }),
    ).toBe(true);
  });

  it("queues a batch of selections in one call", () => {
    expect(
      addComposerFileSelections([
        { path: "a.ts", startLine: 5, endLine: 5, snippet: "a" },
        { path: "a.ts", startLine: 8, endLine: 9, snippet: "b\nc" },
      ]),
    ).toBe(true);
    const pending =
      useComposerFileContextStore.getState().pendingByConversation["session-a"];
    expect(pending).toHaveLength(1);
    expect(pending?.[0]?.selections).toEqual([
      { path: "a.ts", startLine: 5, endLine: 5, snippet: "a" },
      { path: "a.ts", startLine: 8, endLine: 9, snippet: "b\nc" },
    ]);
  });

  it("refuses quotes when no conversation is selected", () => {
    useWorkspaceSelectionStore.setState({
      selection: {
        projectId: null,
        taskId: null,
        sessionId: null,
        workflowRunId: null,
        draftId: null,
      },
      pendingRestore: null,
      createFocus: null,
    });
    const warn = vi.spyOn(toast, "warning").mockImplementation(() => "id");
    expect(
      addComposerFileSelections([
        { path: "a.ts", startLine: 1, endLine: 1, snippet: "x" },
      ]),
    ).toBe(false);
    expect(warn).toHaveBeenCalled();
    expect(
      useComposerFileContextStore.getState().pendingByConversation,
    ).toEqual({});
    warn.mockRestore();
  });

  it("does not grow a consumed snapshot when a second quote arrives", () => {
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "a.ts",
      startLine: 1,
      endLine: 1,
    });
    const firstId =
      useComposerFileContextStore.getState().pendingByConversation[
        "session-a"
      ]![0]!.id;
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "b.ts",
      startLine: 2,
      endLine: 2,
    });
    useComposerFileContextStore
      .getState()
      .consumeSelections("session-a", firstId);
    expect(pendingSelections("session-a")).toEqual([
      { path: "b.ts", startLine: 2, endLine: 2 },
    ]);
  });

  it("ignores consumeSelections for a stale request id", () => {
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "a.ts",
      startLine: 1,
      endLine: 1,
    });
    const id =
      useComposerFileContextStore.getState().pendingByConversation[
        "session-a"
      ]![0]!.id;
    useComposerFileContextStore
      .getState()
      .consumeSelections("session-a", id + 1);
    expect(
      useComposerFileContextStore.getState().pendingByConversation["session-a"],
    ).toBeDefined();
  });

  it("delivers into a bound composer without leaving a replayable queue", async () => {
    const delivered: unknown[] = [];
    const unbind = useComposerFileContextStore
      .getState()
      .bindDelivery("session-a", (selections) => {
        delivered.push(selections);
      });
    expect(
      useComposerFileContextStore.getState().addSelection("session-a", {
        path: "a.ts",
        startLine: 1,
        endLine: 1,
      }),
    ).toBe(true);
    expect(delivered).toEqual([]);
    await Promise.resolve();
    expect(delivered).toEqual([[{ path: "a.ts", startLine: 1, endLine: 1 }]]);
    expect(
      useComposerFileContextStore.getState().pendingByConversation["session-a"],
    ).toBeUndefined();
    unbind();
  });

  it("drains queued quotes once when the composer binds", async () => {
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "a.ts",
      startLine: 1,
      endLine: 1,
    });
    const delivered: unknown[] = [];
    const unbind = useComposerFileContextStore
      .getState()
      .bindDelivery("session-a", (selections) => {
        delivered.push(selections);
      });
    expect(delivered).toEqual([]);
    await Promise.resolve();
    expect(delivered).toEqual([[{ path: "a.ts", startLine: 1, endLine: 1 }]]);
    expect(
      useComposerFileContextStore.getState().pendingByConversation["session-a"],
    ).toBeUndefined();
    unbind();
  });

  it("does not re-deliver after the composer rebinds to the same conversation", async () => {
    const delivered: unknown[] = [];
    const deliver = (selections: unknown) => {
      delivered.push(selections);
    };
    const unbind = useComposerFileContextStore
      .getState()
      .bindDelivery("session-a", deliver);
    useComposerFileContextStore.getState().addSelection("session-a", {
      path: "a.ts",
      startLine: 1,
      endLine: 1,
    });
    await Promise.resolve();
    expect(delivered).toHaveLength(1);
    unbind();
    const unbindAgain = useComposerFileContextStore
      .getState()
      .bindDelivery("session-a", deliver);
    await Promise.resolve();
    expect(delivered).toHaveLength(1);
    unbindAgain();
  });
});
