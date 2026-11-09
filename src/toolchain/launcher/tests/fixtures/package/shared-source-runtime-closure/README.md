# Shared source runtime closure

The executable imports a source library declared with `deploy = "shared"`.
That library imports another source library. Both must be built for compilation,
but neither belongs to the executable's private runtime deployment.

The existing SDK fixture runner builds the same graph through z42c and direct
z42b, then checks dist membership without deleting outputs before the assertion.

A private bridge to the same mid package makes its runtime closure necessary.
Removing that branch on a reused dist retires owned copies and preserves a user file.
