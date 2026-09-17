import { NextResponse } from "next/server";

import { getVisitorContainer } from "@/presentation/server/container";
import { requestEvidence } from "@/presentation/server/request-evidence";
import { readCookie, visitorCookieName } from "@/presentation/server/visitor-cookie";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const noStore = { "Cache-Control": "no-store" };

interface RegisterVisitBody {
  readonly challenge: string;
  readonly nonce: number;
  readonly webdriver: boolean;
}

function isRegisterVisitBody(value: unknown): value is RegisterVisitBody {
  if (!value || typeof value !== "object") {
    return false;
  }
  const body = value as Record<string, unknown>;
  return (
    typeof body.challenge === "string" &&
    body.challenge.length > 0 &&
    typeof body.nonce === "number" &&
    Number.isSafeInteger(body.nonce) &&
    body.nonce >= 0 &&
    typeof body.webdriver === "boolean"
  );
}

function rejected(): NextResponse {
  return NextResponse.json({ error: "visit not qualified" }, { status: 403, headers: noStore });
}

export async function POST(request: Request): Promise<NextResponse> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return rejected();
  }
  if (!isRegisterVisitBody(body)) {
    return rejected();
  }

  const result = await getVisitorContainer().registerQualifiedVisitor.execute({
    evidence: requestEvidence(request, body.webdriver),
    challenge: body.challenge,
    nonce: body.nonce,
    existingVisitorCookie: readCookie(request, visitorCookieName),
  });
  if (result.kind === "rejected") {
    return rejected();
  }

  const response = NextResponse.json({ total: result.total }, { headers: noStore });
  if (result.kind === "counted") {
    response.cookies.set(visitorCookieName, result.visitorCookie, {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      path: "/",
      maxAge: 60 * 60 * 24 * 365,
    });
  }
  return response;
}
