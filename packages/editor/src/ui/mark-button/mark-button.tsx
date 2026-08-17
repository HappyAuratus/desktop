import * as React from "react";

// --- Lib ---
import { parseShortcutKeys } from "../../utils";

// --- Hooks ---
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";

// --- Tiptap UI ---
import type { Mark, UseMarkConfig } from "../mark-button";
import { MARK_SHORTCUT_KEYS, useMark } from "../mark-button";

// --- UI Primitives ---
import type { ButtonProps } from "../../primitive/button";
import { Button } from "../../primitive/button";
import { Badge } from "../../primitive/badge";
import { UserLangEnum } from "../../types";
import { useEditorLanguage } from "../../context";

export interface MarkButtonProps
  extends Omit<ButtonProps, "type">, UseMarkConfig {
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

export function MarkShortcutBadge({
  type,
  shortcutKeys = MARK_SHORTCUT_KEYS[type],
}: {
  type: Mark;
  shortcutKeys?: string;
}) {
  return <Badge>{parseShortcutKeys({ shortcutKeys })}</Badge>;
}

/**
 * Button component for toggling marks in a Tiptap editor.
 *
 * For custom button implementations, use the `useMark` hook instead.
 */
export const MarkButton = React.forwardRef<HTMLButtonElement, MarkButtonProps>(
  (
    {
      editor: providedEditor,
      type,
      text,
      hideWhenUnavailable = false,
      onToggled,
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
      handleMark,
      label,
      canToggle,
      isActive,
      Icon,
      shortcutKeys,
    } = useMark({
      editor,
      type,
      hideWhenUnavailable,
      onToggled,
    });

    const handleClick = React.useCallback(
      (event: React.MouseEvent<HTMLButtonElement>) => {
        onClick?.(event);
        if (event.defaultPrevented) return;
        handleMark();
      },
      [handleMark, onClick],
    );

    if (!isVisible) {
      return null;
    }

    return (
      <Button
        type="button"
        disabled={!canToggle}
        data-style="ghost"
        data-active-state={isActive ? "on" : "off"}
        data-disabled={!canToggle}
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
              <MarkShortcutBadge type={type} shortcutKeys={shortcutKeys} />
            )}
          </>
        )}
      </Button>
    );
  },
);

const i18nText = {
  [UserLangEnum.ZHCN]: {
    Bold: "加粗",
    Italic: "斜体",
    Underline: "下划线",
    Strike: "删除线",
    Code: "代码",
    Highlight: "高亮",
    Superscript: "上标",
    Subscript: "下标",
  },
  [UserLangEnum.ENUS]: {
    Bold: "Bold",
    Italic: "Italic",
    Underline: "Underline",
    Strike: "Strike",
    Code: "Code",
    Highlight: "Highlight",
    Superscript: "Superscript",
    Subscript: "Subscript",
  },
};

MarkButton.displayName = "MarkButton";
