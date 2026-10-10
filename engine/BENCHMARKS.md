# lc0 benchmarks

Measured 2026-10-10 with lc0 v0.32.1 built by `engine/build-lc0.sh` (x86_64, Metal + BLAS + Eigen).

**Machine:** MacBookPro15,1, Intel Core i9-9880H, 16 GB RAM, AMD Radeon Pro 560X (4 GB) plus Intel UHD 630, macOS 15.8.1.

## Raw network speed (`lc0 backendbench`, nodes per second)

| Network | Backend | Batch 1 | Batch 64 | Batch 256 |
|---|---|---|---|---|
| 791556 (192x15) | metal, gpu=1 (Radeon Pro 560X) | 23 | 376 | **607** |
| 791556 | metal, gpu=0 (Intel UHD 630) | 9 | ~13 | — |
| 791556 | blas (Accelerate) | 52 | ~106 | — |
| T1-256x10 (transformer) | metal, gpu=1 | — | 42 | 51 |
| T1-256x10 | blas | 25 | 53 | — |

## Search speed

`lc0 benchmark --backend=metal --backend-opts=gpu=1 --nodes=3000`, network 791556: 130–400 nps per position, **227 nps overall**. The first position includes Metal graph warm-up.

## Correctness

`--backend=check --backend-opts="mode=check,freq=1.0,metal(gpu=1),blas"`: every batch passed. Metal on the Radeon agrees with the CPU within a relative tolerance of 1e-4.

## Conclusions for this Mac

- **Full-strength default:** network 791556 on `metal` with `gpu=1`.
- lc0's default Metal device (`gpu=0`) is the integrated Intel GPU here, which is about 45x slower. The app benchmarks every GPU and stores the fastest; it does not rely on the default.
- The transformer nets (T1, BT3, BT4) are too slow on this GPU for comfortable analysis. They stay in the catalog for Apple Silicon.
- Maia and the weak Leela levels use the `blas` backend at 1 to 4 nodes per move, so there is no GPU start-up cost.

## Opponent level calibration

Engine matches with `cargo run --profile perf -p grandie-dev-tools --bin calibrate` on this Mac (2026-10-10), 16 games per pairing, alternating colours. Anchors: Maia levels at their nominal rating (Maia uses 1 node and opening variety), and Stockfish 19 with `UCI_LimitStrength`. With 16 games each estimate is about ±150 Elo, so the ratings in `src-tauri/resources/levels.json` are smoothed to keep the ladder in order.

| Level | Network | Nodes | Temperature | Anchor | Score | Measured | Listed |
|---|---|---|---|---|---|---|---|
| Leela 1 | 703810 | 1 | 1.0 | Maia 1100 | 2.5/16 | ≈ 805 | 750 |
| Leela 2 | 703810 | 1 | 0.85 | Maia 1100 | 4/16 | ≈ 910 | 900 |
| Leela 3 | 703810 | 1 | 0.7 | Maia 1100 | 8/16 | ≈ 1100 | 1100 |
| Leela 4 | 703810 | 1 | 0.65 | Maia 1500 | 8/16 | ≈ 1500 | 1450 |
| Leela 5 | 703810 | 1 | 0.55 | Maia 1900 | 12.5/16 | ≈ 2120 | 1900 |
| Leela 6 | 703810 | 1 | 0.4 | Maia 1900 | 10.5/16 | ≈ 2010 | 2050 |
| Leela 7 | 791556 | 1 | 0.1 | Stockfish 2300 | 10.5/16 | ≈ 2410 | 2400 |
| Leela 8 | 791556 | 8 | 0.1 → 0 over 20 moves | Stockfish 2300 | 14/16 | ≈ 2640 | 2650 |

Weak levels keep a constant temperature (`TempDecayMoves 0`): with decay, even very random settings play near-perfect chess once the temperature reaches zero (an earlier ladder measured over 2300 for "beginner" settings). The Progress screen also tracks your actual results against each level.
