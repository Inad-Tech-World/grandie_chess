import { Stack, Text, Title } from "@mantine/core";

/** Temporary placeholder for screens built in later phases. */
export function ComingSoon({ title, phase }: { title: string; phase: number }) {
  return (
    <Stack align="center" justify="center" h="100%" gap={4}>
      <Title order={3}>{title}</Title>
      <Text c="dimmed">Arrives in phase {phase}.</Text>
    </Stack>
  );
}
