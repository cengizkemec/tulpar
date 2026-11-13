import http from "k6/http";
import { check, sleep } from "k6";

const base = __ENV.TULPAR_API_URL || "http://127.0.0.1:18080";

export const options = {
  vus: Number(__ENV.TULPAR_VUS || 10),
  duration: __ENV.TULPAR_DURATION || "30s",
  thresholds: {
    http_req_failed: ["rate<0.01"],
    http_req_duration: ["p(95)<500"],
  },
};

export function setup() {
  const stamp = `${Date.now()}${Math.floor(Math.random() * 10000)}`;
  const username = `k6${stamp}`.slice(0, 30);
  const email = `${username}@example.com`;
  const password = "correct horse battery staple";
  const register = http.post(
    `${base}/api/v1/auth/register`,
    JSON.stringify({
      username,
      display_name: "k6 user",
      email,
      password,
    }),
    { headers: { "content-type": "application/json" } },
  );
  if (register.status !== 202 && register.status !== 200) {
    throw new Error(`register failed: ${register.status} ${register.body}`);
  }
  const login = http.post(
    `${base}/api/v1/auth/login`,
    JSON.stringify({ login: email, password }),
    { headers: { "content-type": "application/json" } },
  );
  if (login.status !== 200) {
    throw new Error(`login failed: ${login.status} ${login.body}`);
  }
  const token = login.json().access_token;
  const community = http.post(
    `${base}/api/v1/communities`,
    JSON.stringify({
      name: `k6 ${stamp}`,
      slug: `k6-${stamp}`,
      visibility: "public",
    }),
    {
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${token}`,
      },
    },
  );
  if (community.status !== 200) {
    throw new Error(`community failed: ${community.status} ${community.body}`);
  }
  const communityId = community.json().community.id;
  const sync = http.get(`${base}/api/v1/sync`, {
    headers: { authorization: `Bearer ${token}` },
  });
  const selected = sync
    .json()
    .communities.find((entry) => entry.community.id === communityId);
  const channel = selected.channels.find((entry) => entry.kind === "text");
  return { token, channelId: channel.id };
}

export default function (data) {
  const response = http.post(
    `${base}/api/v1/channels/${data.channelId}/messages`,
    JSON.stringify({
      content: `k6 ${__VU}/${__ITER} ${Date.now()}`,
    }),
    {
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${data.token}`,
      },
    },
  );
  check(response, { "message status is 200": (value) => value.status === 200 });
  sleep(1);
}
