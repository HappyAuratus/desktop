import { waitFor } from "@testing-library/react";
import { RemoteContractError, type AppEvent } from "@ora/contracts";
import { describe, expect, it, vi } from "vitest";
import { createMockClient, createMockClientState } from "../../test/mock-client";
import { createTestQueryClient, renderHookWithClient } from "../../test/hook-harness";
import { queryKeys } from "./query-keys";
import { useAppEvents } from "./use-app-events";

describe("useAppEvents", () => {
  it("refetches after Ready and invalidates sessions for title events", async () => {
    const client = createMockClient(createMockClientState());
    client.appEvents.watch = async function* (_request, options): AsyncGenerator<AppEvent> {
      yield { type: "ready" };
      yield { type: "session_title_updated", session_id: "session-1" };
      await new Promise<void>((resolve) => {
        const signal = options?.signal;
        if (signal === undefined || signal.aborted) {
          resolve();
          return;
        }
        signal.addEventListener("abort", () => resolve(), { once: true });
      });
    };
    const queryClient = createTestQueryClient();
    const refetch = vi.spyOn(queryClient, "refetchQueries");
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");

    const { result, unmount } = renderHookWithClient(() => useAppEvents(client), client, queryClient);

    await waitFor(() => expect(result.current.ready).toBe(true));
    expect(refetch).toHaveBeenCalledWith({ queryKey: queryKeys.sessions });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.sessions });

    unmount();
  });

  it("keeps the document client identity across a hot module reload", async () => {
    const client = createMockClient(createMockClientState());
    let activeClientInstanceId: string | undefined;
    client.appEvents.watch = async function* (request, options): AsyncGenerator<AppEvent> {
      if (activeClientInstanceId !== undefined && activeClientInstanceId !== request.clientInstanceId) {
        throw new RemoteContractError(
          {
            requestId: "hmr-app-event-test",
            code: "multiple_clients_unsupported",
            params: {},
          },
          409,
          null,
        );
      }
      activeClientInstanceId = request.clientInstanceId;
      yield { type: "ready" };
      await new Promise<void>((resolve) => {
        const signal = options?.signal;
        if (signal === undefined || signal.aborted) {
          resolve();
          return;
        }
        signal.addEventListener("abort", () => resolve(), { once: true });
      });
    };

    const first = renderHookWithClient(() => useAppEvents(client), client);
    await waitFor(() => expect(first.result.current.ready).toBe(true));

    vi.resetModules();
    const { useAppEvents: useHotReloadedAppEvents } = await import("./use-app-events");
    const hotReloaded = renderHookWithClient(() => useHotReloadedAppEvents(client), client);

    await waitFor(() => expect(hotReloaded.result.current.multipleClientsUnsupported).toBe(false));
    await waitFor(() => expect(hotReloaded.result.current.ready).toBe(true));

    hotReloaded.unmount();
    first.unmount();
  });
});
