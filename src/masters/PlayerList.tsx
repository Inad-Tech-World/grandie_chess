// The list of masters: the bundled legends first, then every player in the
// PGN Mentor catalogue, with what is already in the library.

import { useMemo, useState } from "react";
import { ActionIcon, Badge, Button, Group, Menu, Progress, ScrollArea, Stack, Text, TextInput, UnstyledButton } from "@mantine/core";
import { IconDots, IconDownload, IconFileImport, IconSearch, IconStar, IconTrash } from "@tabler/icons-react";
import type { MasterPlayer } from "../bindings";
import { downloadFraction, useDownloads } from "../stores/downloads";
import { LEGENDS_ID, useMasters } from "./store";

interface Props {
  players: MasterPlayer[];
  downloading: Record<string, string | null>;
  onDownload: (id: string) => void;
  onRemove: (id: string) => void;
  onImport: () => void;
}

export function PlayerList({ players, downloading, onDownload, onRemove, onImport }: Props) {
  const { selected, select } = useMasters();
  const [query, setQuery] = useState("");
  const progress = useDownloads((s) => s.progress);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = q ? players.filter((p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q)) : players;
    // Players already in the library first, then by name.
    return [...list].sort((a, b) => Number(b.gamesInLibrary > 0) - Number(a.gamesInLibrary > 0) || a.name.localeCompare(b.name));
  }, [players, query]);

  return (
    <div className="panel masters-list">
      <Stack gap={8} p={10}>
        <TextInput
          size="xs"
          leftSection={<IconSearch size={14} />}
          placeholder="Find a player"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
          aria-label="Find a player"
        />
        <Button size="xs" variant="default" leftSection={<IconFileImport size={14} />} onClick={onImport}>
          Import PGN as master games
        </Button>
      </Stack>
      <ScrollArea style={{ flex: 1 }} type="auto">
        <UnstyledButton className="masters-item" data-active={selected === LEGENDS_ID} onClick={() => select(LEGENDS_ID)} aria-label="Legends">
          <Group gap={8} wrap="nowrap">
            <IconStar size={16} />
            <div>
              <Text size="sm" fw={600}>
                Legends
              </Text>
              <Text size="xs" c="dimmed">
                Famous games, ready to study
              </Text>
            </div>
          </Group>
        </UnstyledButton>
        {shown.map((p) => {
          const job = `masters:${p.id}`;
          const phase = downloading[p.id];
          const busy = phase !== undefined;
          return (
            <div key={p.id} className="masters-item" data-active={selected === p.id}>
              <Group gap={6} wrap="nowrap" justify="space-between">
                <UnstyledButton style={{ flex: 1, minWidth: 0 }} onClick={() => select(p.id)} aria-label={p.name}>
                  <Text size="sm" fw={selected === p.id ? 600 : 500} truncate>
                    {p.name}
                  </Text>
                  <Group gap={6}>
                    <Text size="xs" c="dimmed">
                      {p.gamesAvailable.toLocaleString()} games
                    </Text>
                    {p.gamesInLibrary > 0 && (
                      <Badge size="xs" variant="light" color="teal">
                        Downloaded · {p.gamesInLibrary.toLocaleString()}
                      </Badge>
                    )}
                  </Group>
                </UnstyledButton>
                {p.gamesInLibrary === 0 ? (
                  <ActionIcon variant="subtle" size="sm" aria-label={`Download ${p.name}`} loading={busy} onClick={() => onDownload(p.id)}>
                    <IconDownload size={14} />
                  </ActionIcon>
                ) : (
                  <Menu position="bottom-end" withinPortal>
                    <Menu.Target>
                      <ActionIcon variant="subtle" size="sm" aria-label={`More for ${p.name}`}>
                        <IconDots size={14} />
                      </ActionIcon>
                    </Menu.Target>
                    <Menu.Dropdown>
                      <Menu.Item color="red" leftSection={<IconTrash size={14} />} onClick={() => onRemove(p.id)}>
                        Remove from the library
                      </Menu.Item>
                    </Menu.Dropdown>
                  </Menu>
                )}
              </Group>
              {busy && (
                <Stack gap={2} mt={4}>
                  <Progress size="xs" value={phase ? 100 : downloadFraction(progress[job]) * 100} animated={!!phase} />
                  {phase && (
                    <Text size="xs" c="dimmed">
                      {phase}
                    </Text>
                  )}
                </Stack>
              )}
            </div>
          );
        })}
      </ScrollArea>
    </div>
  );
}
