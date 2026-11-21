import { expect, test, type Page } from "@playwright/test";
import { dragMove, freshApp, startGameAsWhite } from "./helpers";

test.beforeEach(async ({ page }) => freshApp(page));

const board = (page: Page) => page.locator(".board-box").first();

async function reviewedGame(page: Page) {
  await startGameAsWhite(page);
  for (const [from, to] of [["d2", "d4"], ["e2", "e4"], ["g1", "f3"]]) {
    await dragMove(page, from, to);
    await expect(page.getByText("Your move")).toBeVisible();
  }
  await page.getByRole("button", { name: "Resign" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Resign" }).click();
  await expect(page.getByText("Key moments")).toBeVisible({ timeout: 10_000 });
}

test("the coach explains a reviewed move on the board, then gives it back", async ({ page }) => {
  await reviewedGame(page);
  await page.getByRole("button", { name: /Key moment 2… a4/ }).click();
  const feedback = page.getByLabel("Move feedback");
  await feedback.getByRole("button", { name: "Explain on the board" }).click();

  // The coach takes over the board (ringed) and narrates.
  const coach = page.getByLabel("Coach", { exact: true });
  await expect(coach).toContainText(/is a mistake|is the best move/);
  await expect(page.getByLabel("Conversation with the coach")).toContainText("That's the idea.");
  // When the demonstration ends the board is the user's again.
  await expect(board(page)).not.toHaveAttribute("data-coaching", "true", { timeout: 10_000 });
});

test("coach me through the key moments walks the game", async ({ page }) => {
  await reviewedGame(page);
  await page.getByRole("button", { name: "Coach me through them" }).click();
  await expect(page.getByLabel("Conversation with the coach")).toContainText("key moments of this game");
});

test("the engine line's Why? button demonstrates the line", async ({ page }) => {
  await page.getByRole("button", { name: "Analysis board" }).click();
  await page.keyboard.press("Space");
  const why = page.getByRole("button", { name: "Why line 1?" });
  await expect(why).toBeVisible();
  await why.click();
  await expect(page.getByLabel("Conversation with the coach")).toContainText("The engine's idea here is");
  await page.keyboard.press("Space");
});

test("in a coached game a mistake is shown and can be taken back", async ({ page }) => {
  await page.getByRole("button", { name: "New game" }).first().click();
  await page.locator("label").filter({ hasText: /^White$/ }).click();
  await page.locator("label").filter({ hasText: /^After my mistakes$/ }).click();
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();

  // The mock counts moves to the a- and h-files as mistakes.
  await dragMove(page, "h2", "h4");
  const verdict = page.getByLabel("Coach verdict");
  await expect(verdict).toContainText("mistake");
  await expect(page.getByLabel("Conversation with the coach")).toContainText("is a mistake");
  await verdict.getByRole("button", { name: "Take back and try again" }).click();
  await expect(page.getByText("Your move")).toBeVisible();
  await expect(verdict).toHaveCount(0);
});

test("asking the coach streams an answer and shows a line", async ({ page }) => {
  // AI needs a saved key.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByLabel("OpenRouter API key").fill("sk-or-v1-0123456789abcdef0123");
  await page.getByRole("button", { name: "Save key" }).click();
  await expect(page.getByText("saved", { exact: false }).first()).toBeVisible();

  await startGameAsWhite(page);
  const ask = page.getByLabel("Ask the coach");
  await ask.fill("What is the plan here?");
  await ask.press("Enter");
  await expect(page.getByLabel("Conversation with the coach")).toContainText("The engine likes developing your pieces here.");
});

test("learn from your mistakes collects mistakes from reviews", async ({ page }) => {
  await reviewedGame(page);
  await page.getByRole("button", { name: "Puzzles" }).first().click();
  await page.locator("label").filter({ hasText: /^My mistakes/ }).click();
  await expect(page.getByText(/Learn from your mistakes|No mistakes to practise/)).toBeVisible();
});
