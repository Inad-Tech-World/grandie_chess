import { useEffect, useState } from "react";
import { Alert, Group, Loader, SegmentedControl, Text, Tooltip } from "@mantine/core";
import { commands, type ExplorerResult, type Side } from "../lib/api";

export type ExplorerSource = "mine" | "database" | "lichess" | "masters";

interface Props {
  fen: string;
  /** Colour the user plays in this context (filters "My games"). */
  color: Side;
  onPlay: (uci: string) => void;
  source: ExplorerSource;
  onSourceChange: (s: ExplorerSource) => void;
}

function ResultBar({ white, draws, black }: { white: number; draws: number; black: number }) {
  const total = white + draws + black || 1;
  const pct = (n: number) => (n / total) * 100;
  const label = (n: number) => (pct(n) >= 14 ? `${Math.round(pct(n))}%` : "");
  return (
    <div
      style={{ display: "flex", height: 16, borderRadius: 4, overflow: "hidden", fontSize: 10, fontWeight: 600, lineHeight: "16px" }}
      aria-label={`White ${Math.round(pct(white))}%, draws ${Math.round(pct(draws))}%, Black ${Math.round(pct(black))}%`}
    >
      <div style={{ width: `${pct(white)}%`, background: "var(--eval-white)", color: "#333", textAlign: "center" }}>{label(white)}</div>
      <div style={{ width: `${pct(draws)}%`, background: "var(--text-dim)", color: "#fff", textAlign: "center" }}>{label(draws)}</div>
      <div style={{ width: `${pct(black)}%`, background: "var(--eval-black)", color: "#eee", textAlign: "center" }}>{label(black)}</div>
    </div>
  );
}

export function ExplorerPanel({ fen, color, onPlay, source, onSourceChange }: Props) {
  const [result, setResult] = useState<ExplorerResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    const t = setTimeout(async () => {
      const r =
        source === "lichess" || source === "masters"
          ? await commands.explorerLichess(source, fen, ["blitz", "rapid", "classical"], [1600, 1800, 2000, 2200, 2500])
          : await commands.explorerDb(fen, { onlyMine: source === "mine", color: source === "mine" ? color : null });
      if (!live) return;
      setLoading(false);
      if (r.status === "ok") setResult(r.data);
      else {
        setResult(null);
        setError(r.error);
      }
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [fen, source, color]);

  return (
    <div className="panel" style={{ padding: 12 }} aria-label="Opening explorer">
      <Group justify="space-between" mb={8}>
        <Text fw={600} size="sm">
          Explorer
        </Text>
        <SegmentedControl
          size="xs"
          value={source}
          onChange={(v) => onSourceChange(v as ExplorerSource)}
          data={[
            { value: "mine", label: "My games" },
            { value: "database", label: "Database" },
            { value: "lichess", label: "Lichess" },
            { value: "masters", label: "Masters" },
          ]}
        />
      </Group>
      {result?.opening && (
        <Text size="sm" mb={6}>
          <span className="muted">{result.opening.eco}</span> {result.opening.name}
        </Text>
      )}
      {error && (
        <Alert color="yellow" variant="light" p="xs">
          <Text size="xs">{error}</Text>
        </Alert>
      )}
      {loading && !result && <Loader size="sm" />}
      {result && result.moves.length === 0 && !error && (
        <Text size="xs" c="dimmed">
          No games reached this position{source === "mine" ? " in your games" : ""}.
        </Text>
      )}
      {result && result.moves.length > 0 && (
        <div style={{ opacity: loading ? 0.6 : 1 }}>
          {result.moves.map((m) => (
            <div
              key={m.uci}
              className="engine-line"
              role="button"
              aria-label={`Explorer move ${m.san}`}
              onClick={() => onPlay(m.uci)}
              style={{ display: "grid", gridTemplateColumns: "52px 56px 1fr 44px", alignItems: "center", gap: 8 }}
            >
              <span style={{ fontWeight: 650 }}>{m.san}</span>
              <span className="tabular muted" style={{ textAlign: "right" }}>
                {Intl.NumberFormat("en", { notation: "compact" }).format(m.games)}
              </span>
              <ResultBar white={m.white} draws={m.draws} black={m.black} />
              <Tooltip label="Average rating of players who chose this move">
                <span className="tabular muted" style={{ textAlign: "right" }}>
                  {m.avgRating ? Math.round(m.avgRating) : ""}
                </span>
              </Tooltip>
            </div>
          ))}
          <Text size="xs" c="dimmed" mt={6}>
            {result.total.toLocaleString()} games
          </Text>
        </div>
      )}
    </div>
  );
}
