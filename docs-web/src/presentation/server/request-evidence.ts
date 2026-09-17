import type { BrowserEvidence } from "@/domain/visitor";

export function requestEvidence(request: Request, webdriver: boolean): BrowserEvidence {
  const forwardedFor = request.headers.get("x-forwarded-for");
  return {
    host: request.headers.get("host"),
    origin: request.headers.get("origin"),
    fetchSite: request.headers.get("sec-fetch-site"),
    fetchMode: request.headers.get("sec-fetch-mode"),
    userAgent: request.headers.get("user-agent"),
    webdriver,
    clientAddress: forwardedFor?.split(",")[0].trim() || request.headers.get("x-real-ip"),
  };
}
