import { expect, test, type Page } from "@playwright/test";
import { dragMove } from "./helpers";

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
  await dragMove(page, "e2", "e4");
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
  await dragMove(page, "e2", "e5");
  await expect(page.getByRole("listitem")).toHaveCount(0);
});

test("analyses a finished game", async ({ page }) => {
  await page.getByRole("button", { name: "New game" }).first().click();
  await chooseWhite(page);
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await dragMove(page, "d2", "d4");
  await expect(page.getByRole("listitem").first()).toContainText("a5");
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();
  await page.getByRole("button", { name: "Review game" }).click();
  await page.locator("label").filter({ hasText: /^Moves$/ }).click();
  await expect(page.getByLabel("Move tree")).toContainText("d4");
  // The finished review adds the evaluation graph under the board, which
  // shrinks it; wait for that before dragging.
  await expect(page.getByLabel("Evaluation graph")).toBeVisible();
  // The review tab opens at the start position; add a different first move.
  await dragMove(page, "e2", "e4");
  await expect(page.getByLabel("Move tree")).toContainText("e4");
});
