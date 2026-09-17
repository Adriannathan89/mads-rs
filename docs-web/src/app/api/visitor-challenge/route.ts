import { NextResponse } from "next/server";

import { getVisitorContainer } from "@/presentation/server/container";
import { requestEvidence } from "@/presentation/server/request-evidence";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const noStore = { "Cache-Control": "no-store" };

export async function GET(request: Request): Promise<NextResponse> {
  const challenge = getVisitorContainer().issueVisitorChallenge.execute(requestEvidence(request, false));
  if (!challenge) {
    return NextResponse.json({ error: "visit not qualified" }, { status: 403, headers: noStore });
  }
  return NextResponse.json(
    { challenge: challenge.token, difficulty: challenge.difficulty, expiresAt: challenge.expiresAt.toISOString() },
    { headers: noStore },
  );
}
