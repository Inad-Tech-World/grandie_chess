import { describe, expect, it } from "vitest";
import { detectMac, platformLabels } from "./platform";

const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

describe("platform", () => {
  it("tells macOS's webview from Windows's", () => {
    expect(detectMac(MAC_UA)).toBe(true);
    expect(detectMac(WINDOWS_UA)).toBe(false);
  });

  it("writes shortcuts the macOS way", () => {
    const mac = platformLabels(true);
    expect(mac.shortcut("mod", "N")).toBe("⌘N");
    expect(mac.shortcut("mod", "shift", "N")).toBe("⌘⇧N");
    expect(mac.deviceName).toBe("Mac");
    expect(mac.credentialStore).toBe("Keychain");
  });

  it("writes shortcuts the Windows way", () => {
    const win = platformLabels(false);
    expect(win.shortcut("mod", "N")).toBe("Ctrl+N");
    expect(win.shortcut("mod", "shift", "N")).toBe("Ctrl+Shift+N");
    expect(win.shortcut("mod", ",")).toBe("Ctrl+,");
    expect(win.modKey).toBe("Ctrl");
    expect(win.deviceName).toBe("PC");
    expect(win.credentialStore).toBe("Windows Credential Manager");
  });
});
