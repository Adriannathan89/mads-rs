import { expect, test } from "@playwright/test";

import { ComposeHarness } from "./support/compose-harness";

const compose = new ComposeHarness(process.cwd());

test.setTimeout(120_000);

async function healthIsOk(path: string): Promise<boolean> {
  try {
    return (await fetch(`http://127.0.0.1:3000${path}`)).ok;
  } catch {
    return false;
  }
}

test.afterEach(async () => {
  await compose.down();
});

test("the Compose stack migrates before serving liveness and readiness", async () => {
  await compose.up(["postgres", "migrate", "docs-web"]);
  await expect.poll(() => healthIsOk("/api/health/live")).toBe(true);
  await expect.poll(() => healthIsOk("/api/health/ready")).toBe(true);
});

test("PostgreSQL has no published host port", async () => {
  await compose.up(["postgres"]);
  await expect(compose.publishedPorts("postgres")).resolves.toEqual([]);
});
