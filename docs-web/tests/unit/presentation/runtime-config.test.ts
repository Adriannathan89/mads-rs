import { describe, expect, it } from "vitest";
import { getRuntimeConfig } from "@/presentation/server/runtime-config";

describe("getRuntimeConfig", () => {
  it("uses safe local defaults only in development", () => {
    expect(getRuntimeConfig({ NODE_ENV: "development" })).toMatchObject({
      allowedHosts: ["localhost"],
      proofDifficulty: 12,
    });
  });

  it("rejects a production environment without every visitor secret", () => {
    expect(() => getRuntimeConfig({ NODE_ENV: "production" })).toThrow(
      "DATABASE_URL is required in production",
    );
  });
});
