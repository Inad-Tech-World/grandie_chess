import { expect, test } from "@playwright/test";
import { freshApp } from "./helpers";

// The UI as it appears in WebView2 on Windows: Ctrl shortcuts and the
// DirectML and ONNX Runtime backends of the Windows lc0 build.
test.use({
  userAgent:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
});

test("Windows shows Ctrl shortcuts and its own lc0 backends", async ({ page }) => {
  await freshApp(page);
  await expect(page.locator("html")).toHaveAttribute("data-platform", "windows");

  await page.getByRole("button", { name: "New game" }).first().hover();
  await expect(page.getByRole("tooltip")).toHaveText("New game (Ctrl+N)");

  await page.getByRole("button", { name: "Engines" }).click();
  await expect(page.getByText("Windows (x64)")).toBeVisible();
  await page.getByRole("combobox", { name: "Backend" }).click();
  await expect(page.getByRole("option", { name: "DirectML, GPU 0" })).toBeVisible();
  await expect(page.getByRole("option", { name: "CPU (ONNX Runtime)" })).toBeVisible();
  await expect(page.getByRole("option", { name: /Metal|Accelerate/ })).toHaveCount(0);
});
