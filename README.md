# migoyugo-tools

This monorepo houses **Itgo**, an engine (computer player) for a modern abstract strategy game called [Migoyugo](https://www.migoyugo.com/), as well as multiple supporting tools developed for the purpose of utilizing and improving the engine.

## Building

The project uses [Rust](https://rust-lang.org/) nightly. Build using `cargo`.

## Usage

`cargo r -r -p itgo` to run the **Itgo** engine. It communicates via stdin/stdout using **myu-protocol**. Software such as [`rlwrap`](https://github.com/hanslub42/rlwrap) is recommended for manual usage.

## Contributions

Accepting general infrastructure contributions (see issues for priority tasks). Contributions to the engine may be considered, but must at minimum be backed by an appropriate passing SPRT or justification for a lack thereof.

## LLM note

This repo features LLM-generated code in non-critical parts of the codebase.