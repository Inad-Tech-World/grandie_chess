import { ActionIcon, CloseButton, Tooltip } from "@mantine/core";
import { IconChessKnight, IconMicroscope, IconPlayerPlay, IconPlus } from "@tabler/icons-react";
import { closeTab, openNewGame } from "../lib/actions";
import { useApp, type Tab } from "../stores/app";
import { usePlay } from "../stores/play";

const icon = (tab: Tab) =>
  tab.kind === "analysis" ? <IconMicroscope size={14} /> : tab.kind === "play" ? <IconChessKnight size={14} /> : <IconPlayerPlay size={14} />;

function LiveDot({ sessionId }: { sessionId: number }) {
  const live = usePlay((s) => {
    const snap = s.snapshots[sessionId];
    return !!snap && !snap.outcome;
  });
  return live ? <span aria-label="Game in progress" style={{ width: 6, height: 6, borderRadius: 3, background: "var(--good)" }} /> : null;
}

export function TabBar() {
  const { tabs, activeTabId, activate, section } = useApp();
  return (
    <div className="titlebar" data-tauri-drag-region role="tablist" aria-label="Open boards">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className="tab"
          role="tab"
          aria-selected={section === "boards" && tab.id === activeTabId}
          data-active={section === "boards" && tab.id === activeTabId}
          onClick={() => activate(tab.id)}
          onAuxClick={(e) => e.button === 1 && void closeTab(tab.id)}
        >
          {icon(tab)}
          <span className="tab-title">{tab.title}</span>
          {tab.kind === "play" && <LiveDot sessionId={tab.sessionId} />}
          <CloseButton
            size="xs"
            aria-label={`Close ${tab.title}`}
            onClick={(e) => {
              e.stopPropagation();
              void closeTab(tab.id);
            }}
          />
        </div>
      ))}
      <Tooltip label="New game (⌘N)">
        <ActionIcon variant="subtle" color="gray" size="sm" mb={5} ml={4} onClick={openNewGame} aria-label="New game">
          <IconPlus size={15} />
        </ActionIcon>
      </Tooltip>
      <div style={{ flex: 1, alignSelf: "stretch" }} data-tauri-drag-region />
    </div>
  );
}
