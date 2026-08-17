import * as React from "react";
import { type Editor } from "@tiptap/react";

// --- Icons ---
import { EmojiIcon } from "../../icons/emoji-icon";

// --- UI Primitives ---
import type { ButtonProps } from "../../primitive/button";
import { Button } from "../../primitive/button";
import {
  Popover,
  PopoverTrigger,
  PopoverContent,
} from "../../primitive/popover";
import { Card, CardBody } from "../../primitive/card";

// --- Tiptap UI ---
import { useEmoji, EMOJI_LIST } from "../emoji-button/use-emoji";
import { UserLangEnum } from "../../types";
import { useEditorLanguage } from "../../context";

export interface EmojiPopoverContentProps {
  /**
   * The Tiptap editor instance.
   */
  editor?: Editor | null;
  /**
   * Callback when an emoji is selected
   */
  onEmojiSelect?: () => void;
}

export interface EmojiPopoverProps extends Omit<ButtonProps, "type"> {
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
  /**
   * Callback when an emoji is selected (for closing popover)
   */
  onEmojiSelect?: () => void;
}

export const EmojiPopoverButton = React.forwardRef<
  HTMLButtonElement,
  ButtonProps
>(({ className, children, ...props }, ref) => {
  const language = useEditorLanguage();
  return (
    <Button
      type="button"
      className={className}
      data-style="ghost"
      data-appearance="default"
      role="button"
      tabIndex={-1}
      aria-label="Insert emoji"
      tooltip={i18nText[language].tooltip}
      ref={ref}
      {...props}
    >
      {children ?? <EmojiIcon className="tiptap-button-icon" />}
    </Button>
  );
});

EmojiPopoverButton.displayName = "EmojiPopoverButton";

export const EmojiPopoverContent = React.forwardRef<
  HTMLDivElement,
  EmojiPopoverContentProps
>(({ editor: providedEditor, onEmojiSelect }, ref) => {
  const { editor, insertEmoji } = useEmoji({ editor: providedEditor });

  const handleEmojiClick = React.useCallback(
    (emojiId: string) => {
      insertEmoji(emojiId);
      onEmojiSelect?.();
    },
    [insertEmoji, onEmojiSelect],
  );

  if (!editor) {
    return null;
  }

  return (
    <Card ref={ref}>
      <CardBody className="!p-2">
        <div className="grid max-h-[300px] max-w-[280px] grid-cols-8 gap-1 overflow-y-auto">
          {EMOJI_LIST.map((emoji) => (
            <Button
              key={emoji.id}
              type="button"
              data-style="ghost"
              className="flex h-8 w-8 items-center justify-center !p-0 hover:bg-gray-100 dark:hover:bg-gray-800"
              onClick={() => handleEmojiClick(emoji.id)}
              aria-label={`Insert emoji ${emoji.id}`}
            >
              <img
                src={emoji.url}
                alt={emoji.name}
                title={emoji.name}
                width={24}
                height={24}
                style={{
                  display: "block",
                }}
              />
            </Button>
          ))}
        </div>
      </CardBody>
    </Card>
  );
});

EmojiPopoverContent.displayName = "EmojiPopoverContent";

export const EmojiPopover = React.forwardRef<
  HTMLButtonElement,
  EmojiPopoverProps
>(
  (
    {
      editor: providedEditor,
      hideWhenUnavailable = true,
      onInserted,
      onEmojiSelect,
      children,
      ...props
    },
    ref,
  ) => {
    const [open, setOpen] = React.useState(false);
    const { shouldShow } = useEmoji({
      editor: providedEditor,
      hideWhenUnavailable,
      onInserted,
    });

    const handleEmojiSelect = React.useCallback(() => {
      setOpen(false);
      onEmojiSelect?.();
    }, [onEmojiSelect]);

    if (!shouldShow) {
      return null;
    }

    return (
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <EmojiPopoverButton ref={ref} {...props}>
            {children}
          </EmojiPopoverButton>
        </PopoverTrigger>
        <PopoverContent side="bottom" align="start" className="w-auto p-0">
          <EmojiPopoverContent
            editor={providedEditor}
            onEmojiSelect={handleEmojiSelect}
          />
        </PopoverContent>
      </Popover>
    );
  },
);

const i18nText = {
  [UserLangEnum.ZHCN]: {
    tooltip: "表情",
  },
  [UserLangEnum.ENUS]: {
    tooltip: "Emoji",
  },
};

EmojiPopover.displayName = "EmojiPopover";
