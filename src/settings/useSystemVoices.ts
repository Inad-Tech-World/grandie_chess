import { useEffect, useState } from "react";

/** Whether this web view can speak with the computer's own voices. */
export function systemSpeechSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window && typeof window.SpeechSynthesisUtterance === "function";
}

/** The computer's voices. They load asynchronously in most browsers, so the
 *  list updates on `voiceschanged`. Empty where speech isn't available. */
export function useSystemVoices(): SpeechSynthesisVoice[] {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([]);
  useEffect(() => {
    if (!systemSpeechSupported()) return;
    const synth = window.speechSynthesis;
    const read = () => {
      try {
        setVoices(synth.getVoices());
      } catch {
        setVoices([]);
      }
    };
    read();
    synth.addEventListener?.("voiceschanged", read);
    return () => synth.removeEventListener?.("voiceschanged", read);
  }, []);
  return voices;
}
