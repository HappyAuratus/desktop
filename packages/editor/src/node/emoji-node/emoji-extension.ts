import { Node, mergeAttributes } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import { EmojiComponent } from "./emoji-component";
import { wechatEmojis } from "../../emoji/wechat-emoji";

export type EmojiAttributes = {
  src: string;
  alt: string;
  title: string;
  width: string;
  height: string;
  style: string;
  contenteditable: string;
  draggable: string;
  "data-emoji-id": string;
};

export const emojiAttributeGetter = (
  emojiId: string,
): EmojiAttributes | null => {
  if (!emojiId) return null;

  const emoji = wechatEmojis.find((e) => e.id === emojiId);
  if (!emoji) return null;

  return {
    src: emoji.url,
    alt: emoji.name,
    title: emoji.name,
    width: "20",
    height: "20",
    style: "display: inline-block; vertical-align: top; margin-top: 1px;",
    contenteditable: "false",
    draggable: "false",
    "data-emoji-id": emojiId,
  };
};

export interface EmojiOptions {
  HTMLAttributes: Record<string, any>;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    emoji: {
      /**
       * Insert an emoji
       */
      setEmoji: (emojiId: string) => ReturnType;
    };
  }
}

export const EmojiExtension = Node.create<EmojiOptions>({
  name: "emoji",

  addOptions() {
    return {
      HTMLAttributes: {},
    };
  },

  group: "inline",

  inline: true,

  selectable: false,

  atom: true,

  addAttributes() {
    return {
      emojiId: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-emoji-id"),
        renderHTML: (attributes) => {
          if (!attributes.emojiId) {
            return {};
          }
          return {
            "data-emoji-id": attributes.emojiId,
          };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: `img[data-emoji-id]`,
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    const emojiId = HTMLAttributes["data-emoji-id"];
    const emojiAttrs = emojiAttributeGetter(emojiId);

    if (!emojiAttrs) {
      return [
        "img",
        mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      ];
    }

    return [
      "img",
      mergeAttributes(this.options.HTMLAttributes, emojiAttrs, HTMLAttributes),
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer(EmojiComponent);
  },

  addCommands() {
    return {
      setEmoji:
        (emojiId: string) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: { emojiId },
          });
        },
    };
  },
});
