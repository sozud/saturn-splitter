# copy the compiler, sources, and compile script into a temp folder
mkdir -p temp
cp -r ./GCCSH/* temp/
cp  ./rust-dis/output/inc_asm.h ./temp/
cp  ./rust-dis/output/macro.inc ./temp/
cp  ./rust-dis/output/output.c ./temp/
cp -r ./rust-dis/output/funcs ./temp/
cp  ./scripts/compile_dosemu/compile_dosemu.sh ./temp/
chmod +x ./temp/compile_dosemu.sh
mkdir -p .dosemu

# execute in docker
docker run --rm -v $(pwd)/:/pwd -w /pwd/temp dosemu:latest /bin/bash -c "./compile_dosemu.sh"
