# Long Lines Relay API, v1 and v2

This is the contract between Long Lines Relay (the desktop app, "the client") and Long Lines
(the server). A server author and a client author build from this file alone. The key words
MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in RFC 2119.

Status: v2, phase 2. The server serves v1 and v2 side by side, and v1 behaves exactly as it did
in phase 1.

- v1 (phase 1) routes: `POST /device/start`, `POST /device/token`, `GET /me`,
  `POST /contacts`.
- v2 (phase 2) routes: the same four, where `POST /device/start` takes `scopes` and `platform`
  (4.1) and `GET /me` adds `relay_id`, `device_name`, `scopes` and `spots_url` (5); plus the
  spots stream at `spots_url` (section 9), which the client re-serves to its logging software
  as a DX cluster on the user's computer (sections 10 and 11).

## 1. Base URL and transport

| Version | Environment | Base URL |
| --- | --- | --- |
| v2 | Production | `https://api.longlines.io/relay/v2` |
| v2 | Local development | `http://127.0.0.1:<port>/functions/v1/relay-api/v2` |
| v1 | Production | `https://api.longlines.io/relay/v1` |
| v1 | Local development | `http://127.0.0.1:<port>/functions/v1/relay-api/v1` |

`<port>` is the local Supabase API port, 54321 by default. In this document a route such as
`POST /device/start` is relative to the base URL of the version in use. The spots stream is the
exception: it is not under the base URL, and the client takes its URL from `GET /me` (section 9).

- In production, Vercel rewrites `https://api.longlines.io/relay/<version>/<path>` to the
  Supabase Edge Function `relay-api` at `/functions/v1/relay-api/<version>/<path>`. Clients use
  the production base URL or a local one, never the Supabase project URL.
- The function runs without gateway JWT verification. Locally a client MAY send an `apikey`
  header; the server ignores it, and no route requires it.
- The client SHOULD let a developer override the base URL (for example with an environment
  variable); release builds default to the production URL.
- Requests with a body MUST send `Content-Type: application/json` and a UTF-8 JSON object.
  Bodies over 256 KiB are refused with 400 `invalid_request`.
- Responses with a body are `application/json`, except the spots stream's 200
  (`text/event-stream`, section 9). A 5xx or 429 response MAY have no body or a non-JSON body
  (a proxy may answer); clients MUST NOT require one.
- Every request, the spots stream's included, MUST carry
  `User-Agent: LongLinesRelay/<relay_version> (<os>)`, for example
  `LongLinesRelay/0.2.0 (windows)`. `<os>` is one of `windows`, `macos`, `linux`.
- Timestamps are RFC 3339 in UTC with a `Z` suffix and no fraction:
  `YYYY-MM-DDTHH:MM:SSZ`.
