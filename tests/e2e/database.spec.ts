import { expect, test } from "@playwright/test";
import { freshApp } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

test("imports games, filters them, searches a position and opens a game", async ({ page }) => {
  await page.getByRole("button", { name: "Database" }).click();
  await expect(page.getByText(/No games yet/)).toBeVisible();

  await page.getByRole("button", { name: "Import games" }).click();
  await page.getByLabel("Lichess username").fill("tester");
  await page.getByRole("dialog").getByRole("button", { name: "Import" }).click();
  await expect(page.getByText("Import finished")).toBeVisible();
  await expect(page.getByText("2 new games added")).toBeVisible();
  await page.keyboard.press("Escape");

  const games = page.getByRole("rowgroup", { name: "Games" });
  await expect(games.getByRole("row")).toHaveCount(2);
  await expect(page.getByText(/^2 games · (<1|\d+) ms$/)).toBeVisible();

  // Text filter.
  await page.getByLabel("Search games").fill("sicilian");
  await expect(games.getByRole("row")).toHaveCount(1);
  await page.getByLabel("Search games").fill("");
  await expect(games.getByRole("row")).toHaveCount(2);

  // Exact position search from the start position matches every game.
  await page.getByRole("button", { name: "Position…" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Search" }).click();
  await expect(page.getByRole("button", { name: "Clear position search" })).toBeVisible();
  await expect(games.getByRole("row")).toHaveCount(2);
  await page.getByRole("button", { name: "Clear position search" }).click();

  // Open a game in analysis.
  await games.getByRole("row").filter({ hasText: "Sicilian" }).dblclick();
  await expect(page.getByRole("tab", { name: /Rival – Tester/ })).toBeVisible();
  await page.locator("label").filter({ hasText: /^Moves$/ }).click();
  await expect(page.getByLabel("Move tree")).toContainText("cxd4");
});
