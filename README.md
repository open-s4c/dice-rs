# dicers
rust wrapper for dice

> [!NOTE]
> [Dice](https://github.com/open-s4c/dice) is a lightweight, extensible C framework for capturing and distributing execution events in multithreaded programs. Designed for low overhead and high flexibility, Dice enables powerful tooling for runtime monitoring, concurrency testing, and deterministic replay using a modular publish-subscribe (pubsub) architecture.

> [!WARNING]
> might require: [#106 Memory Alignment Patch](https://github.com/open-s4c/dice/issues/106)

## use
1. build dice
2. build crate
3. run

build and run the record example
```bash
# Standard build (builds dice from source)
gcc -std=c11 -O1 -g -pthread -fsanitize=thread tests/atomic_stable.c -o tests/atomic_stable
cargo build && cc -shared  -Wl,--whole-archive target/debug/librecorder.a -Wl,--no-whole-archive -o target/debug/librecorder.so && TSANO_LIBDIR=target/debug ./dice/deps/tsano/tsano LD_PRELOAD=target/debug/librecorder.so ./tests/atomic_stable
```

### Manual Linking (with TSANO)
If you want to link against a pre-compiled dice library (e.g., `libdice_solo.a`) instead of building it from source, and run with ThreadSanitizer:

#### 1. Build TSANO Interceptor
First, acquire the `libtsano.so` shared library by building the `tsano` target from the dice submodule. This avoids building the entire dice project if you only need the interceptor.
```bash
cmake -S dice -B dice/build
cmake --build dice/build --target tsano
# Output will be at: dice/build/deps/tsano/libtsano.so
```

#### 2. Compile Test with Sanitizer
```bash
gcc -std=c11 -O1 -g -pthread -fsanitize=thread tests/atomic_stable.c -o tests/atomic_stable
```

#### 3. Build Rust Crate (Manual Link)
Build the crate with the `manual-link` feature to skip compiling the bundled dice C library.
```bash
cargo build -p recorder --features dice-rs/manual-link
```

#### 4. Link Manually
Link the Rust static library (`librecorder.a`) with your standalone dice library (`libdice_solo.a`).
> [!IMPORTANT]
> `--whole-archive` is required for the static libs to ensure interceptors and plugins are correctly included in the shared object.

```bash
cc -shared -o librecorder.so \
    -Wl,--whole-archive target/debug/librecorder.a libdice_solo.a -Wl,--no-whole-archive \
    -lpthread -ldl
```

#### 5. Run with TSANO
Use the `tsano` runner script (located in source) and point it to the directory containing the built `libtsano.so`.
```bash
TSANO_LIBDIR=dice/build/deps/tsano ./dice/deps/tsano/tsano LD_PRELOAD=./librecorder.so ./tests/atomic_stable
```


## run examples:

> [!WARNING]
> when compiling with clang, we additionally require `-shared-libsan`

for llvm these flags may also be nice if only atomics are of focus for interception
```
-mllvm -tsan-instrument-memory-accesses=0
```

### malloc_exmaple:
simplest malloc test case
```
gcc -std=c11 -O1 -g tests/malloc_example.c -o tests/malloc_example
cc -shared  -Wl,--whole-archive target/debug/libdice_rs.a -Wl,--no-whole-archive -o target/debug/libdice.so

LD_PRELOAD=target/debug/libdice.so ./tests/malloc_example
```

### atomic_racecon
race condition
```
g++ -std=c++17 -O1 -g -fsanitize=thread -fno-omit-frame-pointer -pthread tests/atomic_racecon.cc -o tests/atomic_racecon
```

### atomic_stable
tests atomic operations and thread shedules while keeping the result stable
```
gcc -std=c11 -O1 -g -pthread -fsanitize=thread tests/atomic_stable.c -o tests/atomic_stable
```

