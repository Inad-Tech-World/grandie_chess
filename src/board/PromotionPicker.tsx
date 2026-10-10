import type { Side } from "../bindings";

export type PromotionRole = "queen" | "rook" | "bishop" | "knight";

const ROLES: PromotionRole[] = ["queen", "rook", "bishop", "knight"];

/** Overlay asking which piece a pawn promotes to. Uses the board's piece set
 *  through chessground's `piece` CSS classes. */
export function PromotionPicker({ color, onChoose }: { color: Side; onChoose: (role: PromotionRole | null) => void }) {
  return (
    <div className="promotion" onClick={() => onChoose(null)} role="dialog" aria-label="Choose promotion piece">
      <div className="promotion-choices cg-wrap" onClick={(e) => e.stopPropagation()}>
        {ROLES.map((role) => (
          <button key={role} aria-label={`Promote to ${role}`} onClick={() => onChoose(role)} style={{ position: "relative" }}>
            <piece className={`${role} ${color}`} style={{ position: "absolute", inset: 4, width: "auto", height: "auto" }} />
          </button>
        ))}
      </div>
    </div>
  );
}

declare module "react" {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      piece: React.DetailedHTMLProps<React.HTMLAttributes<HTMLElement>, HTMLElement>;
    }
  }
}
