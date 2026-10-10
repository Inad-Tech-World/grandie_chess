// Immutable helpers for the flat move tree shared with Rust (`GameTree`).
// Invariant: nodes[i].id === i and node 0 is the root.

import type { AppliedMove, GameTree, TreeNode } from "../bindings";

export const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

export function emptyTree(fen: string = START_FEN, headers: GameTree["headers"] = []): GameTree {
  return {
    headers,
    nodes: [
      { id: 0, parent: null, children: [], ply: 0, fen, uci: null, san: null, comment: null, nags: [], clockMs: null, eval: null },
    ],
  };
}

export function header(tree: GameTree, name: string): string | undefined {
  return tree.headers.find((h) => h.name === name)?.value;
}

export function setHeader(tree: GameTree, name: string, value: string): GameTree {
  const exists = tree.headers.some((h) => h.name === name);
  return {
    ...tree,
    headers: exists ? tree.headers.map((h) => (h.name === name ? { ...h, value } : h)) : [...tree.headers, { name, value }],
  };
}

/** Add a move after `parentId`; returns the existing child if the move is already there. */
export function addApplied(tree: GameTree, parentId: number, applied: AppliedMove): { tree: GameTree; id: number } {
  const parent = tree.nodes[parentId];
  const existing = parent.children.find((c) => tree.nodes[c].uci === applied.uci);
  if (existing !== undefined) return { tree, id: existing };
  const id = tree.nodes.length;
  const node: TreeNode = {
    id,
    parent: parentId,
    children: [],
    ply: parent.ply + 1,
    fen: applied.position.fen,
    uci: applied.uci,
    san: applied.san,
    comment: null,
    nags: [],
    clockMs: null,
    eval: null,
  };
  const nodes = tree.nodes.slice();
  nodes[parentId] = { ...parent, children: [...parent.children, id] };
  nodes.push(node);
  return { tree: { ...tree, nodes }, id };
}

export function mainline(tree: GameTree, from = 0): number[] {
  const out: number[] = [];
  let cur = tree.nodes[from];
  while (cur && cur.children.length > 0) {
    const next = cur.children[0];
    out.push(next);
    cur = tree.nodes[next];
  }
  return out;
}

/** Node ids from the root (exclusive) to `id` (inclusive). */
export function pathTo(tree: GameTree, id: number): number[] {
  const out: number[] = [];
  let cur: number | null = id;
  while (cur !== null && cur !== 0) {
    out.push(cur);
    cur = tree.nodes[cur].parent;
  }
  return out.reverse();
}

export function uciPath(tree: GameTree, id: number): string[] {
  return pathTo(tree, id).map((n) => tree.nodes[n].uci ?? "");
}

/** The line currently shown: path to `id` followed by its main continuation. */
export function currentLine(tree: GameTree, id: number): number[] {
  return [...pathTo(tree, id), ...mainline(tree, id)];
}

export function updateNode(tree: GameTree, id: number, patch: Partial<TreeNode>): GameTree {
  const nodes = tree.nodes.slice();
  nodes[id] = { ...nodes[id], ...patch };
  return { ...tree, nodes };
}

export function toggleNag(tree: GameTree, id: number, nag: number): GameTree {
  const node = tree.nodes[id];
  // Move-quality NAGs (1-6) are exclusive; others toggle independently.
  const quality = nag >= 1 && nag <= 6;
  const has = node.nags.includes(nag);
  let nags = node.nags.filter((n) => n !== nag && !(quality && n >= 1 && n <= 6));
  if (!has) nags = [...nags, nag].sort((a, b) => a - b);
  return updateNode(tree, id, { nags });
}

/** Make the line through `id` the main line at every branching point above it. */
export function promoteToMainline(tree: GameTree, id: number): GameTree {
  let t = tree;
  let child = id;
  let parent = t.nodes[id].parent;
  while (parent !== null) {
    const p = t.nodes[parent];
    if (p.children[0] !== child) {
      t = updateNode(t, parent, { children: [child, ...p.children.filter((c) => c !== child)] });
    }
    child = parent;
    parent = t.nodes[parent].parent;
  }
  return t;
}

/** Remove `id` and everything after it, renumbering nodes to keep ids dense. */
export function deleteFrom(tree: GameTree, id: number): GameTree {
  if (id === 0) return tree;
  const doomed = new Set<number>();
  const stack = [id];
  while (stack.length) {
    const n = stack.pop()!;
    doomed.add(n);
    stack.push(...tree.nodes[n].children);
  }
  const remap = new Map<number, number>();
  tree.nodes.forEach((n) => {
    if (!doomed.has(n.id)) remap.set(n.id, remap.size);
  });
  const nodes = tree.nodes
    .filter((n) => !doomed.has(n.id))
    .map((n) => ({
      ...n,
      id: remap.get(n.id)!,
      parent: n.parent === null ? null : remap.get(n.parent)!,
      children: n.children.filter((c) => !doomed.has(c)).map((c) => remap.get(c)!),
    }));
  return { ...tree, nodes };
}

/** Id of `oldId`'s nearest surviving ancestor after a deletion of `deleted`. */
export function survivorAfterDelete(tree: GameTree, deleted: number, current: number): number {
  const path = pathTo(tree, current);
  if (!path.includes(deleted)) return current;
  return tree.nodes[deleted].parent ?? 0;
}
