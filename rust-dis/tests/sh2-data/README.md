# SH-2 named data fixture

This fixture verifies Splat-style named subsegments with the Saturn target
compiler. It compiles three translation units, links their `.data` and `.text`
sections in YAML order, runs `rust-dis`, relinks with the generated linker
script, and requires byte-identical output.

Run it from the repository root inside the SOTN Docker environment:

```sh
sh tools/saturn-splitter/rust-dis/tests/sh2-data/run.sh
```

The expected SHA-1 is recorded in `run.sh`; no generated binary is checked in.
Compiler and linker artifacts are written under `build/sh2data`.
