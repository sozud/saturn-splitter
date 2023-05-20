# launch compare script in docker
chmod +x scripts/compile_dosemu/compare.sh
docker run --rm -it -v $(pwd):/dis binutils-sh-elf:latest /bin/bash -c ./dis/scripts/compile_dosemu/compare.sh
