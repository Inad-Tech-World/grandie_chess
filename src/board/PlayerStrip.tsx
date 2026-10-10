import { Group, Loader, Text } from "@mantine/core";
import { IconCpu, IconUser } from "@tabler/icons-react";
import { formatClock } from "../lib/format";

interface PlayerStripProps {
  name: string;
  rating?: number | null;
  isEngine?: boolean;
  thinking?: boolean;
  clockMs?: number | null;
  clockRunning?: boolean;
  width: number;
}

export function PlayerStrip({ name, rating, isEngine, thinking, clockMs, clockRunning, width }: PlayerStripProps) {
  const low = clockMs !== undefined && clockMs !== null && clockMs < 20_000;
  return (
    <div className="player-strip" style={{ width }}>
      <Group gap={8} wrap="nowrap">
        {isEngine ? <IconCpu size={18} stroke={1.6} /> : <IconUser size={18} stroke={1.6} />}
        <span className="player-name">{name}</span>
        {rating ? <span className="player-meta">≈{rating}</span> : null}
        {thinking && <Loader size={12} type="dots" />}
      </Group>
      {clockMs !== undefined && clockMs !== null ? (
        <div className="clock" data-running={clockRunning} data-low={low && clockRunning} aria-label={`${name} clock`}>
          {formatClock(clockMs)}
        </div>
      ) : (
        <Text size="xs" c="dimmed">
          {isEngine ? "Engine" : ""}
        </Text>
      )}
    </div>
  );
}
