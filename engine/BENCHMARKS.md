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
