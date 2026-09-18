import { describe, expect, it } from "vitest";

import { HmacChallengeCodec } from "@/infrastructure/security/hmac-challenge-codec";

describe("HmacChallengeCodec", () => {
  const codec = new HmacChallengeCodec({
    challengeSecret: "challenge-secret-for-tests",
    cookieSecret: "cookie-secret-for-tests",
    difficulty: 12,
  });
  const now = new Date("2026-09-17T00:00:00Z");

  it("issues and verifies a five-minute signed challenge", () => {
    const challenge = codec.issue(now);

    expect(challenge.expiresAt.getTime() - challenge.issuedAt.getTime()).toBe(5 * 60 * 1000);
    expect(codec.verify(challenge.token, new Date("2026-09-17T00:04:59Z"))).toMatchObject({
      id: challenge.id,
      difficulty: 12,
    });
    expect(codec.verify(challenge.token, new Date("2026-09-17T00:05:01Z"))).toBeNull();
    expect(codec.verify(`${challenge.token}tampered`, now)).toBeNull();
  });

  it("issues opaque visitor cookies and hashes server-side keys", () => {
    const cookie = codec.issueVisitorCookie();
    const verified = codec.verifyVisitorCookie(cookie);

    expect(verified).toMatchObject({ id: expect.any(String) });
    expect(codec.hashChallenge("challenge-token")).not.toBe("challenge-token");
    expect(codec.hashVisitorId(verified?.id ?? "")).not.toBe(verified?.id);
    expect(codec.verifyVisitorCookie(`${cookie}tampered`)).toBeNull();
  });
});
