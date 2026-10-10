import { Button, Group, Progress, SegmentedControl, SimpleGrid, Stack, Text } from "@mantine/core";
import { IconArrowForwardUp, IconBulb, IconRepeat, IconSparkles } from "@tabler/icons-react";
import { useState } from "react";
import type { MoveClass, MoveReview, ReviewDepth, ReviewResult, SideSummary } from "../bindings";
import { CLASS_STYLE } from "../lib/score";

interface ReviewPanelProps {
  review: ReviewResult | undefined;
  progress: { done: number; total: number } | null;
  whiteName: string;
  blackName: string;
  /** Review of the move leading to the current position, if any. */
  currentMove: MoveReview | null;
  onStart: (depth: ReviewDepth) => void;
  onCancel: () => void;
  onJump: (ply: number) => void;
  onShowBetter: (move: MoveReview) => void;
  onRetry: (move: MoveReview) => void;
}

const ROWS: { cls: MoveClass; key: keyof SideSummary }[] = [
  { cls: "brilliant", key: "brilliant" },
  { cls: "great", key: "great" },
  { cls: "best", key: "best" },
  { cls: "inaccuracy", key: "inaccuracies" },
  { cls: "mistake", key: "mistakes" },
  { cls: "blunder", key: "blunders" },
];

function moveLabel(m: MoveReview): string {
  const n = Math.ceil(m.ply / 2);
  return `${n}${m.side === "white" ? "." : "…"} ${m.san}`;
}

export function ReviewPanel(props: ReviewPanelProps) {
  const { review, progress, currentMove } = props;
  const [depth, setDepth] = useState<ReviewDepth>("standard");

  if (!review) {
    return (
      <div className="panel" style={{ padding: 14 }}>
        <Group gap={8} mb={6}>
          <IconSparkles size={18} />
          <Text fw={600}>Game review</Text>
        </Group>
        {progress ? (
          <>
            <Text size="sm" c="dimmed" mb={6}>
              Leela is checking every move… {progress.done}/{progress.total}
            </Text>
            <Progress value={(progress.done / Math.max(1, progress.total)) * 100} mb="sm" animated />
            <Button size="xs" variant="default" onClick={props.onCancel}>
              Cancel
            </Button>
          </>
        ) : (
          <>
            <Text size="sm" c="dimmed" mb="sm">
              Find the mistakes, the better moves and the turning points of the main line.
            </Text>
            <Group justify="space-between">
              <SegmentedControl
                size="xs"
                value={depth}
                onChange={(v) => setDepth(v as ReviewDepth)}
                data={[
                  { value: "quick", label: "Quick" },
                  { value: "standard", label: "Standard" },
                  { value: "deep", label: "Deep" },
                ]}
              />
              <Button size="xs" onClick={() => props.onStart(depth)}>
                Review game
              </Button>
            </Group>
          </>
        )}
      </div>
    );
  }

  const keyMoves = review.keyMoments.map((ply) => review.moves[ply - 1]).filter((m): m is MoveReview => !!m);

  return (
    <Stack gap={10}>
      <div className="panel" style={{ padding: 14 }}>
        <SimpleGrid cols={2} spacing={10} mb={10}>
          {(["white", "black"] as const).map((side) => {
            const s = review[side];
            return (
              <div key={side}>
                <Text size="xs" c="dimmed" truncate>
                  {side === "white" ? props.whiteName : props.blackName}
                </Text>
                <Text fw={700} size="xl" className="tabular" aria-label={`${side} accuracy`}>
                  {s.accuracy !== null && s.accuracy !== undefined ? `${s.accuracy.toFixed(1)}%` : "–"}
                </Text>
                <Text size="xs" c="dimmed">
                  accuracy
                </Text>
              </div>
            );
          })}
        </SimpleGrid>
        {ROWS.map(({ cls, key }) => (
          <Group key={cls} justify="space-between" gap={4} wrap="nowrap" style={{ fontSize: 13 }}>
            <Text size="sm" className="tabular" w={24} ta="left">
              {review.white[key] as number}
            </Text>
            <Group gap={6} justify="center" style={{ flex: 1 }}>
              <span className="annot" style={{ color: CLASS_STYLE[cls].color, width: 18, textAlign: "center" }}>
                {CLASS_STYLE[cls].symbol}
              </span>
              <Text size="sm">{CLASS_STYLE[cls].label}</Text>
            </Group>
            <Text size="sm" className="tabular" w={24} ta="right">
              {review.black[key] as number}
            </Text>
          </Group>
        ))}
      </div>

      {currentMove && (
        <div className="panel" style={{ padding: 12 }} aria-label="Move feedback">
          <Text size="sm" fw={600}>
            <span className="annot" style={{ color: CLASS_STYLE[currentMove.class].color }}>
              {CLASS_STYLE[currentMove.class].symbol}
            </span>{" "}
            {moveLabel(currentMove)} is {currentMove.class === "inaccuracy" ? "an" : "a"} {CLASS_STYLE[currentMove.class].label.toLowerCase()}
          </Text>
          <Text size="xs" c="dimmed" mt={2}>
            Winning chances {Math.round(currentMove.winBefore ?? 0)}% → {Math.round(currentMove.winAfter ?? 0)}%
            {currentMove.bestSan && currentMove.best !== currentMove.uci ? ` · best was ${currentMove.bestSan}` : ""}
          </Text>
          {currentMove.best && currentMove.best !== currentMove.uci && (
            <Group gap={6} mt={8}>
              <Button size="xs" variant="light" leftSection={<IconBulb size={14} />} onClick={() => props.onShowBetter(currentMove)}>
                Show better move
              </Button>
              <Button size="xs" variant="default" leftSection={<IconRepeat size={14} />} onClick={() => props.onRetry(currentMove)}>
                Retry this position
              </Button>
            </Group>
          )}
        </div>
      )}

      {keyMoves.length > 0 && (
        <div className="panel" style={{ padding: 12 }}>
          <Text size="sm" fw={600} mb={4}>
            Key moments
          </Text>
          {keyMoves.map((m) => (
            <Group
              key={m.ply}
              gap={6}
              className="engine-line"
              onClick={() => props.onJump(m.ply)}
              role="button"
              aria-label={`Key moment ${moveLabel(m)}`}
            >
              <span className="annot" style={{ color: CLASS_STYLE[m.class].color, width: 18 }}>
                {CLASS_STYLE[m.class].symbol}
              </span>
              <Text size="sm" fw={500}>
                {moveLabel(m)}
              </Text>
              <Text size="xs" c="dimmed">
                {m.class === "brilliant" ? "brilliant" : `−${Math.round((m.winBefore ?? 0) - (m.winAfter ?? 0))}%`}
                {m.bestSan && m.class !== "brilliant" ? ` · ${m.bestSan} was better` : ""}
              </Text>
              <IconArrowForwardUp size={14} style={{ marginLeft: "auto", opacity: 0.5 }} />
            </Group>
          ))}
        </div>
      )}
    </Stack>
  );
}
