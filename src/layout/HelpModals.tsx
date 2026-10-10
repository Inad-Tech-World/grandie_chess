import { Anchor, Group, Image, Kbd, Modal, Stack, Table, Text } from "@mantine/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { create } from "zustand";

type HelpModal = "shortcuts" | "credits" | null;

export const useHelp = create<{ open: HelpModal; show: (m: HelpModal) => void }>()((set) => ({
  open: null,
  show: (open) => set({ open }),
}));

const SHORTCUTS: [string[], string][] = [
  [["⌘", "N"], "New game"],
  [["⌘", "⇧", "N"], "New analysis board"],
  [["⌘", "O"], "Open PGN"],
  [["⌘", "S"], "Save the current game as PGN"],
  [["⌘", "W"], "Close the current tab"],
  [["⌘", ","], "Settings"],
  [["⌘", "⇧", "C"], "Copy the position (FEN)"],
  [["⌘", "⇧", "V"], "Paste a FEN or PGN"],
  [["←", "→"], "Previous / next move"],
  [["↑", "↓"], "Start / end of the line"],
  [["Space"], "Start or stop the engine (analysis)"],
  [["F"], "Flip the board"],
  [["Enter"], "Next puzzle"],
];

const LINKS: [string, string][] = [
  ["Leela Chess Zero (GPL-3.0)", "https://lczero.org"],
  ["Maia Chess (GPL-3.0)", "https://maiachess.com"],
  ["chessground, shakmaty, pgn-reader (GPL-3.0)", "https://github.com/lichess-org/chessground"],
  ["Lichess puzzles and opening names (CC0)", "https://database.lichess.org"],
  ["Piece sets from Lichess (GPL, Apache, MIT, CC0)", "https://github.com/lichess-org/lila/blob/master/COPYING.md"],
  ["Tauri, React, Mantine, Recharts (MIT)", "https://tauri.app"],
  ["Inspired by En Croissant (GPL-3.0)", "https://github.com/franciscoBSalgueiro/en-croissant"],
];

export function HelpModals() {
  const { open, show } = useHelp();
  return (
    <>
      <Modal opened={open === "shortcuts"} onClose={() => show(null)} title="Keyboard shortcuts" centered>
        <Table verticalSpacing={6}>
          <Table.Tbody>
            {SHORTCUTS.map(([keys, label]) => (
              <Table.Tr key={label}>
                <Table.Td w={140}>
                  <Group gap={4}>
                    {keys.map((k) => (
                      <Kbd key={k}>{k}</Kbd>
                    ))}
                  </Group>
                </Table.Td>
                <Table.Td>{label}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Modal>
      <Modal opened={open === "credits"} onClose={() => show(null)} title="About Grandie Chess" centered>
        <Stack align="center" gap="xs" mb="md">
          <Image src="/grandie-logo.jpg" alt="" w={90} h={135} fit="cover" radius="md" />
          <Text fw={650}>Grandie Chess</Text>
          <Text size="sm" c="dimmed" ta="center">
            Free software under the GNU General Public License v3.0 or later.
          </Text>
          <Anchor size="sm" component="button" onClick={() => void openUrl("https://github.com/Inad-Tech-World/grandie_chess")}>
            Source code on GitHub
          </Anchor>
        </Stack>
        <Text size="sm" fw={600} mb={4}>
          Built with
        </Text>
        <Stack gap={2}>
          {LINKS.map(([label, url]) => (
            <Anchor key={label} size="sm" component="button" ta="left" onClick={() => void openUrl(url)}>
              {label}
            </Anchor>
          ))}
        </Stack>
        <Text size="xs" c="dimmed" mt="md">
          The full list of licences is in CREDITS.md in the source code.
        </Text>
      </Modal>
    </>
  );
}
