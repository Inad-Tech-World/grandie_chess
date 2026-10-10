// Small synthesised sounds (Web Audio), so no sound files with their own
// licences are needed. Kept quiet and short; switched off in Settings.

import { useSettings } from "../stores/settings";

export type SoundKind = "move" | "capture" | "check" | "end" | "correct" | "wrong";

let ctx: AudioContext | null = null;

function audio(): AudioContext | null {
  if (typeof window === "undefined" || !("AudioContext" in window)) return null;
  ctx ??= new AudioContext();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

function tone(a: AudioContext, freq: number, start: number, duration: number, type: OscillatorType, volume: number, endFreq?: number) {
  const osc = a.createOscillator();
  const gain = a.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, start);
  if (endFreq) osc.frequency.exponentialRampToValueAtTime(endFreq, start + duration);
  gain.gain.setValueAtTime(0.0001, start);
  gain.gain.exponentialRampToValueAtTime(volume, start + 0.004);
  gain.gain.exponentialRampToValueAtTime(0.0001, start + duration);
  osc.connect(gain).connect(a.destination);
  osc.start(start);
  osc.stop(start + duration + 0.02);
}

/** A short filtered noise burst: the "wood" in a piece being set down. */
function knock(a: AudioContext, start: number, volume: number, cutoff: number) {
  const length = Math.floor(a.sampleRate * 0.05);
  const buffer = a.createBuffer(1, length, a.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / length) ** 3;
  const src = a.createBufferSource();
  src.buffer = buffer;
  const filter = a.createBiquadFilter();
  filter.type = "lowpass";
  filter.frequency.value = cutoff;
  const gain = a.createGain();
  gain.gain.value = volume;
  src.connect(filter).connect(gain).connect(a.destination);
  src.start(start);
}

export function playSound(kind: SoundKind) {
  if (!useSettings.getState().settings.sound) return;
  const a = audio();
  if (!a) return;
  const t = a.currentTime + 0.01;
  switch (kind) {
    case "move":
      knock(a, t, 0.5, 1800);
      tone(a, 220, t, 0.06, "sine", 0.05, 160);
      break;
    case "capture":
      knock(a, t, 0.7, 2600);
      knock(a, t + 0.035, 0.45, 1500);
      break;
    case "check":
      knock(a, t, 0.5, 1800);
      tone(a, 880, t + 0.02, 0.12, "triangle", 0.06);
      break;
    case "end":
      tone(a, 523.25, t, 0.25, "sine", 0.07);
      tone(a, 659.25, t + 0.12, 0.3, "sine", 0.07);
      tone(a, 783.99, t + 0.24, 0.45, "sine", 0.06);
      break;
    case "correct":
      tone(a, 660, t, 0.12, "sine", 0.06);
      tone(a, 990, t + 0.08, 0.18, "sine", 0.05);
      break;
    case "wrong":
      tone(a, 180, t, 0.22, "sawtooth", 0.025, 140);
      break;
  }
}

/** The sound for a move given its SAN. */
export function soundForSan(san: string | null | undefined): SoundKind {
  if (!san) return "move";
  if (san.includes("+") || san.includes("#")) return "check";
  if (san.includes("x")) return "capture";
  return "move";
}
