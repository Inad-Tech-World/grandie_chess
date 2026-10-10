import { Button, Group, Image, Stack, Text, Title } from "@mantine/core";
import { IconMicroscope, IconPlayerPlay } from "@tabler/icons-react";
import { Sidebar } from "./Sidebar";
import { TabBar } from "./TabBar";
import { useApp } from "../stores/app";
import { openAnalysis, openNewGame } from "../lib/actions";
import { NewGame } from "../screens/NewGame";
import { PlayScreen } from "../screens/PlayScreen";
import { AnalysisScreen } from "../screens/AnalysisScreen";
import { EnginesScreen } from "../screens/EnginesScreen";
import { SettingsScreen } from "../screens/SettingsScreen";
import { ComingSoon } from "../screens/ComingSoon";

function Welcome() {
  return (
    <Stack align="center" justify="center" h="100%" gap="sm">
      <Image src="/grandie-logo.jpg" alt="" w={120} h={180} fit="cover" radius="lg" />
      <Title order={2}>Grandie Chess</Title>
      <Text c="dimmed">Play, review and train with Leela.</Text>
      <Group mt="md">
        <Button leftSection={<IconPlayerPlay size={16} />} onClick={openNewGame}>
          New game
        </Button>
        <Button variant="default" leftSection={<IconMicroscope size={16} />} onClick={() => openAnalysis()}>
          Analysis board
        </Button>
      </Group>
    </Stack>
  );
}

function Boards() {
  const { tabs, activeTabId } = useApp();
  const tab = tabs.find((t) => t.id === activeTabId);
  if (!tab) return <Welcome />;
  switch (tab.kind) {
    case "new-game":
      return <NewGame key={tab.id} tabId={tab.id} />;
    case "play":
      return <PlayScreen key={tab.id} sessionId={tab.sessionId} />;
    case "analysis":
      return <AnalysisScreen key={tab.id} analysisId={tab.analysisId} tabId={tab.id} />;
  }
}

export function Shell() {
  const section = useApp((s) => s.section);
  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TabBar />
        <div className="content">
          {section === "boards" && <Boards />}
          {section === "engines" && <EnginesScreen />}
          {section === "settings" && <SettingsScreen />}
          {section === "database" && <ComingSoon title="Database" phase={3} />}
          {section === "openings" && <ComingSoon title="Openings" phase={4} />}
          {section === "puzzles" && <ComingSoon title="Puzzles" phase={5} />}
          {section === "progress" && <ComingSoon title="Progress" phase={5} />}
        </div>
      </main>
    </div>
  );
}
