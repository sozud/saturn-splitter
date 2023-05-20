# strip our binary
sh-elf-objcopy /dis/temp/output.o -O binary /dis/temp/mine.bin &&
# disassemble our binary
sh-elf-objdump -z -m sh2 -b binary -D /dis/temp/mine.bin > /dis/temp/mine.txt &&
# disassemble their binary
sh-elf-objdump -z -m sh2 -b binary -D /dis/T_BAT.PRG > /dis/temp/theirs.txt &&
# diff
diff /dis/temp/mine.txt /dis/temp/theirs.txt > /dis/temp/diff.txt
