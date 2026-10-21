# Exchange protocol v1

HTTP health endpoint: `GET /healthz` → `{"status":"ok","protocol":1,"storage":"none"}`.

Upgrade to WebSocket at `/v1/exchange` (`ws://` for HTTP, `wss://` for HTTPS). All application frames are UTF-8 JSON text, maximum 128 KiB. A client sends one of the following within eight seconds:

```json
{"type":"create","version":1}
{"type":"join","version":1,"code":"1G6TW"}
```

Codes are five uppercase ASCII letters/digits. The server generates random, unused codes. Clients normalize lowercase input before joining. A successful handshake returns:

```json
{"type":"welcome","version":1,"code":"1G6TW","peer":"SERVER_GENERATED_32_HEX_ID","peers":["EXISTING_PEER_ID"]}
```

Existing peers receive `{"type":"peer_joined","peer":"ID"}`; departures produce `{"type":"peer_left","peer":"ID"}`. Peer IDs belong to one connection and change on reconnect. Rooms remain alive without their creator.

Send an opaque JSON payload to the whole room (excluding the sender) or to one peer in the same room:

```json
{"type":"relay","to":null,"payload":{"anything":"client defined"}}
{"type":"relay","to":"PEER_ID","payload":{"anything":"client defined"}}
```

Recipients receive `{"type":"relay","from":"AUTHENTIC_SENDER_ID","payload":...}`. The server supplies `from`; clients cannot spoof it. A nonexistent destination receives nothing. The server never acknowledges document persistence, interprets document data, or replays previous payloads. No sender echo is generated.

Errors before admission have type `error` and code `invalid_hello`, `invalid_code`, `room_not_found`, `room_full` or `server_full`; the connection then closes. HTTP 429/503 may be returned before upgrade. Invalid or oversized messages and congested connections close. Clients need to resynchronize from an online peer; they must not assume every edit was delivered after a disconnect.

Server ping interval is 20 seconds, with a 65-second inactivity deadline. Clients answer WebSocket pings. Admission, frame size, byte rate, participant count and outbound queue limits are enforced. The server keeps no acknowledgements or transfer state.

## PinePaper payload convention

The relay does not depend on this convention. PinePaper serializes one packet as JSON, compresses it with zlib, splits the compressed stream into ordered chunks of up to 32 KiB, and base64-encodes each chunk:

```json
{"transfer":"32_HEX_RANDOM_ID","part":0,"total":2,"data":"BASE64"}
```

A transfer is scoped to its sender and ID. Clients cap simultaneous assemblies, aggregate bytes, expiry and decompressed size. A malformed transfer fails closed; reconnection exchanges complete client state. Fragments are paced to stay under the relay byte budget and reads continue during sends.

Decoded packets:

- `{"kind":"need_state"}` — directed to an existing peer.
- `{"kind":"snapshot","state":SHARED_STATE,"source":null}` — directed response, or broadcast after reconnection to reconcile offline edits. PDFs include `source:{"sha256":"HEX","data":"BASE64_PDF"}`. Hashes and PDF headers are checked, and files receive locally generated safe paths.
- `{"kind":"ops","document":"DOCUMENT_ID","changes":[CHANGE]}` — broadcast changes on pen-up or another completed edit, never on hover or pen motion.

Shared state has a document ID, kind (`notebook`, `book`, `pdf`) and a map of registers. Keys describe title, page metadata/order/visibility, strokes, and text objects. Each register contains a current `(clock,actor)` stamp, birth stamp and value (an absent `value` field marks a deleted object; explicit JSON null is a nullable metadata value). The maximum stamp wins; distinct IDs merge. Page anchors and birth stamps establish deterministic insertion order. Page deletion is a visibility flag, retaining ordering anchors. Source paths, local database IDs and folder positions are never taken from a remote client. Client-local page IDs are mapped to shared IDs.

Snapshots contain registers including deletion markers, not just the visible page. This permits idempotent merging, reconnect reconciliation and deterministic conflict resolution. An undo writes new register values only where the current stamp still matches that user's original edit. Client saves are local SQLite transactions. CRDT registers exist for the lifetime of the app's sharing session; after restarting the app, share the materialized local document as a new room.
