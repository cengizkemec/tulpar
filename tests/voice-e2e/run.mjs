import { createServer } from "node:http";
import { copyFile, readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const apiBase = process.env.TULPAR_API_URL ?? "http://127.0.0.1:18080";
const relay = process.env.TULPAR_VOICE_RELAY === "1";
const edgePath =
  process.env.EDGE_PATH ??
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const here = fileURLToPath(new URL(".", import.meta.url));
const publicDir = join(here, "public");

const contentTypes = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
};

await copyFile(
  join(here, "node_modules/livekit-client/dist/livekit-client.umd.js"),
  join(publicDir, "livekit-client.umd.js"),
);
await copyFile(join(here, "client.js"), join(publicDir, "client.js"));

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, "http://127.0.0.1");
    const file = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
    const body = await readFile(join(publicDir, file));
    response.writeHead(200, {
      "content-type": contentTypes[extname(file)] ?? "application/octet-stream",
    });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end("not found");
  }
});

function listen() {
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

async function api(path, { method = "GET", token, body } = {}) {
  const response = await fetch(`${apiBase}${path}`, {
    method,
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body ? { "content-type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = null;
  }
  if (!response.ok) {
    const error = new Error(`${method} ${path} -> ${response.status}: ${text}`);
    error.status = response.status;
    throw error;
  }
  return json;
}

async function createAccount(label) {
  const username = label;
  const email = `${username}@example.com`;
  const password = "correct horse battery staple";
  let login;
  try {
    login = await api("/api/v1/auth/login", {
      method: "POST",
      body: { login: email, password },
    });
  } catch (error) {
    if (error.status !== 401 && error.status !== 404) {
      throw error;
    }
    await api("/api/v1/auth/register", {
      method: "POST",
      body: { username, display_name: username, email, password },
    });
    login = await api("/api/v1/auth/login", {
      method: "POST",
      body: { login: email, password },
    });
  }
  return { username, token: login.access_token };
}

async function setupRoom() {
  const owner = await createAccount("voicee2eowner");
  const member = await createAccount("voicee2emember");
  const stamp = Date.now();
  let communityId;
  try {
    const community = await api("/api/v1/communities", {
      method: "POST",
      token: owner.token,
      body: {
        name: "Voice E2E",
        slug: `voice-e2e-${stamp}`,
        visibility: "private",
      },
    });
    communityId = community.community.id;
  } catch {
    const communities = await api("/api/v1/users/me/communities", {
      token: owner.token,
    });
    const existing = communities.find((entry) => entry.name === "Voice E2E");
    if (!existing) {
      throw new Error("community creation failed and no existing Voice E2E community was found");
    }
    communityId = existing.id;
  }
  const invite = await api(`/api/v1/communities/${communityId}/invites`, {
    method: "POST",
    token: owner.token,
    body: { max_uses: 2 },
  });
  await api(`/api/v1/invites/${invite.code}`, {
    method: "POST",
    token: member.token,
  });
  const channel = await api(`/api/v1/communities/${communityId}/channels`, {
    method: "POST",
    token: owner.token,
    body: { kind: "voice", name: `E2E Voice ${stamp}`, user_limit: 2 },
  });
  const ownerVoice = await api(`/api/v1/channels/${channel.id}/voice/token`, {
    method: "POST",
    token: owner.token,
    body: {},
  });
  const memberVoice = await api(`/api/v1/channels/${channel.id}/voice/token`, {
    method: "POST",
    token: member.token,
    body: {},
  });
  return { channel, ownerVoice, memberVoice };
}

async function waitForVoice(page, label) {
  try {
    await page.waitForFunction(
      () =>
        window.__tulparVoice &&
        window.__tulparVoice.connected &&
        window.__tulparVoice.localAudio &&
        window.__tulparVoice.remoteAudio >= 1,
      null,
      { timeout: 30_000 },
    );
  } catch (error) {
    const state = await page
      .evaluate(() => window.__tulparVoice)
      .catch(() => null);
    console.error("VOICE_STATE", label, state);
    throw error;
  }
  const state = await page.evaluate(() => window.__tulparVoice);
  if (state.error) {
    throw new Error(`${label} reported ${state.error}`);
  }
  return state;
}

const port = await listen();
const origin = `http://127.0.0.1:${port}`;
let browser;
try {
  const room = await setupRoom();
  browser = await chromium.launch({
    executablePath: edgePath,
    headless: true,
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
      "--autoplay-policy=no-user-gesture-required",
      "--no-sandbox",
    ],
  });
  const context = await browser.newContext();
  await context.grantPermissions(["microphone"], { origin });

  const open = async (voice, label) => {
    const page = await context.newPage();
    page.on("console", (message) => {
      if (message.type() === "error" || message.type() === "warning") {
        console.error(`BROWSER_${label}`, message.text());
      }
    });
    page.on("pageerror", (error) => {
      console.error(`PAGE_ERROR_${label}`, error.message);
    });
    const params = new URLSearchParams({
      url: voice.url,
      token: voice.token,
    });
    if (relay) {
      params.set("relay", "1");
    }
    await page.goto(`${origin}/?${params.toString()}`);
    return { page, label };
  };

  const owner = await open(room.ownerVoice, "owner");
  const member = await open(room.memberVoice, "member");
  const [ownerState, memberState] = await Promise.all([
    waitForVoice(owner.page, "owner"),
    waitForVoice(member.page, "member"),
  ]);
  console.log(
    JSON.stringify({
      relay,
      room: room.ownerVoice.room,
      owner: {
        connected: ownerState.connected,
        localAudio: ownerState.localAudio,
        remoteAudio: ownerState.remoteAudio,
      },
      member: {
        connected: memberState.connected,
        localAudio: memberState.localAudio,
        remoteAudio: memberState.remoteAudio,
      },
    }),
  );
  await browser.close();
  server.close();
  process.exit(0);
} catch (error) {
  console.error("VOICE_E2E_FAILED", error);
  if (browser) {
    await browser.close();
  }
  server.close();
  process.exit(1);
}
