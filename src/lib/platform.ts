// What the UI says and shows differently on macOS and Windows. The platform
// comes from the webview's user agent (WKWebView says "Macintosh", WebView2
// says "Windows NT"), so it is known synchronously at start-up.

const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent;

export function detectMac(ua: string = userAgent): boolean {
  return /Mac/i.test(ua);
}

export function platformLabels(mac: boolean) {
  return {
    /** The modifier key for app shortcuts. */
    modKey: mac ? "⌘" : "Ctrl",
    shiftKey: mac ? "⇧" : "Shift",
    /** What to call the user's computer in sentences ("your Mac"). */
    deviceName: mac ? "Mac" : "PC",
    /** Where the Lichess token is stored. */
    credentialStore: mac ? "Keychain" : "Windows Credential Manager",
    /** A shortcut written the platform's way: ⌘⇧N or Ctrl+Shift+N. */
    shortcut: (...keys: string[]) => {
      const names = keys.map((k) => (k === "mod" ? (mac ? "⌘" : "Ctrl") : k === "shift" ? (mac ? "⇧" : "Shift") : k));
      return names.join(mac ? "" : "+");
    },
  };
}

export const isMac = detectMac();
export const { modKey, shiftKey, deviceName, credentialStore, shortcut } = platformLabels(isMac);
