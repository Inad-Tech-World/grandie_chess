import { useMemo, useState } from "react";
import { Group, Loader, Select, Text, type ComboboxItem } from "@mantine/core";
import { commands, errorMessage, unwrap, type AiModel } from "../lib/api";

/** The model catalogue, fetched once per modality per session. */
const cache = new Map<string, Promise<AiModel[]>>();

function loadModels(modality: string): Promise<AiModel[]> {
  let p = cache.get(modality);
  if (!p) {
    p = unwrap(commands.aiModels(modality));
    cache.set(modality, p);
    p.catch(() => cache.delete(modality));
  }
  return p;
}

function dollars(perToken: number | null): string {
  if (perToken == null) return "?";
  const perMillion = perToken * 1e6;
  return "$" + perMillion.toFixed(perMillion < 1 ? 2 : 1);
}

/** "$0.10 / $0.50 per 1M tokens" for text models. */
function priceLabel(m: AiModel, modality: string): string | null {
  if (modality !== "text" || m.promptPrice == null) return null;
  return `${dollars(m.promptPrice)} / ${dollars(m.completionPrice)} per 1M tokens`;
}

interface Props {
  label: string;
  description?: string;
  /** OpenRouter output modality: "text", "transcription" or "speech". */
  modality: "text" | "transcription" | "speech";
  value: string;
  onChange: (id: string) => void;
}

/** A searchable OpenRouter model picker. The list loads when it is opened;
 *  the current model stays selectable even if the catalogue doesn't list it. */
export function ModelSelect({ label, description, modality, value, onChange }: Props) {
  const [models, setModels] = useState<AiModel[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function open() {
    if (models || loading) return;
    setLoading(true);
    loadModels(modality)
      .then((m) => {
        setModels(m);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setLoading(false));
  }

  const byId = useMemo(() => new Map((models ?? []).map((m) => [m.id, m])), [models]);
  const data = useMemo(() => {
    const items: ComboboxItem[] = (models ?? []).map((m) => ({ value: m.id, label: m.name && m.name !== m.id ? `${m.name} (${m.id})` : m.id }));
    if (value && !items.some((i) => i.value === value)) items.unshift({ value, label: value });
    return items;
  }, [models, value]);

  return (
    <Select
      label={label}
      description={error ? `Couldn't load the model list: ${error}` : description}
      searchable
      allowDeselect={false}
      data={data}
      value={value}
      onChange={(v) => v && onChange(v)}
      onDropdownOpen={open}
      nothingFoundMessage={loading ? "Loading models…" : "No matching model"}
      rightSection={loading ? <Loader size={14} /> : undefined}
      maxDropdownHeight={280}
      renderOption={({ option }) => {
        const m = byId.get(option.value);
        const price = m ? priceLabel(m, modality) : null;
        return (
          <Group gap={6} justify="space-between" wrap="nowrap" w="100%">
            <Text size="sm" truncate>
              {option.label}
            </Text>
            {price && (
              <Text size="xs" c="dimmed" style={{ whiteSpace: "nowrap" }}>
                {price}
              </Text>
            )}
          </Group>
        );
      }}
    />
  );
}
