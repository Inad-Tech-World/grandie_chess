import { Fragment, useEffect, useRef, type ReactNode } from "react";
import type { GameTree } from "../bindings";
import { moveNumber, nagSymbol } from "../lib/format";

interface MoveTreeProps {
  tree: GameTree;
  current: number;
  onSelect: (id: number) => void;
  onContextMenu?: (id: number, e: React.MouseEvent) => void;
  /** Optional per-node decoration, e.g. review annotations. */
  decorate?: (id: number) => ReactNode;
}

/** Inline PGN-style move tree with indented variations. */
export function MoveTree({ tree, current, onSelect, onContextMenu, decorate }: MoveTreeProps) {
  const currentRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    currentRef.current?.scrollIntoView({ block: "nearest" });
  }, [current]);

  const moveEl = (id: number, forceNumber: boolean) => {
    const node = tree.nodes[id];
    const parent = tree.nodes[node.parent ?? 0];
    const { number, white } = moveNumber(parent.fen);
    const prefix = white ? `${number}. ` : forceNumber ? `${number}… ` : "";
    return (
      <Fragment key={`m${id}`}>
        <span
          className="move"
          data-current={id === current}
          ref={id === current ? currentRef : undefined}
          onClick={() => onSelect(id)}
          onContextMenu={(e) => onContextMenu?.(id, e)}
        >
          <span className="muted">{prefix}</span>
          {node.san}
          {node.nags.map((n) => (
            <span key={n} className="nag">
              {nagSymbol(n)}
            </span>
          ))}
          {decorate?.(id)}
        </span>{" "}
        {node.comment && <span className="comment">{node.comment} </span>}
      </Fragment>
    );
  };

  // Renders the line starting with the children of `parentId`.
  const line = (parentId: number, forceNumber: boolean): ReactNode[] => {
    const out: ReactNode[] = [];
    let p = parentId;
    let force = forceNumber;
    for (;;) {
      const children = tree.nodes[p].children;
      if (children.length === 0) break;
      const [main, ...alts] = children;
      out.push(moveEl(main, force));
      force = !!tree.nodes[main].comment;
      if (alts.length) {
        out.push(
          ...alts.map((alt) => (
            <span className="variation" key={`v${alt}`}>
              {moveEl(alt, true)}
              {line(alt, !!tree.nodes[alt].comment)}
            </span>
          )),
        );
        force = true;
      }
      p = main;
    }
    return out;
  };

  const root = tree.nodes[0];
  return (
    <div className="tree" aria-label="Move tree">
      {root.comment && <span className="comment">{root.comment} </span>}
      {line(0, true)}
      {root.children.length === 0 && <span className="muted">Make a move on the board to start.</span>}
    </div>
  );
}
