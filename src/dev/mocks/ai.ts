// Mock OpenRouter key, model catalogue, spending, coaches and the cloud voice.

import type { AiModel } from "../../bindings";
import type { MockHelpers, Register } from "./index";

const MODELS: Record<string, AiModel[]> = {
  text: [
    { id: "anthropic/claude-haiku-5.5", name: "Anthropic: Claude Haiku 5.5", promptPrice: 0.0000001, completionPrice: 0.0000005, contextLength: 1_000_000 },
    { id: "anthropic/claude-sonnet-5.5", name: "Anthropic: Claude Sonnet 5.5", promptPrice: 0.000002, completionPrice: 0.00001, contextLength: 1_000_000 },
    { id: "anthropic/claude-opus-5.5", name: "Anthropic: Claude Opus 5.5", promptPrice: 0.000004, completionPrice: 0.00002, contextLength: 1_000_000 },
  ],
  transcription: [
    { id: "openai/gpt-4o-mini-transcribe", name: "OpenAI: GPT-4o mini Transcribe", promptPrice: 0.00000125, completionPrice: 0.000005, contextLength: null },
    { id: "openai/whisper-large-v3-turbo", name: "OpenAI: Whisper Large v3 Turbo", promptPrice: 0.00000333, completionPrice: 0, contextLength: null },
  ],
  speech: [
    { id: "mistralai/voxtral-mini-tts-3-0", name: "Mistral: Voxtral Mini TTS", promptPrice: 0.000013, completionPrice: 0, contextLength: null },
    { id: "microsoft/mai-voice-2.1-flash", name: "Microsoft: MAI Voice 2.1 Flash", promptPrice: 0.000015, completionPrice: 0, contextLength: null },
  ],
};

const VOICES: Record<string, string[]> = {
  "mistralai/voxtral-mini-tts-3-0": ["en_paul_neutral", "en_paul_confident", "en_paul_cheerful", "gb_oliver_neutral"],
  "microsoft/mai-voice-2.1-flash": ["en-GB-Emily:MAI-Voice-2.1-Flash", "en-GB-Harry:MAI-Voice-2.1-Flash"],
};

export function install(register: Register, _helpers: MockHelpers) {
  let keySaved = false;
  let requests = 0;

  register("ai_key_status", () => keySaved);
  register("ai_key_set", (a) => {
    const key = String(a.key ?? "").trim();
    if (!key) {
      keySaved = false;
      return null;
    }
    if (/\s/.test(key)) throw { kind: "other", message: "The OpenRouter key can't contain spaces." };
    if (!key.startsWith("sk-or-") || key.length < 20) throw { kind: "other", message: "That doesn't look like an OpenRouter key (they start with sk-or-)." };
    keySaved = true;
    return null;
  });
  register("ai_key_info", () => {
    if (!keySaved) throw { kind: "noApiKey", message: "Add an OpenRouter API key in Settings → AI coach to use AI features." };
    return { label: "mock key", usage: 0.42, limit: 5, isFreeTier: false };
  });
  register("ai_models", (a) => MODELS[String(a.modality ?? "text")] ?? []);
  register("ai_voices", (a) => VOICES[String(a.model ?? "")] ?? []);
  register("ai_usage", () => ({ requests, tokensIn: requests * 900, tokensOut: requests * 120, cost: requests * 0.0004 }));
  register("coach_list", () => [
    { id: "stockfish", name: "Stockfish", tagline: "Concrete and precise: calculates every line.", available: true, default: true },
    { id: "leela", name: "Leela (AlphaZero-style)", tagline: "Intuitive and positional, in the spirit of AlphaZero.", available: true, default: false },
  ]);
  register("voice_speak", () => {
    if (!keySaved) throw { kind: "noApiKey", message: "Add an OpenRouter API key in Settings → AI coach to use AI features." };
    requests += 1;
    // No real audio in the mock; the UI must cope with playback failing.
    return "";
  });
}
