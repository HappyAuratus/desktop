import {
  composerFileLabel,
  type ComposerFileAttrs,
} from "@ora/editor/composer";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { WorkspaceFileIcon } from "../files/workspace-file-visuals";

/**
 * Inline path mention chip: type/folder icon + basename, Cursor-style (no pill).
 * Wired through `AppComposerFile` so the explorer and @ picker share one visual.
 */
export function ComposerFileChipView({ node }: NodeViewProps) {
  const kind =
    node.attrs.kind === "directory"
      ? ("directory" as const)
      : ("file" as const);
  const attrs: ComposerFileAttrs = {
    path: String(node.attrs.path),
    startLine:
      node.attrs.startLine === null ? undefined : Number(node.attrs.startLine),
    endLine:
      node.attrs.endLine === null ? undefined : Number(node.attrs.endLine),
    kind,
  };
  const title = attrs.path;

  return (
    <NodeViewWrapper
      as="span"
      className="composer-file-ref"
      data-composer-file={attrs.path}
      contentEditable={false}
      title={title}
    >
      <WorkspaceFileIcon
        path={attrs.path}
        kind={kind}
        className="composer-file-ref-icon"
      />
      <span className="composer-file-ref-label">
        {composerFileLabel(attrs)}
      </span>
    </NodeViewWrapper>
  );
}
