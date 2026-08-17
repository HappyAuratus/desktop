import * as React from "react";
import { type UserLang, UserLangEnum } from "./types";

interface EditorContextValue {
  language: UserLang;
}

const EditorContext = React.createContext<EditorContextValue>({
  language: UserLangEnum.ENUS,
});

export interface EditorProviderProps {
  language?: UserLang;
  children: React.ReactNode;
}

/** Provides editor-level configuration (e.g. display language) to all descendant editor components. */
export function EditorProvider({
  language = UserLangEnum.ENUS,
  children,
}: EditorProviderProps) {
  const value = React.useMemo(() => ({ language }), [language]);
  return (
    <EditorContext.Provider value={value}>{children}</EditorContext.Provider>
  );
}

/** Returns the display language configured by the nearest EditorProvider. Defaults to en-US. */
export function useEditorLanguage(): UserLang {
  return React.useContext(EditorContext).language;
}
