/** Compare FENs ignoring the move counters (engines and the explorer may
 *  normalise them): same placement, side to move, castling and en passant. */
export function sameFen(a: string, b: string): boolean {
  return a.split(" ").slice(0, 4).join(" ") === b.split(" ").slice(0, 4).join(" ");
}
