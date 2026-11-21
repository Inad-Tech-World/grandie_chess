# Credits and licences

Grandie Chess is free software, released under the **GNU General Public License v3.0 or later** (see `LICENSE`). It builds on the work of many people. Thank you.

## Engines and neural networks

| Component | Authors | Licence | How it's used |
|---|---|---|---|
| [Leela Chess Zero (lc0)](https://github.com/LeelaChessZero/lc0) | The LCZero Authors | GPL-3.0-or-later (with an NVIDIA linking exception that does not apply to these builds) | Bundled engine. macOS: built from source (`engine/lc0`). Windows: the official v0.32.1 `onnx-dml` release, whose source is the same tag of `engine/lc0` |
| Leela networks (e.g. 791556, 703810, T1, BT4) | The LCZero project and contributors | Not stated; downloaded at runtime from `storage.lczero.org`, not redistributed | Downloaded by the user from the Engines screen |
| [Maia Chess](https://github.com/CSSLab/maia-chess) networks 1100–1900 | CSSLab, University of Toronto | GPL-3.0 | Downloaded at runtime from the official GitHub release |
| Maia 2200 (community) | [CallOn84](https://github.com/CallOn84/LeelaNets) | GPL-3.0 | Optional runtime download |
| [Stockfish](https://stockfishchess.org) | The Stockfish developers | GPL-3.0 | Runtime download of the official Stockfish 19 release (pinned, SHA-256 checked); the default coach and review engine |

## Windows runtime components

Bundled with lc0 in the Windows build only. Their licence texts are installed in the `licenses` folder of the app.

| Component | Authors | Licence | How it's used |
|---|---|---|---|
| [ONNX Runtime](https://onnxruntime.ai) (`onnxruntime.dll`) | Microsoft | MIT | Runs Leela's networks: on the CPU, and on the GPU through DirectML |
| [DirectML](https://github.com/microsoft/DirectML) (`DirectML.dll`) | Microsoft | Microsoft Software License Terms (redistributable in Windows applications) | GPU acceleration on any DirectX 12 graphics card |
| Microsoft Visual C++ runtime (`msvcp140*.dll`, `vcruntime140*.dll`) | Microsoft | Visual Studio redistributable terms | Needed by lc0 and ONNX Runtime |

## Chess libraries

| Component | Authors | Licence |
|---|---|---|
| [chessground](https://github.com/lichess-org/chessground) | Lichess contributors | GPL-3.0-or-later |
| [shakmaty](https://github.com/niklasf/shakmaty), [pgn-reader](https://github.com/niklasf/rust-pgn-reader) | Niklas Fiekas | GPL-3.0-or-later |
| [chessops](https://github.com/niklasf/chessops) (test mock backend only) | Niklas Fiekas | GPL-3.0-or-later |

## Data

| Data | Source | Licence |
|---|---|---|
| Opening names (`src-tauri/resources/openings`) | [lichess-org/chess-openings](https://github.com/lichess-org/chess-openings) | CC0-1.0 |
| Puzzles | [Lichess puzzle database](https://database.lichess.org/#puzzles) | CC0-1.0 (downloaded at runtime) |
| Opening explorer | [Lichess opening explorer API](https://lichess.org/api#tag/Opening-Explorer) | Online service, used with the user's own API token |
| Master games by player | [PGN Mentor](https://www.pgnmentor.com/files.html) | Free downloads, no licence stated; downloaded at runtime when the user picks a player, not redistributed. The player list (`resources/masters/players.json`) is built by `scripts/masters-catalog.py` |
| Legends pack (`resources/masters/legends.pgn`) | 20 famous historical games, game scores taken from PGN Mentor player files by `scripts/build-legends.py` | Game scores are historical records; the titles and notes are ours |

## AI services (optional)

| Service | How it's used |
|---|---|
| [OpenRouter](https://openrouter.ai) | With the user's own API key (kept in the system credential store): the coach's narration and answers (default models: Anthropic Claude Haiku 5.5 and Sonnet 5.5), speech-to-text for voice input, and the optional cloud voice. Nothing is sent unless a key is saved and AI is switched on. |

## Piece sets

Bundled in `public/pieces`, from [lichess-org/lila](https://github.com/lichess-org/lila/blob/master/COPYING.md):

| Set | Author | Licence |
|---|---|---|
| cburnett | Colin M.L. Burnett | GPL-2.0-or-later |
| merida | Armando Hernandez Marroquin | GPL-2.0-or-later |
| chessnut | Alexis Luengas | Apache-2.0 |
| fantasy, spatial, celtic | Maurizio Monge | MIT |
| rhosgfx | RhosGFX | CC0-1.0 |
| mpchess | Maxime Chupin | GPL-3.0-or-later |

Board colour themes are generated CSS; the "brown" board comes from chessground (GPL-3.0-or-later).

## Application framework and UI

| Component | Licence |
|---|---|
| [Tauri](https://tauri.app) and plugins | MIT / Apache-2.0 |
| [tauri-specta](https://github.com/specta-rs/tauri-specta), specta | MIT |
| [React](https://react.dev) | MIT |
| [Mantine](https://mantine.dev) | MIT |
| [Recharts](https://recharts.org) | MIT |
| [Zustand](https://github.com/pmndrs/zustand) | MIT |
| [Tabler Icons](https://tabler.io/icons) | MIT |
| [TanStack Virtual](https://tanstack.com/virtual) | MIT |
| [rs-fsrs](https://github.com/open-spaced-repetition/rs-fsrs) (spaced repetition) | MIT |
| [cpal](https://github.com/RustAudio/cpal) (microphone), [hound](https://github.com/ruuda/hound) (WAV) | Apache-2.0 / Apache-2.0 |
| [zip](https://github.com/zip-rs/zip2), [base64](https://github.com/marshallpierce/rust-base64) | MIT |
| [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite | MIT / public domain |
| [tokio](https://tokio.rs), [reqwest](https://github.com/seanmonstar/reqwest), serde, rayon, zstd, csv, chrono | MIT / Apache-2.0 |

## Inspiration

Layout and engine-handling ideas from [En Croissant](https://github.com/franciscoBSalgueiro/en-croissant) (GPL-3.0), engine management from Arena, and database and opening tools from ChessBase. Accuracy and move-classification formulas follow those [published by Lichess](https://lichess.org/page/accuracy).

## Logo

The Grandie Chess logo is © its owner and is not covered by the GPL licence of the code.
