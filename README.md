# dicers
Rust wrapper for Dice

> [!NOTE]
> [Dice](https://github.com/open-s4c/dice) is a lightweight, extensible C framework for capturing and distributing execution events in multithreaded programs. Designed for low overhead and high flexibility, Dice enables powerful tooling for runtime monitoring, concurrency testing, and deterministic replay using a modular publish-subscribe (pubsub) architecture.

## use
build the object file and link

build and run the record example
```
gcc -std=c11 -O1 -g -pthread -fsanitize=thread tests/atomic_stable.c -o tests/atomic_stable
cargo build && cc -shared  -Wl,--whole-archive target/debug/librecorder.a -Wl,--no-whole-archive -o target/debug/librecorder.so && TSANO_LIBDIR=target/debug ./dice/deps/tsano/tsano LD_PRELOAD=target/debug/librecorder.so ./tests/atomic_stable
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

