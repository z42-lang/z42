# Threads, as in every other language here. CPython's GIL serialises them, so this
# measures the GIL, not parallel speedup — that is the point of including it.
import sys
import threading

N = int(sys.argv[1])
THREADS = 4
partials = [0] * THREADS


def partial(t):
    chunk = N // THREADS
    lo = t * chunk
    hi = N if t == THREADS - 1 else lo + chunk
    acc = 0
    for i in range(lo, hi):
        acc += (i * i) % 7
    partials[t] = acc


ts = [threading.Thread(target=partial, args=(t,)) for t in range(THREADS)]
for t in ts:
    t.start()
for t in ts:
    t.join()
print("RESULT %d" % sum(partials))
