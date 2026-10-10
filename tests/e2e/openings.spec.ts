import { expect, test } from "@playwright/test";
import { dragMove, freshApp } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

test("build a repertoire line and drill it", async ({ page }) => {
  await page.getByRole("button", { name: "Openings" }).click();
  await page.locator("label").filter({ hasText: /^Build$/ }).click();

  const line = page.getByLabel("Current line");
  await dragMove(page, "e2", "e4");
  await expect(line).toContainText("e4");
  await expect(page.getByText("King's Pawn Game")).toBeVisible();
  await dragMove(page, "c7", "c5");
  await expect(line).toContainText("c5");
  await dragMove(page, "g1", "f3");
  await expect(line).toContainText("Nf3");
  await page.getByRole("button", { name: "Add this line" }).click();
  await expect(page.getByText("Added 3 moves to your white repertoire")).toBeVisible();

  // Back at the start the prepared move is listed.
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByLabel("Repertoire builder")).toContainText("e4");

  // Drill: two positions where White must answer (start, and after 1...c5).
  await page.locator("label").filter({ hasText: /^Train$/ }).click();
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(page.getByText(/what's your move\?/)).toBeVisible();
  await dragMove(page, "e2", "e4");
  await expect(page.getByText("Correct")).toBeVisible();
  // Second card: after 1. e4 c5 — play a wrong move first.
  await expect(page.getByText(/what's your move\?/)).toBeVisible({ timeout: 5000 });
  await dragMove(page, "d2", "d4");
  await expect(page.getByText("Your repertoire plays Nf3")).toBeVisible();
  await dragMove(page, "g1", "f3");
  await expect(page.getByText("Correct")).toBeVisible();
  // The missed card comes back once at the end of the session.
  await expect(page.getByText(/2 \/ 3|3 \/ 3/)).toBeVisible();
});
