import { Tooltip } from "@mantine/core";
import {
  IconBook2,
  IconChartLine,
  IconCpu,
  IconDatabase,
  IconMicroscope,
  IconPlayerPlay,
  IconPuzzle,
  IconSettings,
} from "@tabler/icons-react";
import type { ReactNode } from "react";
import { openAnalysis, openNewGame } from "../lib/actions";
import { useApp, type Section } from "../stores/app";

interface NavItem {
  label: string;
  icon: ReactNode;
  active: boolean;
  onClick: () => void;
  hint: string;
}

export function Sidebar() {
  const { section, tabs, activeTabId, setSection, activate } = useApp();
  const active = tabs.find((t) => t.id === activeTabId);
  const onBoards = section === "boards";

  const goBoards = (kind: "play" | "analysis") => {
    const match = kind === "play" ? tabs.find((t) => t.kind === "play" || t.kind === "new-game") : tabs.find((t) => t.kind === "analysis");
    if (onBoards && active && (kind === "play" ? active.kind !== "analysis" : active.kind === "analysis")) {
      // Already here: a second click opens something new.
      if (kind === "play") openNewGame();
      else openAnalysis();
      return;
    }
    if (match) activate(match.id);
    else if (kind === "play") openNewGame();
    else openAnalysis();
  };

  const screen = (s: Section) => () => setSection(s);
  const items: NavItem[] = [
    {
      label: "Play",
      icon: <IconPlayerPlay size={22} stroke={1.7} />,
      active: onBoards && active?.kind !== "analysis",
      onClick: () => goBoards("play"),
      hint: "Play Leela or Maia (⌘N)",
    },
    {
      label: "Analyse",
      icon: <IconMicroscope size={22} stroke={1.7} />,
      active: onBoards && active?.kind === "analysis",
      onClick: () => goBoards("analysis"),
      hint: "Analysis board",
    },
    { label: "Database", icon: <IconDatabase size={22} stroke={1.7} />, active: section === "database", onClick: screen("database"), hint: "Your games" },
    { label: "Openings", icon: <IconBook2 size={22} stroke={1.7} />, active: section === "openings", onClick: screen("openings"), hint: "Opening explorer and repertoire" },
    { label: "Puzzles", icon: <IconPuzzle size={22} stroke={1.7} />, active: section === "puzzles", onClick: screen("puzzles"), hint: "Tactics training" },
    { label: "Progress", icon: <IconChartLine size={22} stroke={1.7} />, active: section === "progress", onClick: screen("progress"), hint: "Your improvement over time" },
    { label: "Engines", icon: <IconCpu size={22} stroke={1.7} />, active: section === "engines", onClick: screen("engines"), hint: "Engines and networks" },
  ];

  const button = (item: NavItem) => (
    <Tooltip key={item.label} label={item.hint} position="right">
      <button className="nav-item" data-active={item.active} onClick={item.onClick} aria-current={item.active ? "page" : undefined}>
        {item.icon}
        {item.label}
      </button>
    </Tooltip>
  );

  return (
    <nav className="sidebar" aria-label="Main">
      <img className="sidebar-logo" src="/grandie-logo.jpg" alt="Grandie Chess" data-tauri-drag-region />
      {items.map(button)}
      <div className="sidebar-spacer" data-tauri-drag-region />
      {button({
        label: "Settings",
        icon: <IconSettings size={22} stroke={1.7} />,
        active: section === "settings",
        onClick: screen("settings"),
        hint: "Settings (⌘,)",
      })}
    </nav>
  );
}
