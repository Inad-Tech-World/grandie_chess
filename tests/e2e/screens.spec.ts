import { expect, test } from "@playwright/test";

// Captures key screens for visual review: `pnpm e2e -g @screens`.
// Images land in test-results/screens/.
const shot = (name: string) => ({ path: `test-results/screens/${name}.png` });

test("@screens main screens", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.goto("/");
  await page.screenshot(shot("01-welcome"));

  await page.getByRole("button", { name: "New game" }).first().click();
  await page.screenshot(shot("02-new-game"));

  await page.locator("label").filter({ hasText: /^White$/ }).click();
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();
  for (const [from, to] of [["e2", "e4"], ["g1", "f3"], ["f1", "c4"]]) {
    const board = (await page.locator("cg-board").boundingBox())!;
    const sq = board.width / 8;
    const c = (s: string) => ({ x: board.x + (s.charCodeAt(0) - 97 + 0.5) * sq, y: board.y + (8 - Number(s[1]) + 0.5) * sq });
    await page.mouse.move(c(from).x, c(from).y);
    await page.mouse.down();
    await page.mouse.move(c(to).x, c(to).y, { steps: 5 });
    await page.mouse.up();
    await expect(page.getByText("Your move")).toBeVisible();
  }
  await page.getByRole("button", { name: "Hint" }).click();
  await page.waitForTimeout(300);
  await page.screenshot(shot("03-play"));

  await page.getByRole("button", { name: "Engines" }).click();
  await page.screenshot(shot("04-engines"));
  await page.getByRole("button", { name: "Settings" }).click();
  await page.screenshot(shot("05-settings"));
});

test.use({ colorScheme: "dark" });

test("@screens light mode play", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.locator("label").filter({ hasText: /^Light$/ }).click();
  await page.getByRole("button", { name: "New game" }).first().click();
  await page.locator("label").filter({ hasText: /^Black$/ }).click();
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();
  await page.screenshot(shot("06-play-light"));
});
