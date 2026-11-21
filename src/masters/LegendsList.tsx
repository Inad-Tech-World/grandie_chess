// The bundled famous games: the Immortal Game, the Opera Game, the Game of
// the Century and friends, each ready for a guided replay.

import { useEffect, useState } from "react";
import { Alert, Badge, Button, Card, Group, Loader, SimpleGrid, Text, Title } from "@mantine/core";
import { IconEye, IconQuestionMark, IconSchool } from "@tabler/icons-react";
import { commands, type LegendGame } from "../lib/api";
import { openDbGame } from "../lib/actions";
import { gameTitle, useMasters, yearOf } from "./store";

/** The side whose moves are worth following: the winner, else White. */
function heroOf(g: LegendGame["game"]): { name: string; color: "white" | "black" } {
  if (g.result === "0-1") return { name: g.black ?? "Black", color: "black" };
  return { name: g.white ?? "White", color: "white" };
}

export function LegendsList() {
  const openStudy = useMasters((s) => s.openStudy);
  const [legends, setLegends] = useState<LegendGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void commands.mastersLegends().then((r) => {
      if (!live) return;
      if (r.status === "ok") setLegends(r.data);
      else setError(r.error.message);
    });
    return () => {
      live = false;
    };
  }, []);

  function study(mode: "replay" | "guess", l: LegendGame) {
    const hero = heroOf(l.game);
    openStudy({ mode, gameId: l.game.id, subject: null, masterName: hero.name, masterColor: hero.color, title: l.title });
  }

  return (
    <div className="screen" style={{ paddingTop: 8 }}>
      <Title order={2} mb={4}>
        Legends
      </Title>
      <Text c="dimmed" mb="md">
        Famous games every chess player should know. Let the coach replay one and stop at the moments that made it famous, or play the winner's side and guess
        each move.
      </Text>
      {error && (
        <Alert color="red" variant="light" mb="sm">
          {error}
        </Alert>
      )}
      {!legends && !error && <Loader size="sm" />}
      <SimpleGrid cols={{ base: 1, md: 2 }} spacing="sm">
        {legends?.map((l) => (
          <Card key={l.game.id} withBorder radius="md" padding="md" aria-label={l.title}>
            <Group justify="space-between" align="flex-start" wrap="nowrap" mb={4}>
              <Text fw={650}>{l.title}</Text>
              <Badge variant="light" color="gray" style={{ flexShrink: 0 }}>
                {yearOf(l.game.date) || "?"}
              </Badge>
            </Group>
            <Text size="sm" c="dimmed" mb={6}>
              {gameTitle(l.game.white, l.game.black, null)} · {l.game.result}
              {l.game.opening ? ` · ${l.game.opening}` : ""}
            </Text>
            <Text size="sm" mb="sm">
              {l.why}
            </Text>
            <Group gap={6}>
              <Button size="xs" color="teal" leftSection={<IconSchool size={14} />} onClick={() => study("replay", l)}>
                Guided replay
              </Button>
              <Button size="xs" variant="light" leftSection={<IconQuestionMark size={14} />} onClick={() => study("guess", l)}>
                Guess the move
              </Button>
              <Button size="xs" variant="subtle" leftSection={<IconEye size={14} />} onClick={() => void openDbGame(l.game.id)}>
                Open in analysis
              </Button>
            </Group>
          </Card>
        ))}
      </SimpleGrid>
    </div>
  );
}
