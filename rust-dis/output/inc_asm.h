#ifndef INCLUDE_ASM_H
#define INCLUDE_ASM_H

#define STRINGIFY_(x) #x
#define STRINGIFY(x) STRINGIFY_(x)

#ifndef PERMUTER

#ifndef INCLUDE_ASM

// #define INCLUDE_ASM(FOLDER, NAME)                                              \
//     __asm__(".section .text\n"                                                 \
//             "\t.align\t2\n"                                                    \
//             "\t.globl\t" #NAME "\n"                                            \
//             ".include \"" FOLDER "/" #NAME ".s\"\n"                            \
//             "\t.end\t" #NAME);
// #endif

	// .text
	// .align 2
	// .global	_func_80173E78

#define INCLUDE_ASM(FOLDER, NAME)                                              \
    __asm__(".text\n"                                                 \
            "\t.align\t2\n"                                                    \
            "\t.global\t" #NAME "\n"                                            \
            ".include \"" FOLDER "/" #NAME ".s\"\n");
#endif

// #define INCLUDE_ASM(FOLDER, NAME)                                              \
//     __asm__(".include \"" FOLDER "/" #NAME ".s\"\n");
// #endif

// omit .global
__asm__(".include \"macro.inc\"\n");

#else
#define INCLUDE_ASM(FOLDER, NAME)
#endif

#endif
