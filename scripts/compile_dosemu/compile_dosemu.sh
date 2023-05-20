# dosemu commands

# build object
HOME="." dosemu -dumb -f ./dosemurc -K . -E "GCC.EXE -c -O2 -m2 -fsigned-char output.c -o output.o"
# build asm for debugging
HOME="." dosemu -dumb -f ./dosemurc -K . -E "GCC.EXE -c -O2 -m2 -fsigned-char -S output.c -o output.s"
