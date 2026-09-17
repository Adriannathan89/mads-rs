import { drizzle } from "drizzle-orm/node-postgres";
import { Pool } from "pg";

export function createPostgresPool(databaseUrl: string): Pool {
  return new Pool({ connectionString: databaseUrl });
}

export function createDatabase(pool: Pool) {
  return drizzle({ client: pool });
}
