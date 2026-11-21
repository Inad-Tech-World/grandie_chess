import { useCallback, useEffect, useState } from "react";
import {
  Accordion,
  Anchor,
  Autocomplete,
  Badge,
  Button,
  Card,
  Group,
  PasswordInput,
  SegmentedControl,
  Select,
  Slider,
  Stack,
  Switch,
  Text,
  Tooltip,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { IconPlayerPlay } from "@tabler/icons-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, notifyError, tryCommand, type AiKeyInfo, type AiUsageSummary, type Verbosity, type VoiceOutput } from "../lib/api";
import { credentialStore } from "../lib/platform";
import { useDebouncedField } from "../lib/useDebouncedField";
import { useSettings } from "../stores/settings";
import { CoachSelect } from "./CoachSelect";
import { COACH_LANGUAGES, updateAi, updateCoach } from "./coachSettings";
import { ModelSelect } from "./ModelSelect";
import { ReviewEngineSelect } from "./ReviewEngineSelect";
import { speakSample } from "./testVoice";
import { systemSpeechSupported, useSystemVoices } from "./useSystemVoices";

function money(n: number | null | undefined): string {
  return "$" + (n ?? 0).toFixed((n ?? 0) < 1 ? 3 : 2);
}

/** Voices drop down: the computer's own, or the cloud voice of the chosen model. */
function VoicePicker() {
  const { coach, ai } = useSettings((s) => s.settings);
  const voices = useSystemVoices();
  const [cloudVoices, setCloudVoices] = useState<string[] | null>(null);
  const ttsModel = ai.ttsModel ?? "";

  useEffect(() => {
    setCloudVoices(null);
  }, [ttsModel]);

  if (coach.voiceOutput === "cloud") {
    const data = [...(cloudVoices ?? [])];
    if (ai.ttsVoice && !data.includes(ai.ttsVoice)) data.unshift(ai.ttsVoice);
    return (
      <Select
        label="Speaker"
        description="Voices of the cloud voice model (Advanced below)."
        searchable
        allowDeselect={false}
        data={data}
        value={ai.ttsVoice ?? null}
        onDropdownOpen={() => {
          if (cloudVoices || !ttsModel) return;
          commands
            .aiVoices(ttsModel)
            .then((r) => setCloudVoices(r.status === "ok" ? r.data : []))
            .catch(() => setCloudVoices([]));
        }}
        onChange={(v) => v && void updateAi({ ttsVoice: v })}
      />
    );
  }
  if (coach.voiceOutput !== "system") return null;
  if (!systemSpeechSupported()) {
    return (
      <Text size="xs" c="dimmed">
        This computer's voices aren't available here; choose the cloud voice or captions only.
      </Text>
    );
  }
  const data = [{ value: "", label: "Default voice" }, ...voices.map((v) => ({ value: v.name, label: `${v.name} (${v.lang})` }))];
  return (
    <Select
      label="Voice"
      searchable
      allowDeselect={false}
      data={data}
      value={coach.systemVoice ?? ""}
      onChange={(v) => void updateCoach({ systemVoice: v || null })}
      nothingFoundMessage="No voice by that name"
    />
  );
}

/** Settings → the coach and the AI models behind it. */
export function AiCoachSettings() {
  const { coach, ai } = useSettings((s) => s.settings);
  const [hasKey, setHasKey] = useState(false);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [keyInfo, setKeyInfo] = useState<AiKeyInfo | null>(null);
  const [usage, setUsage] = useState<AiUsageSummary | null>(null);
  const [testing, setTesting] = useState(false);
  const [language, setLanguage] = useDebouncedField(coach.language ?? "English", (v) => void updateCoach({ language: v.trim() || "English" }));
  const [rate, setRate] = useState(coach.speechRate ?? 1);
  useEffect(() => setRate(coach.speechRate ?? 1), [coach.speechRate]);

  const refreshAccount = useCallback(async (saved: boolean) => {
    if (!saved) {
      setKeyInfo(null);
      setUsage(null);
      return;
    }
    // Offline or a revoked key: just leave the numbers out.
    const [info, spent] = await Promise.all([commands.aiKeyInfo(), commands.aiUsage(30)]);
    setKeyInfo(info.status === "ok" ? info.data : null);
    setUsage(spent.status === "ok" ? spent.data : null);
  }, []);

  useEffect(() => {
    void commands.aiKeyStatus().then((r) => {
      const saved = r.status === "ok" && r.data;
      setHasKey(saved);
      void refreshAccount(saved);
    });
  }, [refreshAccount]);

  async function saveKey(value: string) {
    setSaving(true);
    const ok = await tryCommand(commands.aiKeySet(value.trim()), "Key not saved");
    setSaving(false);
    if (ok === undefined) return;
    const saved = value.trim().length > 0;
    setHasKey(saved);
    setKey("");
    // The backend switches AI on with a new key and off without one.
    await updateAi({ enabled: saved });
    if (!saved && coach.voiceOutput === "cloud") await updateCoach({ voiceOutput: "system" });
    notifications.show({ color: "green", message: saved ? `OpenRouter key saved in your ${credentialStore}` : "OpenRouter key removed" });
    void refreshAccount(saved);
  }

  async function testVoice() {
    setTesting(true);
    try {
      await speakSample({ output: coach.voiceOutput ?? "system", systemVoice: coach.systemVoice ?? null, rate: coach.speechRate ?? 1 });
    } catch (e) {
      notifyError("Couldn't play the voice", e);
    } finally {
      setTesting(false);
    }
  }

  const creditLine = keyInfo
    ? keyInfo.limit != null
      ? `Credits used: ${money(keyInfo.usage)} of ${money(keyInfo.limit)}`
      : `Credits used: ${money(keyInfo.usage)}${keyInfo.isFreeTier ? " (free tier)" : ""}`
    : null;

  return (
    <>
      <Card withBorder radius="lg" padding="lg">
        <Text fw={600} mb={4}>
          Coach
        </Text>
        <Text size="xs" c="dimmed" mb="sm">
          The coach shows ideas on the board and explains them out loud. Its chess always comes from an engine.
        </Text>
        <Stack gap="sm">
          <CoachSelect />
          <Group justify="space-between">
            <Text size="sm">How much the coach says</Text>
            <SegmentedControl
              aria-label="How much the coach says"
              value={coach.verbosity ?? "normal"}
              onChange={(v) => void updateCoach({ verbosity: v as Verbosity })}
              data={[
                { value: "brief", label: "Brief" },
                { value: "normal", label: "Normal" },
                { value: "detailed", label: "Detailed" },
              ]}
            />
          </Group>
          <Autocomplete
            label="Language"
            description="The coach speaks and writes in this language (AI explanations only; built-in ones are in English)."
            data={COACH_LANGUAGES}
            value={language}
            onChange={setLanguage}
          />
          <Group justify="space-between">
            <Text size="sm">Voice</Text>
            <SegmentedControl
              aria-label="Coach voice"
              value={coach.voiceOutput ?? "system"}
              onChange={(v) => void updateCoach({ voiceOutput: v as VoiceOutput })}
              data={[
                { value: "system", label: "System voice" },
                { value: "cloud", label: "Cloud voice", disabled: !hasKey },
                { value: "off", label: "Captions only" },
              ]}
            />
          </Group>
          <VoicePicker />
          {coach.voiceOutput !== "off" && (
            <div>
              <Text size="sm" mb={6}>
                Speaking speed
              </Text>
              <Slider
                aria-label="Speaking speed"
                min={0.5}
                max={2}
                step={0.1}
                value={rate}
                onChange={setRate}
                onChangeEnd={(v) => void updateCoach({ speechRate: v })}
                label={(v) => `${v.toFixed(1)}×`}
                marks={[
                  { value: 0.5, label: "Slow" },
                  { value: 1, label: "Normal" },
                  { value: 2, label: "Fast" },
                ]}
                mb="lg"
              />
            </div>
          )}
          <Group justify="space-between" align="flex-start">
            <Switch
              label="Say the opponent's moves out loud"
              checked={coach.announceMoves ?? false}
              onChange={(e) => void updateCoach({ announceMoves: e.currentTarget.checked })}
            />
            <Button size="compact-sm" variant="default" leftSection={<IconPlayerPlay size={14} />} loading={testing} disabled={coach.voiceOutput === "off"} onClick={() => void testVoice()}>
              Test voice
            </Button>
          </Group>
          <ReviewEngineSelect />
        </Stack>
      </Card>

      <Card withBorder radius="lg" padding="lg">
        <Group justify="space-between" mb="sm">
          <Text fw={600}>AI coach (OpenRouter)</Text>
          {hasKey && (
            <Badge color="green" variant="light">
              OpenRouter key saved
            </Badge>
          )}
        </Group>
        <Stack gap="sm">
          <Text size="sm" c="dimmed">
            With an OpenRouter key, the coach puts the engine's lines into its own words, answers your questions and understands your voice. Without
            one, it still demonstrates every move with built-in explanations.
          </Text>
          <Group align="flex-end" gap="xs">
            <PasswordInput
              label="OpenRouter API key"
              placeholder={hasKey ? "•••••• (saved)" : "sk-or-…"}
              value={key}
              onChange={(e) => setKey(e.currentTarget.value)}
              style={{ flex: 1 }}
            />
            <Button onClick={() => void saveKey(key)} disabled={!key.trim()} loading={saving}>
              Save key
            </Button>
            {hasKey && (
              <Button variant="default" onClick={() => void saveKey("")} disabled={saving}>
                Remove key
              </Button>
            )}
          </Group>
          <Anchor size="xs" component="button" onClick={() => void openUrl("https://openrouter.ai/keys")}>
            Get a key at openrouter.ai/keys
          </Anchor>
          <Tooltip label="Save an OpenRouter key first" disabled={hasKey}>
            <div>
              <Switch
                label="Use AI for the coach's words"
                description="Positions, moves and recorded speech are sent to OpenRouter while this is on. Requests are charged to your OpenRouter account."
                checked={hasKey && (ai.enabled ?? false)}
                disabled={!hasKey}
                onChange={(e) => void updateAi({ enabled: e.currentTarget.checked })}
              />
            </div>
          </Tooltip>
          {hasKey && (creditLine || usage) && (
            <Text size="xs" c="dimmed">
              {creditLine}
              {creditLine && usage ? " · " : ""}
              {usage ? `Last 30 days in Grandie: ${usage.requests} request${usage.requests === 1 ? "" : "s"}, ${money(usage.cost)}` : ""}
            </Text>
          )}
          <Accordion variant="contained" radius="md" chevronPosition="left">
            <Accordion.Item value="models">
              <Accordion.Control>
                <Text size="sm">Advanced: AI models</Text>
              </Accordion.Control>
              <Accordion.Panel>
                <Stack gap="sm">
                  <ModelSelect
                    label="Narration (fast)"
                    description="Words for the moves the coach shows on the board."
                    modality="text"
                    value={ai.narrateModel ?? ""}
                    onChange={(v) => void updateAi({ narrateModel: v })}
                  />
                  <ModelSelect
                    label="Answers and explanations"
                    description="Questions you ask the coach, and player profiles."
                    modality="text"
                    value={ai.explainModel ?? ""}
                    onChange={(v) => void updateAi({ explainModel: v })}
                  />
                  <ModelSelect label="Speech to text" description="Understands what you say." modality="transcription" value={ai.sttModel ?? ""} onChange={(v) => void updateAi({ sttModel: v })} />
                  <ModelSelect label="Cloud voice" description="Speaks for the coach when the cloud voice is chosen." modality="speech" value={ai.ttsModel ?? ""} onChange={(v) => void updateAi({ ttsModel: v })} />
                </Stack>
              </Accordion.Panel>
            </Accordion.Item>
          </Accordion>
        </Stack>
      </Card>
    </>
  );
}
