# Outgoing webhooks

**Status:** Implemented for `message.created`. Webhook subscriptions are managed per
community by users with `MANAGE_COMMUNITY`.

## 1. Create a subscription

```http
POST /api/v1/communities/{community_id}/webhooks
Authorization: Bearer <access token>
Content-Type: application/json

{
  "url": "https://example.com/tulpar-webhook",
  "events": ["message.created"]
}
```

The response includes a `whsec_...` secret exactly once. Store it securely; Tulpar
cannot show it again.

## 2. Delivery format

Tulpar sends a POST with a JSON body:

```json
{
  "id": "event uuid",
  "type": "message.created",
  "created_at": "2026-11-13T12:00:00Z",
  "data": {
    "id": "message uuid",
    "channel_id": "channel uuid",
    "content": "hello",
    "created_at": "2026-11-13T12:00:00Z"
  }
}
```

Headers:

| Header | Value |
|---|---|
| `X-Tulpar-Event` | `message.created` |
| `X-Tulpar-Delivery` | Event UUID |
| `X-Tulpar-Timestamp` | Unix timestamp in seconds |
| `X-Tulpar-Signature` | `sha256=<hex HMAC>` |

The signature is HMAC-SHA256 over `"<timestamp>.<raw body>"` using the webhook secret.

## 3. Verification example

```ts
import { createHmac, timingSafeEqual } from "node:crypto";

function verify(secret: string, timestamp: string, body: string, header: string) {
  const expected = `sha256=${createHmac("sha256", secret)
    .update(`${timestamp}.${body}`)
    .digest("hex")}`;
  const left = Buffer.from(expected);
  const right = Buffer.from(header);
  return left.length === right.length && timingSafeEqual(left, right);
}
```

Reject requests whose timestamp is outside a small tolerance window (for example five
minutes) to limit replay.

## 4. Retries and SSRF protection

- Deliveries are queued durably and retried with exponential backoff (30 seconds up to
  one hour).
- Redirects are not followed.
- HTTPS is required except for localhost development.
- Loopback, private, link-local, unspecified, multicast, and broadcast addresses are
  rejected at subscription creation and again before each delivery.
- Failed deliveries record `last_error` on the webhook and delivery row.

## 5. Limits

- One event type is currently emitted: `message.created`.
- Webhook secrets are stored in the database and should be treated as credentials.
- A delivery dashboard and more event types are follow-ups.
