import { expect, test } from "@playwright/test";
import { freshApp } from "./helpers";

test.beforeEach(async ({ page }) => {
  await freshApp(page);
});

async function openSettings(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
}

test("an invalid OpenRouter key is rejected with a reason", async ({ page }) => {
  await openSettings(page);
  await page.getByLabel("OpenRouter API key").fill("abc123");
  await page.getByRole("button", { name: "Save key" }).click();
  await expect(page.getByText("That doesn't look like an OpenRouter key")).toBeVisible();
  await expect(page.getByText("OpenRouter key saved", { exact: true })).toHaveCount(0);
  await expect(page.getByLabel("Use AI for the coach's words")).toBeDisabled();
});

test("a valid key is saved and switches AI on", async ({ page }) => {
  await openSettings(page);
  const aiSwitch = page.getByLabel("Use AI for the coach's words");
  await expect(aiSwitch).toBeDisabled();
  await page.getByLabel("OpenRouter API key").fill("sk-or-v1-0123456789abcdef0123");
  await page.getByRole("button", { name: "Save key" }).click();
  await expect(page.getByText("OpenRouter key saved", { exact: true })).toBeVisible();
  await expect(aiSwitch).toBeEnabled();
  await expect(aiSwitch).toBeChecked();
  // Credits from the key, and the cloud voice becomes available.
  await expect(page.getByText(/Credits used: \$0\.420 of \$5\.00/)).toBeVisible();
  await expect(page.getByRole("radio", { name: "Cloud voice" })).toBeEnabled();
  // Turning AI off keeps the key.
  await aiSwitch.click({ force: true });
  await expect(aiSwitch).not.toBeChecked();
  await expect(page.getByText("OpenRouter key saved", { exact: true })).toBeVisible();
});

test("coach options persist after a reload", async ({ page }) => {
  await openSettings(page);
  await page.locator("label").filter({ hasText: /^Brief$/ }).click();
  await page.locator("label").filter({ hasText: /^Captions only$/ }).click();
  await expect(page.getByRole("radio", { name: "Brief" })).toBeChecked();
  // Let the save reach the (mock) backend before reloading.
  await page.waitForTimeout(300);
  await page.reload();
  await openSettings(page);
  await expect(page.getByRole("radio", { name: "Brief" })).toBeChecked();
  await expect(page.getByRole("radio", { name: "Captions only" })).toBeChecked();
  await expect(page.getByRole("button", { name: "Test voice" })).toBeDisabled();
});

test("the advanced model pickers list OpenRouter models", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("button", { name: "Advanced: AI models" }).click();
  const narration = page.getByRole("combobox", { name: "Narration (fast)" });
  await expect(narration).toHaveValue("anthropic/claude-haiku-5.5");
  await narration.click();
  await expect(page.getByRole("option", { name: /Claude Sonnet 5\.5/ })).toBeVisible();
  await page.getByRole("option", { name: /Claude Sonnet 5\.5/ }).click();
  await expect(narration).toHaveValue(/Claude Sonnet 5\.5/);
});

test("onboarding offers Stockfish alongside the networks", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("grandie-mock-settings", JSON.stringify({ onboardingDone: false })));
  await page.reload();
  await expect(page.getByRole("heading", { name: "Welcome to Grandie Chess" })).toBeVisible();
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByText("Engines & networks")).toBeVisible();
  const stockfish = page.getByRole("checkbox", { name: "Stockfish 19" });
  await expect(stockfish).toBeChecked();
  await expect(page.getByText("The default coach and review engine: fast on any computer.")).toBeVisible();
  // The essential networks are already installed in the mock, so only Stockfish downloads.
  await page.getByRole("button", { name: /^Download/ }).click();
  await expect(page.getByText(/times your/)).toBeVisible();
});

test("settings open with the computer's voices as macOS lists them", async ({ page }) => {
  // WebKit on macOS gives many voices the same name in different languages
  // and lists some twice; the voice picker used to crash the screen on that.
  await page.addInitScript(() => {
    const voice = (name: string, lang: string, voiceURI: string) => ({ name, lang, voiceURI, localService: true, default: false });
    const list = [
      voice("Eddy", "en-US", "com.apple.eloquence.en-US.Eddy"),
      voice("Eddy", "de-DE", "com.apple.eloquence.de-DE.Eddy"),
      voice("Daniel", "en-GB", "com.apple.voice.compact.en-GB.Daniel"),
      voice("Daniel", "en-GB", "com.apple.voice.compact.en-GB.Daniel"),
      voice("Anna", "de-DE", "com.apple.voice.compact.de-DE.Anna"),
    ];
    Object.defineProperty(window.speechSynthesis, "getVoices", { value: () => list });
  });
  await page.reload();
  await openSettings(page);
  await expect(page.getByRole("alert").filter({ hasText: "ran into a problem" })).toHaveCount(0);
  await page.getByRole("combobox", { name: "Voice", exact: true }).click();
  await expect(page.getByRole("option", { name: "Daniel (en-GB)" })).toHaveCount(1);
  await expect(page.getByRole("option", { name: "Anna (de-DE)" })).toHaveCount(0);
  await page.getByRole("option", { name: "Daniel (en-GB)" }).click();
  await expect(page.getByRole("combobox", { name: "Voice", exact: true })).toHaveValue("Daniel (en-GB)");
  const uiErrors = await page.evaluate(() => (window as unknown as { __GRANDIE_MOCK_STATS__: { uiErrors: string[] } }).__GRANDIE_MOCK_STATS__.uiErrors);
  expect(uiErrors).toEqual([]);
});
