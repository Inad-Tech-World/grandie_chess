import { expect, type Page } from "@playwright/test";

export async function dragMove(page: Page, from: string, to: string) {
  const board = page.locator("cg-board").first();
  const box = (await board.boundingBox())!;
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
