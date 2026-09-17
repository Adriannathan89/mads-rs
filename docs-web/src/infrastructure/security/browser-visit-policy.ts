import type { BrowserEvidence, VisitEvaluation } from "@/domain/visitor";

const automatedUserAgent = /bot|crawler|spider|slurp|curl|wget|facebookexternalhit|discordbot|slackbot|headless/i;

function hostname(host: string): string {
  return host.startsWith("[") ? host.slice(1, host.indexOf("]")) : host.split(":")[0];
}

export class BrowserVisitPolicy {
  private readonly allowedHosts: ReadonlySet<string>;

  public constructor(allowedHosts: readonly string[]) {
    this.allowedHosts = new Set(allowedHosts.map((host) => host.toLowerCase()));
  }

  public evaluate(evidence: BrowserEvidence): VisitEvaluation {
    if (
      !evidence.host ||
      !evidence.origin ||
      !evidence.fetchSite ||
      !evidence.fetchMode ||
      !evidence.userAgent ||
      !evidence.clientAddress ||
      evidence.webdriver ||
      automatedUserAgent.test(evidence.userAgent)
    ) {
      return "rejected";
    }

    const requestHost = hostname(evidence.host).toLowerCase();
    if (!this.allowedHosts.has(requestHost) || evidence.fetchSite !== "same-origin" || evidence.fetchMode !== "cors") {
      return "rejected";
    }

    try {
      const origin = new URL(evidence.origin);
      const expectedProtocol = requestHost === "localhost" ? "http:" : "https:";
      if (origin.hostname.toLowerCase() !== requestHost || origin.protocol !== expectedProtocol) {
        return "rejected";
      }
    } catch {
      return "rejected";
    }

    return "accepted";
  }
}
