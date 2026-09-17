import { NextResponse } from "next/server";

import { getVisitorContainer } from "@/presentation/server/container";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

export async function GET(): Promise<NextResponse> {
  const ready = await getVisitorContainer().visitorRepository.isReady();
  return ready
    ? NextResponse.json({ status: "ok" })
    : NextResponse.json({ status: "unavailable" }, { status: 503 });
}
