SECTIONS
{
    .fixture 0x06010000 :
    {
        build/sh2data/header.o(.data)
        ASSERT(. - ADDR(.fixture) == 0x8, "main starts at the wrong offset");
        build/sh2data/main.o(.text)
        ASSERT(. - ADDR(.fixture) == 0x2C, "animations starts at the wrong offset");
        build/sh2data/anim.o(.data)
        ASSERT(. - ADDR(.fixture) == 0x34, "links starts at the wrong offset");
        build/sh2data/links.o(.data)
    }
    /DISCARD/ : { *(*); }
}
