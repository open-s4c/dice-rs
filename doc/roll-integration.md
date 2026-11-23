# Embedding Dice Roll with dice-rs

The legacy build remains the default. Set `DICE_ROLL_CONFIG` to opt into
a configured Dice runtime. In that mode the build script:

1. Configures the Dice submodule with the requested `.dice` manifest.
2. Builds the `dice-roll-runtime` CMake OBJECT target and only archives objects
   from that target into `libdice.a` (never the legacy dispatcher).
3. Runs bindgen **after** the generated header is ready, so Rust event IDs use
   the same configuration as the C runtime.
4. Builds the existing logging shim and `libtsano` as before.

```sh
export DICE_ROLL_CONFIG=/absolute/path/to/dice-extension/roll/craps.dice
export DICE_ROLL_IMPORT_PATHS=dice=/absolute/path/to/dice-extension/roll
export DICE_EXTENSION_DIRS=/absolute/path/to/dice-extension
export DICE_MODULES='fcntl;ioctl;poll;epoll;socket;stdio;unistd'
cargo build -p dice-rs --features 'dice dice-self dice-pthread_create dice-malloc'
```

Enabled `dice-*` features and the semicolon-delimited `DICE_MODULES` list
select the compiled interceptors. The manifest must import and declare routes
for every selected module. Event and chain IDs belong to the generated Roll
header; this mode requires rebuilding C and Rust consumers together.

**Compatibility:** Rust continues to register dynamic callback handlers.
Do not seal pubsub or disable callbacks. Existing record/replay traces require
explicit numeric event ID pins in the Roll configuration. The rolled build is
experimental until compiled and regression-tested on a machine with Cargo.
