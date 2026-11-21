import { expect, test, type Page } from "@playwright/test";
import { dragMove, freshApp, startGameAsWhite } from "./helpers";

// Screens unmount whenever another tab or section is shown. These check that
// one-time actions (opening the review, running it) don't repeat on return.

test.beforeEach(async ({ page }) => freshApp(page));

const reviewStarts = (page: Page) => page.evaluate(() => (window as unknown as { __GRANDIE_MOCK_STATS__: { reviewStarts: number } }).__GRANDIE_MOCK_STATS__.reviewStarts);

async function playAndResign(page: Page) {
  await startGameAsWhite(page);
  for (const [from, to] of [["d2", "d4"], ["e2", "e4"], ["g1", "f3"]]) {
    await dragMove(page, from, to);
    await expect(page.getByText("Your move")).toBeVisible();
  }
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();
}

test("returning to a finished game doesn't open its review again", async ({ page }) => {
  await playAndResign(page);
  const reviewTabs = page.getByRole("tab", { name: /Review:/ });
  await expect(reviewTabs).toHaveCount(1);
  await expect(page.getByText("Key moments")).toBeVisible({ timeout: 10_000 });

  // Back to the game tab, long enough for the automatic review to fire again.
  await page.getByRole("tab", { name: /^vs / }).click();
  await expect(page.getByRole("button", { name: "Review game" })).toBeVisible();
  await page.waitForTimeout(1500);
  await expect(reviewTabs).toHaveCount(1);
  expect(await reviewStarts(page)).toBe(1);
});

test("switching away from a running review doesn't start a second one", async ({ page }) => {
  // Slow the mock review down so it is still running while we switch tabs.
  await page.waitForFunction(() => "__GRANDIE_MOCK_STATS__" in window);
  await page.evaluate(() => {
    (window as unknown as { __GRANDIE_MOCK_STATS__: { reviewPlyMs: number } }).__GRANDIE_MOCK_STATS__.reviewPlyMs = 250;
  });
  await playAndResign(page);
  await expect(page.getByRole("tab", { name: /Review:/ })).toBeVisible();
  await expect(page.getByText(/checking every move/)).toBeVisible();

  // Away to another section and to the game tab, then back.
  await page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: /Settings/ }).click();
  await page.getByRole("tab", { name: /^vs / }).click();
  await page.getByRole("tab", { name: /Review:/ }).click();

  await expect(page.getByText("Key moments")).toBeVisible({ timeout: 15_000 });
  expect(await reviewStarts(page)).toBe(1);
  await expect(page.getByText("Review failed")).toHaveCount(0);
});
