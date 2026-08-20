import { create } from "zustand";
import { useDraftSessionsStore } from "./draft-sessions-store";

export interface WorkspaceSelection {
  projectId: string | null;
  taskId: string | null;
  sessionId: string | null;
  /** Graph workflow run; mutually exclusive with task/session legs. */
  workflowRunId: string | null;
  /** Client-only new-chat row; mutually exclusive with a persisted session. */
  draftId: string | null;
}

interface WorkspaceSelectionState {
  selection: WorkspaceSelection;
  /** Selects a project and clears any task/session/run underneath. */
  selectProject: (projectId: string) => void;
  /** Selects a task under a project and clears any session/run underneath. */
  selectTask: (taskId: string, projectId: string) => void;
  /**
   * Selects a session whose owning task is still being created.
   *
   * A direct chat gets its session before its task, because the task's title
   * comes from the first message. The session id is already final, so this only
   * leaves the task leg empty until the task exists.
   */
  selectSessionBeforeTask: (sessionId: string, projectId: string) => void;
  /** Selects a specific session, recording its owning task and project. */
  selectSession: (sessionId: string, taskId: string, projectId: string) => void;
  /**
   * Selects a client-only draft chat. Session and run legs clear so the
   * composer stays on the landing surface until the first send attaches.
   */
  selectDraft: (
    draftId: string,
    taskId: string | null,
    projectId: string,
  ) => void;
  /** Selects a graph workflow run under a project (clears task/session). */
  selectWorkflowRun: (workflowRunId: string, projectId: string) => void;
  /** Clears the entire selection. */
  clearSelection: () => void;
  /** Clears only the session leg (used after the selected session is deleted). */
  clearSessionSelection: () => void;
  /** Clears the task and session legs (used after the selected task is deleted). */
  clearTaskSelection: (projectId: string) => void;
  /** Clears the workflow-run leg while keeping the project. */
  clearWorkflowRunSelection: (projectId: string) => void;
  /** Replaces the project leg, clearing task/session/run (used after the selected project is deleted). */
  setProject: (projectId: string | null) => void;
}

const EMPTY_SELECTION: WorkspaceSelection = {
  projectId: null,
  taskId: null,
  sessionId: null,
  workflowRunId: null,
  draftId: null,
};

/**
 * Drops the previously selected draft when it was never used, so empty new-chat
 * rows do not pile up as the user moves around the tree.
 */
function settleLeftDraft(nextDraftId: string | null): void {
  const prev = useWorkspaceSelectionStore.getState().selection.draftId;
  if (prev !== null && prev !== nextDraftId) {
    useDraftSessionsStore.getState().discardIfEmpty(prev);
  }
}

/**
 * Owns the workspace tree selection without coupling to query data. Callers pass
 * the owning project/task ids they already have from react-query results, which
 * keeps this store a pure state machine that is trivial to unit-test.
 */
export const useWorkspaceSelectionStore = create<WorkspaceSelectionState>(
  (set) => ({
    selection: EMPTY_SELECTION,
    selectProject: (projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId: null,
          sessionId: null,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    selectTask: (taskId, projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId,
          sessionId: null,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    selectSessionBeforeTask: (sessionId, projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId: null,
          sessionId,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    selectSession: (sessionId, taskId, projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId,
          sessionId,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    selectDraft: (draftId, taskId, projectId) => {
      settleLeftDraft(draftId);
      set({
        selection: {
          projectId,
          taskId,
          sessionId: null,
          workflowRunId: null,
          draftId,
        },
      });
    },
    selectWorkflowRun: (workflowRunId, projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId: null,
          sessionId: null,
          workflowRunId,
          draftId: null,
        },
      });
    },
    clearSelection: () => {
      settleLeftDraft(null);
      set({ selection: EMPTY_SELECTION });
    },
    clearSessionSelection: () => {
      settleLeftDraft(null);
      set((state) => ({
        selection: {
          projectId: state.selection.projectId,
          taskId: state.selection.taskId,
          sessionId: null,
          workflowRunId: state.selection.workflowRunId,
          draftId: null,
        },
      }));
    },
    clearTaskSelection: (projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId: null,
          sessionId: null,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    clearWorkflowRunSelection: (projectId) => {
      settleLeftDraft(null);
      set({
        selection: {
          projectId,
          taskId: null,
          sessionId: null,
          workflowRunId: null,
          draftId: null,
        },
      });
    },
    setProject: (projectId) => {
      settleLeftDraft(null);
      set({
        selection:
          projectId === null
            ? EMPTY_SELECTION
            : {
                projectId,
                taskId: null,
                sessionId: null,
                workflowRunId: null,
                draftId: null,
              },
      });
    },
  }),
);
