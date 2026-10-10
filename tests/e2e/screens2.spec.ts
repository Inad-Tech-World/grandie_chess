import { expect, test } from "@playwright/test";
import { dragMove, freshApp } from "./helpers";

// Visual review of phase 3–6 screens: `pnpm e2e -g @screens`.
const shot = (name: string) => ({ path: `test-results/screens/${name}.png` });

test("@screens database, openings, puzzles, progress, settings", async ({ page }) => {
  await freshApp(page);
  await page.getByRole("button", { name: "Database" }).click();
  await page.getByRole("button", { name: "Import games" }).click();
  await page.getByLabel("Lichess username").fill("tester");
  await page.getByRole("dialog").getByRole("button", { name: "Import" }).click();
  await expect(page.getByText("Import finished")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Import games" }).click();
  await page.getByRole("tab", { name: "Chess.com" }).click();
  await page.getByLabel("Chess.com username").fill("tester");
  await page.getByRole("dialog").getByRole("button", { name: "Import" }).click();
  await expect(page.getByText("Import finished")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.screenshot(shot("07-database"));
  await page.getByRole("button", { name: "Position…" }).click();
  await page.waitForTimeout(300);
  await page.screenshot(shot("08-position-search"));
  await page.keyboard.press("Escape");

  await page.getByRole("button", { name: "Openings" }).click();
  await page.locator("label").filter({ hasText: /^Build$/ }).click();
  await dragMove(page, "e2", "e4");
  await expect(page.getByLabel("Current line")).toContainText("e4");
  await page.getByRole("button", { name: "Add this line" }).click();
  await page.waitForTimeout(300);
  await page.screenshot(shot("09-openings"));

  await page.getByRole("button", { name: "Puzzles" }).click();
  await page.screenshot(shot("10-puzzles-setup"));
  await page.getByRole("button", { name: "Recommended" }).click();
  await expect(page.getByText("Find the best move for black")).toBeVisible();
  await page.screenshot(shot("11-puzzle"));

  await page.getByRole("button", { name: "Progress" }).click();
  await page.waitForTimeout(300);
  await page.screenshot(shot("12-progress"));

  await page.getByRole("button", { name: "Settings" }).click();
  await page.locator("button[aria-label='merida']").click();
  await page.locator("button[aria-label='Blue']").click();
  await page.screenshot(shot("13-settings"), );
  await page.getByRole("button", { name: "New game" }).first().click();
  await page.locator("label").filter({ hasText: /^White$/ }).click();
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();
  await page.screenshot(shot("14-play-merida-blue"));
});
