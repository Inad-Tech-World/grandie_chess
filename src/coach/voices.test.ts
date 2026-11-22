import { describe, expect, it } from "vitest";
import { findVoice, languageOf, uniqueVoices, voiceOptions } from "./voices";

// As WebKit on macOS reports them: "Eddy" in several languages, and one
// voice listed twice with the same URI.
const voices = [
  { name: "Eddy", lang: "en-US", voiceURI: "com.apple.eloquence.en-US.Eddy" },
  { name: "Eddy", lang: "de-DE", voiceURI: "com.apple.eloquence.de-DE.Eddy" },
  { name: "Daniel", lang: "en-GB", voiceURI: "com.apple.voice.compact.en-GB.Daniel" },
  { name: "Daniel", lang: "en-GB", voiceURI: "com.apple.voice.compact.en-GB.Daniel" },
  { name: "Anna", lang: "de-DE", voiceURI: "com.apple.voice.compact.de-DE.Anna" },
];

describe("system voices", () => {
  it("lists each voice once", () => {
    expect(uniqueVoices(voices)).toHaveLength(4);
  });

  it("offers unique options in the coach's language, with a default", () => {
    const options = voiceOptions(voices, "English", null);
    const values = options.map((o) => o.value);
    expect(new Set(values).size).toBe(values.length);
    expect(options.map((o) => o.label)).toEqual(["Default voice", "Daniel (en-GB)", "Eddy (en-US)"]);
  });

  it("keeps the saved voice listed even in another language", () => {
    const options = voiceOptions(voices, "English", "com.apple.voice.compact.de-DE.Anna");
    expect(options.map((o) => o.label)).toContain("Anna (de-DE)");
  });

  it("finds a voice saved by id, or by name as older versions saved it", () => {
    expect(findVoice(voices, "com.apple.eloquence.de-DE.Eddy")?.lang).toBe("de-DE");
    expect(findVoice(voices, "Daniel")?.lang).toBe("en-GB");
    expect(findVoice(voices, null)).toBeUndefined();
  });

  it("names languages like the coach setting does", () => {
    expect(languageOf("en-GB")).toBe("English");
    expect(languageOf("de_DE")).toBe("German");
  });

  it("shows every voice when none matches the language", () => {
    expect(voiceOptions(voices, "Klingon", null)).toHaveLength(5);
  });
});
