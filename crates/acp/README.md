# ora-acp

`ora-acp` is Ora's provider-neutral ACP v1 transport over newline-delimited JSON-RPC stdio. It turns an asynchronous reader and writer into a typed request client plus an ordered stream of session events.

## Responsibilities

- `AcpPeer` owns the reader task and exposes an `AcpClient` for serialized writes.
- `AcpClient` correlates concurrent requests by `RequestId`, decodes typed responses, sends notifications, and answers agent-originated requests.
- Direct requests complete through a oneshot so initialization works before inbound dispatch starts. Session requests return a typed pending handle, and their responses enter the ordered inbound stream.
- Session updates, permission requests, and session responses remain in reader-observed wire order so a response cannot overtake tail updates from its turn.
- Protocol, framing, I/O, and response-decoding failures are normalized as `AcpError`.

## Boundaries and failure semantics

- Frames are newline-delimited JSON with an 8 MiB maximum. An oversized or malformed frame is fatal to the connection.
- Direct requests fail when the stream closes before their oneshot completes. Session requests unregister on abandon or drop so a late response is discarded instead of being routed into a newer turn.
- Invalid response envelopes and stdio loss remain fatal to the connection.
- Recognized permission requests are emitted as `PermissionRequest`. Unknown agent-originated methods receive a correlated method-not-found response without terminating the connection.
- The connection-to-router event channel is intentionally unbounded. Per-session bounds and overflow policy belong to the backend runtime, where one noisy session can be isolated from others.
- Writes share one mutex so concurrent JSON-RPC frames cannot interleave.

This crate does not spawn provider processes, supervise reconnects, route updates to Ora sessions, or enforce session lifecycle policy. Those responsibilities belong to `ora-backend`. See [ACP Agent Runtime](../../docs/agent-runtime.md).
