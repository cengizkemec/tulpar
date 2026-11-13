import { performance } from "node:perf_hooks";

const apiBase = process.env.TULPAR_API_URL ?? "http://127.0.0.1:18080";
const users = Number(process.env.TULPAR_LOAD_USERS ?? 10);
const messagesPerUser = Number(process.env.TULPAR_LOAD_MESSAGES ?? 5);

async function api(path, { method = "GET", token, body } = {}) {
  const started = performance.now();
  const response = await fetch(`${apiBase}${path}`, {
    method,
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body ? { "content-type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const elapsed = performance.now() - started;
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`${method} ${path} -> ${response.status}: ${text}`);
  }
  return { data: text ? JSON.parse(text) : null, elapsed };
}

const stamp = Date.now().toString(36);
const latencies = [];
const owner = `load${stamp}`;
const email = `${owner}@example.com`;
const password = "correct horse battery staple";

await api("/api/v1/auth/register", {
  method: "POST",
  body: {
    username: owner,
    display_name: "Load Owner",
    email,
    password,
  },
});
const login = await api("/api/v1/auth/login", {
  method: "POST",
  body: { login: email, password },
});
const ownerToken = login.data.access_token;

const community = await api("/api/v1/communities", {
  method: "POST",
  token: ownerToken,
  body: {
    name: `Load ${stamp}`,
    slug: `load-${stamp}`,
    visibility: "public",
  },
});
const communityId = community.data.community.id;
const sync = await api("/api/v1/sync", { token: ownerToken });
const selected = sync.data.communities.find(
  (entry) => entry.community.id === communityId,
);
const channelId = selected.channels.find((entry) => entry.kind === "text").id;

for (let userIndex = 0; userIndex < users; userIndex += 1) {
  const username = `load${stamp}${userIndex}`;
  const userEmail = `${username}@example.com`;
  await api("/api/v1/auth/register", {
    method: "POST",
    body: {
      username,
      display_name: `Load ${userIndex}`,
      email: userEmail,
      password,
    },
  });
  const userLogin = await api("/api/v1/auth/login", {
    method: "POST",
    body: { login: userEmail, password },
  });
  const token = userLogin.data.access_token;
  await api(`/api/v1/communities/${communityId}/join`, {
    method: "POST",
    token,
    body: {},
  });
  for (let messageIndex = 0; messageIndex < messagesPerUser; messageIndex += 1) {
    const sent = await api(`/api/v1/channels/${channelId}/messages`, {
      method: "POST",
      token,
      body: { content: `load message ${userIndex}/${messageIndex}` },
    });
    latencies.push(sent.elapsed);
  }
}

const read = await api(`/api/v1/channels/${channelId}/messages?limit=50`, {
  token: ownerToken,
});
latencies.push(read.elapsed);

latencies.sort((left, right) => left - right);
const percentile = (value) =>
  latencies[Math.min(latencies.length - 1, Math.floor(latencies.length * value))];
console.log(
  JSON.stringify(
    {
      users,
      messagesPerUser,
      requests: latencies.length,
      p50Ms: Math.round(percentile(0.5)),
      p95Ms: Math.round(percentile(0.95)),
      maxMs: Math.round(latencies[latencies.length - 1]),
      communityId,
      channelId,
    },
    null,
    2,
  ),
);
