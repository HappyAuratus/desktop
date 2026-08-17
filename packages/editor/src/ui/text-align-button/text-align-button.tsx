import * as React from "react";

// --- Lib ---
import { parseShortcutKeys } from "../../utils";

// --- Hooks ---
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";

// --- Tiptap UI ---
import type { TextAlign, UseTextAlignConfig } from "../text-align-button";
import { TEXT_ALIGN_SHORTCUT_KEYS, useTextAlign } from "../text-align-button";

// --- UI Primitives ---
import type { ButtonProps } from "../../primitive/button";
import { Button } from "../../primitive/button";
import { Badge } from "../../primitive/badge";
import { UserLangEnum } from "../../types";
import { useEditorLanguage } from "../../context";

export interface TextAlignButtonProps
  extends Omit<ButtonProps, "type">, UseTextAlignConfig {
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

export function TextAlignShortcutBadge({
  align,
  shortcutKeys = TEXT_ALIGN_SHORTCUT_KEYS[align],
}: {
  align: TextAlign;
  shortcutKeys?: string;
}) {
  return <Badge>{parseShortcutKeys({ shortcutKeys })}</Badge>;
}

/**
 * Button component for setting text alignment in a Tiptap editor.
 *
 * For custom button implementations, use the `useTextAlign` hook instead.
 */
export const TextAlignButton = React.forwardRef<
  HTMLButtonElement,
  TextAlignButtonProps
>(
  (
    {
      editor: providedEditor,
      align,
      text,
      hideWhenUnavailable = false,
      onAligned,
      showShortcut = false,
      onClick,
      children,
      ...buttonProps
    },
    ref,
  ) => {
    const language = useEditorLanguage();
    const { editor } = useTiptapEditor(providedEditor);
    const {
      isVisible,
      handleTextAlign,
      label,
      canAlign,
      isActive,
      Icon,
      shortcutKeys,
    } = useTextAlign({
      editor,
      align,
      hideWhenUnavailable,
      onAligned,
    });

    const handleClick = React.useCallback(
      (event: React.MouseEvent<HTMLButtonElement>) => {
        onClick?.(event);
        if (event.defaultPrevented) return;
        handleTextAlign();
      },
      [handleTextAlign, onClick],
    );

    if (!isVisible) {
      return null;
    }

    return (
      <Button
        type="button"
        disabled={!canAlign}
        data-style="ghost"
        data-active-state={isActive ? "on" : "off"}
        data-disabled={!canAlign}
        role="button"
        tabIndex={-1}
        aria-label={label}
        aria-pressed={isActive}
        tooltip={
          i18nText[language][label as keyof (typeof i18nText)[typeof language]]
        }
        onClick={handleClick}
        {...buttonProps}
        ref={ref}
      >
        {children ?? (
          <>
            <Icon className="tiptap-button-icon" />
            {text && <span className="tiptap-button-text">{text}</span>}
            {showShortcut && (
              <TextAlignShortcutBadge
                align={align}
                shortcutKeys={shortcutKeys}
              />
            )}
          </>
        )}
      </Button>
    );
  },
);

const i18nText = {
  [UserLangEnum.ZHCN]: {
    "Align left": "左对齐",
    "Align center": "居中对齐",
    "Align right": "右对齐",
    "Align justify": "两端对齐",
  },
  [UserLangEnum.ENUS]: {
    "Align left": "Align left",
    "Align center": "Align center",
    "Align right": "Align right",
    "Align justify": "Align justify",
  },
};

TextAlignButton.displayName = "TextAlignButton";
