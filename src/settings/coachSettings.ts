import type { AiSettings, CoachSettings } from "../bindings";
import { useSettings } from "../stores/settings";

/** Change some coach options, building on the latest saved values so quick
 *  successive edits don't undo each other. */
export function updateCoach(patch: CoachSettings) {
  const current = useSettings.getState().settings.coach;
  return useSettings.getState().update({ coach: { ...current, ...patch } });
}

/** Change some AI options (models, the on/off switch). */
export function updateAi(patch: AiSettings) {
  const current = useSettings.getState().settings.ai;
  return useSettings.getState().update({ ai: { ...current, ...patch } });
}

/** Common languages for the coach; any other can be typed in. */
export const COACH_LANGUAGES = [
  "English",
  "Spanish",
  "French",
  "German",
  "Italian",
  "Portuguese",
  "Dutch",
  "Russian",
  "Swahili",
  "Hindi",
  "Chinese",
  "Japanese",
];
