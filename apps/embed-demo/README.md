# Tulpar Embed demo

A static host page for the Tulpar embed SDK. It runs on a different origin from the
Tulpar instance so the embed origin allowlist is exercised.

## Run

Serve this directory on port 8090:

```bash
npx serve -l 8090 .
```

Then open `http://localhost:8090`.

## Create an embed application

Use a community owner/admin account and call:

```http
POST /api/v1/communities/{community_id}/embed-apps
Authorization: Bearer <access token>
Content-Type: application/json

{
  "name": "Demo embed",
  "allowed_origins": ["http://localhost:8090"],
  "widgets": ["community", "join"]
}
```

Paste the returned `public_key` into the demo page and mount the widget.

The widget iframe is served from the Tulpar instance origin. The host page never
receives the embed token or the community session.
