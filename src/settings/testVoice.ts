// Speaks a sample sentence with the chosen voice, for the "Test voice"
// button. (The coach itself speaks through src/coach/speaker.ts.)

import type { VoiceOutput } from "../bindings";
import { commands, unwrap } from "../lib/api";
import { systemSpeechSupported } from "./useSystemVoices";

export const SAMPLE_SENTENCE = "Knight to f3. Developing a piece and controlling the centre.";

export interface TestVoiceOptions {
  output: VoiceOutput;
  systemVoice: string | null;
  rate: number;
}

/** Resolves when speaking ends (or right away for captions only). */
export async function speakSample(opts: TestVoiceOptions, text = SAMPLE_SENTENCE): Promise<void> {
  if (opts.output === "off") return;
  if (opts.output === "system") return speakSystem(text, opts.systemVoice, opts.rate);
  return speakCloud(text);
}

function speakSystem(text: string, voiceName: string | null, rate: number): Promise<void> {
  if (!systemSpeechSupported()) {
    return Promise.reject(new Error("This computer's voices aren't available here. Try the cloud voice, or captions only."));
  }
  const synth = window.speechSynthesis;
  synth.cancel();
  const u = new SpeechSynthesisUtterance(text);
  const voice = voiceName ? synth.getVoices().find((v) => v.name === voiceName) : undefined;
  if (voice) u.voice = voice;
  u.rate = Math.min(2, Math.max(0.5, rate || 1));
  return new Promise((resolve, reject) => {
    // Some engines never fire `end`; don't wait forever.
    const timer = setTimeout(resolve, 15_000);
    u.onend = () => {
      clearTimeout(timer);
      resolve();
    };
    u.onerror = (e) => {
      clearTimeout(timer);
      if (e.error === "canceled" || e.error === "interrupted") resolve();
      else reject(new Error(`The voice couldn't speak (${e.error}).`));
    };
    synth.speak(u);
  });
}

function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const bytes = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

async function speakCloud(text: string): Promise<void> {
  const b64 = await unwrap(commands.voiceSpeak(text));
  if (!b64) throw new Error("The cloud voice returned no audio.");
  const url = URL.createObjectURL(new Blob([base64ToBytes(b64)], { type: "audio/mpeg" }));
  const audio = new Audio(url);
  try {
    await new Promise<void>((resolve, reject) => {
      audio.onended = () => resolve();
      audio.onerror = () => reject(new Error("The audio couldn't be played."));
      audio.play().catch(reject);
    });
  } finally {
    URL.revokeObjectURL(url);
  }
}
