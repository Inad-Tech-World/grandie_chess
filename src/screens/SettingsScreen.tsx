import { useEffect, useState } from "react";
import { Anchor, Badge, Button, Card, Group, PasswordInput, SegmentedControl, Slider, Stack, Switch, Text, TextInput, Title } from "@mantine/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { notifications } from "@mantine/notifications";
import { commands, notifyError, tryCommand, type AppInfo } from "../lib/api";
import { useSettings } from "../stores/settings";
import { credentialStore } from "../lib/platform";
import { useDebouncedField } from "../lib/useDebouncedField";
import { AiCoachSettings } from "../settings/AiCoachSettings";

const BOARD_THEMES = [
  { id: "brown", label: "Brown", light: "#f0d9b5", dark: "#b58863" },
  { id: "blue", label: "Blue", light: "#dee3e6", dark: "#8ca2ad" },
  { id: "green", label: "Green", light: "#ffffdd", dark: "#86a666" },
  { id: "grey", label: "Grey", light: "#dcdcdc", dark: "#a9a9a9" },
  { id: "purple", label: "Purple", light: "#e8e0f0", dark: "#9d8bb5" },
  { id: "olive", label: "Olive", light: "#ececd7", dark: "#b7b07d" },
  { id: "slate", label: "Slate", light: "#c9d1d9", dark: "#5f7487" },
];

const PIECE_SETS = ["cburnett", "merida", "chessnut", "fantasy", "spatial", "celtic", "rhosgfx", "mpchess"];

export function SettingsScreen() {
  const { settings, update } = useSettings();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [hasToken, setHasToken] = useState(false);
  // The app was built with its own Lichess token, so the explorer works without one.
  const [builtInToken, setBuiltInToken] = useState(false);
  const [token, setToken] = useState("");
  // Typing and dragging save once the user pauses, not on every keystroke.
  const [playerName, setPlayerName] = useDebouncedField(settings.playerName, (v) => void update({ playerName: v }));
  const [lichessName, setLichessName] = useDebouncedField(settings.lichessUsername ?? "", (v) => void update({ lichessUsername: v || null }));
  const [chesscomName, setChesscomName] = useDebouncedField(settings.chesscomUsername ?? "", (v) => void update({ chesscomUsername: v || null }));
  const [animationMs, setAnimationMs] = useState(settings.animationMs);
  useEffect(() => setAnimationMs(settings.animationMs), [settings.animationMs]);

  useEffect(() => {
    commands
      .appInfo()
      .then(setInfo)
      .catch((e) => notifyError("Could not read the app details", e));
    void commands.lichessTokenStatus().then((r) => {
      if (r.status !== "ok") return;
      setHasToken(r.data.own);
      setBuiltInToken(r.data.builtIn);
    });
  }, []);

  async function saveToken(value: string) {
    const ok = await tryCommand(commands.lichessTokenSet(value), "Token not saved");
    if (ok === undefined) return;
    setHasToken(value.trim().length > 0);
    setToken("");
    notifications.show({ color: "green", message: value ? `Lichess token saved in your ${credentialStore}` : "Lichess token removed" });
  }

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
            value={playerName}
            onChange={(e) => setPlayerName(e.currentTarget.value)}
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
          <Switch mb="md" label="Sounds" checked={settings.sound} onChange={(e) => void update({ sound: e.currentTarget.checked })} />
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
            value={animationMs}
            onChange={setAnimationMs}
            onChangeEnd={(v) => void update({ animationMs: v })}
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
            Board
          </Text>
          <Text size="sm" mb={6}>
            Colours
          </Text>
          <Group gap="xs" mb="md" role="radiogroup" aria-label="Board colours">
            {BOARD_THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                role="radio"
                aria-checked={settings.boardTheme === t.id}
                aria-label={t.label}
                title={t.label}
                className="theme-swatch"
                data-active={settings.boardTheme === t.id}
                onClick={() => void update({ boardTheme: t.id })}
                style={{ background: `linear-gradient(135deg, ${t.light} 50%, ${t.dark} 50%)` }}
              />
            ))}
          </Group>
          <Text size="sm" mb={6}>
            Pieces
          </Text>
          <Group gap="xs" role="radiogroup" aria-label="Piece set">
            {PIECE_SETS.map((p) => (
              <button
                key={p}
                type="button"
                role="radio"
                aria-checked={settings.pieceSet === p}
                aria-label={p}
                title={p}
                className="piece-swatch"
                data-active={settings.pieceSet === p}
                onClick={() => void update({ pieceSet: p })}
              >
                <img src={`/pieces/${p}/wN.svg`} alt="" />
                <img src={`/pieces/${p}/bQ.svg`} alt="" />
              </button>
            ))}
          </Group>
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

        <AiCoachSettings />

        <Card withBorder radius="lg" padding="lg">
          <Group justify="space-between" mb="sm">
            <Text fw={600}>Accounts</Text>
            {hasToken ? (
              <Badge color="green" variant="light">
                Your Lichess token saved
              </Badge>
            ) : (
              builtInToken && (
                <Badge color="green" variant="light">
                  Lichess explorer ready
                </Badge>
              )
            )}
          </Group>
          <Stack gap="sm">
            <Group grow>
              <TextInput
                label="Lichess username"
                value={lichessName}
                onChange={(e) => setLichessName(e.currentTarget.value)}
              />
              <TextInput
                label="Chess.com username"
                value={chesscomName}
                onChange={(e) => setChesscomName(e.currentTarget.value)}
              />
            </Group>
            <Text size="xs" c="dimmed">
              Games played under these names are marked as yours when imported.
            </Text>
            <Group align="flex-end" gap="xs">
              <PasswordInput
                label={builtInToken ? "Your own Lichess API token (optional)" : "Lichess API token"}
                description={
                  builtInToken
                    ? "The opening explorer already works with the app's built-in access, which everyone shares. Add your own only if Lichess says it's busy. No permissions are needed."
                    : "Needed for the Lichess and Masters opening explorer. No permissions are required."
                }
                placeholder={hasToken ? "•••••• (saved)" : "lip_…"}
                value={token}
                onChange={(e) => setToken(e.currentTarget.value)}
                style={{ flex: 1 }}
              />
              <Button onClick={() => void saveToken(token)} disabled={!token.trim()}>
                Save
              </Button>
              {hasToken && (
                <Button variant="default" onClick={() => void saveToken("")}>
                  Remove
                </Button>
              )}
            </Group>
            <Anchor
              size="xs"
              component="button"
              onClick={() => void openUrl("https://lichess.org/account/oauth/token/create?description=Grandie+Chess")}
            >
              Create a token on lichess.org
            </Anchor>
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
