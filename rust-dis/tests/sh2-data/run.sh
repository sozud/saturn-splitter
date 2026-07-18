#!/bin/sh
set -eu

fixture_dir=tools/saturn-splitter/rust-dis/tests/sh2-data
build_dir=build/sh2data

mkdir -p "$build_dir"

for name in header main anim links; do
    cpp -I"$fixture_dir/src" -undef -D__GNUC__=2 -D__GNUC_MINOR__=7 \
        -D__sh__ -D__sh2__ "$fixture_dir/src/$name.c" "$build_dir/$name.cpp"
    sh tools/builds/dosemu_wrapper.sh \
        "$build_dir/$name.cpp" "$build_dir/$name.s" O2
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

tools/saturn-splitter/rust-dis/target/release/rust-dis "$fixture_dir/config.yaml"

sh-elf-ld --no-check-sections -nostdlib \
    -T "$build_dir/fixture.ld" \
    -o "$build_dir/actual.elf"
sh-elf-objcopy -O binary "$build_dir/actual.elf" \
    "$build_dir/actual.bin"

cmp "$build_dir/original.bin" "$build_dir/actual.bin"
echo "SH-2 named data fixture: PASS"
