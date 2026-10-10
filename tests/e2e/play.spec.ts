import { expect, test, type Page } from "@playwright/test";

async function dragMoveLocal(page: Page, from: string, to: string) {
  const board = page.locator("cg-board").first();
  const box = (await board.boundingBox())!;
  const sq = box.width / 8;
  const center = (s: string) => {
    const file = s.charCodeAt(0) - 97;
    const rank = Number(s[1]) - 1;
    return { x: box.x + (file + 0.5) * sq, y: box.y + (7 - rank + 0.5) * sq };
  };
  const a = center(from);
  const b = center(to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
  await page.mouse.up();
}

async function chooseWhite(page: Page) {
  await page.locator("label").filter({ hasText: /^White$/ }).click();
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.goto("/");
});

test("plays a game against Maia and resigns", async ({ page }) => {
  await page.getByRole("button", { name: "New game" }).first().click();
  await expect(page.getByRole("heading", { name: "New game" })).toBeVisible();
  await chooseWhite(page);
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();

  await expect(page.getByText("Your move")).toBeVisible();
  await dragMoveLocal(page, "e2", "e4");
  // The mock engine replies with the alphabetically first legal move (a7a5).
  await expect(page.getByRole("listitem").first()).toContainText("e4");
  await expect(page.getByRole("listitem").first()).toContainText("a5");
  await expect(page.getByText("Your move")).toBeVisible();

  await page.getByRole("button", { name: "Hint" }).click();
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();
  await expect(page.getByText("You lost by resignation")).toBeVisible();
  await expect(page.getByRole("button", { name: "Review game" })).toBeVisible();
});

test("illegal drag snaps back", async ({ page }) => {
  await page.getByRole("button", { name: "New game" }).first().click();
  await chooseWhite(page);
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();
  await dragMoveLocal(page, "e2", "e5");
  await expect(page.getByRole("listitem")).toHaveCount(0);
});

test("analyses a finished game", async ({ page }) => {
  await page.getByRole("button", { name: "New game" }).first().click();
  await chooseWhite(page);
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await dragMoveLocal(page, "d2", "d4");
  await expect(page.getByRole("listitem").first()).toContainText("a5");
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();
  await page.getByRole("button", { name: "Review game" }).click();
  await page.locator("label").filter({ hasText: /^Moves$/ }).click();
  await expect(page.getByLabel("Move tree")).toContainText("d4");
  // The review tab opens at the start position; add a different first move.
  await dragMoveLocal(page, "e2", "e4");
  await expect(page.getByLabel("Move tree")).toContainText("e4");
});
