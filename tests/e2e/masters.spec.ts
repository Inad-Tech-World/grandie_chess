import { expect, test } from "@playwright/test";
import { dragMove, freshApp } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

test("download a master, see the profile, and guess their moves", async ({ page }) => {
  await page.getByRole("button", { name: "Masters", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Legends" })).toBeVisible();

  await page.getByRole("button", { name: "Mikhail Tal", exact: true }).click();
  await page.getByRole("button", { name: "Download Tal's games" }).click();
  await expect(page.getByText("Results as White")).toBeVisible();
  await expect(page.getByText("Favourite openings with White")).toBeVisible();
  await expect(page.getByText(/Downloaded · 3/)).toBeVisible();

  // The newest game first: Tal (White) vs Spassky, 1.d4.
  const games = page.getByRole("table", { name: "Mikhail Tal's games" });
  await expect(games.getByText("Tal, Mikhail – Spassky, Boris")).toBeVisible();
  await games.getByRole("button", { name: "Guess the move" }).first().click();

  const panel = page.getByLabel("Guess the move");
  await panel.locator("label").filter({ hasText: /^From move 1$/ }).click();
  await panel.getByRole("button", { name: "Start guessing" }).click();
  await expect(panel.getByText("What did Tal play here?")).toBeVisible();
  await dragMove(page, "d2", "d4");
  await expect(panel.getByText("+3")).toBeVisible();
  await expect(panel.getByLabel("Score")).toHaveText("Score: 3 / 3");
  // The opponent answers by itself and it's Tal's move again.
  await expect(panel.getByText("What did Tal play here?")).toBeVisible({ timeout: 5000 });
  await dragMove(page, "e2", "e4");
  await expect(panel.getByText("+0")).toBeVisible();
  await expect(panel.getByLabel("Score")).toHaveText("Score: 3 / 6");
  await panel.getByRole("button", { name: "Continue" }).click();
});

test("the coach replays a legend on the board", async ({ page }) => {
  await page.getByRole("button", { name: "Masters", exact: true }).click();
  const opera = page.getByLabel("The Opera Game");
  await expect(opera).toBeVisible();
  // Real pacing, so the demonstration lasts long enough to see.
  await page.evaluate(() => ((window as unknown as { __GRANDIE_COACH_FAST__: boolean }).__GRANDIE_COACH_FAST__ = false));
  await opera.getByRole("button", { name: "Guided replay" }).click();
  await expect(page.locator('.board-box[data-coaching="true"]')).toBeVisible();
  await expect(page.locator(".coach-caption")).toContainText("Morphy");
  await page.getByRole("button", { name: "Stop the coach" }).click();
  await expect(page.locator('.board-box[data-coaching="true"]')).toHaveCount(0);
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.getByRole("heading", { name: "Legends" })).toBeVisible();
});
