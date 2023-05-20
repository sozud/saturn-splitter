# Saturn Splitting Tool

This is a a work-in-progress binary splitting tool for Sega Saturn written in Rust. The goal is functionality similar to https://github.com/ethteck/splat

Since we are using a DOS compiler source filenames must be valid DOS filenames (8.3).

The project is currently just targeting T_BAT.PRG. This needs to be extracted from the Saturn SOTN ISO and copied to the root directory.

## Usage

```
# compile
cd rust-dis
cargo build --release

# run tests
cargo test

# execute
cargo run
```

### Building the docker containers

This project uses two docker containers. The first, `scripts/docker/binutils_dockerfile` has sh-elf-gcc and binutils for objdump, objcopy and as. The second, `scripts/docker/dosemu_dockerfile` has dosemu to run the original cygnus DOS compiler. The following script will build both containers:

```
sh scripts/docker/build_docker.sh  
```

### Get the compiler

```
sh scripts/get_gccsh.sh
```

### Building the source (requires docker containers)

This script will copy the splitter output and compiler to a temp folder and build the source.

```
sh scripts/compile_dosemu/compile_docker.sh
```

### Comparing binaries (requires docker containers)

This will objdump the original binary and the recompiled binary and compare. The output is put in temp/diff.txt.

```
sh scripts/compile_dosemu/compare_docker.sh
```
