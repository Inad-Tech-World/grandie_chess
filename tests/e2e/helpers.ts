import { expect, type Page } from "@playwright/test";

/** The board's box once it has stopped moving: a panel appearing next to or
 *  below the board can still be resizing it, and a drag computed from the old
 *  box drops on the wrong square. */
async function settledBoardBox(page: Page) {
  const board = page.locator("cg-board").first();
  let prev = await board.boundingBox();
  for (let i = 0; i < 20; i++) {
    await page.waitForTimeout(50);
    const box = await board.boundingBox();
    if (prev && box && box.x === prev.x && box.y === prev.y && box.width === prev.width) return box;
    prev = box;
  }
  return prev!;
}

export async function dragMove(page: Page, from: string, to: string) {
  const box = await settledBoardBox(page);
  const sq = box.width / 8;
  const flipped = await page.locator(".cg-wrap.orientation-black").count();
  const center = (s: string) => {
    let file = s.charCodeAt(0) - 97;
    let rank = Number(s[1]) - 1;
    if (flipped) {
      file = 7 - file;
      rank = 7 - rank;
    }
    return { x: box.x + (file + 0.5) * sq, y: box.y + (7 - rank + 0.5) * sq };
  };
  const a = center(from);
  const b = center(to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
  await page.mouse.up();
}

export async function freshApp(page: Page) {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.goto("/");
}

export async function startGameAsWhite(page: Page) {
  await page.getByRole("button", { name: "New game" }).first().click();
  await page.locator("label").filter({ hasText: /^White$/ }).click();
  await page.locator("main").getByRole("button", { name: "Play", exact: true }).click();
  await expect(page.getByText("Your move")).toBeVisible();
}
