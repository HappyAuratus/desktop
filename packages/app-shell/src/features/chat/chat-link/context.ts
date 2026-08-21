import { createContext, useContext } from "react";
import type { SessionArtifactIndex } from "./artifact-index";

export interface ChatLinkContextValue {
  index: SessionArtifactIndex;
  /** Present for task conversations; Desktop OS handoff resolves through this id. */
  taskId?: string;
  /** Present for project-only draft chats so absolute paths strip against the checkout. */
  projectId?: string;
  cwd?: string | null;
}

export const ChatLinkContext = createContext<ChatLinkContextValue | null>(null);

/** Returns the session artifact index when the thread has a project or task checkout. */
export function useChatLinkContext(): ChatLinkContextValue | null {
  return useContext(ChatLinkContext);
}
