#include "types.h"

extern u8 header_magic[];
extern u32 fixture_sum(void);

u8* header_pointer = header_magic;
u32 (*function_pointer)(void) = fixture_sum;

