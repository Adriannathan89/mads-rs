import type { BrowserEvidence } from "@/domain/visitor";

export function requestEvidence(request: Request, webdriver: boolean): BrowserEvidence {
  const forwardedFor = request.headers.get("x-forwarded-for");
  let referrerOrigin: string | null = null;
  try {
    referrerOrigin = new URL(request.headers.get("referer") ?? "").origin;
  } catch {
    referrerOrigin = null;
  }
  return {
    host: request.headers.get("host"),
    origin: request.headers.get("origin") ?? referrerOrigin,
    fetchSite: request.headers.get("sec-fetch-site"),
    fetchMode: request.headers.get("sec-fetch-mode"),
    userAgent: request.headers.get("user-agent"),
    webdriver,
    clientAddress: forwardedFor?.split(",")[0].trim() || request.headers.get("x-real-ip"),
  };
}
