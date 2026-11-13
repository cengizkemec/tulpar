# @tulpar/embed-sdk

Typed wrapper for the Tulpar embed loader.

```ts
import { mount } from "@tulpar/embed-sdk";

const handle = mount(document.querySelector("#community")!, {
  key: "emb_...",
  instance: "https://tulpar.example.com",
  widget: "community",
  height: 240,
});

// Later
handle.destroy();
```

The host page must load the instance's loader first:

```html
<script src="https://tulpar.example.com/embed/v1/tulpar-embed.js"></script>
```

The iframe is served by the Tulpar instance origin. The host page never receives the
embed token or the community session. Allowed origins are configured on the embed
application.
