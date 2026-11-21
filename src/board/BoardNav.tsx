import { ActionIcon, Tooltip } from "@mantine/core";
import { useHotkeys, type HotkeyItem } from "@mantine/hooks";
import { IconChevronLeft, IconChevronRight, IconChevronsLeft, IconChevronsRight, IconSwitchVertical } from "@tabler/icons-react";

/** Moving through a game on a board. */
export interface BoardNav {
  start: () => void;
  back: () => void;
  forward: () => void;
  end: () => void;
  /** Omitted where the board's side is fixed. */
  flip?: () => void;
}

export interface BoardNavLabels {
  start: string;
  back: string;
  forward: string;
  end?: string;
}

/** The arrow-key, Home/End and F shortcuts every board screen shares. */
export function useBoardNav(nav: BoardNav, extra: HotkeyItem[] = []) {
  const keys: HotkeyItem[] = [
    ["ArrowLeft", nav.back],
    ["ArrowRight", nav.forward],
    ["ArrowUp", nav.start],
    ["ArrowDown", nav.end],
    ["Home", nav.start],
    ["End", nav.end],
    ...(nav.flip ? ([["f", nav.flip]] as HotkeyItem[]) : []),
    ...extra,
  ];
  useHotkeys(keys);
}

/** Start / back / forward / end buttons, plus a flip button when the board
 *  can be flipped. The caller lays them out. */
export function BoardNavButtons({ nav, labels }: { nav: BoardNav; labels: BoardNavLabels }) {
  return (
    <>
      <ActionIcon variant="subtle" color="gray" onClick={nav.start} aria-label={labels.start}>
        <IconChevronsLeft size={18} />
      </ActionIcon>
      <ActionIcon variant="subtle" color="gray" onClick={nav.back} aria-label={labels.back}>
        <IconChevronLeft size={18} />
      </ActionIcon>
      <ActionIcon variant="subtle" color="gray" onClick={nav.forward} aria-label={labels.forward}>
        <IconChevronRight size={18} />
      </ActionIcon>
      {labels.end && (
        <ActionIcon variant="subtle" color="gray" onClick={nav.end} aria-label={labels.end}>
          <IconChevronsRight size={18} />
        </ActionIcon>
      )}
      {nav.flip && (
        <Tooltip label="Flip board (F)">
          <ActionIcon variant="subtle" color="gray" onClick={nav.flip} aria-label="Flip board">
            <IconSwitchVertical size={18} />
          </ActionIcon>
        </Tooltip>
      )}
    </>
  );
}
