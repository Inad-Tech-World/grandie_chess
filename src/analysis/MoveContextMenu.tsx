import { Button, Group, Paper, Stack } from "@mantine/core";
import { IconX } from "@tabler/icons-react";

const NAGS: { nag: number; label: string }[] = [
  { nag: 1, label: "!" },
  { nag: 2, label: "?" },
  { nag: 3, label: "!!" },
  { nag: 4, label: "??" },
  { nag: 5, label: "!?" },
  { nag: 6, label: "?!" },
];

interface Props {
  x: number;
  y: number;
  /** The move's current annotations. */
  nags: number[];
  onToggleNag: (nag: number) => void;
  onComment: () => void;
  onPromote: () => void;
  onDelete: () => void;
  onClose: () => void;
}

/** Right-click menu on a move in the tree: annotate, comment, promote, delete. */
export function MoveContextMenu({ x, y, nags, onToggleNag, onComment, onPromote, onDelete, onClose }: Props) {
  const then = (action: () => void) => () => {
    action();
    onClose();
  };
  return (
    <>
      <div
        style={{ position: "fixed", inset: 0, zIndex: 99 }}
        onClick={onClose}
        onContextMenu={(e) => {
          e.preventDefault();
          onClose();
        }}
      />
      <Paper shadow="md" withBorder p={6} style={{ position: "fixed", left: x, top: y, zIndex: 100, minWidth: 190 }} role="menu">
        <Stack gap={2}>
          <Group gap={2} mb={4}>
            {NAGS.map((n) => (
              <Button
                key={n.nag}
                size="compact-xs"
                variant={nags.includes(n.nag) ? "filled" : "default"}
                onClick={() => onToggleNag(n.nag)}
                aria-label={`Annotate ${n.label}`}
              >
                {n.label}
              </Button>
            ))}
          </Group>
          <Button size="xs" variant="subtle" justify="flex-start" onClick={then(onComment)}>
            Comment…
          </Button>
          <Button size="xs" variant="subtle" justify="flex-start" onClick={then(onPromote)}>
            Promote to main line
          </Button>
          <Button size="xs" variant="subtle" color="red" justify="flex-start" leftSection={<IconX size={14} />} onClick={then(onDelete)}>
            Delete from here
          </Button>
        </Stack>
      </Paper>
    </>
  );
}
