import type {
  Edge,
  Node,
  OnConnect,
  OnEdgesChange,
  OnNodesChange,
  OnReconnect,
  Viewport,
  XYPosition,
} from "@xyflow/react";
import type {
  MockWorkflowVersion,
  WorkflowCapabilities,
  WorkflowNodeData,
  WorkflowNodeKind,
} from "@ora/workflow-mock";

/** Defines the React Flow element boundary consumed by the workflow canvas. */
export interface WorkflowCanvasProps {
  capabilities: WorkflowCapabilities;
  nodes: Node<WorkflowNodeData, "workflow">[];
  edges: Edge[];
  initialViewport: Viewport;
  onNodesChange: OnNodesChange<Node<WorkflowNodeData, "workflow">>;
  onEdgesChange: OnEdgesChange<Edge>;
  onAddNode: (kind: WorkflowNodeKind, position: XYPosition) => void;
  onConnect: OnConnect;
  onReconnect: OnReconnect<Edge>;
  inspectorCollapsed: boolean;
  inspectorAvailable: boolean;
  onExpandInspector: () => void;
  versionHistory: MockWorkflowVersion[];
  previewedVersion: MockWorkflowVersion | null;
  /** Version string of the workflow's currently active published snapshot, if any. */
  activeVersion: string | null;
  /** Formatted last-edit time of the draft (workflow_snapshots.updated_at). */
  draftUpdatedAt?: string;
  onPreviewVersion: (version: MockWorkflowVersion | null) => void;
  onActivateVersion: (version: MockWorkflowVersion) => void;
  /** Opens the same publish flow as the header, freezing the current draft. */
  onPublishDraft: () => void;
  onDeleteVersion: (version: MockWorkflowVersion) => void;
  readOnly: boolean;
}
