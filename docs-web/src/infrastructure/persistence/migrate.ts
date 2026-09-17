import { migrate } from "drizzle-orm/node-postgres/migrator";

import { getRuntimeConfig } from "@/presentation/server/runtime-config";

import { createDatabase, createPostgresPool } from "./db";

async function run(): Promise<void> {
  const config = getRuntimeConfig(process.env);
  const pool = createPostgresPool(config.databaseUrl);

  try {
    await migrate(createDatabase(pool), { migrationsFolder: "drizzle" });
  } finally {
    await pool.end();
  }
}

void run();
