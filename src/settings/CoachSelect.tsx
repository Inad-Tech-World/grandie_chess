import { useEffect, useState } from "react";
import { Select } from "@mantine/core";
import { commands, notifyError, type CoachInfo } from "../lib/api";
import { useSettings } from "../stores/settings";
import { useEngines } from "../stores/engines";
import { updateCoach } from "./coachSettings";

const AUTO = "auto";

/** Pick the coach: Stockfish, or Leela (AlphaZero-style). Coaches whose engine
 *  isn't installed are listed but disabled. */
export function CoachSelect({ label = "Coach", description }: { label?: string; description?: string }) {
  const coachId = useSettings((s) => s.settings.coach.coachId ?? null);
  // Re-read the list when the engines change (e.g. Stockfish was just installed).
  const engines = useEngines((s) => s.engines);
  const [coaches, setCoaches] = useState<CoachInfo[]>([]);

  useEffect(() => {
    commands
      .coachList()
      .then(setCoaches)
      .catch((e) => notifyError("Could not list the coaches", e));
  }, [engines]);

  const fallback = coaches.find((c) => c.default);
  const data = [
    { value: AUTO, label: `Automatic${fallback ? ` (${fallback.name})` : ""}` },
    ...coaches.map((c) => ({ value: c.id, label: c.available ? c.name : `${c.name}: download it in Engines`, disabled: !c.available })),
  ];
  const tagline = coaches.find((c) => c.id === (coachId ?? fallback?.id))?.tagline;

  return (
    <Select
      label={label}
      description={description ?? tagline}
      data={data}
      value={coachId ?? AUTO}
      allowDeselect={false}
      onChange={(v) => void updateCoach({ coachId: !v || v === AUTO ? null : v })}
    />
  );
}