- Lengths in this document count characters as Unicode code points (Rust's `chars().count()`,
  Postgres' `char_length`), not bytes or UTF-16 units. The spot line (section 10) is ASCII, so
  there a character is a byte.
- No string value may contain U+0000 or an unpaired surrogate escape (`\ud800`); Postgres cannot
  store either. Where section 6.1 makes a field a request-level rule, such a string is a 400;
  elsewhere in a contact it is that item's `rejected` / `invalid`.

## 2. Versioning

- The path is the version. This document specifies `v1` and `v2`.
- The server MUST keep accepting `v1` requests with the behavior this document gives `v1`.
  Retiring `v1` is a separate, announced decision; deployed clients lag the server. A `v1`
  route never gains a `v2` field, status, reason or error code.
- `v2` is `v1` plus the additions marked "v2" in this document. A `v2` client SHOULD use `v2`
  routes only, and MUST poll `POST /device/token` with the version it started the pairing with.
- Tokens are not versioned: a token works on either version's authenticated routes, subject to
  its scopes (3.1).
- The server MUST ignore request fields it does not know. Clients MUST ignore response fields,
  stream event names and spot fields they do not know.
- Within a version the server MAY add response fields and optional request fields. It MUST NOT
  add a new item `status`, a new `reason`, or a new `error` code that a client of that version
  can receive, and it MUST NOT make an optional field required; such changes need a new version.
- Defensively, a client that receives an item `status` or `reason` it does not know MUST treat
  the item as rejected and not retry it, and a client that receives an `error` code it does not
  know MUST act on the HTTP status alone (section 8).

## 3. Authentication and scopes

- A relay token is `ll_relay_` followed by exactly 52 characters from
  `abcdefghijklmnopqrstuvwxyz234567` (lower-case base32), 61 characters in all. Clients MUST
  treat it as opaque apart from that shape.
- The server stores only the lower-case hex SHA-256 of the token and returns the plaintext
  exactly once, from `POST /device/token`.
- Authenticated routes, and the spots stream, take `Authorization: Bearer <token>`.
- A token does not expire. It stops working when the account owner revokes it in the web app
  (the API tokens page lists relay tokens by device name) or deletes the account.
- The client MUST keep the token only in the OS credential store and MUST NOT write it to a
  config file, a log, or a crash report.
- A missing, malformed, unknown or revoked token gets 401 `bad_token`. On 401 the client MUST
  delete its stored token and go to the signed-out screen.
- Signing out in the client deletes the token locally. Neither version has a revoke route: the
  token stays valid on the server until it is revoked in the web app.

### 3.1 Scopes

| Scope | Grants | Needed by |
| --- | --- | --- |
| `contacts:write` | Posting the contacts the user logs. | `POST /contacts` |
| `spots:read` | Receiving the spots that match the account's connections, for the client's local DX cluster. | The spots stream (section 9) |

- `GET /me` needs a valid token and no particular scope.
- A valid token without the scope a route needs gets 403 `insufficient_scope`.
- A token's scopes are fixed when it is paired and never change. A `v1` pairing grants exactly
  `["contacts:write"]`. A `v2` pairing grants the scopes the client asked for in
  `POST /device/start` (4.1); the approval page shows what each one grants, and the user allows
  all of them or denies the pairing.
- Re-pair rule. There is no route that adds a scope to a token. A client holding a token
  without `spots:read` (every token paired by a phase 1 client) MUST NOT open the spots stream,
  and on a 403 `insufficient_scope` from it MUST NOT retry. It tells the user to sign in again:
  sign out (which deletes the token locally) and pair with `v2`, asking for both scopes. The
  old token stays valid and listed in the web app until the user revokes it there. A Local DX
  cluster destination targets one relay (one token, by `relay_id`), so after a re-pair the user
  picks the new relay in that destination.

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
| `scopes` | array of string | v2 only. The scopes to ask for (3.1): one or both of `contacts:write` and `spots:read`, distinct, in any order. A v2 client asks for `["contacts:write", "spots:read"]`. When the field is absent the server asks for both. An empty array, a repeated value, a non-string or an unknown scope is 400 `invalid_request`. |
| `platform` | string | v2 only, required. The client's OS: `windows`, `macos` or `linux`, the same word as the `<os>` in its `User-Agent`. Absent or any other value is 400 `invalid_request`. The web app shows it next to the device name. |

The v1 route ignores `scopes` and `platform` if a client sends them and always asks for
`["contacts:write"]`.

Response 200:

| Field | Type | Meaning |
| --- | --- | --- |
| `device_code` | string | Opaque secret for polling, at most 128 characters. The client MUST NOT show or log it. |
| `user_code` | string | `XXXX-XXXX`: eight characters from `0123456789BCDFGHJKMNPQRSTVWXYZ` (Crockford base32 without vowels), a hyphen after the fourth. The client shows it as given. |
| `verification_uri` | string | `https://longlines.io/relay` in production: where a user types the code by hand. The client shows it without the scheme ("Go to longlines.io/relay and enter the code."). |
| `verification_uri_complete` | string | The approval page with the code filled in, `https://longlines.io/relay/approve?code=<user_code>` in production. The client opens this in the default browser. |
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
  "verification_uri": "https://longlines.io/relay",
  "verification_uri_complete": "https://longlines.io/relay/approve?code=WDJB-MJHT",
  "expires_in": 600,
  "interval": 5
}
```

The same on v2:

```http
POST /relay/v2/device/start HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.2.0 (windows)
Content-Type: application/json

{
  "client_name": "Long Lines Relay",
  "client_version": "0.2.0",
  "device_name": "SHACK-PC",
  "scopes": ["contacts:write", "spots:read"],
  "platform": "windows"
}
```

The response has the same shape as on v1.

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
| `scopes` | array of string | The scopes granted, in the order of the table in 3.1: `["contacts:write"]` on v1; on v2 the scopes asked for in 4.1, such as `["contacts:write", "spots:read"]`. |

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

On v2 the success response names the scopes granted:

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "access_token": "ll_relay_exampleexampleexampleexampleexampleexampleexampleabc",
  "token_type": "bearer",
  "scopes": ["contacts:write", "spots:read"]
}
```

## 5. `GET /me`

Bearer token, any scope. Returns the account the token belongs to, and on v2 the token itself.

| Field | Version | Type | Meaning |
| --- | --- | --- | --- |
| `callsign` | v1, v2 | string | The account callsign, upper case, never empty. |
| `email` | v1, v2 | string | The account email. |
| `relay_id` | v2 | string | This token's id, a lower-case UUID (`8-4-4-4-12` hex digits). It is stable for the token's life, names this relay in the web app's Local DX cluster destinations, and is the `relay_id` of the stream's `hello` event. A re-pair gives a new one. |
| `device_name` | v2 | string | The `device_name` sent when this token was paired, 1 to 64 characters. |
| `scopes` | v2 | array of string | The scopes this token carries (3.1), in the order of the scope table. |
| `spots_url` | v2 | string | The absolute URL of the spots stream (section 9). Always present, whatever the token's scopes. |

