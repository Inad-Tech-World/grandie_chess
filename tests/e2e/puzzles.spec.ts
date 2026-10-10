import { expect, test } from "@playwright/test";
import { dragMove, freshApp } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

test("downloads puzzles, solves one and the rating goes up", async ({ page }) => {
  await page.getByRole("button", { name: "Puzzles" }).click();
  await page.getByRole("button", { name: "Recommended" }).click();
  await expect(page.getByLabel("Puzzle rating")).toHaveText("1500");
  // The opponent's first move (Qd6?! from White) is played automatically; we solve as Black.
  await expect(page.getByText("Find the best move for black")).toBeVisible();
  await expect(page.getByLabel("Moves found")).toHaveText("0 / 2 moves");
  await dragMove(page, "f8", "d8");
  // Wait for the opponent's reply (Qxd8+) before the second move.
  await expect(page.getByLabel("Moves found")).toHaveText("1 / 2 moves");
  await expect(page.getByText("Find the best move for black")).toBeVisible();
  await dragMove(page, "f6", "d8");
  await expect(page.getByText("Solved!")).toBeVisible();
  await expect(page.getByText("+12")).toBeVisible();
  await expect(page.getByLabel("Puzzle rating")).toHaveText("1512");
  await expect(page.getByRole("button", { name: "Next puzzle" })).toBeVisible();
});

test("progress dashboard shows strength, levels and weak openings", async ({ page }) => {
  await page.getByRole("button", { name: "Progress" }).click();
  await expect(page.getByText("Estimated strength", { exact: true })).toBeVisible();
  await expect(page.getByText("1633").first()).toBeVisible();
  await expect(page.getByRole("cell", { name: /Maia 1500/ })).toBeVisible();
  await expect(page.getByRole("cell", { name: /Sicilian Defense/ })).toBeVisible();
  await expect(page.getByText("Mistakes and blunders per game")).toBeVisible();
});
