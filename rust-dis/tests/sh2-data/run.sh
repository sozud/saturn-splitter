#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
rust_dis_dir=$(CDPATH= cd -- "$script_dir/../.." && pwd)
cd "$rust_dis_dir"

fixture_dir=tests/sh2-data
build_dir=build/sh2data
sh2_cc=${SH2_CC:-sh2-gcc}

mkdir -p "$build_dir"

for name in header main anim links; do
    "$sh2_cc" -O2 -m2 -fsigned-char -S -I"$fixture_dir/src" \
        "$fixture_dir/src/$name.c" -o "$build_dir/$name.s"
    sh-elf-as -no-pad-sections "$build_dir/$name.s" \
        -o "$build_dir/$name.cof"
    sh-elf-objcopy -Icoff-sh -Oelf32-sh "$build_dir/$name.cof" \
        "$build_dir/$name.o"
done

sh-elf-ld --no-check-sections -nostdlib \
    -T "$fixture_dir/bootstrap.ld" \
    -o "$build_dir/original.elf"
sh-elf-objcopy -O binary "$build_dir/original.elf" \
    "$build_dir/original.bin"

echo "b5bd17ac8e0e1ce45801ed22c984f73cb6ad2c12  $build_dir/original.bin" | sha1sum -c -

target/release/rust-dis "$fixture_dir/config.yaml"

sh-elf-ld --no-check-sections -nostdlib \
    -T "$build_dir/fixture.ld" \
    -o "$build_dir/actual.elf"
sh-elf-objcopy -O binary "$build_dir/actual.elf" \
    "$build_dir/actual.bin"

cmp "$build_dir/original.bin" "$build_dir/actual.bin"
echo "SH-2 named data fixture: PASS"
