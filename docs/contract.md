# Long Lines Relay API, v1

This is the contract between Long Lines Relay (the desktop app, "the client") and Long Lines
(the server). A server author and a client author build from this file alone. The key words
MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in RFC 2119.

Status: v1, phase 1. Routes: `POST /device/start`, `POST /device/token`, `GET /me`,
`POST /contacts`.

## 1. Base URL and transport

| Environment | Base URL |
| --- | --- |
| Production | `https://api.longlines.io/relay/v1` |
| Local development | `http://127.0.0.1:<port>/functions/v1/relay-api/v1` (`<port>` is the local Supabase API port, 54321 by default) |

- In production, Vercel rewrites `https://api.longlines.io/relay/v1/<path>` to the Supabase Edge
  Function `relay-api` at `/functions/v1/relay-api/v1/<path>`. Clients use the production base
  URL or a local one, never the Supabase project URL.
- The function runs without gateway JWT verification. Locally a client MAY send an `apikey`
  header; the server ignores it, and no route requires it.
- The client SHOULD let a developer override the base URL (for example with an environment
  variable); release builds default to the production URL.
- Requests with a body MUST send `Content-Type: application/json` and a UTF-8 JSON object.
  Bodies over 256 KiB are refused with 400 `invalid_request`.
- Responses with a body are `application/json`. A 5xx or 429 response MAY have no body or a
  non-JSON body (a proxy may answer); clients MUST NOT require one.
- Every request MUST carry `User-Agent: LongLinesRelay/<relay_version> (<os>)`, for example
  `LongLinesRelay/0.1.0 (windows)`. `<os>` is one of `windows`, `macos`, `linux`.
- Timestamps are RFC 3339 in UTC with a `Z` suffix and no fraction:
  `YYYY-MM-DDTHH:MM:SSZ`.

## 2. Versioning

- The path is the version. This document is `v1`.
- When `v2` ships, the server MUST keep accepting `v1` requests with the behavior described
  here. Retiring `v1` is a separate, announced decision; deployed clients lag the server.
- The server MUST ignore request fields it does not know. Clients MUST ignore response fields
  they do not know.
- Within `v1` the server MAY add response fields and optional request fields. It MUST NOT add a
  new item `status`, a new `reason`, or a new `error` code that a `v1` client can receive, and it
  MUST NOT make an optional field required; such changes need `v2`.
- Defensively, a client that receives an item `status` or `reason` it does not know MUST treat
  the item as rejected and not retry it, and a client that receives an `error` code it does not
  know MUST act on the HTTP status alone (section 8).

## 3. Authentication and scopes

- A relay token is `ll_relay_` followed by exactly 52 characters from
  `abcdefghijklmnopqrstuvwxyz234567` (lower-case base32), 61 characters in all. Clients MUST
  treat it as opaque apart from that shape.
- The server stores only the lower-case hex SHA-256 of the token and returns the plaintext
  exactly once, from `POST /device/token`.
- Authenticated routes take `Authorization: Bearer <token>`.
- A token does not expire. It stops working when the account owner revokes it in the web app
  (the API tokens page lists relay tokens by device name) or deletes the account.
- The client MUST keep the token only in the OS credential store and MUST NOT write it to a
  config file, a log, or a crash report.
- Scopes. Phase 1 has one scope, `contacts:write`, and every token issued in phase 1 carries
  exactly `["contacts:write"]`. `GET /me` needs a valid token and no particular scope.
  `POST /contacts` needs `contacts:write`. A valid token without the scope a route needs gets
  403 `insufficient_scope`.
- A missing, malformed, unknown or revoked token gets 401 `bad_token`. On 401 the client MUST
  delete its stored token and go to the signed-out screen.
- Signing out in the client deletes the token locally. Phase 1 has no revoke route: the token
  stays valid on the server until it is revoked in the web app.

## 4. Pairing

