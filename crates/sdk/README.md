# tulpar-sdk

Minimal Rust client for the Tulpar HTTP API.

```rust
use tulpar_sdk::TulparClient;

let client = TulparClient::new("https://tulpar.example.com")?;
let login = client.login("demo@example.com", "password").await?;
let mut client = client.with_token(login.access_token);
let sync = client.sync().await?;
client.send_message(channel_id, "hello from Rust").await?;
```

The SDK exposes typed login and embed responses, plus JSON values for the
fast-moving sync/messaging surface. Embed widget tokens are public/audience-scoped;
never pass a normal user access token to a host page.

This is the initial v1 surface. Rich typed models, pagination helpers, gateway/WebSocket
support, and retry policies are follow-ups.
