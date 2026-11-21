// Hold-to-talk: press and hold the mic button (or the V key) to speak, let go
// to send. Recording happens natively in Rust; the transcript comes back with
// what it means (a move, a command or a question).

import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type VoiceResult } from "../bindings";
import { errorMessage } from "../lib/api";

export interface VoiceState {
  listening: boolean;
  busy: boolean;
  level: number;
  error: string | null;
  lastTranscript: string | null;
}

export function useVoice(opts: { fen: () => string | null; onResult: (r: VoiceResult) => void; enabled?: boolean }) {
  const [state, setState] = useState<VoiceState>({ listening: false, busy: false, level: 0, error: null, lastTranscript: null });
  const listening = useRef(false);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void events.voiceLevel.listen((e) => {
      if (listening.current) setState((s) => ({ ...s, level: Number(e.payload) || 0 }));
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, []);

  const start = useCallback(async () => {
    if (listening.current || optsRef.current.enabled === false) return;
    listening.current = true;
    setState((s) => ({ ...s, listening: true, error: null, level: 0 }));
    const r = await commands.voiceStart();
    if (r.status === "error") {
      listening.current = false;
      setState((s) => ({ ...s, listening: false, error: r.error.message }));
    }
  }, []);

  const stop = useCallback(async () => {
    if (!listening.current) return;
    listening.current = false;
    setState((s) => ({ ...s, listening: false, busy: true, level: 0 }));
    try {
      const r = await commands.voiceStop(optsRef.current.fen());
      if (r.status === "error") throw r.error;
      setState((s) => ({ ...s, busy: false, lastTranscript: r.data.transcript || null }));
      optsRef.current.onResult(r.data);
    } catch (e) {
      setState((s) => ({ ...s, busy: false, error: errorMessage(e) }));
    }
  }, []);

  const cancel = useCallback(() => {
    listening.current = false;
    void commands.voiceCancel();
    setState((s) => ({ ...s, listening: false, level: 0 }));
  }, []);

  // Hold V to talk (not while typing).
  useEffect(() => {
    const typing = (t: EventTarget | null) => t instanceof HTMLElement && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));
    const down = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "v" && !e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey && !typing(e.target)) {
        e.preventDefault();
        void start();
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "v") void stop();
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      if (listening.current) void commands.voiceCancel();
    };
  }, [start, stop]);

  return { ...state, start, stop, cancel };
}
