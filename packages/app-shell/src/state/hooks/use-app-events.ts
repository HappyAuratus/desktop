import { useEffect, useState } from "react";
import { RemoteContractError, type ContractsClient } from "@ora/contracts";
import { useQueryClient } from "@tanstack/react-query";
import { queryKeys } from "./query-keys";

const INITIAL_RECONNECT_DELAY_MS = 1_000;
const MAX_RECONNECT_DELAY_MS = 30_000;

const DOCUMENT_CLIENT_INSTANCE_ID = Symbol.for("ora.app-shell.document-client-instance-id");

type AppEventDocumentGlobal = typeof globalThis & {
  [DOCUMENT_CLIENT_INSTANCE_ID]?: string;
};

/** Returns one in-memory identifier shared by every shell mounted in this document. */
function getDocumentClientInstanceId(): string {
  const documentGlobal = globalThis as AppEventDocumentGlobal;
  if (documentGlobal[DOCUMENT_CLIENT_INSTANCE_ID] === undefined) {
    // The global realm survives Vite module replacement, while a separate tab receives its own
    // realm. Module-local state would make HMR look like a second client to the backend lease.
    documentGlobal[DOCUMENT_CLIENT_INSTANCE_ID] = globalThis.crypto?.randomUUID?.()
      ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }
  return documentGlobal[DOCUMENT_CLIENT_INSTANCE_ID];
}

/** Maintains the application stream and invalidates authoritative session state on loss. */
export function useAppEvents(client: ContractsClient) {
  const queryClient = useQueryClient();
  const [retryGeneration, setRetryGeneration] = useState(0);
  const [ready, setReady] = useState(false);
  const [multipleClientsUnsupported, setMultipleClientsUnsupported] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    const clientInstanceId = getDocumentClientInstanceId();
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let reconnectDelay = INITIAL_RECONNECT_DELAY_MS;
    let disposed = false;

    const refetchSessions = () => {
      void queryClient.refetchQueries({ queryKey: queryKeys.sessions });
    };
    const invalidateSessions = () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessions });
    };
    const scheduleReconnect = () => {
      if (disposed) return;
      reconnectTimer = setTimeout(() => {
        reconnectTimer = undefined;
        void consume();
      }, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY_MS);
    };
    const handleDisconnect = () => {
      setReady(false);
      refetchSessions();
      scheduleReconnect();
    };
    const consume = async (): Promise<void> => {
      if (disposed) return;
      try {
        const events = client.appEvents.watch(
          { clientInstanceId },
          { signal: controller.signal },
        );
        for await (const event of events) {
          if (disposed) return;
          if (event.type === "ready") {
            reconnectDelay = INITIAL_RECONNECT_DELAY_MS;
            setMultipleClientsUnsupported(false);
            setReady(true);
            // The initial refetch closes the gap between database changes and stream ownership.
            refetchSessions();
          } else if (event.type === "session_title_updated") {
            invalidateSessions();
          }
        }
        handleDisconnect();
      } catch (error) {
        if (disposed || controller.signal.aborted) return;
        if (error instanceof RemoteContractError && error.code === "multiple_clients_unsupported") {
          setReady(false);
          setMultipleClientsUnsupported(true);
          return;
        }
        handleDisconnect();
      }
    };

    void consume();
    return () => {
      disposed = true;
      controller.abort();
      if (reconnectTimer !== undefined) clearTimeout(reconnectTimer);
    };
  }, [client, queryClient, retryGeneration]);

  return {
    ready,
    multipleClientsUnsupported,
    retry: () => setRetryGeneration((generation) => generation + 1),
  };
}
