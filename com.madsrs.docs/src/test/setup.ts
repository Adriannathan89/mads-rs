import "@testing-library/jest-dom/vitest";

import { afterEach } from "vitest";

afterEach(() => {
  process.env = { ...process.env };
});
