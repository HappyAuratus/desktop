"use client";

import * as React from "react";
import { useHotkeys } from "react-hotkeys-hook";
import { type Editor } from "@tiptap/react";

// --- Hooks ---
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";
import { useIsMobile } from "../../hooks/use-mobile";

// --- Lib ---
import { isExtensionAvailable } from "../../utils";
import { UserLangEnum } from "../../types";
import { useEditorLanguage } from "../../context";

// --- Icons ---
import { VideoIcon } from "lucide-react";

export const VIDEO_UPLOAD_SHORTCUT_KEY = "mod+shift+v";

export type VideoUploadFunction = (
  file: File,
  onProgress?: (event: { progress: number }) => void,
  abortSignal?: AbortSignal,
) => Promise<string>;

/**
 * Configuration for the video upload functionality
 */
export interface UseVideoUploadConfig {
  /** The Tiptap editor instance. */
  editor?: Editor | null;
  /** Whether the button should hide when insertion is not available. @default false */
  hideWhenUnavailable?: boolean;
  /** Callback function called after a successful video insertion. */
  onInserted?: () => void;
  /** Function that performs the actual video upload, receiving the file and returning a URL. */
  upload?: VideoUploadFunction;
  /** Callback invoked when an upload error occurs. */
  onError?: (error: Error) => void;
}

/** Checks if video can be inserted in the current editor state */
export function canInsertVideo(editor: Editor | null): boolean {
  if (!editor || !editor.isEditable) return false;
  if (!isExtensionAvailable(editor, "video")) return false;

  return editor.can().insertContent({ type: "video" });
}

/** Checks if video is currently active */
export function isVideoActive(editor: Editor | null): boolean {
  if (!editor || !editor.isEditable) return false;
  return editor.isActive("video");
}

/** Handles video file selection and upload */
export function insertVideo(
  editor: Editor | null,
  upload: VideoUploadFunction,
  onSuccess?: () => void,
  onError?: (error: Error) => void,
): void {
  if (!editor || !editor.isEditable) return;
  if (!canInsertVideo(editor)) return;

  const input = document.createElement("input");
  input.type = "file";
  input.accept = "video/*";

  input.onchange = async (e) => {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;

    try {
      const url = await upload(file);

      if (url) {
        editor.chain().focus().setVideo({ src: url, controls: true }).run();

        onSuccess?.();
      }
    } catch (error) {
      const err =
        error instanceof Error ? error : new Error("Failed to upload video");
      console.error("Video upload failed:", err);
      onError?.(err);
    }
  };

  input.click();
}

/** Determines if the video button should be shown */
export function shouldShowButton(props: {
  editor: Editor | null;
  hideWhenUnavailable: boolean;
}): boolean {
  const { editor, hideWhenUnavailable } = props;

  if (!editor || !editor.isEditable) return false;
  if (!isExtensionAvailable(editor, "video")) return false;

  if (hideWhenUnavailable && !editor.isActive("code")) {
    return canInsertVideo(editor);
  }

  return true;
}

/**
 * Custom hook that provides video upload functionality for Tiptap editor.
 * Requires an `upload` function to perform the actual file upload.
 */
export function useVideoUpload(config?: UseVideoUploadConfig) {
  const language = useEditorLanguage();
  const {
    editor: providedEditor,
    hideWhenUnavailable = false,
    onInserted,
    upload,
    onError,
  } = config || {};

  const { editor } = useTiptapEditor(providedEditor);
  const isMobile = useIsMobile();
  const [isVisible, setIsVisible] = React.useState<boolean>(true);
  const canInsert = canInsertVideo(editor);
  const isActive = isVideoActive(editor);

  React.useEffect(() => {
    if (!editor) return;

    const handleSelectionUpdate = () => {
      setIsVisible(shouldShowButton({ editor, hideWhenUnavailable }));
    };

    handleSelectionUpdate();
    editor.on("selectionUpdate", handleSelectionUpdate);

    return () => {
      editor.off("selectionUpdate", handleSelectionUpdate);
    };
  }, [editor, hideWhenUnavailable]);

  const handleVideo = React.useCallback(() => {
    if (!editor || !upload) return;
    insertVideo(editor, upload, onInserted, onError);
  }, [editor, upload, onInserted, onError]);

  useHotkeys(
    VIDEO_UPLOAD_SHORTCUT_KEY,
    (event) => {
      event.preventDefault();
      handleVideo();
    },
    {
      enabled: isVisible && canInsert && !!upload,
      enableOnContentEditable: !isMobile,
      enableOnFormTags: true,
    },
  );

  return {
    isVisible: isVisible && !!upload,
    isActive,
    handleVideo,
    canInsert,
    label: i18nText[language].label,
    shortcutKeys: VIDEO_UPLOAD_SHORTCUT_KEY,
    Icon: VideoIcon,
  };
}

const i18nText = {
  [UserLangEnum.ZHCN]: {
    label: "添加视频",
  },
  [UserLangEnum.ENUS]: {
    label: "Add video",
  },
};
