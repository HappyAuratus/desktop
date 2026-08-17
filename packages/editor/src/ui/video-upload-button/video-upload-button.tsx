import * as React from "react";

// --- Lib ---
import { parseShortcutKeys } from "../../utils";

// --- Hooks ---
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";

// --- Tiptap UI ---
import type { UseVideoUploadConfig } from "../video-upload-button";
import {
  VIDEO_UPLOAD_SHORTCUT_KEY,
  useVideoUpload,
} from "../video-upload-button";

// --- UI Primitives ---
import type { ButtonProps } from "../../primitive/button";
import { Button } from "../../primitive/button";
import { Badge } from "../../primitive/badge";

export interface VideoUploadButtonProps
  extends Omit<ButtonProps, "type" | "onError">, UseVideoUploadConfig {
  /**
   * Optional text to display alongside the icon.
   */
  text?: string;
  /**
   * Optional show shortcut keys in the button.
   * @default false
   */
  showShortcut?: boolean;
}

export function VideoShortcutBadge({
  shortcutKeys = VIDEO_UPLOAD_SHORTCUT_KEY,
}: {
  shortcutKeys?: string;
}) {
  return <Badge>{parseShortcutKeys({ shortcutKeys })}</Badge>;
}

/**
 * Button component for uploading/inserting videos in a Tiptap editor.
 *
 * For custom button implementations, use the `useVideoUpload` hook instead.
 */
export const VideoUploadButton = React.forwardRef<
  HTMLButtonElement,
  VideoUploadButtonProps
>(
  (
    {
      editor: providedEditor,
      text,
      hideWhenUnavailable = false,
      onInserted,
      upload,
      onError,
      showShortcut = false,
      onClick,
      children,
      ...buttonProps
    },
    ref,
  ) => {
    const { editor } = useTiptapEditor(providedEditor);
    const {
      isVisible,
      canInsert,
      handleVideo,
      label,
      isActive,
      shortcutKeys,
      Icon,
    } = useVideoUpload({
      editor,
      hideWhenUnavailable,
      onInserted,
      upload,
      onError,
    });

    const handleClick = React.useCallback(
      (event: React.MouseEvent<HTMLButtonElement>) => {
        onClick?.(event);
        if (event.defaultPrevented) return;
        handleVideo();
      },
      [handleVideo, onClick],
    );

    if (!isVisible) {
      return null;
    }

    return (
      <Button
        type="button"
        data-style="ghost"
        data-active-state={isActive ? "on" : "off"}
        role="button"
        tabIndex={-1}
        disabled={!canInsert}
        data-disabled={!canInsert}
        aria-label={label}
        aria-pressed={isActive}
        tooltip={label}
        onClick={handleClick}
        {...buttonProps}
        ref={ref}
      >
        {children ?? (
          <>
            <Icon className="tiptap-button-icon" />
            {text && <span className="tiptap-button-text">{text}</span>}
            {showShortcut && <VideoShortcutBadge shortcutKeys={shortcutKeys} />}
          </>
        )}
      </Button>
    );
  },
);

VideoUploadButton.displayName = "VideoUploadButton";
