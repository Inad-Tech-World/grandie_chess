import { useEffect } from "react";
import { Select } from "@mantine/core";
import { useSettings } from "../stores/settings";
import { useEngines } from "../stores/engines";

const AUTO = "auto";

/** The engine used for full-game reviews. */
export function ReviewEngineSelect() {
  const { settings, update } = useSettings();
  const engines = useEngines((s) => s.engines);
  const refresh = useEngines((s) => s.refresh);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  const current = settings.reviewEngineId ?? AUTO;
  const data = [{ value: AUTO, label: "Automatic (Stockfish when installed, otherwise Leela)" }, ...engines.map((e) => ({ value: e.id, label: e.name }))];
  if (current !== AUTO && !data.some((d) => d.value === current)) data.push({ value: current, label: `${current} (not installed)` });
  return (
    <Select
      label="Engine for game reviews"
      description="Stockfish reviews a game quickly on any processor; Leela needs a graphics card to be quick."
      data={data}
      value={current}
      allowDeselect={false}
      onChange={(v) => void update({ reviewEngineId: !v || v === AUTO ? null : v })}
    />
  );
}
