# SH-2 named data fixture

This fixture verifies Splat-style named subsegments with the Saturn target
compiler. It compiles three translation units, links their `.data` and `.text`
sections in YAML order, runs `rust-dis`, relinks with the generated linker
script, and requires byte-identical output.

Build rust-dis, then run the fixture from any working directory:

```sh
cargo build --release
SH2_CC=sh2-gcc sh tests/sh2-data/run.sh
```

`SH2_CC` must name an SH-2 compiler driver that accepts GCC-compatible
arguments. It defaults to `sh2-gcc`. The SH binutils (`sh-elf-as`,
`sh-elf-ld`, and `sh-elf-objcopy`) must also be on `PATH`.

The expected SHA-1 is recorded in `run.sh`; no generated binary is checked in.
Compiler and linker artifacts are written under rust-dis's `build/sh2data`,
regardless of the caller's working directory. No path outside the rust-dis
repository is referenced by the fixture.
