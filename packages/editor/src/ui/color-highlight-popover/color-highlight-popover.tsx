import * as React from "react";
import { type Editor } from "@tiptap/react";

// --- Hooks ---
import { useMenuNavigation } from "../../hooks/use-menu-navigation";
import { useIsMobile } from "../../hooks/use-mobile";
import { useTiptapEditor } from "../../hooks/use-tiptap-editor";

// --- Icons ---
import { BanIcon } from "../../icons/ban-icon";
import { HighlighterIcon } from "../../icons/highlighter-icon";

// --- UI Primitives ---
import type { ButtonProps } from "../../primitive/button";
import { Button, ButtonGroup } from "../../primitive/button";
import {
  Popover,
  PopoverTrigger,
  PopoverContent,
} from "../../primitive/popover";
import { Separator } from "../../primitive/separator";
import { Card, CardBody, CardItemGroup } from "../../primitive/card";

// --- Tiptap UI ---
import type {
  HighlightColor,
  UseColorHighlightConfig,
} from "../color-highlight-button";
import {
  ColorHighlightButton,
  pickHighlightColorsByValue,
  useColorHighlight,
} from "../color-highlight-button";
import { UserLangEnum } from "../../types";
import { useEditorLanguage } from "../../context";

export interface ColorHighlightPopoverContentProps {
  /**
   * The Tiptap editor instance.
   */
  editor?: Editor | null;
  /**
   * Optional colors to use in the highlight popover.
   * If not provided, defaults to a predefined set of colors.
   */
  colors?: HighlightColor[];
}

export interface ColorHighlightPopoverProps
  extends
    Omit<ButtonProps, "type">,
    Pick<
      UseColorHighlightConfig,
      "editor" | "hideWhenUnavailable" | "onApplied"
    > {
  /**
   * Optional colors to use in the highlight popover.
   * If not provided, defaults to a predefined set of colors.
   */
  colors?: HighlightColor[];
}

export const ColorHighlightPopoverButton = React.forwardRef<
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
      aria-label="Highlight text"
      tooltip={i18nText[language]["Highlight"]}
      ref={ref}
      {...props}
    >
      {children ?? <HighlighterIcon className="tiptap-button-icon" />}
    </Button>
  );
});

ColorHighlightPopoverButton.displayName = "ColorHighlightPopoverButton";

export function ColorHighlightPopoverContent({
  editor,
  colors = pickHighlightColorsByValue([
    "var(--tt-color-highlight-green)",
    "var(--tt-color-highlight-blue)",
    "var(--tt-color-highlight-red)",
    "var(--tt-color-highlight-purple)",
    "var(--tt-color-highlight-yellow)",
  ]),
}: ColorHighlightPopoverContentProps) {
  const language = useEditorLanguage();
  const { handleRemoveHighlight } = useColorHighlight({ editor });
  const isMobile = useIsMobile();
  const containerRef = React.useRef<HTMLDivElement>(null);

  const menuItems = React.useMemo(
    () => [...colors, { label: "Remove highlight", value: "none" }],
    [colors],
  );

  const { selectedIndex } = useMenuNavigation({
    containerRef,
    items: menuItems,
    orientation: "both",
    onSelect: (item) => {
      if (!containerRef.current) return false;
      const highlightedElement = containerRef.current.querySelector(
        '[data-highlighted="true"]',
      ) as HTMLElement;
      if (highlightedElement) highlightedElement.click();
      if (item.value === "none") handleRemoveHighlight();
    },
    autoSelectFirstItem: false,
  });

  return (
    <Card
      ref={containerRef}
      tabIndex={0}
      style={isMobile ? { boxShadow: "none", border: 0 } : {}}
    >
      <CardBody style={isMobile ? { padding: 0 } : {}}>
        <CardItemGroup orientation="horizontal">
          <ButtonGroup orientation="horizontal">
            {colors.map((color, index) => (
              <ColorHighlightButton
                key={color.value}
                editor={editor}
                highlightColor={color.value}
                tooltip={
                  i18nText[language][
                    color.label as keyof (typeof i18nText)[typeof language]
                  ]
                }
                aria-label={`${color.label} highlight color`}
                tabIndex={index === selectedIndex ? 0 : -1}
                data-highlighted={selectedIndex === index}
              />
            ))}
          </ButtonGroup>
          <Separator />
          <ButtonGroup orientation="horizontal">
            <Button
              onClick={handleRemoveHighlight}
              aria-label="Remove highlight"
              tooltip={i18nText[language]["Remove highlight"]}
              tabIndex={selectedIndex === colors.length ? 0 : -1}
              type="button"
              role="menuitem"
              data-style="ghost"
              data-highlighted={selectedIndex === colors.length}
            >
              <BanIcon className="tiptap-button-icon" />
            </Button>
          </ButtonGroup>
        </CardItemGroup>
      </CardBody>
    </Card>
  );
}

export function ColorHighlightPopover({
  editor: providedEditor,
  colors = pickHighlightColorsByValue([
    "var(--tt-color-highlight-green)",
    "var(--tt-color-highlight-blue)",
    "var(--tt-color-highlight-red)",
    "var(--tt-color-highlight-purple)",
    "var(--tt-color-highlight-yellow)",
  ]),
  hideWhenUnavailable = false,
  onApplied,
  ...props
}: ColorHighlightPopoverProps) {
  const language = useEditorLanguage();
  const { editor } = useTiptapEditor(providedEditor);
  const [isOpen, setIsOpen] = React.useState(false);
  const { isVisible, canColorHighlight, isActive, label, Icon } =
    useColorHighlight({
      editor,
      hideWhenUnavailable,
      onApplied,
    });

  if (!isVisible) return null;

  return (
    <Popover open={isOpen} onOpenChange={setIsOpen}>
      <PopoverTrigger asChild>
        <ColorHighlightPopoverButton
          disabled={!canColorHighlight}
          data-active-state={isActive ? "on" : "off"}
          data-disabled={!canColorHighlight}
          aria-pressed={isActive}
          aria-label={label}
          tooltip={
            i18nText[language][
              label as keyof (typeof i18nText)[typeof language]
            ]
          }
          {...props}
        >
          <Icon className="tiptap-button-icon" />
        </ColorHighlightPopoverButton>
      </PopoverTrigger>
      <PopoverContent aria-label="Highlight colors">
        <ColorHighlightPopoverContent editor={editor} colors={colors} />
      </PopoverContent>
    </Popover>
  );
}

const i18nText = {
  [UserLangEnum.ZHCN]: {
    Highlight: "高亮",
    "Green background": "绿背景色",
    "Blue background": "蓝背景色",
    "Red background": "红背景色",
    "Purple background": "紫背景色",
    "Yellow background": "黄背景色",
    "Remove highlight": "移除高亮",
  },
  [UserLangEnum.ENUS]: {
    Highlight: "Highlight",
    "Green background": "Green background",
    "Blue background": "Blue background",
    "Red background": "Red background",
    "Purple background": "Purple background",
    "Yellow background": "Yellow background",
    "Remove highlight": "Remove highlight",
  },
};

export default ColorHighlightPopover;
