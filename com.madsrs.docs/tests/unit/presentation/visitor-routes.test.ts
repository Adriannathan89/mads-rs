import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({ container: undefined as never }));

vi.mock("@/presentation/server/container", () => ({
  getVisitorContainer: () => state.container,
}));

import { GET as getChallenge } from "@/app/api/visitor-challenge/route";
import { POST } from "@/app/api/visitor/route";

const browserHeaders = {
  host: "docs.madsrs.com",
  origin: "https://docs.madsrs.com",
  "sec-fetch-site": "same-origin",
  "sec-fetch-mode": "cors",
  "user-agent": "Mozilla/5.0 Chrome/140.0 Safari/537.36",
  "x-forwarded-for": "203.0.113.25",
};

describe("visitor routes", () => {
  beforeEach(() => {
    state.container = {
      issueVisitorChallenge: {
        execute: () => ({ token: "challenge", difficulty: 12, expiresAt: new Date("2026-09-17T00:05:00Z") }),
      },
      registerQualifiedVisitor: {
        execute: async (input: { evidence: { webdriver: boolean } }) =>
          input.evidence.webdriver ? { kind: "rejected" } : { kind: "counted", total: 1, visitorCookie: "signed-cookie" },
      },
      getVisitorCount: { execute: async () => 1 },
      visitorRepository: { isReady: async () => true },
    } as never;
  });

  it("does not cache a challenge response", async () => {
    const response = await getChallenge(new Request("https://docs.madsrs.com/api/visitor-challenge", { headers: browserHeaders }));

    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe("no-store");
  });

  it("returns a generic 403 for an unqualified request", async () => {
    const response = await POST(
      new Request("https://docs.madsrs.com/api/visitor", {
        method: "POST",
        headers: { ...browserHeaders, "content-type": "application/json" },
        body: JSON.stringify({ challenge: "challenge", nonce: 1, webdriver: true }),
      }),
    );

    expect(response.status).toBe(403);
    await expect(response.json()).resolves.toEqual({ error: "visit not qualified" });
  });
});
