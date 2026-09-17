import { createHash } from "node:crypto";

import type { VisitChallenge } from "@/domain/visitor";

function hasLeadingZeroBits(digest: Uint8Array, difficulty: number): boolean {
  let remaining = difficulty;
  for (const byte of digest) {
    if (remaining <= 0) {
      return true;
    }
    if (byte === 0) {
      remaining -= 8;
      continue;
    }
    for (let mask = 0b1000_0000; mask > 0 && remaining > 0; mask >>= 1) {
      if ((byte & mask) !== 0) {
        return false;
      }
      remaining -= 1;
    }
    return remaining <= 0;
  }
  return remaining <= 0;
}

function digest(token: string, nonce: number): Uint8Array {
  return createHash("sha256").update(`${token}:${nonce}`).digest();
}

export async function verifyProof(challenge: VisitChallenge, nonce: number, now = new Date()): Promise<boolean> {
  if (
    challenge.expiresAt.getTime() <= now.getTime() ||
    !Number.isSafeInteger(nonce) ||
    nonce < 0 ||
    !Number.isInteger(challenge.difficulty) ||
    challenge.difficulty < 12 ||
    challenge.difficulty > 24
  ) {
    return false;
  }
  return hasLeadingZeroBits(digest(challenge.token, nonce), challenge.difficulty);
}

export async function solveProof(challenge: VisitChallenge, maximumNonce = 20_000_000): Promise<number> {
  for (let nonce = 0; nonce <= maximumNonce; nonce += 1) {
    if (hasLeadingZeroBits(digest(challenge.token, nonce), challenge.difficulty)) {
      return nonce;
    }
  }
  throw new Error("Unable to solve proof within the nonce limit");
}
