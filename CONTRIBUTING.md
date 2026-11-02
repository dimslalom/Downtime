# Contributing to Downtime

Bug reports, ideas and pull requests are welcome. For anything large, open an issue first so we can agree on the approach.

## Set up

You need Windows, [Node.js](https://nodejs.org) 24, the [Rust toolchain](https://rustup.rs) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

## Before you open a pull request

These must all pass:

```bash
npm run tsc
npm run lint
npm test
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

## Project rules

The rules that matter most:

- No UI component libraries.
- The overlay stays keyboard-driven, and its animations stay under 100ms.
- Hiding the overlay must never change the card on screen.
- Only an explicit grade changes learning history.

## Test data

Grades are permanent, so don't test against your own review history. To use a separate data folder, run the app with the development identifier:

```bash
npm run tauri dev -- --config src-tauri/tauri.dev.conf.json
```

## Secrets

Never commit an API key or your personal decks and review data.
