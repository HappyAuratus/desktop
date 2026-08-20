import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { useDraftSessionsStore } from "./draft-sessions-store";
import {
  useWorkspaceSelectionStore,
  WORKSPACE_SELECTION_STORAGE_KEY,
} from "./workspace-selection-store";
import { EMPTY_WORKSPACE_SELECTION } from "./sanitize-workspace-selection";

const empty = EMPTY_WORKSPACE_SELECTION;

beforeEach(() => {
  window.localStorage.clear();
  useDraftSessionsStore.getState().clear();
  useWorkspaceSelectionStore.setState({
    selection: empty,
    pendingRestore: null,
  });
});

afterEach(() => {
  window.localStorage.clear();
  useWorkspaceSelectionStore.setState({
    selection: empty,
    pendingRestore: null,
  });
});

describe("useWorkspaceSelectionStore", () => {
  it("starts empty", () => {
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
  });

  it("selectProject sets project and clears task/session/run", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().selectProject("p2");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p2",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("selectTask records the owning project and clears session/run", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().selectTask("t2", "p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: "t2",
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("selectSession records project, task, and session together", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: "t1",
      sessionId: "s1",
      workflowRunId: null,
      draftId: null,
    });
  });

  it("selectSessionBeforeTask records a project-scoped session before a task exists", () => {
    useWorkspaceSelectionStore
      .getState()
      .selectSessionBeforeTask("draft-1", "p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: null,
      sessionId: "draft-1",
      workflowRunId: null,
      draftId: null,
    });
  });

  it("selectDraft records a client-only chat and clears session/run", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().selectDraft("d1", null, "p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: "d1",
    });
  });

  it("selectWorkflowRun clears task and session under the project", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().selectWorkflowRun("gwr-1", "p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: null,
      sessionId: null,
      workflowRunId: "gwr-1",
      draftId: null,
    });
  });

  it("selectSession clears an active workflow run", () => {
    useWorkspaceSelectionStore.getState().selectWorkflowRun("gwr-1", "p1");
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    expect(
      useWorkspaceSelectionStore.getState().selection.workflowRunId,
    ).toBeNull();
  });

  it("clearSelection empties all legs", () => {
    useWorkspaceSelectionStore.getState().selectWorkflowRun("gwr-1", "p1");
    useWorkspaceSelectionStore.getState().clearSelection();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
  });

  it("clearSessionSelection keeps project and task", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().clearSessionSelection();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: "t1",
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("clearTaskSelection keeps the project leg only", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().clearTaskSelection("p1");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("setProject(null) empties the whole selection", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().setProject(null);
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
  });

  it("setProject(id) switches project and clears children", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    useWorkspaceSelectionStore.getState().setProject("p2");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p2",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("leaving an empty draft discards it; a typed draft stays", () => {
    const emptyId = useDraftSessionsStore
      .getState()
      .ensureEmptyDraft({ projectId: "p1", taskId: null });
    useWorkspaceSelectionStore.getState().selectDraft(emptyId, null, "p1");
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    expect(useDraftSessionsStore.getState().drafts).toHaveLength(0);

    const typedId = useDraftSessionsStore
      .getState()
      .ensureEmptyDraft({ projectId: "p1", taskId: null });
    useDraftSessionsStore.getState().updateContent(typedId, { text: "keep" });
    useWorkspaceSelectionStore.getState().selectDraft(typedId, null, "p1");
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    expect(
      useDraftSessionsStore.getState().drafts.map((draft) => draft.id),
    ).toEqual([typedId]);
  });

  it("updates selection before a failing draft cleanup", () => {
    const draftId = useDraftSessionsStore
      .getState()
      .ensureEmptyDraft({ projectId: "p1", taskId: null });
    useWorkspaceSelectionStore.getState().selectDraft(draftId, null, "p1");
    const cleanup = vi
      .spyOn(useDraftSessionsStore.getState(), "discardIfEmpty")
      .mockImplementation(() => {
        throw new Error("persistence failed");
      });

    expect(() =>
      useWorkspaceSelectionStore.getState().selectProject("p2"),
    ).toThrow("persistence failed");
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p2",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
    cleanup.mockRestore();
  });

  it("persists selection to localStorage under the v1 key", () => {
    useWorkspaceSelectionStore.getState().selectSession("s1", "t1", "p1");
    const raw = window.localStorage.getItem(WORKSPACE_SELECTION_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw!) as {
      state: {
        selection: typeof empty;
        pendingRestore: typeof empty | null;
      };
    };
    expect(parsed.state.selection).toEqual({
      projectId: "p1",
      taskId: "t1",
      sessionId: "s1",
      workflowRunId: null,
      draftId: null,
    });
    expect(parsed.state.pendingRestore).toBeNull();
  });

  it("rehydrates disk selection into pendingRestore and keeps live selection empty", async () => {
    window.localStorage.setItem(
      WORKSPACE_SELECTION_STORAGE_KEY,
      JSON.stringify({
        state: {
          selection: {
            projectId: "p1",
            taskId: "t1",
            sessionId: "s1",
            workflowRunId: null,
            draftId: null,
          },
          pendingRestore: null,
        },
        version: 0,
      }),
    );
    await useWorkspaceSelectionStore.persist.rehydrate();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toEqual({
      projectId: "p1",
      taskId: "t1",
      sessionId: "s1",
      workflowRunId: null,
      draftId: null,
    });
  });

  it("prefers an in-flight pendingRestore over the last live selection on rehydrate", async () => {
    window.localStorage.setItem(
      WORKSPACE_SELECTION_STORAGE_KEY,
      JSON.stringify({
        state: {
          selection: {
            projectId: "p-old",
            taskId: null,
            sessionId: null,
            workflowRunId: null,
            draftId: null,
          },
          pendingRestore: {
            projectId: "p1",
            taskId: "t1",
            sessionId: "s1",
            workflowRunId: null,
            draftId: null,
          },
        },
        version: 0,
      }),
    );
    await useWorkspaceSelectionStore.persist.rehydrate();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
    expect(
      useWorkspaceSelectionStore.getState().pendingRestore?.sessionId,
    ).toBe("s1");
  });

  it("sanitizes illegal session+draft combos on rehydrate", async () => {
    window.localStorage.setItem(
      WORKSPACE_SELECTION_STORAGE_KEY,
      JSON.stringify({
        state: {
          selection: {
            projectId: "p1",
            taskId: "t1",
            sessionId: "s1",
            workflowRunId: null,
            draftId: "d1",
          },
          pendingRestore: null,
        },
        version: 0,
      }),
    );
    await useWorkspaceSelectionStore.persist.rehydrate();
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toEqual({
      projectId: "p1",
      taskId: "t1",
      sessionId: null,
      workflowRunId: null,
      draftId: "d1",
    });
  });

  it("falls back to empty pendingRestore when persisted JSON is corrupt", async () => {
    window.localStorage.setItem(WORKSPACE_SELECTION_STORAGE_KEY, "{not json");
    await useWorkspaceSelectionStore.persist.rehydrate();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
  });

  it("clearPendingRestore drops the candidate without changing live selection", () => {
    useWorkspaceSelectionStore.setState({
      selection: empty,
      pendingRestore: {
        projectId: "p1",
        taskId: "t1",
        sessionId: "s1",
        workflowRunId: null,
        draftId: null,
      },
    });
    useWorkspaceSelectionStore.getState().clearPendingRestore();
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(empty);
  });
});
