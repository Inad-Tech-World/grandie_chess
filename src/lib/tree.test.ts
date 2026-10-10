import { describe, expect, it } from "vitest";
import type { AppliedMove } from "../bindings";
import { addApplied, currentLine, deleteFrom, emptyTree, mainline, pathTo, promoteToMainline, toggleNag } from "./tree";

const mv = (uci: string, san: string, fen = `fen-${uci}`): AppliedMove => ({
  uci,
  san,
  position: { fen, turn: "white", dests: {}, check: false, fullmoves: 1, halfmoves: 0, outcome: null },
});

function sample() {
  let t = emptyTree();
  let r = addApplied(t, 0, mv("e2e4", "e4"));
  t = r.tree;
  const e4 = r.id;
  r = addApplied(t, e4, mv("e7e5", "e5"));
  t = r.tree;
  const e5 = r.id;
  r = addApplied(t, e4, mv("c7c5", "c5"));
  t = r.tree;
  const c5 = r.id;
  r = addApplied(t, c5, mv("g1f3", "Nf3"));
  t = r.tree;
  return { t, e4, e5, c5, nf3: r.id };
}

describe("tree", () => {
  it("adds moves and reuses existing children", () => {
    const { t, e4, e5 } = sample();
    expect(addApplied(t, e4, mv("e7e5", "e5")).id).toBe(e5);
    expect(t.nodes[e5].ply).toBe(2);
    expect(mainline(t)).toEqual([e4, e5]);
  });

  it("walks paths and current lines", () => {
    const { t, e4, c5, nf3 } = sample();
    expect(pathTo(t, nf3)).toEqual([e4, c5, nf3]);
    expect(currentLine(t, c5)).toEqual([e4, c5, nf3]);
  });

  it("promotes a variation", () => {
    const { t, e4, c5, nf3 } = sample();
    const p = promoteToMainline(t, nf3);
    expect(p.nodes[e4].children[0]).toBe(c5);
    expect(mainline(p)).toEqual([e4, c5, nf3]);
  });

  it("deletes a subtree and keeps ids dense", () => {
    const { t, e4, c5 } = sample();
    const d = deleteFrom(t, c5);
    expect(d.nodes).toHaveLength(3);
    d.nodes.forEach((n, i) => expect(n.id).toBe(i));
    expect(d.nodes[e4].children).toHaveLength(1);
  });

  it("keeps move-quality NAGs exclusive", () => {
    const { t, e4 } = sample();
    let n = toggleNag(t, e4, 1);
    n = toggleNag(n, e4, 2);
    expect(n.nodes[e4].nags).toEqual([2]);
    n = toggleNag(n, e4, 2);
    expect(n.nodes[e4].nags).toEqual([]);
  });
});