The v1 route answers with `callsign` and `email` as before.

The account callsign can change in the web app. The client MUST call `GET /me` after pairing
and at every launch, and SHOULD call it again at least once an hour while running, so its
callsign check (section 7) uses the current callsign. The server checks against the current
callsign at submission time regardless. A v2 client also takes `scopes` and `spots_url` from
the latest answer: it opens the stream only when `scopes` holds `spots:read`, and it uses
`spots_url` exactly as given, never building it from the base URL. `spots_url` is `https` in
production (for example `https://spots.longlines.io/v2/spots`; the host is the server's to
choose) and `http://127.0.0.1:8081/v2/spots` against a local stack. The client MUST NOT send
its token to an `http` URL whose host is not a loopback address (`127.0.0.1`, `::1`,
`localhost`).

Example (v1):

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

Example (v2):

```http
GET /relay/v2/me HTTP/1.1
Host: api.longlines.io
User-Agent: LongLinesRelay/0.2.0 (windows)
Authorization: Bearer ll_relay_exampleexampleexampleexampleexampleexampleexampleabc
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "callsign": "KK4PWJ",
  "email": "operator@example.com",
  "relay_id": "3f6c2a9e-8b1d-4c7a-9e2f-5a0b1c2d3e4f",
  "device_name": "SHACK-PC",
  "scopes": ["contacts:write", "spots:read"],
  "spots_url": "https://spots.longlines.io/v2/spots"
}
```

## 6. `POST /contacts`

Bearer token with `contacts:write`. Submits a batch of logged contacts.

The route is the same on v1 and v2: the same request, semantics, response and limits, except
that a v2 upsert item may carry the optional `state` and `gridsquare` fields (6.1). The examples
use v1 paths; on v2 the path is `/relay/v2/contacts`.

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
| `mycall` | string | required | ignored | The station callsign as logged, trimmed and upper-cased. 1 to 64 characters, no whitespace or control characters (see below). |
| `call` | string | required | ignored | The worked station as logged, trimmed and upper-cased. 1 to 64 characters, no whitespace or control characters. |
| `frequency_hz` | integer | required | ignored | 1 to 300000000000. N1MM's `rxfreq` (else `txfreq`) is in 10 Hz units: multiply by 10. |
| `band` | string or null | required | ignored | The band of `frequency_hz` from Appendix A, or null when it is in none. |
| `mode` | string | required | ignored | As logged, trimmed and upper-cased, 1 to 32 characters. |
| `timestamp_utc` | string | required | required | The contact's logged time, `YYYY-MM-DDTHH:MM:SSZ` exactly. |
| `logger_app` | string or null | optional | ignored | The logger's `app` value, at most 64 characters; null or absent when the packet has none. |
| `state` | string | optional, v2 | ignored | The worked station's US state or the District of Columbia: exactly one of the 51 upper-case two-letter postal codes in Appendix B. Absent when the logger gave none; see "Location fields" below. |
| `gridsquare` | string | optional, v2 | ignored | The worked station's Maidenhead locator as the logger gave it, upper-cased: 4, 6 or 8 characters, matching `^[A-R]{2}[0-9]{2}([A-X]{2}([0-9]{2})?)?$`. Absent when the logger gave none. |

A delete item needs only `external_id`, `op` and `timestamp_utc`; any other fields on it are
ignored.

Location fields (v2). `state` and `gridsquare` describe the worked station, never the user's
own station, and come only from the logger's packet:

- The client sends `state` only when one of the packet's `state`, `section`, `qth` or
  `exchange1` elements (N1MM element names, matched without regard to case), trimmed and
  upper-cased, is exactly a code in Appendix B. A longer value is never shortened or mapped:
  an ARRL section such as `ENY`, a county or a province gives no `state`. When two of those
  elements hold different codes, the client sends no `state`.
- The client sends `gridsquare` only when the packet's `gridsquare` element, trimmed and
  upper-cased, matches the rule in the table. Any other value (2 or 10 characters, a field
  letter past `R`, a subsquare letter past `X`) gives no `gridsquare`.
- The client never works either out from the callsign, a lookup, the frequency or the user's
  own location, and never sends either as null or an empty string: a field it has no value
  for is absent.
- Neither is part of `external_id` (6.2) or of a contact's content (6.3).
- A v1 server, and a server from before these fields, ignores them as unknown fields (section
  2). A v2 server that knows them MUST NOT reject an item because of them: a `state` or
  `gridsquare` that breaks its rule is treated as absent, so a contact is never lost to an
  optional field. The client MUST NOT send them on v1.

