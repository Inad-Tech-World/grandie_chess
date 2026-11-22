// The coach's voice. Three ways to speak, chosen in Settings:
// - system: the computer's own voices (Web Speech API), free and offline;
// - cloud: a neural voice from OpenRouter, synthesised in Rust;
// - off: captions only (the coach waits as long as reading would take).
// Every speaker resolves when the line is finished, so the coach can move
// the pieces in step with its words.

import { commands } from "../bindings";
import { useSettings } from "../stores/settings";
import { findVoice } from "./voices";

export interface Speaker {
  /** Speak a line; resolves when it has finished (or was cancelled). */
  speak(text: string, signal: AbortSignal): Promise<void>;
  cancel(): void;
}

/** Test hook: e2e tests set this so scripts don't wait for real speech. */
function fastMode(): boolean {
  return typeof window !== "undefined" && (window as unknown as { __GRANDIE_COACH_FAST__?: boolean }).__GRANDIE_COACH_FAST__ === true;
}

/** How long reading a line takes, at the user's speech rate. */
export function readingMs(text: string, rate = 1): number {
  if (fastMode()) return 30;
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  return Math.min(12_000, Math.max(1_200, (words * 360) / Math.max(0.5, rate)));
}

function wait(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const t = setTimeout(resolve, ms);
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        resolve();
      },
      { once: true },
    );
  });
}

class SilentSpeaker implements Speaker {
  constructor(private rate: number) {}
  speak(text: string, signal: AbortSignal) {
    return wait(readingMs(text, this.rate), signal);
  }
  cancel() {}
}

export function systemSpeechAvailable(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window && typeof SpeechSynthesisUtterance !== "undefined";
}

class SystemSpeaker implements Speaker {
  constructor(
    private voiceName: string | null,
    private rate: number,
  ) {}

  speak(text: string, signal: AbortSignal): Promise<void> {
    if (fastMode()) return wait(30, signal);
    return new Promise((resolve) => {
      if (signal.aborted) return resolve();
      const synth = window.speechSynthesis;
      const u = new SpeechSynthesisUtterance(text);
      const voice = findVoice(synth.getVoices(), this.voiceName);
      if (voice) u.voice = voice;
      u.rate = this.rate;
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        clearTimeout(guard);
        resolve();
      };
      u.onend = finish;
      u.onerror = finish;
      // Some web views never fire `end`; don't let the coach hang.
      const guard = setTimeout(finish, readingMs(text, this.rate) * 2 + 3000);
      signal.addEventListener(
        "abort",
        () => {
          synth.cancel();
          finish();
        },
        { once: true },
      );
      synth.speak(u);
    });
  }

  cancel() {
    if (systemSpeechAvailable()) window.speechSynthesis.cancel();
  }
}

/** Cloud audio, cached by text so replays cost nothing. */
const audioCache = new Map<string, string>();

async function cloudAudioUrl(text: string): Promise<string> {
  const hit = audioCache.get(text);
  if (hit) return hit;
  const r = await commands.voiceSpeak(text);
  if (r.status === "error") throw new Error(r.error.message);
  const bytes = Uint8Array.from(atob(r.data), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: "audio/mpeg" }));
  audioCache.set(text, url);
  if (audioCache.size > 60) {
    const first = audioCache.keys().next().value as string;
    URL.revokeObjectURL(audioCache.get(first)!);
    audioCache.delete(first);
  }
  return url;
}

class CloudSpeaker implements Speaker {
  private audio: HTMLAudioElement | null = null;
  constructor(
    private fallback: Speaker,
    private onFallback: (reason: string) => void,
  ) {}

  async speak(text: string, signal: AbortSignal): Promise<void> {
    if (fastMode()) return wait(30, signal);
    let url: string;
    try {
      url = await cloudAudioUrl(text);
    } catch (e) {
      this.onFallback(e instanceof Error ? e.message : String(e));
      return this.fallback.speak(text, signal);
    }
    if (signal.aborted) return;
    await new Promise<void>((resolve) => {
      const audio = new Audio(url);
      this.audio = audio;
      const finish = () => {
        audio.onended = null;
        audio.onerror = null;
        resolve();
      };
      audio.onended = finish;
      audio.onerror = finish;
      signal.addEventListener(
        "abort",
        () => {
          audio.pause();
          finish();
        },
        { once: true },
      );
      audio.play().catch(() => finish());
    });
  }

  cancel() {
    this.audio?.pause();
    this.fallback.cancel();
  }
}

let fallbackNotified = false;

/** The speaker for the current settings. */
export function currentSpeaker(onNotice?: (message: string) => void): Speaker {
  const { coach } = useSettings.getState().settings;
  const rate = coach.speechRate ?? 1;
  const system = systemSpeechAvailable() ? new SystemSpeaker(coach.systemVoice ?? null, rate) : new SilentSpeaker(rate);
  switch (coach.voiceOutput) {
    case "off":
      return new SilentSpeaker(rate);
    case "cloud":
      return new CloudSpeaker(system, (reason) => {
        if (!fallbackNotified) {
          fallbackNotified = true;
          onNotice?.(`The cloud voice isn't available (${reason}); using the system voice.`);
        }
      });
    default:
      return system;
  }
}

/** A one-off announcement (e.g. the opponent's move), interrupting the last. */
let announcement: AbortController | null = null;
export function announce(text: string) {
  announcement?.abort();
  const ctrl = new AbortController();
  announcement = ctrl;
  void currentSpeaker().speak(text, ctrl.signal);
}

const PIECES: Record<string, string> = { K: "King", Q: "Queen", R: "Rook", B: "Bishop", N: "Knight" };

/** "Nxe5+" -> "Knight takes e5, check". */
export function sanToSpeech(san: string): string {
  const check = san.endsWith("#") ? ", checkmate" : san.endsWith("+") ? ", check" : "";
  const s = san.replace(/[+#!?]+$/, "");
  if (s === "O-O" || s === "0-0") return `Castles kingside${check}`;
  if (s === "O-O-O" || s === "0-0-0") return `Castles queenside${check}`;
  const [body, promo] = s.split("=");
  const piece = PIECES[body[0]];
  const rest = piece ? body.slice(1) : body;
  const capture = rest.includes("x");
  const [from, to] = capture ? rest.split("x") : ["", rest];
  const target = to.slice(-2);
  const disambiguation = (capture ? from : to.slice(0, -2)) || "";
  let words: string;
  if (piece) {
    words = `${piece}${disambiguation ? ` ${disambiguation}` : ""}${capture ? " takes" : " to"} ${target}`;
  } else {
    words = capture ? `${from} takes ${target}` : target;
  }
  if (promo) words += `, promotes to ${(PIECES[promo[0]] ?? "queen").toLowerCase()}`;
  return words + check;
}
