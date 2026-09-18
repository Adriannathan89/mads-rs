export interface BrowserEvidence {
  readonly host: string | null;
  readonly origin: string | null;
  readonly fetchSite: string | null;
  readonly fetchMode: string | null;
  readonly userAgent: string | null;
  readonly webdriver: boolean;
  readonly clientAddress: string | null;
}

export interface VisitChallenge {
  readonly id: string;
  readonly token: string;
  readonly issuedAt: Date;
  readonly expiresAt: Date;
  readonly difficulty: number;
}

export interface VisitSolution {
  readonly challenge: string;
  readonly nonce: number;
}

export interface VisitorCookie {
  readonly id: string;
}

export type VisitEvaluation = "accepted" | "rejected";
