import { spawn, spawnSync } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright-core";

const here = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = join(here, "..", "..");
const exePath =
  process.env.TULPAR_DESKTOP_EXE ??
  join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "tulpar-desktop.exe");
const webUrl = process.env.TULPAR_WEB_URL ?? "http://localhost:15173";
const edgePath =
  process.env.EDGE_PATH ??
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const debugPort = Number(process.env.TULPAR_DESKTOP_DEBUG_PORT ?? 9223);

if (process.platform !== "win32") {
  throw new Error("The desktop smoke test currently targets Windows WebView2.");
}
if (!existsSync(exePath)) {
  throw new Error(
    `Desktop executable not found: ${exePath}. Run the Tauri debug build first.`,
  );
}

const localAppData = process.env.LOCALAPPDATA ?? "";
if (localAppData) {
  rmSync(join(localAppData, "com.tulpar.desktop"), {
    recursive: true,
    force: true,
  });
}

const app = spawn(exePath, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  },
  stdio: "ignore",
});

async function waitForCdp() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${debugPort}/json/version`);
      if (response.ok) {
        return;
      }
    } catch {
      // Retry until WebView2 starts.
    }
    await delay(300);
  }
  throw new Error("WebView2 remote debugging endpoint did not start.");
}

let browser;
let edge;
try {
  await waitForCdp();
  browser = await chromium.connectOverCDP(
    `http://127.0.0.1:${debugPort}`,
  );
  const desktop = browser.contexts()[0].pages()[0];
  await desktop.waitForLoadState("domcontentloaded");
  await desktop.waitForSelector(
    '[data-testid="desktop-server-url"], [data-testid="desktop-start-signin"]',
    { timeout: 20_000 },
  );

  await desktop.evaluate(async (instance) => {
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (invoke) {
      try {
        await invoke("desktop_logout", { instance });
      } catch {
        // No stored session to clear.
      }
    }
  }, webUrl);

  if (await desktop.getByTestId("desktop-server-url").count()) {
    await desktop.getByTestId("desktop-server-url").fill(webUrl);
    await desktop.getByTestId("desktop-check-server").click();
    await desktop.getByTestId("desktop-connect").waitFor({ timeout: 15_000 });
    await desktop.getByTestId("desktop-connect").click();
  }

  await desktop.getByTestId("desktop-start-signin").waitFor({ timeout: 15_000 });
  await desktop.getByTestId("desktop-start-signin").click();
  const code = (
    await desktop.getByTestId("desktop-user-code").innerText({ timeout: 15_000 })
  ).trim();

  edge = await chromium.launch({
    executablePath: edgePath,
    headless: true,
    args: ["--no-sandbox"],
  });
  const context = await edge.newContext({ locale: "en-US" });
  const page = await context.newPage();
  await page.goto(`${webUrl}/login`);
  await page.getByTestId("login-email").fill("demo@tulpar.local");
  await page.getByTestId("login-password").fill("tulpar-demo-2026");
  await page.getByTestId("login-submit").click();
  await page.getByText("# general").first().waitFor({ timeout: 20_000 });
  await page.goto(`${webUrl}/device`);
  await page.getByTestId("device-code").fill(code);
  await page.getByTestId("device-approve").click();
  await page.getByText("Device approved").waitFor({ timeout: 15_000 });

  await desktop.waitForURL("**/app**", { timeout: 30_000 });
  await desktop.getByTestId("message-input").waitFor({ timeout: 20_000 });

  const result = {
    deviceApproved: true,
    desktopOrigin: await desktop.evaluate(() => window.location.origin),
    keychainSession: await desktop.evaluate(async (instance) => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      return invoke
        ? await invoke("desktop_keychain_has", { instance })
        : false;
    }, webUrl),
    capabilities: await desktop.evaluate(async () =>
      window.__TAURI_INTERNALS__?.invoke("desktop_capabilities"),
    ),
    pttRegistered: await desktop.evaluate(async () =>
      window.__TAURI_INTERNALS__?.invoke("desktop_register_ptt", {
        shortcut: "F8",
      }),
    ),
  };
  await desktop.evaluate(async () =>
    window.__TAURI_INTERNALS__?.invoke("desktop_register_ptt", {
      shortcut: "",
    }),
  );

  await desktop.reload({ waitUntil: "domcontentloaded" });
  await desktop.getByTestId("message-input").waitFor({ timeout: 20_000 });
  result.refreshAfterReload = true;

  const deepLink = spawn(
    exePath,
    ["tulpar://add-instance?url=https%3A%2F%2Fexample.com"],
    { env: process.env, stdio: "ignore" },
  );
  await deepLinkExit(deepLink);
  await desktop.waitForURL("**/connect?url=**", { timeout: 15_000 });
  result.deepLink = await desktop
    .getByTestId("desktop-server-url")
    .inputValue();

  await desktop.evaluate(async (instance) => {
    await window.__TAURI_INTERNALS__?.invoke("desktop_logout", { instance });
  }, webUrl);

  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser?.close();
  await edge?.close();
  if (app.pid) {
    spawnSync("taskkill", ["/PID", String(app.pid), "/T", "/F"], {
      stdio: "ignore",
    });
  }
}

async function deepLinkExit(child) {
  await new Promise((resolve) => {
    const timer = setTimeout(resolve, 5_000);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
}
