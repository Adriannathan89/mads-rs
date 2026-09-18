import { Pool } from "pg";
import { describe, expect, it } from "vitest";

describe("PostgreSQL pool factory", () => {
  it("loads through the declared pg package and creates a pool", async () => {
    const { createPostgresPool } = await import("@/infrastructure/persistence/db");
    const pool = createPostgresPool("postgres://mads:password@localhost:5432/mads_docs");

    expect(pool).toBeInstanceOf(Pool);

    await pool.end();
  });
});