Pairing follows RFC 8628 (OAuth 2.0 Device Authorization Grant) in shape. The client starts a
pairing, opens the browser at the verification URL, shows the user code, and polls until the
signed-in web user allows or denies it, or it expires.

### 4.1 `POST /device/start`

No authentication. Limited per client IP (429, section 8).

Request:

| Field | Type | Rule |
| --- | --- | --- |
| `client_name` | string | MUST be `Long Lines Relay`. 1 to 64 characters. |
| `client_version` | string | The app's version, for example `0.1.0`. 1 to 32 characters. |
| `device_name` | string | The computer's host name, shown on the approval page and the tokens list. 1 to 64 characters; the client MUST truncate a longer name to 64. |

Response 200:

| Field | Type | Meaning |
| --- | --- | --- |
| `device_code` | string | Opaque secret for polling, at most 128 characters. The client MUST NOT show or log it. |
| `user_code` | string | `XXXX-XXXX`: eight characters from `0123456789BCDFGHJKMNPQRSTVWXYZ` (Crockford base32 without vowels), a hyphen after the fourth. The client shows it as given. |
| `verification_uri` | string | `https://longlines.io/relay/approve` in production. |
| `verification_uri_complete` | string | `verification_uri` with `?code=<user_code>`. The client opens this in the default browser. |
| `expires_in` | integer | Seconds until the pairing expires (600, ten minutes). |
| `interval` | integer | Minimum seconds between polls (5). |

The server stores only a hash of `device_code`. Errors: 400 `invalid_request`, 429, 5xx.

Example:

```http
POST /relay/v1/device/start HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.1.0 (windows)
Content-Type: application/json

{"client_name": "Long Lines Relay", "client_version": "0.1.0", "device_name": "SHACK-PC"}
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "device_code": "5c6f2b1e0f7a4c0f9e3b8d2a61c4e7f0b9a3d5e8c1f2a4b6",
  "user_code": "WDJB-MJHT",
  "verification_uri": "https://longlines.io/relay/approve",
  "verification_uri_complete": "https://longlines.io/relay/approve?code=WDJB-MJHT",
  "expires_in": 600,
  "interval": 5
}
```

### 4.2 `POST /device/token`

No authentication. Limited per client IP.

Request: `{"device_code": "<device_code>"}`.

Polling rules:

- The client MUST wait at least `interval` seconds between polls. On `slow_down` it MUST add 5
  seconds to its interval for the rest of this pairing.
- The client MUST stop polling on success, `access_denied`, `expired_token`, when `expires_in`
  has passed, or when the user cancels.
- The server answers a poll that comes sooner than `interval` seconds after the previous one
  with `slow_down`.

Pending and failure responses are HTTP 400 with `{"error": "<code>"}` and MAY carry an
`error_description` string that clients ignore:

| `error` | Meaning | Client action |
| --- | --- | --- |
| `authorization_pending` | Not answered yet. | Poll again after `interval`. |
| `slow_down` | Polled too soon. | Add 5 s to the interval, poll again. |
| `access_denied` | The web user chose Deny. | Stop; back to signed out. |
| `expired_token` | Expired, already claimed, or the `device_code` is unknown. | Stop; back to signed out. |

Success, HTTP 200:

| Field | Type | Value |
| --- | --- | --- |
| `access_token` | string | The relay token (section 3). |
| `token_type` | string | `bearer` |
| `scopes` | array of string | `["contacts:write"]` |

The token is returned exactly once. Any later poll with the same `device_code` gets
`expired_token`. If the success response is lost, the client starts a new pairing; the orphaned
token appears in the web app's list and can be revoked there.

The server MUST NOT approve a pairing for an account without a callsign.

Example (pending, then success):

```http
POST /relay/v1/device/token HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.1.0 (windows)
Content-Type: application/json

{"device_code": "5c6f2b1e0f7a4c0f9e3b8d2a61c4e7f0b9a3d5e8c1f2a4b6"}
```

