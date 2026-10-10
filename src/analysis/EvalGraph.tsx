import { Area, AreaChart, ReferenceLine, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { ReviewResult } from "../bindings";
import { CLASS_STYLE, formatScore } from "../lib/score";

interface EvalGraphProps {
  review: ReviewResult;
  /** Ply of the position shown on the board. */
  currentPly: number | null;
  onSelectPly: (ply: number) => void;
}

interface Point {
  ply: number;
  win: number;
  label: string;
  score: string;
  cls?: keyof typeof CLASS_STYLE;
}

/** White's winning chances across the game; click a point to jump there. */
export function EvalGraph({ review, currentPly, onSelectPly }: EvalGraphProps) {
  const data: Point[] = review.evals.map((e) => {
    const move = review.moves[e.ply - 1];
    const number = Math.ceil(e.ply / 2);
    return {
      ply: e.ply,
      win: Math.round((e.whiteWin ?? 50) * 10) / 10,
      label: move ? `${number}${move.side === "white" ? "." : "…"} ${move.san}` : "Start",
      score: formatScore(e.score),
      cls: move?.class,
    };
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const dot = (props: any) => {
    const p = props.payload as Point | undefined;
    if (!p?.cls || !["inaccuracy", "mistake", "blunder", "brilliant"].includes(p.cls)) return <g key={`d${p?.ply}`} />;
    return (
      <circle
        key={`d${p.ply}`}
        cx={props.cx}
        cy={props.cy}
        r={4}
        fill={CLASS_STYLE[p.cls].color}
        stroke="var(--surface-raised)"
        strokeWidth={2}
      />
    );
  };

  return (
    <div className="eval-graph panel" aria-label="Evaluation graph" style={{ padding: "6px 4px 2px" }}>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart
          data={data}
          margin={{ top: 4, right: 6, bottom: 0, left: 6 }}
          onClick={(state) => {
            const i = Number(state?.activeIndex ?? state?.activeTooltipIndex);
            if (Number.isFinite(i) && data[i]) onSelectPly(data[i].ply);
          }}
          style={{ cursor: "pointer" }}
        >
          <XAxis dataKey="ply" hide />
          <YAxis domain={[0, 100]} hide />
          <ReferenceLine y={50} stroke="var(--text-dim)" strokeOpacity={0.5} strokeDasharray="3 3" />
          {currentPly !== null && <ReferenceLine x={currentPly} stroke="var(--mantine-primary-color-filled)" strokeWidth={2} />}
          <Tooltip
            cursor={{ stroke: "var(--text-dim)", strokeWidth: 1 }}
            isAnimationActive={false}
            content={({ active, payload }) => {
              const p = active && payload?.[0]?.payload as Point | undefined;
              if (!p) return null;
              return (
                <div className="panel" style={{ padding: "6px 10px", fontSize: 12 }}>
                  <div style={{ fontWeight: 600 }}>{p.label}</div>
                  <div className="muted">
                    White {Math.round(p.win)}%{p.score ? ` · ${p.score}` : ""}
                    {p.cls && p.cls !== "good" && p.cls !== "best" ? ` · ${CLASS_STYLE[p.cls].label}` : ""}
                  </div>
                </div>
              );
            }}
          />
          <Area
            type="monotone"
            dataKey="win"
            baseValue={0}
            stroke="var(--text-dim)"
            strokeWidth={2}
            fill="var(--eval-white)"
            fillOpacity={0.75}
            isAnimationActive={false}
            dot={dot}
            activeDot={{ r: 5, stroke: "var(--surface-raised)", strokeWidth: 2 }}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
