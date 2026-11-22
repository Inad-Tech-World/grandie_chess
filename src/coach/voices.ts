// The computer's voices as the web view reports them. WebKit on macOS lists
// some voices twice and gives many voices the same name in different
// languages (there's an "Eddy" in ten languages), so a voice is identified by
// its voiceURI and the list is de-duplicated before it's shown anywhere.

/** A stable id for a voice: its URI, or name and language if it has none. */
export function voiceId(v: Pick<SpeechSynthesisVoice, "voiceURI" | "name" | "lang">): string {
  return v.voiceURI || `${v.name}|${v.lang}`;
}

/** Each voice once, in the order the system gives them. */
export function uniqueVoices<V extends Pick<SpeechSynthesisVoice, "voiceURI" | "name" | "lang">>(voices: readonly V[]): V[] {
  const seen = new Set<string>();
  return voices.filter((v) => {
    const id = voiceId(v);
    if (seen.has(id)) return false;
    seen.add(id);
    return true;
  });
}

/** The saved voice. Settings store its id; older versions stored its name. */
export function findVoice<V extends Pick<SpeechSynthesisVoice, "voiceURI" | "name" | "lang">>(voices: readonly V[], saved: string | null | undefined): V | undefined {
  if (!saved) return undefined;
  return voices.find((v) => voiceId(v) === saved) ?? voices.find((v) => v.name === saved);
}

let languageNames: Intl.DisplayNames | null | undefined;

/** "en-GB" → "English", matching the coach's language setting. */
export function languageOf(lang: string): string | null {
  if (languageNames === undefined) {
    try {
      languageNames = new Intl.DisplayNames(["en"], { type: "language" });
    } catch {
      languageNames = null;
    }
  }
  const base = lang.split(/[-_]/)[0];
  if (!languageNames || !base) return null;
  try {
    return languageNames.of(base) ?? null;
  } catch {
    return null;
  }
}

export interface VoiceOption {
  value: string;
  label: string;
}

/** Options for the voice picker: "Default voice", then each voice in the
 *  coach's language once (every voice if none matches). The saved voice is
 *  always listed so the picker can show it. */
export function voiceOptions<V extends Pick<SpeechSynthesisVoice, "voiceURI" | "name" | "lang">>(voices: readonly V[], language: string, saved: string | null | undefined): VoiceOption[] {
  const all = uniqueVoices(voices);
  const matching = all.filter((v) => languageOf(v.lang) === language);
  const shown = matching.length ? [...matching] : [...all];
  const current = findVoice(all, saved);
  if (current && !shown.includes(current)) shown.unshift(current);
  shown.sort((a, b) => a.name.localeCompare(b.name) || a.lang.localeCompare(b.lang));
  return [{ value: "", label: "Default voice" }, ...shown.map((v) => ({ value: voiceId(v), label: `${v.name} (${v.lang})` }))];
}