```http
HTTP/1.1 400 Bad Request
Content-Type: application/json

{"error": "authorization_pending"}
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "access_token": "ll_relay_exampleexampleexampleexampleexampleexampleexampleabc",
  "token_type": "bearer",
  "scopes": ["contacts:write"]
}
```

## 5. `GET /me`

Bearer token, any scope. Returns the account the token belongs to.

| Field | Type | Meaning |
| --- | --- | --- |
| `callsign` | string | The account callsign, upper case, never empty. |
| `email` | string | The account email. |

The account callsign can change in the web app. The client MUST call `GET /me` after pairing
and at every launch, and SHOULD call it again at least once an hour while running, so its
callsign check (section 7) uses the current callsign. The server checks against the current
callsign at submission time regardless.

Example:

```http
GET /relay/v1/me HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.1.0 (windows)
Authorization: Bearer ll_relay_exampleexampleexampleexampleexampleexampleexampleabc
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{"callsign": "KK4PWJ", "email": "operator@example.com"}
```

## 6. `POST /contacts`

Bearer token with `contacts:write`. Submits a batch of logged contacts.

### 6.1 Request

| Field | Type | Rule |
| --- | --- | --- |
| `relay_version` | string | The app's version, 1 to 32 characters. |
| `contacts` | array | 1 to 100 items, applied in array order. |

Each item:

| Field | Type | Upsert | Delete | Rule |
| --- | --- | --- | --- | --- |
| `external_id` | string | required | required | 1 to 256 characters; see 6.2. |
| `op` | string | `upsert` | `delete` | |
| `mycall` | string | required | ignored | The station callsign as logged, trimmed and upper-cased. 1 to 64 characters, no whitespace. |
| `call` | string | required | ignored | The worked station as logged, trimmed and upper-cased. 1 to 64 characters, no whitespace. |
| `frequency_hz` | integer | required | ignored | 1 to 300000000000. N1MM's `rxfreq` (else `txfreq`) is in 10 Hz units: multiply by 10. |
| `band` | string or null | required | ignored | The band of `frequency_hz` from Appendix A, or null when it is in none. |
| `mode` | string | required | ignored | As logged, trimmed and upper-cased, 1 to 32 characters. |
| `timestamp_utc` | string | required | required | The contact's logged time, `YYYY-MM-DDTHH:MM:SSZ` exactly. |
| `logger_app` | string or null | optional | ignored | The logger's `app` value, at most 64 characters; null or absent when the packet has none. |

A delete item needs only `external_id`, `op` and `timestamp_utc`; any other fields on it are
ignored.

Request-level validation. The whole request is refused with 400 `invalid_request` (nothing
applied) when the body is not a JSON object, `relay_version` is missing or invalid, `contacts`
is not an array of 1 to 100 objects, or any item lacks a valid `external_id`. Every other
problem with an item is that item's `rejected` / `invalid` result (6.4); the rest of the batch
is still applied.

### 6.2 `external_id`

