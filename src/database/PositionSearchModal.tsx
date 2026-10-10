import { useState } from "react";
import { Button, Group, Modal, SegmentedControl, Stack, Text, TextInput } from "@mantine/core";
import { BoardEditor, fullFen } from "../board/BoardEditor";
import type { PositionQuery, Side } from "../bindings";
import { activeFen } from "../lib/actions";

const START_BOARD = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

interface Props {
  opened: boolean;
  onClose: () => void;
  onSearch: (q: PositionQuery) => void;
}

/** Set up a position to search for, exactly or as a pattern of pieces. */
export function PositionSearchModal({ opened, onClose, onSearch }: Props) {
  const [initial, setInitial] = useState(START_BOARD);
  const [board, setBoard] = useState(START_BOARD);
  const [turn, setTurn] = useState<Side>("white");
  const [mode, setMode] = useState<"exact" | "partial">("exact");
  const [fenInput, setFenInput] = useState("");

  function load(fen: string) {
    const [placement, side] = fen.trim().split(/\s+/);
    if (!placement) return;
    setInitial(placement);
    setBoard(placement);
    if (side === "b") setTurn("black");
    if (side === "w") setTurn("white");
  }

  const current = activeFen();
  const fen = fullFen(board, turn);

  return (
    <Modal opened={opened} onClose={onClose} title="Search by position" size="auto" centered>
      <Group align="flex-start" gap="lg" wrap="nowrap">
        <BoardEditor initialBoard={initial} size={360} turn={turn} onTurnChange={setTurn} onChange={setBoard} />
        <Stack w={260} gap="sm">
          <SegmentedControl
            value={mode}
            onChange={(v) => setMode(v as "exact" | "partial")}
            data={[
              { value: "exact", label: "Exact position" },
              { value: "partial", label: "Pieces pattern" },
            ]}
          />
          <Text size="xs" c="dimmed">
            {mode === "exact"
              ? "Games that reached exactly this position, with this side to move."
              : "Games where all these pieces stood on these squares at the same moment. Empty squares can hold anything."}
          </Text>
          <Group gap={6}>
            <Button size="xs" variant="default" onClick={() => load(START_BOARD)}>
              Start position
            </Button>
            <Button size="xs" variant="default" onClick={() => load("8/8/8/8/8/8/8/8")}>
              Empty board
            </Button>
            {current && (
              <Button size="xs" variant="default" onClick={() => load(current)}>
                Current board
              </Button>
            )}
          </Group>
          <TextInput
            size="xs"
            label="Paste a FEN"
            value={fenInput}
            onChange={(e) => setFenInput(e.currentTarget.value)}
            onBlur={() => fenInput && load(fenInput)}
            spellCheck={false}
          />
          <Text size="xs" c="dimmed" style={{ wordBreak: "break-all" }} ff="monospace">
            {fen}
          </Text>
          <Button
            onClick={() => {
              onSearch({ fen: mode === "exact" ? fen : board, exact: mode === "exact" });
              onClose();
            }}
          >
            Search
          </Button>
        </Stack>
      </Group>
    </Modal>
  );
}
