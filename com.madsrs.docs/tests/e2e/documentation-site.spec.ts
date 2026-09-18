import { expect, test } from "@playwright/test";

test("a new reader can reach the quick start, search validation, and use mobile navigation", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("link", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Quick start" })).toBeVisible();

  await page.keyboard.press("Control+k");
  await page.getByRole("searchbox").fill("validation");
  await page.getByRole("option", { name: /request validation/i }).click();
  await expect(page).toHaveURL(/build-an-api\/validation/);

  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Menu" }).click();
  await page.getByRole("link", { name: "Quick start" }).click();
  await expect(page).toHaveURL(/introduction\/quick-start/);
});
