"use client";

import * as React from "react";
import { type Editor } from "@tiptap/react";

// --- Hooks ---
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";
import { useIsMobile } from "../../hooks/use-mobile";

// --- Lib ---
import { isNodeInSchema } from "../../utils";

// --- Icons ---
import { EmojiIcon } from "../../icons/emoji-icon";
import { wechatEmojis } from "../../emoji/wechat-emoji";

/**
 * Configuration for the emoji functionality
 */
export interface UseEmojiConfig {
  /**
   * The Tiptap editor instance.
   */
  editor?: Editor | null;
  /**
   * Whether to hide the button when the functionality is unavailable.
   * @default true
   */
  hideWhenUnavailable?: boolean;
  /**
   * Callback function called when an emoji is inserted.
   */
  onInserted?: () => void;
}

/**
 * Export emoji list directly from wechat emojis
 */
export const EMOJI_LIST = wechatEmojis;

/**
 * Check if emoji command is available in the current editor state
 */
export function canInsertEmoji(editor: Editor | null): boolean {
  if (!editor) return false;
  return (
    isNodeInSchema("emoji", editor) &&
    editor.can().insertContent({ type: "emoji" })
  );
}

/**
 * Check if the emoji extension should be shown
 */
export function shouldShowEmojiButton(
  editor: Editor | null,
  hideWhenUnavailable: boolean = true,
): boolean {
  if (!hideWhenUnavailable) return true;
  return canInsertEmoji(editor);
}

/**
 * Insert an emoji at the current cursor position
 */
export function insertEmoji(editor: Editor | null, emojiId: string): boolean {
  if (!editor || !canInsertEmoji(editor)) return false;

  return editor.chain().focus().setEmoji(emojiId).run();
}

/**
 * Custom hook that provides emoji functionality for Tiptap editor
 *
 * @example
 * ```tsx
 * // Simple usage
 * const emoji = useEmoji()
 *
 * // With custom editor
 * const emoji = useEmoji({ editor: customEditor })
 *
 * // With callback
 * const emoji = useEmoji({
 *   onInserted: () => console.log('Emoji inserted!')
 * })
 * ```
 */
export function useEmoji(config: UseEmojiConfig = {}) {
  const {
    editor: providedEditor,
    hideWhenUnavailable = true,
    onInserted,
  } = config;

  const { editor } = useTiptapEditor(providedEditor);
  const isMobile = useIsMobile();

  const canInsert = React.useMemo(() => canInsertEmoji(editor), [editor]);
  const shouldShow = React.useMemo(
    () => shouldShowEmojiButton(editor, hideWhenUnavailable),
    [editor, hideWhenUnavailable],
  );

  const handleInsertEmoji = React.useCallback(
    (emojiId: string) => {
      const success = insertEmoji(editor, emojiId);
      if (success && onInserted) {
        onInserted();
      }
      return success;
    },
    [editor, onInserted],
  );

  return {
    editor,
    canInsert,
    shouldShow,
    insertEmoji: handleInsertEmoji,
    emojiList: EMOJI_LIST,
    icon: EmojiIcon,
    isMobile,
  };
}
