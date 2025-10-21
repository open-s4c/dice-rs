// gcc -std=c11 -O1 -g -pthread -fsanitize=thread tests/atomic_stable.c -o tests/atomic_stable
#include <stdio.h>
#include <pthread.h>
#include <stdatomic.h>

#define NUM_THREADS 16
#define MAX_OPS     10000
#define WORK_ITERS  10000

static atomic_int global_ops = 0;

static inline void local_work(void) {
    // Deterministic, non-random arithmetic to burn a bit of CPU per loop.
    // 'volatile' prevents the compiler from optimizing the loop away.
    volatile unsigned x = 1;
    for (size_t i = 0; i < WORK_ITERS; ++i) {
        x = x * 1664525u + 1013904223u;
    }
    (void)x;
}

static void* worker(void* arg) {
    // lock
    int id = (int)(size_t)arg;
    int my_ops = 0;

    for (;;) {
        local_work();

        int expected = atomic_load_explicit(&global_ops, memory_order_relaxed);
        while (expected < MAX_OPS) {
            if (atomic_compare_exchange_strong_explicit(
                    &global_ops, &expected, expected + 1,
                    memory_order_acq_rel, memory_order_relaxed)) {
                ++my_ops;
                break;
            }
        }
        if (expected >= MAX_OPS) break;
    }

    printf("thread %d did %d operations\n", id + 1, my_ops);
    return NULL;
}

int main(void) {
    pthread_t threads[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; ++i)
        pthread_create(&threads[i], NULL, worker, (void*)(size_t)i);

    for (int i = 0; i < NUM_THREADS; ++i)
        pthread_join(threads[i], NULL);

    int loads = atomic_load(&global_ops);
    int max = MAX_OPS;
    printf("total counted operations = %d (target %d)\n", loads, max);
    return 0;
}
