import { describe, expect, it } from "vitest";

import type { BrowserEvidence } from "@/domain/visitor";
import { BrowserVisitPolicy } from "@/infrastructure/security/browser-visit-policy";

const browserEvidence = (overrides: Partial<BrowserEvidence> = {}): BrowserEvidence => ({
  host: "docs.madsrs.com",
  origin: "https://docs.madsrs.com",
  fetchSite: "same-origin",
  fetchMode: "cors",
  userAgent: "Mozilla/5.0 Chrome/140.0 Safari/537.36",
  webdriver: false,
  clientAddress: "203.0.113.25",
  ...overrides,
});

describe("BrowserVisitPolicy", () => {
  const policy = new BrowserVisitPolicy(["docs.madsrs.com", "localhost"]);

  it("rejects Googlebot, missing origin, and automation evidence", () => {
    expect(policy.evaluate(browserEvidence({ userAgent: "Googlebot/2.1" }))).toBe("rejected");
    expect(policy.evaluate(browserEvidence({ origin: null }))).toBe("rejected");
    expect(policy.evaluate(browserEvidence({ webdriver: true }))).toBe("rejected");
  });

  it("accepts a same-origin browser fetch", () => {
    expect(policy.evaluate(browserEvidence())).toBe("accepted");
  });

  it("fails closed for an unconfigured host or missing request evidence", () => {
    expect(policy.evaluate(browserEvidence({ host: "example.com" }))).toBe("rejected");
    expect(policy.evaluate(browserEvidence({ fetchMode: null }))).toBe("rejected");
  });
});
