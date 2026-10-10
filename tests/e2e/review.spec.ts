import { expect, test } from "@playwright/test";
import { dragMove, freshApp, startGameAsWhite } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

test("a finished game opens a review that flags the blunder with the better move", async ({ page }) => {
  await startGameAsWhite(page);
  for (const [from, to] of [["d2", "d4"], ["e2", "e4"], ["g1", "f3"]]) {
    await dragMove(page, from, to);
    await expect(page.getByText("Your move")).toBeVisible();
  }
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();

  // The review tab opens by itself and runs.
  await expect(page.getByRole("tab", { name: /Review:/ })).toBeVisible();
  await expect(page.getByText("Key moments")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByLabel("white accuracy")).toBeVisible();
  const moment = page.getByRole("button", { name: /Key moment 2… a4/ });
  await expect(moment).toContainText("was better");

  await moment.click();
  const feedback = page.getByLabel("Move feedback");
  await expect(feedback).toContainText("blunder");
  await expect(feedback).toContainText("best was");
  await expect(page.getByLabel("Evaluation graph")).toBeVisible();

  await feedback.getByRole("button", { name: "Retry this position" }).click();
  await expect(page.getByText("Find a better move")).toBeVisible();
  // The mock engine's best move for Black is its alphabetically last move (h7-h6).
  await dragMove(page, "a5", "a4");
  await expect(page.getByText(/still drops/)).toBeVisible();
  await page.getByRole("button", { name: "Try again" }).click();
  await dragMove(page, "h7", "h6");
  await expect(page.getByText("Well found!")).toBeVisible();
});

test("live engine lines and variation editing on the analysis board", async ({ page }) => {
  await page.getByRole("button", { name: "Analysis board" }).click();
  const tree = page.getByLabel("Move tree");
  await dragMove(page, "e2", "e4");
  await expect(tree).toContainText("e4");
  await dragMove(page, "e7", "e5");
  await expect(tree).toContainText("e5");
  await page.keyboard.press("ArrowLeft");
  await dragMove(page, "c7", "c5");
  await expect(tree).toContainText("e5");
  await expect(tree).toContainText("c5");

  await page.keyboard.press("Space");
  await expect(page.getByRole("button", { name: /^Line 1/ })).toBeVisible();
  await expect(page.getByRole("meter", { name: "Evaluation" })).toBeVisible();

  // Right-click the variation and promote it.
  await tree.getByText("c5").click({ button: "right" });
  await page.getByRole("button", { name: "Promote to main line" }).click();
  await expect(tree.locator(".move").nth(1)).toContainText("c5");
  await page.keyboard.press("Space");
});
