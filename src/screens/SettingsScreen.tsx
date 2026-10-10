import { useEffect, useState } from "react";
import { Card, Group, SegmentedControl, Slider, Stack, Switch, Text, TextInput, Title } from "@mantine/core";
import { commands, type AppInfo } from "../lib/api";
import { useSettings } from "../stores/settings";

export function SettingsScreen() {
  const { settings, update } = useSettings();
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    void commands.appInfo().then(setInfo);
  }, []);

  return (
    <div className="screen" style={{ maxWidth: 760 }}>
      <Title order={2} mb="lg">
        Settings
      </Title>
      <Stack gap="lg">
        <Card withBorder radius="lg" padding="lg">
          <Text fw={600} mb="sm">
            You
          </Text>
          <TextInput
            label="Your name"
            description="Used in saved games."
            value={settings.playerName}
            onChange={(e) => void update({ playerName: e.currentTarget.value })}
          />
        </Card>

        <Card withBorder radius="lg" padding="lg">
          <Text fw={600} mb="sm">
            Appearance
          </Text>
          <Group justify="space-between" mb="md">
            <Text size="sm">Theme</Text>
            <SegmentedControl
              value={settings.colorScheme}
              onChange={(v) => void update({ colorScheme: v as "auto" | "dark" | "light" })}
              data={[
                { value: "auto", label: "System" },
                { value: "dark", label: "Dark" },
                { value: "light", label: "Light" },
              ]}
            />
          </Group>
          <Switch
            mb="md"
            label="Show board coordinates"
            checked={settings.showCoordinates}
            onChange={(e) => void update({ showCoordinates: e.currentTarget.checked })}
          />
          <Text size="sm" mb={6}>
            Piece animation
          </Text>
          <Slider
            min={0}
            max={500}
            step={50}
            value={settings.animationMs}
            onChange={(v) => void update({ animationMs: v })}
            marks={[
              { value: 0, label: "Off" },
              { value: 200, label: "Normal" },
              { value: 500, label: "Slow" },
            ]}
            mb="lg"
          />
        </Card>

        <Card withBorder radius="lg" padding="lg">
          <Text fw={600} mb="sm">
            Opponents
          </Text>
          <Stack gap="sm">
            <Switch
              label="Human-like thinking time"
              description="Maia and the Leela levels pause briefly before moving instead of answering instantly."
              checked={settings.humanThinkTime}
              onChange={(e) => void update({ humanThinkTime: e.currentTarget.checked })}
            />
            <Switch
              label="Vary the opening"
              description="Maia picks among the moves real players choose, so games don't repeat."
              checked={settings.opponentVariety}
              onChange={(e) => void update({ opponentVariety: e.currentTarget.checked })}
            />
          </Stack>
        </Card>

        {info && (
          <Text size="xs" c="dimmed">
            Grandie Chess {info.version} · data folder: {info.dataDir}
          </Text>
        )}
      </Stack>
    </div>
  );
}
