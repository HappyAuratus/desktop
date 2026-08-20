import { act, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, Session, Task } from "@ora/contracts";
import { createChatStore } from "@ora/chat";
import { createMockClient, createMockClientState } from "../test/mock-client";
import { renderHookWithClient } from "../test/hook-harness";
import { useRestoreWorkspaceSelection } from "./hooks/use-restore-workspace-selection";
import { useWarmSession } from "./hooks/use-warm-session";
import { useUiStore } from "./stores/ui-store";
import { useWorkspaceSelectionStore } from "./stores/workspace-selection-store";
import { useDraftSessionsStore } from "./stores/draft-sessions-store";
import { EMPTY_WORKSPACE_SELECTION } from "./stores/sanitize-workspace-selection";

const PROJECT: Project = { id: "p1", name: "Ora", rootPath: "/ora" };
const TASK: Task = {
  id: "t1",
  projectId: "p1",
  title: "Refactor",
  workspaceMode: "worktree",
  type: "default",
  workflowRunId: null,
};
const SESSION: Session = {
  id: "s1",
  taskId: "t1",
  agentCli: "open_code",
  status: "running",
  title: null,
  historyState: { type: "writable" },
};

beforeEach(() => {
  window.localStorage.clear();
  useDraftSessionsStore.getState().clear();
  useWorkspaceSelectionStore.setState({
    selection: EMPTY_WORKSPACE_SELECTION,
    pendingRestore: null,
  });
  useUiStore.setState({
    expandedProjects: new Set(),
    expandedTasks: new Set(),
  });
  vi.restoreAllMocks();
});

describe("useRestoreWorkspaceSelection", () => {
  it("applies a validated session once the tree is settled", async () => {
    useWorkspaceSelectionStore.setState({
      selection: EMPTY_WORKSPACE_SELECTION,
      pendingRestore: {
        projectId: "p1",
        taskId: "t1",
        sessionId: "s1",
        workflowRunId: null,
        draftId: null,
      },
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: false,
        }),
      client,
    );

    await waitFor(() => {
      expect(useWorkspaceSelectionStore.getState().selection).toEqual({
        projectId: "p1",
        taskId: "t1",
        sessionId: "s1",
        workflowRunId: null,
        draftId: null,
      });
    });
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
    expect(useUiStore.getState().expandedProjects.has("p1")).toBe(true);
    expect(useUiStore.getState().expandedTasks.has("t1")).toBe(true);
  });

  it("clears a stale session candidate without applying it", async () => {
    useWorkspaceSelectionStore.setState({
      selection: EMPTY_WORKSPACE_SELECTION,
      pendingRestore: {
        projectId: "p1",
        taskId: "t1",
        sessionId: "missing",
        workflowRunId: null,
        draftId: null,
      },
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: false,
        }),
      client,
    );

    await waitFor(() => {
      expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
    });
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(
      EMPTY_WORKSPACE_SELECTION,
    );
  });

  it("does not overwrite a live selection the user already made", async () => {
    useWorkspaceSelectionStore.setState({
      selection: {
        projectId: "p1",
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
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: false,
        }),
      client,
    );

    await waitFor(() => {
      expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
    });
    expect(useWorkspaceSelectionStore.getState().selection).toEqual({
      projectId: "p1",
      taskId: null,
      sessionId: null,
      workflowRunId: null,
      draftId: null,
    });
  });

  it("waits while the tree is still pending", async () => {
    useWorkspaceSelectionStore.setState({
      selection: EMPTY_WORKSPACE_SELECTION,
      pendingRestore: {
        projectId: "p1",
        taskId: "t1",
        sessionId: "s1",
        workflowRunId: null,
        draftId: null,
      },
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: true,
        }),
      client,
    );

    await act(async () => {
      await Promise.resolve();
    });
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(
      EMPTY_WORKSPACE_SELECTION,
    );
    expect(useWorkspaceSelectionStore.getState().pendingRestore).not.toBeNull();
  });

  it("waits for draft rehydration before treating a draft candidate as missing", async () => {
    const finishListeners: Array<(state: unknown) => void> = [];
    const hasHydrated = vi
      .spyOn(useDraftSessionsStore.persist, "hasHydrated")
      .mockReturnValue(false);
    vi.spyOn(
      useDraftSessionsStore.persist,
      "onFinishHydration",
    ).mockImplementation((listener) => {
      finishListeners.push(listener as (state: unknown) => void);
      return () => {
        const index = finishListeners.indexOf(
          listener as (state: unknown) => void,
        );
        if (index >= 0) finishListeners.splice(index, 1);
      };
    });

    const draftId = useDraftSessionsStore
      .getState()
      .ensureEmptyDraft({ projectId: "p1", taskId: null });
    useDraftSessionsStore.getState().updateContent(draftId, { text: "parked" });

    useWorkspaceSelectionStore.setState({
      selection: EMPTY_WORKSPACE_SELECTION,
      pendingRestore: {
        projectId: "p1",
        taskId: null,
        sessionId: null,
        workflowRunId: null,
        draftId,
      },
    });

    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: false,
        }),
      client,
    );

    await act(async () => {
      await Promise.resolve();
    });
    expect(useWorkspaceSelectionStore.getState().pendingRestore).not.toBeNull();
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(
      EMPTY_WORKSPACE_SELECTION,
    );

    hasHydrated.mockReturnValue(true);
    await act(async () => {
      for (const listener of finishListeners) listener({});
    });

    await waitFor(() => {
      expect(useWorkspaceSelectionStore.getState().selection).toEqual({
        projectId: "p1",
        taskId: null,
        sessionId: null,
        workflowRunId: null,
        draftId,
      });
    });
    expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
  });

  it("clears a workflow-run candidate that lacks a project id instead of waiting forever", async () => {
    useWorkspaceSelectionStore.setState({
      selection: EMPTY_WORKSPACE_SELECTION,
      // Bypass sanitize: a corrupt in-memory candidate must not hang restore.
      pendingRestore: {
        projectId: null,
        taskId: null,
        sessionId: null,
        workflowRunId: "run-1",
        draftId: null,
      },
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [SESSION];
    const client = createMockClient(state);

    renderHookWithClient(
      () =>
        useRestoreWorkspaceSelection({
          projects: state.projects,
          tasks: state.tasks,
          sessions: state.sessions,
          treePending: false,
        }),
      client,
    );

    await waitFor(() => {
      expect(useWorkspaceSelectionStore.getState().pendingRestore).toBeNull();
    });
    expect(useWorkspaceSelectionStore.getState().selection).toEqual(
      EMPTY_WORKSPACE_SELECTION,
    );
  });
});

describe("useWarmSession restore gate", () => {
  it("does not warm while pendingRestore is set", async () => {
    useWorkspaceSelectionStore.setState({
      selection: {
        projectId: "p1",
        taskId: "t1",
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
    });
    const state = createMockClientState();
    state.projects = [PROJECT];
    state.tasks = [TASK];
    state.sessions = [];
    const client = createMockClient(state);
    const warm = vi.spyOn(client.session, "warm");

    renderHookWithClient(
      () =>
        useWarmSession(
          {
            projectId: "p1",
            taskId: "t1",
            sessionId: null,
          },
          "open_code",
        ),
      client,
      undefined,
      createChatStore(client.session),
    );

    await act(async () => {
      await Promise.resolve();
    });
    expect(warm).not.toHaveBeenCalled();
  });
});
