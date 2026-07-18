#include "types.h"

extern u8 header_magic[];
extern u16 animation_frames[];

u32 fixture_sum(void) {
    return header_magic[0] + header_magic[1] + animation_frames[0];
}

