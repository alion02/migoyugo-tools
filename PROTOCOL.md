# Migoyugo Engine Protocol

The communication protocol for Migoyugo engines, used by the `itgo` engine and `myu-arena`.

## Overview

Communication happens over standard input and standard output. Each message is a single line containing a JSON object. All message types use `snake_case` naming.

*   **User → Engine**: Messages sent to control the engine.
*   **Engine → User**: Messages sent by the engine to report status and results.

## Message Types

### From User

*   `{"set": {...}}`: Change engine settings. Fields are engine-specific.
*   `{"play": ["e3", "d4", ...]}`: Play a sequence of moves on the current board.
*   `{"undo": N}`: Undo the specified number of half-moves (plies).
*   `{"moves": ["e3", "d4", ...]}`: Discard all played moves and replace them with the given sequence.
*   `"reset"`: Reset the game state to the initial position.
*   `"sync"`: Synchronization barrier. The engine must respond with `ready` once it has processed all prior messages.
*   `{"go": {...}}`: Start searching for the best move with optional limits.
    * `depth` (optional): Maximum search depth.
    * `nodes` (optional): Maximum nodes to search.
    * `time` (optional): Hard time limit in milliseconds.
    * `clock_left` (optional): Clock time remaining for the current player in milliseconds.
    * `clock_incr` (optional): Clock time increment in milliseconds.
*   `"stop"`: Stop the current search immediately.
*   `"debug"`: Request printing debug information to stderr.

### From Engine

*   `{"about": {"name": "...", "author": "...", "version": "...", "features": [...]}}`: Sent at startup with engine identification.
    * `features`: A list of features declared by the engine. There is currently no standardized list. Can be used by a user or harness to verify that the engine supports a desired operation.
*   `"ready"`: Sent in response to `sync`.
*   `{"info": {...}}`: Periodic search updates.
    * `pv`: Principal variation (array of moves).
    * `eval`: Evaluation, either `{"score": N}` or `{"decisive": N}`.
    * `depth`: Search depth.
    * `time`: Time elapsed in milliseconds.
    * `nodes`: Nodes searched.
    * `knps`: Kilo-nodes per second.
    * `evals`: Number of evaluations.
    * `keps`: Kilo-evals per second.
    * `pv_nodes`: PV nodes searched.
*   `{"best": "e5"}` or `{"best": null}`: The best move found after a search completes or is stopped. Note that the engine does not play this move on its internal representation.
*   `{"warn": "..."}`: Warning message.
*   `{"error": "..."}`: Error message.

## Example Session

```
// Engine starts up
<<< {"about":{"name":"Itgo","author":"alion02","version":"0.1.0","features":[]}}

// User plays an opening move
>>> {"play":["d4"]}

// User asks engine to think for 1 second
>>> {"go":{"time":1000}}

// Engine sends info during search
<<< {"info":{"pv":["e5"],"eval":{"score":10},"depth":1,"time":10,"nodes":100,"knps":10,"evals":50,"keps":5}}
...
<<< {"info":{"pv":["e5","d5"],"eval":{"score":15},"depth":5,"time":500,"nodes":5000,"knps":10,"evals":2500,"keps":5}}

// Engine returns best move
<<< {"best":"e5"}

// Synchronization check
>>> "sync"
<<< "ready"

// Reset the engine's internal state
>>> "reset"
```

## Implementation Notes

*   **Moves**: Represented as strings (e.g., "a1", "h8") indicating the square to place a piece.
*   **Synchronization**: The `sync` command can be used to wait for the engine to finish processing a message or series of messages, ensuring it is in a known state before starting a new game or search.
*   **Search**: When `go` is received, the engine should start a search. It should periodically send `info` messages and continue checking for `stop` or other input.
*   **Blocking Commands**: Commands like `reset`, `undo`, `play`, and `moves` may block or abort if a search is currently running, depending on implementation details.
