import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { chromium } from "playwright-core";

const apiBase = process.env.TULPAR_API_URL ?? "http://127.0.0.1:18080";
const webUrl = process.env.TULPAR_WEB_URL ?? "http://localhost:5173";
const edgePath =
  process.env.EDGE_PATH ??
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const here = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = join(here, "..", "..");
const viteBin = join(repoRoot, "apps", "web", "node_modules", "vite", "bin", "vite.js");

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
  if (!response.ok) {
    throw new Error(`${method} ${path} -> ${response.status}: ${text}`);
  }
  return text ? JSON.parse(text) : null;
}

async function session() {
  const username = `webe2e${Date.now().toString(36)}`.slice(0, 30);
  const email = `${username}@example.com`;
  const password = "correct horse battery staple";
  await api("/api/v1/auth/register", {
    method: "POST",
    body: { username, display_name: "Web E2E", email, password },
  });
  const login = await api("/api/v1/auth/login", {
    method: "POST",
    body: { login: email, password },
  });
  return { email, password, token: login.access_token };
}

async function setup() {
  const owner = await session();
  const stamp = Date.now();
  let communityId;
  try {
    const created = await api("/api/v1/communities", {
      method: "POST",
      token: owner.token,
      body: { name: "Web E2E", slug: `web-e2e-${stamp}`, visibility: "private" },
    });
    communityId = created.community.id;
  } catch {
    const communities = await api("/api/v1/users/me/communities", {
      token: owner.token,
    });
    const existing = communities.find((entry) => entry.name === "Web E2E");
    if (!existing) {
      throw new Error("no existing Web E2E community");
    }
    communityId = existing.id;
  }
  const voice = await api(`/api/v1/communities/${communityId}/channels`, {
    method: "POST",
    token: owner.token,
    body: { kind: "voice", name: `web-voice-${stamp}`, user_limit: 4 },
  });
  const text = `web smoke ${stamp}`;
  const sync = await api("/api/v1/sync", { token: owner.token });
  const selected = sync.communities.find(
    (entry) => entry.community.id === communityId,
  );
  const textChannel = selected.channels.find((entry) => entry.kind === "text");
  if (!textChannel) {
    throw new Error("text channel missing");
  }
  await api(`/api/v1/channels/${textChannel.id}/messages`, {
    method: "POST",
    token: owner.token,
    body: { content: text },
  });
  return {
    credentials: owner,
    textChannelId: textChannel.id,
    voiceChannelName: voice.name,
    text,
  };
}

async function waitForServer(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      // Retry until Vite is ready.
    }
    await delay(300);
  }
  throw new Error(`server did not start: ${url}`);
}

const setupData = await setup();
const externalWeb = Boolean(process.env.TULPAR_WEB_URL);
const vite = externalWeb
  ? null
  : spawn(process.execPath, [viteBin, "--port", "5173", "--strictPort"], {
      cwd: join(repoRoot, "apps", "web"),
      stdio: "ignore",
    });

let browser;
try {
  await waitForServer(webUrl);
  browser = await chromium.launch({
    executablePath: edgePath,
    headless: true,
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
      "--autoplay-policy=no-user-gesture-required",
      "--auto-select-desktop-capture-source=Entire screen",
      "--enable-usermedia-screen-capturing",
      "--no-sandbox",
    ],
  });
  const context = await browser.newContext();
  await context.grantPermissions(["microphone", "camera"], { origin: webUrl });
  const page = await context.newPage();
  page.on("pageerror", (error) => console.error("PAGE_ERROR", error.message));
  page.on("console", (message) => {
    const text = message.text();
    if (
      message.type() === "error" &&
      !text.includes("Failed to load resource") &&
      !text.includes("401")
    ) {
      console.error("BROWSER", message.type(), text);
    }
  });

  await page.goto(`${webUrl}/login`);
  await page.getByTestId("login-email").fill(setupData.credentials.email);
  await page.getByTestId("login-password").fill(setupData.credentials.password);
  await page.getByTestId("login-submit").click();
  await page.getByText("# general").first().waitFor({ timeout: 20_000 });
  await page.getByText(setupData.text).first().waitFor({ timeout: 20_000 });

  await page.getByRole("button", { name: "Friends" }).click();
  await page.getByText("Find people").waitFor({ timeout: 10_000 });
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByText("Profile").first().waitFor({ timeout: 10_000 });
  await page.goto(`${webUrl}/app`);
  await page.getByText("# general").first().waitFor({ timeout: 20_000 });

  const text = `browser message ${Date.now()}`;
  await page.getByTestId("message-input").fill(text);
  await page.getByTestId("message-send").click();
  await page.getByText(text).first().waitFor({ timeout: 20_000 });

  const gatewayText = `gateway message ${Date.now()}`;
  await api(`/api/v1/channels/${setupData.textChannelId}/messages`, {
    method: "POST",
    token: setupData.credentials.token,
    body: { content: gatewayText },
  });
  await page.getByText(gatewayText).first().waitFor({ timeout: 20_000 });

  await page.getByText(new RegExp(setupData.voiceChannelName)).first().click();
  await page.getByTestId("voice-join").click();
  await page
    .getByTestId("voice-status")
    .filter({ hasText: "Connected" })
    .waitFor({ timeout: 30_000 });
  await page.getByTestId("voice-camera").click();
  await page
    .locator('[data-testid="voice-camera"][data-active="true"]')
    .waitFor({ timeout: 15_000 });
  await page.getByTestId("voice-screen").click();
  await page
    .locator('[data-testid="voice-screen"][data-active="true"]')
    .waitFor({ timeout: 15_000 });
  await page.getByText("Leave voice").click();
  await page.getByTestId("voice-join").waitFor({ timeout: 15_000 });
  await page
    .locator('[data-testid="voice-screen"][data-active="false"]')
    .waitFor({ timeout: 5_000 });

  console.log(
    JSON.stringify({
      login: true,
      textMessage: true,
      gatewayMessage: true,
      voiceConnected: true,
      cameraPublishing: true,
      screenPublishing: true,
      channel: setupData.textChannelId,
    }),
  );
  await browser.close();
  browser = null;
  cleanup();
} catch (error) {
  console.error("WEB_E2E_FAILED", error);
  if (browser) {
    const pages = browser.contexts().flatMap((context) => context.pages());
    for (const [index, current] of pages.entries()) {
      console.error(
        `PAGE_${index}`,
        current.url(),
        await current.locator("body").innerText().catch(() => ""),
      );
    }
    await browser.close();
  }
  cleanup();
  process.exit(1);
}

function cleanup() {
  vite?.kill();
  process.exit(0);
}