- When the logger packet carries a per-contact ID (N1MM's `ID`), `external_id` MUST be that ID
  as received, trimmed.
- Otherwise `external_id` MUST be the lower-case hex SHA-256 of the UTF-8 string
  `mycall|call|band|mode|timestamp_utc`: the item's own field values, exactly as sent, joined
  by `|`, with an empty string for a null `band`.
- Without a logger ID an edit in the logger cannot be followed: an edited contact gets a new
  `external_id` and becomes a new spot, and a delete matches only when every hashed field is
  unchanged.

Example: `KK4PWJ|W1AW|20m|CW|2026-10-28T21:14:00Z` gives
`8c1a593b282999aa3e5714c50d593b3d9b7049c568d78cff4b15f711866f22cb`.

### 6.3 Server semantics

- Application is idempotent on (account, `external_id`). External IDs are per account; two
  accounts never share a contact.
- Items are applied one after another in array order, so a batch may hold several operations
  for one `external_id` (N1MM sends an edit as `contactdelete` then `contactreplace`).
- The server applies a request in one transaction: a 200 means every result is final; any other
  response means nothing was applied.
- Each upsert stores a spot with spotter = `mycall` as logged and spotted callsign = `call` as
  logged. The server derives the stored band from `frequency_hz` and the stored mode from
  `mode` with its own tables; the request's `band` is used for the `external_id` hash only.
- Spots are never deleted. A retracted spot keeps its row with a retraction time and is hidden
  from every reader (profiles, recent spots, rollups, matching). Deliveries already sent are not
  recalled.
- Two upserts carry the same content when `mycall`, `call`, `frequency_hz`, `band`, `mode` and
  `timestamp_utc` are equal after the server's normalization. `logger_app` and `relay_version`
  are not content.

| Op | Existing contact for (account, `external_id`) | Effect | `status` |
| --- | --- | --- | --- |
| `upsert` | none | Stores a new spot. | `created` |
| `upsert` | live, same content | Nothing. | `unchanged` |
| `upsert` | live, different content | Retracts the old spot, stores a new one. | `updated` |
| `upsert` | retracted | Stores a new spot; the contact is live again. | `created` |
| `delete` | live | Retracts its spot. | `retracted` |
| `delete` | none, or retracted | Nothing. | `unknown` |
| either | any | The item fails a check below; nothing changes. | `rejected` |

Checks, in this order, with `now` the server's receipt time:

1. `invalid`: a field is missing, has the wrong type, or breaks a rule in 6.1 (including an
   unknown `op`).
2. `clock_skew` (both ops): `timestamp_utc` is more than `MAX_CLOCK_SKEW` after `now`.
3. `too_old` (upsert only): `timestamp_utc` is more than `MAX_CONTACT_AGE` before `now`. An
   upsert this old is rejected even when it would update a live contact; that contact is left
   as it was. So an N1MM edit (delete, then replace) of a contact logged more than 30 minutes
   ago removes its spot and does not store a new one.
4. `mycall_mismatch` (upsert only): `base(mycall)` differs from `base(account callsign)`
   (section 7).

Deletes are exempt from the age rule: a delete of a contact logged more than 30 minutes ago is
accepted, so a correction in the logger always removes the spot.

Freshness constants, enforced by both sides:

| Constant | Value |
| --- | --- |
| `MAX_CONTACT_AGE` | 30 minutes |
| `MAX_CLOCK_SKEW` | 5 minutes |

### 6.4 Response

HTTP 200 with `{"results": [...]}`: one result per input item, in input order (the client
matches by index; `external_id` is echoed as a check).

| Field | Type | Meaning |
| --- | --- | --- |
| `external_id` | string | The item's `external_id`. |
| `status` | string | `created`, `updated`, `unchanged`, `retracted`, `unknown` or `rejected`. |
| `reason` | string | Present only when `status` is `rejected`: `too_old`, `clock_skew`, `mycall_mismatch` or `invalid`. |

Every status is final; the client MUST NOT retry an item that got a result. After a resend of a
request whose 200 was lost, an upsert answers `unchanged` and a delete `unknown`; the client
treats both as success.

### 6.5 Client rules

- The client MUST NOT have more than one `POST /contacts` in flight per token, and MUST send a
  contact's operations in the order the logger sent them.
- While items wait in its queue, the client MAY coalesce operations for one `external_id`; the
  last operation wins.
- The client MUST apply the freshness rules before enqueueing and again before sending: it
  MUST NOT send an upsert whose `timestamp_utc` is more than `MAX_CONTACT_AGE` before its own
  clock (a queued upsert that ages out is dropped and shown as not sent), and MUST NOT send any
  item more than `MAX_CLOCK_SKEW` ahead of its own clock. Deletes do not age out.
- The client MUST NOT send an upsert whose `mycall` fails the section 7 check, or one with an
  empty `mycall`.
- The client SHOULD keep to 120 contacts a minute without waiting for a 429.

Example. The first item is new, the second is an edit of a contact sent earlier, the third
deletes a contact, the fourth was logged under another callsign:

```http
POST /relay/v1/contacts HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.1.0 (windows)
Authorization: Bearer ll_relay_exampleexampleexampleexampleexampleexampleexampleabc
Content-Type: application/json

{
  "relay_version": "0.1.0",
  "contacts": [
    {
      "external_id": "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b",
      "op": "upsert",
      "mycall": "KK4PWJ",
      "call": "W1AW",
      "frequency_hz": 14025000,
      "band": "20m",
      "mode": "CW",
      "timestamp_utc": "2026-10-28T21:14:00Z",
      "logger_app": "N1MM"
    },
    {
      "external_id": "a7e1b9c0d2f34e5a8b6c7d8e9f0a1b2c",
      "op": "upsert",
      "mycall": "KK4PWJ/P",
      "call": "N4AB",
      "frequency_hz": 7185500,
      "band": "40m",
      "mode": "LSB",
      "timestamp_utc": "2026-10-28T21:10:30Z",
      "logger_app": "N1MM"
    },
    {
      "external_id": "0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e",
      "op": "delete",
      "timestamp_utc": "2026-10-28T20:31:00Z"
    },
    {
      "external_id": "f79e495bd37010e153bd2da07fd4eb22a248faa643887dffdd1d33e918e1c91c",
      "op": "upsert",
      "mycall": "K4XYZ",
      "call": "VE3ABC",
      "frequency_hz": 21074000,
      "band": "15m",
      "mode": "FT8",
      "timestamp_utc": "2026-10-28T21:15:45Z",
      "logger_app": null
    }
  ]
}
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "results": [
    {"external_id": "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b", "status": "created"},
    {"external_id": "a7e1b9c0d2f34e5a8b6c7d8e9f0a1b2c", "status": "updated"},
    {"external_id": "0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e", "status": "retracted"},
    {"external_id": "f79e495bd37010e153bd2da07fd4eb22a248faa643887dffdd1d33e918e1c91c", "status": "rejected", "reason": "mycall_mismatch"}
  ]
}
```

(The fourth item's `external_id` is the fallback hash of
`K4XYZ|VE3ABC|15m|FT8|2026-10-28T21:15:45Z`, as for a logger that sends no ID. A conforming
client would not send it; it appears here to show a rejection.)

## 7. Callsign check

`base(c)` reduces a callsign to its base call. Both sides MUST compute it exactly this way (it
is the server's SQL `base_callsign()`):

1. Trim and upper-case `c`.
2. Drop everything from the first `-` (an SSID).
3. Split on `/` into parts.
4. Discard empty parts, the parts `P`, `M`, `MM`, `AM`, `QRP`, `QRPP` and `R`, and any part that
   is a single digit.
5. Of the remaining parts, take the longest. On a tie, prefer a part that has a letter followed
   by a digit; on a further tie, the later part.
6. If no part remains, the result is the string from step 2.

An upsert passes when `base(mycall) == base(account callsign)`. `mycall` is still sent and
stored exactly as logged.

| Input | `base` |
| --- | --- |
| `KK4PWJ` | `KK4PWJ` |
| `kk4pwj ` | `KK4PWJ` |
| `W4/KK4PWJ` | `KK4PWJ` |
| `KK4PWJ/P` | `KK4PWJ` |
| `KK4PWJ/M`, `KK4PWJ/MM`, `KK4PWJ/AM` | `KK4PWJ` |
| `KK4PWJ/QRP`, `KK4PWJ/QRPP`, `KK4PWJ/R` | `KK4PWJ` |
| `KK4PWJ/4` | `KK4PWJ` |
| `EA8/DL1ABC/P` | `DL1ABC` |
| `VP2V/W1AW` | `W1AW` |
| `W1AW-7` | `W1AW` |

## 8. Errors and rate limits

Error bodies are `{"error": "<code>"}` and MAY carry a human-readable `message`. Clients branch
on `error` (and the HTTP status), never on `message`.

| HTTP | `error` | When | Client action |
| --- | --- | --- | --- |
| 400 | `invalid_request` | Malformed body or request-level rule broken; `message` says which. | Show the error; do not retry the request. |
| 400 | `authorization_pending`, `slow_down`, `access_denied`, `expired_token` | `POST /device/token` only (4.2). | As in 4.2. |
| 401 | `bad_token` | Missing, malformed, unknown or revoked token. | Delete the token; signed out. |
| 403 | `insufficient_scope` | The token lacks the route's scope. | Show the error; do not retry. |
| 404 | `not_found` | Unknown path or method, including a version the server does not serve. | Show the error; do not retry. |
| 429 | `rate_limited` | A limit below was hit. | Wait `Retry-After` seconds, then retry the same request. |
| 5xx | (any or none) | Server or proxy trouble. | Retry with exponential backoff; the request may be resent as is. |

Network failures are handled like 5xx. Network trouble MUST NOT block receiving UDP packets.

Rate limits:

- `POST /contacts`: 120 contacts per token per minute, counted per contact (deletes count),
  in fixed one-minute windows. A request whose items would take the window past 120 is refused
  whole with 429 and none of its items is applied. Refused requests MAY count toward the
  window.
- Every route is also limited per client IP; `POST /device/start` most tightly. The per-IP
  limits are the server's to set and may change.
- A 429 MUST carry `Retry-After` as an integer number of seconds. A client that cannot read it
  MUST wait 60 seconds.

## 9. Reserved for phase 2

Not specified and not served in phase 1. Listed so the names are not used for anything else.

- `GET /spots`: a stream of the account's matched spots, which the relay will serve to the
  logging software through a local telnet server (a DX cluster on loopback).
- Scope `spots:read`, needed by `GET /spots`. Phase 1 tokens never carry it, and the server
  refuses a scope a route does not need. How a client asks for it at pairing is part of the
  phase 2 specification.

Until then the server answers `GET /spots` with 404 `not_found`.

## Appendix A. Bands

The client's `band` value. Ranges are in kHz, inclusive; the first matching row wins, so
QO-100 frequencies get `3cm_qo100`. A frequency in no row has band null. This is the server's
band table (`supabase/functions/_shared/bands.ts`).

| From | To | `band` |
| --- | --- | --- |
| 135 | 138 | `2200m` |
| 472 | 479 | `600m` |
| 1800 | 2000 | `160m` |
| 3500 | 4000 | `80m` |
| 5000 | 5500 | `60m` |
| 7000 | 7300 | `40m` |
| 10000 | 10200 | `30m` |
| 14000 | 14500 | `20m` |
| 18000 | 18200 | `17m` |
| 21000 | 21500 | `15m` |
| 24800 | 25000 | `12m` |
| 26000 | 27999 | `11m` |
| 28000 | 30000 | `10m` |
| 40000 | 41000 | `8m` |
| 50000 | 54000 | `6m` |
| 70000 | 71000 | `4m` |
| 144000 | 148000 | `2m` |
| 219000 | 225000 | `1.25m` |
| 430000 | 440000 | `70cm` |
| 1200000 | 1400000 | `23cm` |
| 2300000 | 2450000 | `13cm` |
| 3300000 | 3500000 | `9cm` |
| 5400000 | 5900000 | `6cm` |
| 10489550 | 10490000 | `3cm_qo100` |
| 10000000 | 10500000 | `3cm` |

The comparison is on `frequency_hz / 1000` as a real number, so 14025000 Hz (14025 kHz) is
`20m` and 14500500 Hz (14500.5 kHz) is null.
