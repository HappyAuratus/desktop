import React from "react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { emojiAttributeGetter } from "./emoji-extension";

interface EmojiAttrs {
  emojiId: string;
}

export const EmojiComponent: React.FC<NodeViewProps> = ({ node }) => {
  const { emojiId } = node.attrs as EmojiAttrs;
  const emojiAttrs = emojiAttributeGetter(emojiId);

  if (!emojiAttrs) {
    return null;
  }

  return (
    <NodeViewWrapper className="emoji-node-view inline">
      <img
        src={emojiAttrs.src}
        alt={emojiAttrs.alt}
        title={emojiAttrs.title}
        width={emojiAttrs.width}
        height={emojiAttrs.height}
        style={{
          display: "inline-block",
          verticalAlign: "top",
          marginTop: "1px",
        }}
        contentEditable={false}
        draggable={false}
        data-emoji-id={emojiAttrs["data-emoji-id"]}
      />
    </NodeViewWrapper>
  );
};
