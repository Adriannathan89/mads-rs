import { createHmac, randomUUID, timingSafeEqual } from "node:crypto";

import type { ChallengeCodec } from "@/application/ports/challenge-codec";
import type { VisitChallenge, VisitorCookie } from "@/domain/visitor";

interface ChallengePayload {
  readonly id: string;
  readonly issuedAt: number;
  readonly expiresAt: number;
  readonly difficulty: number;
}

export interface HmacChallengeCodecOptions {
  readonly challengeSecret: string;
  readonly cookieSecret: string;
  readonly difficulty: number;
}

function sign(value: string, secret: string): string {
  return createHmac("sha256", secret).update(value).digest("base64url");
}

function hasValidSignature(value: string, signature: string, secret: string): boolean {
  const expected = Buffer.from(sign(value, secret));
  const received = Buffer.from(signature);
  return expected.length === received.length && timingSafeEqual(expected, received);
}

function encode(payload: ChallengePayload): string {
  return Buffer.from(JSON.stringify(payload)).toString("base64url");
}

function decode(value: string): ChallengePayload | null {
  try {
    const candidate: unknown = JSON.parse(Buffer.from(value, "base64url").toString("utf8"));
    if (!candidate || typeof candidate !== "object") {
      return null;
    }
    const payload = candidate as Record<string, unknown>;
    if (
      typeof payload.id !== "string" ||
      typeof payload.issuedAt !== "number" ||
      !Number.isInteger(payload.issuedAt) ||
      typeof payload.expiresAt !== "number" ||
      !Number.isInteger(payload.expiresAt) ||
      typeof payload.difficulty !== "number" ||
      !Number.isInteger(payload.difficulty)
    ) {
      return null;
    }
    return {
      id: payload.id,
      issuedAt: payload.issuedAt,
      expiresAt: payload.expiresAt,
      difficulty: payload.difficulty,
    };
  } catch {
    return null;
  }
}

export class HmacChallengeCodec implements ChallengeCodec {
  public constructor(private readonly options: HmacChallengeCodecOptions) {
    if (!Number.isInteger(options.difficulty) || options.difficulty < 12 || options.difficulty > 24) {
      throw new Error("Challenge difficulty must be an integer between 12 and 24");
    }
  }

  public issue(now: Date): VisitChallenge {
    const issuedAt = now.getTime();
    const payload: ChallengePayload = {
      id: randomUUID(),
      issuedAt,
      expiresAt: issuedAt + 5 * 60 * 1000,
      difficulty: this.options.difficulty,
    };
    const encoded = encode(payload);
    return this.asChallenge(`${encoded}.${sign(encoded, this.options.challengeSecret)}`, payload);
  }

  public verify(token: string, now: Date): VisitChallenge | null {
    const parts = token.split(".");
    if (parts.length !== 2 || !hasValidSignature(parts[0], parts[1], this.options.challengeSecret)) {
      return null;
    }
    const payload = decode(parts[0]);
    if (
      !payload ||
      payload.expiresAt <= now.getTime() ||
      payload.expiresAt - payload.issuedAt !== 5 * 60 * 1000 ||
      payload.difficulty !== this.options.difficulty
    ) {
      return null;
    }
    return this.asChallenge(token, payload);
  }

  public hashChallenge(token: string): string {
    return sign(token, this.options.challengeSecret);
  }

  public issueVisitorCookie(): string {
    const id = randomUUID();
    return `${id}.${sign(id, this.options.cookieSecret)}`;
  }

  public verifyVisitorCookie(value: string): VisitorCookie | null {
    const parts = value.split(".");
    if (parts.length !== 2 || !parts[0] || !hasValidSignature(parts[0], parts[1], this.options.cookieSecret)) {
      return null;
    }
    return { id: parts[0] };
  }

  public hashVisitorId(id: string): string {
    return sign(id, this.options.cookieSecret);
  }

  private asChallenge(token: string, payload: ChallengePayload): VisitChallenge {
    return {
      id: payload.id,
      token,
      issuedAt: new Date(payload.issuedAt),
      expiresAt: new Date(payload.expiresAt),
      difficulty: payload.difficulty,
    };
  }
}
