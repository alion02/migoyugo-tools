# myu-protocol

The communication protocol for 'Migoyugo' engines, used by the `itgo` engine and `myu-arena`.

This crate defines the data structures used for communication between the GUI/arena and the engine. The protocol is text-based and uses [RON (Rusty Object Notation)](https://github.com/ron-rs/ron) for serialization.

## Overview

Communication happens over standard input and standard output. Each message is a single line containing a serialized RON value.

*   **User -> Engine**: Messages defined by `UserMsg`.
*   **Engine -> User**: Messages defined by `EngineMsg`.

## Message Types

### From User (`UserMsg`)

*   `Reset`: Resets the game to the initial position and stops any ongoing search.
*   `Sync`: A synchronization barrier. The engine must respond with `Ready` once it has processed all prior messages.
*   `Undo(usize)`: Undoes the specified number of half-moves (plies).
*   `Play(Vec<Sq>)`: Plays a sequence of moves on the current board.
*   `Go(Vec<Limit>)`: Starts searching for the best move with the given limits (depth, nodes, time).
*   `Stop`: Stops the current search immediately.

### From Engine (`EngineMsg`)

*   `Id { ... }`: Sent at startup with engine name, author, and version.
*   `Ready`: Sent in response to `Sync`.
*   `Info { ... }`: Periodic search updates (depth, nodes, time, pv, evaluation).
*   `Best(Option<Sq>)`: The best move found after a search completes or is stopped.
*   `Error(String)`: Error message.

## Example Session

```ron
// Engine starts up
Id(name:Some("Itgo"),author:None,version:None)

// User sets up a game
Reset
Play(["d4"])

// User asks engine to think for 1 second
Go([Ms(1000)])

// Engine sends info during search
Info(pv:["e5"],eval:Score(10),depth:1,nodes:100,time:10,knps:10000)
...
Info(pv:["e5","d5"],eval:Score(15),depth:5,nodes:5000,time:500,knps:10000)

// Engine returns best move
Best(Some("e5"))

// Synchronization check
Sync
Ready
```

## Implementation Notes

*   **Squares**: Represented as strings (e.g., "a1", "h8") in the serialized format.
*   **Synchronization**: The `Sync` command is crucial for ensuring the engine is in a known state before starting a new game or search, especially when interacting with an arena or GUI.
*   **Search**: When `Go` is received, the engine should start a search in a separate thread/task. It should periodically send `Info` messages and check for `Stop` or other input. `Reset`, `Undo`, and `Play` should implicitly stop any running search.