"Whitespace or control characters" in `mycall` and `call` means Unicode White_Space, U+FEFF and
the general category Cc (which holds U+0000 to U+001F and U+007F to U+009F).

Request-level validation. The whole request is refused with 400 `invalid_request` (nothing
applied) when the body is not a JSON object, `relay_version` is missing or invalid, `contacts`
is not an array of 1 to 100 objects, or any item lacks a valid `external_id` (a string of 1 to
256 characters without U+0000). Every other
problem with an item is that item's `rejected` / `invalid` result (6.4); the rest of the batch
is still applied.

### 6.2 `external_id`

- When the logger packet carries a per-contact ID (N1MM's `ID`), `external_id` MUST be that ID
  as received, trimmed.
- An ID that is empty after trimming, longer than 256 characters or holds a control character
  (category Cc) is treated as no ID: the server would refuse the whole request for it.
- Otherwise `external_id` MUST be the lower-case hex SHA-256 of the UTF-8 string
  `mycall|call|band|mode|timestamp_utc`: the item's own field values, exactly as sent, joined
  by `|`, with an empty string for a null `band`. No other field is hashed: `logger_app`,
  `state` and `gridsquare` never are, so a contact's `external_id` does not change when the
  logger adds or edits them.
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
- Two upserts carry the same content when `mycall`, `call`, `frequency_hz`, `mode` and
  `timestamp_utc` are equal after the server's normalization (trimmed and upper-cased). The
  request's `band` is not compared: the server's band follows from `frequency_hz`. `logger_app`,
  `state`, `gridsquare`, `relay_version` and fields the server does not know are not content.
  An upsert that differs from the live contact only in `state` or `gridsquare` answers
  `unchanged`; the server MAY store the new values.

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

On v2 an upsert item may also carry the location fields. This item is the first one above,
logged in a contest where the other station sent the section `NC` and the grid `FM05pu`:

```json
{
  "external_id": "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b",
  "op": "upsert",
  "mycall": "KK4PWJ",
  "call": "W1AW",
  "frequency_hz": 14025000,
  "band": "20m",
  "mode": "CW",
  "timestamp_utc": "2026-10-28T21:14:00Z",
  "logger_app": "N1MM",
  "state": "NC",
  "gridsquare": "FM05PU"
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
| `KK4PWJ-7`, `KK4PWJ/P-7` | `KK4PWJ` |
| `VP2E/KK4PWJ/M` | `KK4PWJ` |
| `9A/DL1ABC/QRP` | `DL1ABC` |
| `G4ABC/A`, `G4ABC/A/P` | `G4ABC` |
| `W1AW/KH6`, `KH6/W1AW`, `W1AW/7/P` | `W1AW` |
| `W1AW/ABCD`, `ABCD/W1AW` (tie: the one with a letter then a digit) | `W1AW` |
| `K1AB/W1AW` (tie: the later part) | `W1AW` |
| `AB/CD` (tie, no digit: the later part) | `CD` |
| `P`, `4`, `/` (no part left) | unchanged |
| `-7`, empty | empty |

Every row was checked against the server's SQL and the relay's Rust port.

## 8. Errors and rate limits

Error bodies are `{"error": "<code>"}` and MAY carry a human-readable `message`. Clients branch
on `error` (and the HTTP status), never on `message`. The same codes apply on v1 and v2, and to
a spots stream request that is not answered with 200.

| HTTP | `error` | When | Client action |
| --- | --- | --- | --- |
| 400 | `invalid_request` | Malformed body or request-level rule broken; `message` says which. | Show the error; do not retry the request. |
| 400 | `authorization_pending`, `slow_down`, `access_denied`, `expired_token` | `POST /device/token` only (4.2). | As in 4.2. |
| 401 | `bad_token` | Missing, malformed, unknown or revoked token. | Delete the token; signed out. |
| 403 | `insufficient_scope` | The token lacks the route's scope. | Show the error; do not retry. On the spots stream: sign in again (3.1). |
| 404 | `not_found` | Unknown path or method, including a version the server does not serve, and `GET /spots` under either base URL (the stream is at `spots_url`, not under the base URL). | Show the error; do not retry. On the spots stream: reconnect with backoff (9.5). |
| 429 | `rate_limited` | A limit below was hit. | Wait `Retry-After` seconds, then retry the same request. |
| 5xx | (any or none) | Server or proxy trouble. | Retry with exponential backoff; the request may be resent as is. |

Network failures are handled like 5xx. Network trouble MUST NOT block receiving UDP packets or
serving the local DX cluster (section 11).

Rate limits:

- `POST /contacts`: 120 contacts per token per minute, counted per contact (deletes count),
  in fixed one-minute windows shared by v1 and v2. A request whose items would take the window
  past 120 is refused whole with 429 and none of its items is applied. Refused requests MAY
  count toward the window.
- `POST /device/start` and `POST /device/token` are also limited per client IP,
  `POST /device/start` most tightly; `GET /me` and `POST /contacts` are also limited in requests
  per token, never per IP. Opening the spots stream MAY be limited per token. These limits are
  the server's to set and may change.
- The spots stream carries at most 120 spot events per relay per minute (9.4).
- A 429 MUST carry `Retry-After` as an integer number of seconds. A client that cannot read it
  MUST wait 60 seconds.

## 9. Spots stream (v2)

`GET <spots_url>`, where `spots_url` comes from `GET /me` (section 5). Bearer token with
`spots:read`. A Server-Sent Events stream (the "Server-sent events" section of the WHATWG HTML
standard) of the spots that match the account's connections and go to a Local DX cluster
destination that targets this relay. The client re-serves each spot to its logging software as
one DX cluster line (sections 10 and 11).

### 9.1 Request

| Header | Value |
| --- | --- |
| `Authorization` | `Bearer <token>` |
| `Accept` | `text/event-stream` |
| `User-Agent` | As in section 1. |

There is no body and no query string. The client MUST NOT send `Last-Event-ID`; the server
ignores it.

The client opens the stream only while all of these hold, and closes it as soon as one stops
holding:

- its DX cluster switch is on;
- it holds a token;
- the latest `GET /me` listed `spots:read` in `scopes`.

The client MUST NOT open a second stream for a token while one is open.

### 9.2 Response

Anything but 200 is an error response from section 8: 401 `bad_token`, 403
`insufficient_scope`, 404 `not_found`, 429 `rate_limited`, 5xx.

A 200 has `Content-Type: text/event-stream` and `Cache-Control: no-store`, and its body does not
end while the stream is healthy.

- An open stream is what makes the relay online. While it is open the web app shows the relay
  as connected and the server sends it spots. While no stream is open the server does not
  queue spots for the relay: a spot that matches then is sent only if a stream opens within 2
  minutes of the match, and is dropped after that. (The 2 minutes is server policy and may
  change; the client MUST NOT depend on it.)
- A newer connection for the same token replaces the older one: the server ends the older
  stream's response without an event. Two copies of the client running with one token
  therefore push each other off; the backoff in 9.5 keeps that from becoming a tight loop.
- The server MAY end the stream at any time (a deploy, a restart). The client reconnects
  (9.5).
- There is no replay: no event ids, no `Last-Event-ID`, no backlog. A spot sent while the
  client was disconnected is not sent again.

### 9.3 Framing

- The body is UTF-8. The server ends lines with LF; the client MUST also accept CRLF and CR, as
  the SSE standard requires.
- An event is an `event:` line, one `data:` line holding a JSON object on one line, and an empty
  line. The server sends no `id:` line. It MAY send a `retry:` line; the client ignores it and
  uses 9.5.
- A line that starts with `:` is a comment. The server sends `: keepalive` followed by an empty
  line every 20 seconds, whatever else it sends, so an idle stream still carries bytes. The
  client ignores comments.
- The client MUST ignore events whose name it does not know and fields it does not know inside
  `data`. Should an event carry several `data:` lines, the client joins them with LF, as the
  standard says; the server sends one.

| Event | When | `data` |
| --- | --- | --- |
| `hello` | First, exactly once per stream, before any `spot`. | `{"relay_id": "<uuid>", "device_name": "<string>"}`: the same values as `GET /me`. |
| `spot` | Once per spot sent to this relay. | A spot object (9.4). |

The client treats the stream as established on `hello`.

### 9.4 Spot event

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | integer | The spot's id in Long Lines, 1 to 2^53 - 1. |
| `spotter` | string | Who spotted it: the spotter as the source gave it (an RBN skimmer, a DX cluster user, a logged contact's `mycall`). 1 to 64 characters. The server sends `LONGLINES` for a spot without one. |
| `frequency_khz` | number | Greater than 0, rounded by the server to at most one decimal place. The server never sends a spot without a frequency. |
| `callsign` | string | The spotted station, 1 to 64 characters. |
| `comment` | string | The finished comment for the line, 0 to 30 characters of printable ASCII (U+0020 to U+007E), with no leading or trailing space: the mode word (10.2) when `mode` is not null, then the fields the destination picked (reference, source, spotter's comment), in its order, separated by single spaces, cut to 30 characters. The client puts it in the line as given. |
| `time_utc` | string | `HHMM`: the hour and minute of `spot_time`, UTC, seconds dropped. |
| `mode` | string or null | The mode word (10.2), the first word of `comment`; null when the spot has no mode. |
| `reference` | string or null | The spot's reference with its program first, for the client's own display: `POTA K-0039`, `SOTA W4C/CM-001`, `WWFF KFF-1234`, `WWBOTA B/G-1042` (several bunkers joined with `, `), `IOTA NA-107`. Null when the spot has none. |
| `source` | string | The source id: `pota`, `sotawatch`, `wwff`, `wwbota`, `dxcluster`, `rbn` or `logged`. The client MUST accept others. |
| `source_label` | string | The source's display name: `POTA`, `SOTAwatch`, `WWFF`, `WWBOTA`, `DX Cluster`, `RBN`, `Logged contacts`. |
| `connection` | string | The name of the connection that matched the spot, as the user named it in the web app. |
| `spot_time` | string | When the spot was made, `YYYY-MM-DDTHH:MM:SSZ`. |

- The same `id` can arrive more than once: two Local DX cluster destinations that target this
  relay can both match a spot (each with its own `comment`), and a spot whose write was cut off
  when a stream broke can be sent on the next one. The client SHOULD serve a given `id` at most
  once in 10 minutes and drop later copies.
- Spots arrive about when they match, not in `spot_time` order. The client serves them in
  arrival order.
- The server sends at most 120 spot events per relay per minute, across every destination that
  targets the relay; it drops the excess, oldest first. (Server policy; it may change.)
- A spot of the user's own logged contacts never comes back to that user's relays.

### 9.5 Reconnecting

| What happened | Client action |
| --- | --- |
| The stream ended, a network error, 404, 5xx, or a 200 that is not `text/event-stream` | Reconnect after the backoff below. |
| No bytes (event or comment) for 60 seconds | Close the stream; reconnect after the backoff. |
| 429 | Reconnect after `Retry-After` (60 s when unreadable) or the backoff, whichever is longer. |
| 401 `bad_token` | Delete the token, go to the signed-out screen; do not reconnect. |
| 403 `insufficient_scope` | Do not reconnect; ask the user to sign in again (3.1). |

- Backoff: 5 seconds after the first failure, doubling with each consecutive failure, capped at
  5 minutes (5, 10, 20, 40, 80, 160, 300, 300 ... seconds). The count goes back to zero once a
  stream has delivered `hello` and stayed open for 60 seconds.
- The stream has no total request timeout. The client MAY time out connecting (30 seconds is
  reasonable) and treats that as a failure.
- The local DX cluster (section 11) keeps running, and its clients stay connected, while the
  stream is down or reconnecting.

Example:

```http
GET /v2/spots HTTP/1.1
Host: spots.longlines.io
User-Agent: LongLinesRelay/0.2.0 (windows)
Authorization: Bearer ll_relay_exampleexampleexampleexampleexampleexampleexampleabc
Accept: text/event-stream
```

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-store

event: hello
data: {"relay_id":"3f6c2a9e-8b1d-4c7a-9e2f-5a0b1c2d3e4f","device_name":"SHACK-PC"}

: keepalive

event: spot
data: {"id":48213377,"spotter":"W4DOG","frequency_khz":14062.0,"callsign":"K4ABC","comment":"CW POTA K-0039","time_utc":"2114","mode":"CW","reference":"POTA K-0039","source":"pota","source_label":"POTA","connection":"POTA states","spot_time":"2026-10-28T21:14:07Z"}

: keepalive

```

The client serves that spot as the first line of the examples in 10.3.

## 10. Spot line

The client serves each spot as one line in the layout DXSpider uses, so logging software that
reads a DX cluster reads it unchanged. The client owns the served line. The server renders the
same layout only for the web app's preview; both sides MUST produce the example lines in 10.3
byte for byte.

### 10.1 Layout

A line is ASCII. Before layout, every character of `spotter` and `callsign` outside U+0021 to
U+007E, and every character of `comment` outside U+0020 to U+007E, becomes `?`. A `spotter`
that is empty or missing becomes `LONGLINES`. `comment` is cut to 30 characters (the server has
done both already; the client does them again).

Columns are 1-based byte positions:

| Columns | Width | Content |
| --- | --- | --- |
| 1 to 6 | 6 | `DX de ` (with the trailing space). |
| 7 to 24 | 18 | `spotter` followed by `:`, starting at column 7; the frequency right-aligned to end at column 24; spaces between them, at least one. |
| 25 to 26 | 2 | Two spaces. |
| 27 to 38 | 12 | `callsign`, left-aligned, padded with spaces. |
| 39 | 1 | One space. |
| 40 to 69 | 30 | `comment`, left-aligned, padded with spaces. |
| 70 | 1 | One space. |
| 71 to 75 | 5 | `time_utc` followed by `Z`, as `HHMMZ`. |
| 76 to 77 | 2 | CR LF. |

- The frequency is `frequency_khz` with exactly one decimal place: `14062` is `14062.0`,
  `7032.5` is `7032.5`.
- When `spotter` + `:` is at most 9 characters and the frequency at most 9 (not both 9), columns
  7 to 24 are exactly `spotter` + `:` left-justified to 9 followed by the frequency
  right-justified to 9. That covers every line of the capture below.
- A field is never cut except `comment`. When `spotter` + `:`, one space and the frequency need
  more than 18 columns, they take what they need with one space between them; a callsign longer
  than 12 characters takes what it needs. Every column after an overflow moves right by it, and
  the separators stay. The line is then longer than 75 characters, still ends with `HHMMZ` and
  CR LF, and still splits on spaces into the same parts.
- Every line ends with CR LF (bytes 13, 10). Nothing follows `HHMMZ` but CR LF: no BEL, no
  locator.

```text
         1         2         3         4         5         6         7
123456789012345678901234567890123456789012345678901234567890123456789012345
DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z
```

### 10.2 Mode word and comment

The server builds `comment` as: the mode word, when the spot has a mode; then, in the order the
destination lists them, each picked field that has a value for this spot: reference (the
`reference` value, program first, such as `POTA K-0039`), source (`source_label`) and spotter's
comment (the spot's own comment, trimmed, each run of whitespace made one space); joined with
single spaces, made ASCII as in 10.1, cut to 30 characters, and trimmed of a trailing space.

The mode word is always the first word of `comment`, in the words logging software knows, from
the spot's stored mode (lower case in Long Lines):

| Stored mode | Mode word |
| --- | --- |
| `cw` | `CW` |
| `usb`, `lsb` | `USB`, `LSB` |
| `ssb` | `LSB` below 10000 kHz, except 5000 to 5500 kHz (60 m, upper sideband); `USB` otherwise |
| `ft8`, `ft4`, `rtty` | `FT8`, `FT4`, `RTTY` |
| `psk`, `psk31` | `PSK31` |
| any other | The stored mode upper-cased, such as `FM`, `JS8`, `DATA` |
| none | No mode word: `mode` is null and `comment` starts with the first picked field. |

Whether a logger needs the mode word is not assumed: N1MM Logger+ is to be checked on Windows.

### 10.3 Examples

From Long Lines spots:

| Spot | Line |
| --- | --- |
| W4DOG spots K4ABC on 14062.0 at 21:14, mode `cw`, POTA K-0039; fields: reference | `DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z` |
| KK4PWJ spots W1AW on 7185.5, mode `ssb`, SOTA W4C/CM-001 from SOTAwatch; fields: reference, source | `DX de KK4PWJ:     7185.5  W1AW         LSB SOTA W4C/CM-001 SOTAwatch  2110Z` |
| The skimmer W3LPL-# hears G4ABC on 3530.0, mode `cw`; fields: source | `DX de W3LPL-#:    3530.0  G4ABC        CW RBN                         2120Z` |
| N4AB spots K4ABC, comment `Shining Rock Wilderness`; fields: reference, spotter's comment (cut to 30) | `DX de N4AB:      14062.0  K4ABC        CW POTA K-0039 Shining Rock Wi 2114Z` |
| A spot with no spotter (`LONGLINES`) | `DX de LONGLINES:  7032.5  W9XYZ        CW POTA K-4571                 2111Z` |
| The client's test spot (11.4) for account KK4PWJ | `DX de KK4PWJ:    14050.0  LLTEST       CW Long Lines Relay test spot  2102Z` |
| A 13-character callsign (overflow) | `DX de KK4PWJ:    14285.0  VP2E/KK4PWJ/M USB                            2120Z` |
| An 8-character spotter on 1296200.0 kHz (overflow) | `DX de UR2680YL: 1296200.0  G4ABC        CW                             2120Z` |

From a real DXSpider node: these lines are copied from
`supabase/functions/_shared/__tests__/fixtures/dxcluster-telnet.txt` in `sonyccd/longlines`
(G1FEF-2, DXSpider 1.57, captured 2026-10-12). Every one of the capture's 23 `DX de` lines fits
the layout and comes out byte for byte when parsed into its parts and formatted again, a
DXSpider-cut comment ending in a space included (the third line).

```text
DX de HA7UL:      7146.0  HA7UL/P      CROTA HU-BCS-003               1316Z
DX de F5PMW:     14238.0  KC1TLT                                      1316Z
DX de M8NAQ:      7180.0  M7VDP        B/G-1042 ROC Group HQ No 12 -  1316Z
DX de YB1LVL:    28074.0  F4HKP        FT8 -19dB from JN18 1524Hz     1316Z
DX de UR2680YL:  14095.6  EA6AJ        WSPR sign -26                  1317Z
DX de EA1FIC:    14305.0  SV8/DJ0MY    SSB [POTA] GR-0131 cq cq       1318Z
DX de EA2DSI:     7140.0  AM5GC        Dia de la Hispanidad y Diploma 1319Z
DX de DK2EA:     50313.0  EA5QE        JO50UF<>IM98PB 1004Hz CQ -8    1319Z
```

The captured lines differ from the served ones only after the time: each carries two BEL
characters (U+0007, the node's beep) after `HHMMZ`, and the capture file keeps LF line ends.
DXSpider can also append the spotter's locator after the time for users who ask for it; this
capture has none. Long Lines never sends a BEL or a locator. A parser written to this layout
SHOULD ignore anything after `HHMMZ`.

## 11. Local DX cluster (client side)

The client re-serves the stream to logging software on the user's computer (or network) as a
read-only DX cluster: a plain TCP line service in the style of a DXSpider node, which N1MM
Logger+ and other loggers reach through their Telnet window. Nothing here reaches the server,
and nothing here tunes a radio. The client MUST NOT turn anything a cluster client sends into a
contact, a spot or a request.

### 11.1 Listening

- The local cluster is off by default and listens only while the user's DX cluster switch is
  on. Nothing listens while it is off.
- Bind address: `127.0.0.1` only (loopback) while "Let other computers on this network connect"
  is off; all IPv4 interfaces (`0.0.0.0`) while it is on. Changing the setting rebinds.
- Port: 7373 by default. If the default port is in use, the client tries 7374, 7375 and so on
  up to 7399, listens on the first that is free, and shows the port actually bound; the saved
  setting stays 7373, so the next start tries 7373 first. A port the user chose ("Change port")
  is used as is, with no fall-through. When no port can be bound the client says the port is in
  use and listens nowhere.
- The cluster runs whether or not the spots stream is connected (9.5); its clients stay
  connected while the stream reconnects. It has no password: while the network setting is on,
  anyone on the network can connect and read the spots.

### 11.2 Session

- The client sends no telnet option negotiation. It strips telnet commands (byte 255 and the
  bytes of its command) and NUL from what it reads, and accepts lines ending in CR LF, LF or CR.
  It MAY drop a line longer than 512 bytes and MAY close a connection that sends one.
- Every line the client sends ends with CR LF.
- Login: on connect the client sends `login: ` with no line end, then reads one line. Its first
  word, upper-cased, is the cluster user's callsign; any non-empty word is accepted. On an empty
  line the client sends `login: ` again.
- Then it sends the greeting and a prompt, where `K4ABC` is that callsign, `SHACK-PC` the
  relay's device name and the date and time are UTC now (`DD-Mon-YYYY` with an English month
  abbreviation):

  ```text
  Hello K4ABC, this is Long Lines Relay on SHACK-PC. Spots from your Long Lines connections appear here.
  K4ABC de LONGLINES 28-Oct-2026 2114Z >
  ```

- After login, each spot from the stream goes to every logged-in connection as one line
  (section 10), in the order the stream delivered them. A connection that has not finished
  logging in gets no spot lines. There is no history: a connection sees only spots that arrive
  after it logged in. One slow connection MUST NOT hold up the others; the client MAY drop lines
  for, or close, a connection whose output backs up.

### 11.3 Commands

After login the client reads lines and acts on the first word, compared without case:

| Line | Client action |
| --- | --- |
| Empty or only spaces (N1MM sends these to keep the connection alive) | Nothing; no reply. |
| `bye` or `quit` | Close the connection. |
| First word `DX` (a spot the logger tries to send) | Reply `Spots can't be sent through Long Lines Relay.` and a prompt. The spot goes nowhere. |
| Anything else (`set/page 0`, `sh/dx`, `set/ve7cc 1`, ...) | Ignored: no reply but a prompt. |

### 11.4 Test spot

The client's "Send test spot" serves one line to every logged-in connection without the server:
spotter the account callsign from `GET /me`, frequency 14050.0, callsign `LLTEST`, comment
`CW Long Lines Relay test spot`, time now (UTC). See the example in 10.3. It never reaches the
server and is never posted as a contact.

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

## Appendix B. US state codes

The values `state` (6.1) may take: the postal codes of the 50 states and the District of
Columbia, upper case. Territories (`PR`, `VI`, `GU`, `AS`, `MP`) and other countries'
subdivisions are not in the list.

`AK` `AL` `AR` `AZ` `CA` `CO` `CT` `DC` `DE` `FL` `GA` `HI` `IA` `ID` `IL` `IN` `KS` `KY` `LA`
`MA` `MD` `ME` `MI` `MN` `MO` `MS` `MT` `NC` `ND` `NE` `NH` `NJ` `NM` `NV` `NY` `OH` `OK` `OR`
`PA` `RI` `SC` `SD` `TN` `TX` `UT` `VA` `VT` `WA` `WI` `WV` `WY`
