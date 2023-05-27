// why is 14c0 made into data?

use serde_derive::Deserialize;
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};

struct DataLabel {
    size: u32,
    label: String,
    source: u32,
}

fn fetch_instruction(
    file_contents: &Vec<u8>,
    virtual_address: u64,
    virtual_address_base: u64,
) -> u32 {
    let physical_address = virtual_address - virtual_address_base;
    let instr: u32 = ((file_contents[physical_address as usize] as u32) << 8)
        | file_contents[physical_address as usize + 1] as u32;
    return instr;
}

fn match_ni_f(
    _v_addr: u32,
    op: u32,
    _mode: bool,
    string: &mut String,
    _data_labels: &HashMap<u32, DataLabel>,
    _branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf000 {
        0x7000 => string.push_str(&format!("add #0x{:02X}, r{}", op & 0xff, (op >> 8) & 0xf)),
        0xe000 => string.push_str(&format!("mov #{}, r{}", op & 0xff, (op >> 8) & 0xf)),
        // unknown
        _ => string.push_str(&format!(".word 0x{:04X} /* unknown instruction */", op)),
    }
}

fn match_i_f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xff00 {
        0xcd00 => string.push_str(&format!("and.b #0x{:02X}, @(r0, gbr)", op & 0xff)),
        0xcf00 => string.push_str(&format!("or.b #0x{:02X}, @(r0, gbr)", op & 0xff)),
        0xcc00 => string.push_str(&format!("tst.b #0x{:02X}, @(r0, gbr)", op & 0xff)),
        0xce00 => string.push_str(&format!("xor.b #0x{:02X}, @(r0, gbr)", op & 0xff)),
        0xc900 => string.push_str(&format!("and #0x{:02X}, r0", op & 0xff)),
        0x8800 => string.push_str(&format!("cmp/eq #0x{:02X}, r0", op & 0xff)),
        0xcb00 => string.push_str(&format!("or #0x{:02X}, r0", op & 0xff)),
        0xc800 => string.push_str(&format!("tst #0x{:02X}, r0", op & 0xff)),
        0xca00 => string.push_str(&format!("xor #0x{:02X}, r0", op & 0xff)),
        0xc300 => string.push_str(&format!("trapa #0x{:X}", op & 0xff)),
        _ => match_ni_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}

fn match_nd8_f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf000 {
        0x9000 => {
            // "mov.w @(0x%03X, pc), r%d"
            string.push_str(&format!(
                "mov.w @(0x{:03X}, pc), r{}",
                (op & 0xff) * 2 + 4,
                (op >> 8) & 0xf
            ));
        }
        0xd000 => {
            // "mov.l @(0x%03X, pc), r%d"
            let v_addr_aligned = (v_addr & 0xfffffffc) == 0;
            // this post explains part of issue. https://dcemulation.org/phpBB/viewtopic.php?style=41&t=105661
            let mut target_a = (op & 0xff) * 4 + 4;
            let target_b = ((op & 0xff) * 4 + 4 + v_addr) & 0xfffffffc;
            let test = (op & 0xff) * 4 + 4 + v_addr;

            // gas alignment issue.
            if (test & 3) == 2 {
                // subtract 2 from target_a
                target_a -= 2;

                string.push_str(&format!(
                    "mov.l @(0x{:03X}, pc), r{}",
                    target_a,
                    (op >> 8) & 0xf
                ));
            } else {
                string.push_str(&format!(
                    "mov.l @(0x{:03X}, pc), r{}",
                    target_a,
                    (op >> 8) & 0xf
                ));
            }
        }
        _ => match_i_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}

fn match_d12_f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf000 {
        0xa000 => {
            if (op & 0x800) == 0x800 {
                let addr = ((op & 0xfff) + 0xfffff000)
                    .wrapping_mul(2)
                    .wrapping_add(v_addr)
                    .wrapping_add(4);
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bra {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bra 0x{:08X}", addr));
                }
            } else {
                let addr = (op & 0xfff) * 2 + v_addr + 4;
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bra {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bra 0x{:08X}", addr));
                }
            }
        }
        0xb000 => {
            if (op & 0x800) == 0x800 {
                let addr = ((op & 0xfff) + 0xfffff000).wrapping_mul(2) + v_addr + 4;
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bsr {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bsr 0x{:08X}", addr));
                }
            } else {
                let addr = (op & 0xfff) * 2 + v_addr + 4;
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bsr {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bsr 0x{:08X}", addr));
                }
            }
        }
        _ => match_nd8_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}

fn match_d_f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xff00 {
        0xc000 => string.push_str(&format!("mov.b r0, @(0x{:03X}, gbr)", (op & 0xff) * 1)),
        0xc100 => string.push_str(&format!("mov.w r0, @(0x{:03X}, gbr)", (op & 0xff) * 2)),
        0xc200 => string.push_str(&format!("mov.l r0, @(0x{:03X}, gbr)", (op & 0xff) * 4)),
        0xc400 => string.push_str(&format!("mov.b @(0x{:03X}, gbr), r0", (op & 0xff) * 1)),
        0xc500 => string.push_str(&format!("mov.w @(0x{:03X}, gbr), r0", (op & 0xff) * 2)),
        0xc600 => string.push_str(&format!("mov.l @(0x{:03X}, gbr), r0", (op & 0xff) * 4)),

        // mova
        0xc600 => {}
        0x8b00 => {
            if (op & 0x80) == 0x80 {
                let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                    .wrapping_add(v_addr)
                    .wrapping_add(4);
                // string.push_str(&format!("bf 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bf {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bf 0x{:08X}", addr));
                }
            } else {
                let addr = ((op & 0xff) * 2) + v_addr + 4;
                // string.push_str(&format!("bf 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bf {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bf 0x{:08X}", addr));
                }
            }
        }
        0x8f00 => {
            if (op & 0x80) == 0x80 {
                let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                    .wrapping_add(v_addr)
                    .wrapping_add(4);
                // string.push_str(&format!("bf.s 0x{:08X}", addr));
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bf.s {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bf.s 0x{:08X}", addr));
                }
            } else {
                let addr = ((op & 0xff) * 2) + v_addr + 4;
                // string.push_str(&format!("bf.s 0x{:08X}", addr));
                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bf.s {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bf.s 0x{:08X}", addr));
                }
            }
        }
        0x8900 => {
            if (op & 0x80) == 0x80 {
                let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                    .wrapping_add(v_addr)
                    .wrapping_add(4);
                // string.push_str(&format!("bt 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bt {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bt 0x{:08X}", addr));
                }
            } else {
                let addr = ((op & 0xff) * 2) + v_addr + 4;
                // string.push_str(&format!("bt 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bt {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bt 0x{:08X}", addr));
                }
            }
        }
        0x8d00 => {
            if (op & 0x80) == 0x80 {
                let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                    .wrapping_add(v_addr)
                    .wrapping_add(4);
                // string.push_str(&format!("bt.s 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bt.s {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bt.s 0x{:08X}", addr));
                }
            } else {
                let addr = ((op & 0xff) * 2) + v_addr + 4;
                // string.push_str(&format!("bt.s 0x{:08X}", addr));

                if branch_labels.contains_key(&addr) {
                    if let Some(value) = branch_labels.get(&addr) {
                        // Use the label
                        string.push_str(&format!("bt.s {}", value));
                    }
                } else {
                    // use an address
                    string.push_str(&format!("bt.s 0x{:08X}", addr));
                }
            }
        }
        _ => match_d12_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}

fn match_nmd_f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf000 {
        0x1000 => string.push_str(&format!(
            "mov.l r{}, @(0x{:03X}, r{})",
            (op >> 4) & 0xf,
            (op & 0xf) * 4,
            (op >> 8) & 0xf
        )),
        0x5000 => string.push_str(&format!(
            "mov.l @(0x{:03X}, r{}), r{}",
            (op & 0xf) * 4,
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        _ => match_d_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}
fn match_ff00(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xff00 {
        0x8400 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.b @(0x{:03X}, r{}), r0",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.b @(0x{:03X}, r{}), r0",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8500 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.b @(0x{:03X}, r{}), r0",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.w @(0x{:03X}, r{}), r0",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8000 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.b r0, @(0x{:03X}, r{})",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.b r0, @(0x{:03X}, r{})",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8100 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.w r0, @(0x{:03X}, r{})",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.w r0, @(0x{:03X}, r{})",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        _ => match_nmd_f(v_addr, op, mode, string, data_labels, branch_labels),
    }
}

fn match_f00f(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf00f {
        0x300c => string.push_str(&format!("add r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x300e => string.push_str(&format!("addc r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x300f => string.push_str(&format!("addv r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x2009 => string.push_str(&format!("and r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x3000 => string.push_str(&format!(
            "cmp/eq r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3002 => string.push_str(&format!(
            "cmp/hs r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3003 => string.push_str(&format!(
            "cmp/ge r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3006 => string.push_str(&format!(
            "cmp/hi r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3007 => string.push_str(&format!(
            "cmp/gt r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x200c => string.push_str(&format!(
            "cmp/str r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3004 => string.push_str(&format!("div1 r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x2007 => string.push_str(&format!("div0s r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x300d => string.push_str(&format!(
            "dmuls.l r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x3005 => string.push_str(&format!(
            "dmulu.l r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x600e => string.push_str(&format!(
            "exts.b r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x600f => string.push_str(&format!(
            "exts.w r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x600c => string.push_str(&format!(
            "extu.b r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x600d => string.push_str(&format!(
            "extu.w r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6003 => string.push_str(&format!("mov r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x0007 => string.push_str(&format!("mul.l r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x200f => string.push_str(&format!("muls r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x200e => string.push_str(&format!("mulu r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x600b => string.push_str(&format!("neg r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x600a => string.push_str(&format!("negc r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x6007 => string.push_str(&format!("not r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x200b => string.push_str(&format!("or r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x3008 => string.push_str(&format!("sub r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x300a => string.push_str(&format!("subc r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x300b => string.push_str(&format!("subv r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x6008 => string.push_str(&format!(
            "swap.b r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6009 => string.push_str(&format!(
            "swap.w r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2008 => string.push_str(&format!("tst r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x200a => string.push_str(&format!("xor r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x200d => string.push_str(&format!("xtrct r{}, r{}", (op >> 4) & 0xf, (op >> 8) & 0xf)),
        0x2000 => string.push_str(&format!(
            "mov.b r{}, @r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2001 => string.push_str(&format!(
            "mov.w r{}, @r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2002 => string.push_str(&format!(
            "mov.l r{}, @r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6000 => string.push_str(&format!(
            "mov.b @r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6001 => string.push_str(&format!(
            "mov.w @r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6002 => string.push_str(&format!(
            "mov.l @r{}, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000f => string.push_str(&format!(
            "mac.l @r{}+, @r{}+",
            (op >> 8) & 0xf,
            (op >> 4) & 0xf
        )),
        0x400f => string.push_str(&format!(
            "mac.w @r{}+, @r{}+",
            (op >> 8) & 0xf,
            (op >> 4) & 0xf
        )),
        0x6004 => string.push_str(&format!(
            "mov.b @r{}+, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6005 => string.push_str(&format!(
            "mov.w @r{}+, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6006 => string.push_str(&format!(
            "mov.l @r{}+, r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2004 => string.push_str(&format!(
            "mov.b r{}, @-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2005 => string.push_str(&format!(
            "mov.w r{}, @-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2006 => string.push_str(&format!(
            "mov.l r{}, @-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0004 => string.push_str(&format!(
            "mov.b r{}, @(r0, r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0005 => string.push_str(&format!(
            "mov.w r{}, @(r0, r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0006 => string.push_str(&format!(
            "mov.l r{}, @(r0, r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000c => string.push_str(&format!(
            "mov.b @(r0, r{}), r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000d => string.push_str(&format!(
            "mov.w @(r0, r{}), r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000e => string.push_str(&format!(
            "mov.l @(r0, r{}), r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        _ => match_ff00(v_addr, op, mode, string, data_labels, branch_labels),
    }
}
fn match_f0ff(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xf0ff {
        0x4015 => string.push_str(&format!("cmp/pl r{}", (op >> 8) & 0xf)),
        0x4011 => string.push_str(&format!("cmp/pz r{}", (op >> 8) & 0xf)),
        0x4010 => string.push_str(&format!("dt r{}", (op >> 8) & 0xf)),
        0x0029 => string.push_str(&format!("movt r{}", (op >> 8) & 0xf)),
        0x4004 => string.push_str(&format!("rotl r{}", (op >> 8) & 0xf)),
        0x4005 => string.push_str(&format!("rotr r{}", (op >> 8) & 0xf)),
        0x4024 => string.push_str(&format!("rotcl r{}", (op >> 8) & 0xf)),
        0x4025 => string.push_str(&format!("rotcr r{}", (op >> 8) & 0xf)),
        0x4020 => string.push_str(&format!("shal r{}", (op >> 8) & 0xf)),
        0x4021 => string.push_str(&format!("shar r{}", (op >> 8) & 0xf)),
        0x4000 => string.push_str(&format!("shll r{}", (op >> 8) & 0xf)),
        0x4001 => string.push_str(&format!("shlr r{}", (op >> 8) & 0xf)),
        0x4008 => string.push_str(&format!("shll2 r{}", (op >> 8) & 0xf)),
        0x4009 => string.push_str(&format!("shlr2 r{}", (op >> 8) & 0xf)),
        0x4018 => string.push_str(&format!("shll8 r{}", (op >> 8) & 0xf)),
        0x4019 => string.push_str(&format!("shlr8 r{}", (op >> 8) & 0xf)),
        0x4028 => string.push_str(&format!("shll16 r{}", (op >> 8) & 0xf)),
        0x4029 => string.push_str(&format!("shlr16 r{}", (op >> 8) & 0xf)),
        0x0002 => string.push_str(&format!("stc sr, r{}", (op >> 8) & 0xf)),
        0x0012 => string.push_str(&format!("stc gbr, r{}", (op >> 8) & 0xf)),
        0x0022 => string.push_str(&format!("stc vbr, r{}", (op >> 8) & 0xf)),
        0x000a => string.push_str(&format!("sts mach, r{}", (op >> 8) & 0xf)),
        0x001a => string.push_str(&format!("sts macl, r{}", (op >> 8) & 0xf)),
        0x402a => string.push_str(&format!("sts pr, r{}", (op >> 8) & 0xf)),
        0x401b => string.push_str(&format!("tas.b r{}", (op >> 8) & 0xf)),
        0x4003 => string.push_str(&format!("stc.l sr, @-r{}", (op >> 8) & 0xf)),
        0x4013 => string.push_str(&format!("stc.l gbr, @-r{}", (op >> 8) & 0xf)),
        0x4023 => string.push_str(&format!("stc.l vbr, @-r{}", (op >> 8) & 0xf)),
        0x4002 => string.push_str(&format!("sts.l mach, @-r{}", (op >> 8) & 0xf)),
        0x4012 => string.push_str(&format!("sts.l macl, @-r{}", (op >> 8) & 0xf)),
        0x4022 => string.push_str(&format!("sts.l pr, @-r{}", (op >> 8) & 0xf)),
        0x400e => string.push_str(&format!("ldc r{}, sr", (op >> 8) & 0xf)),
        0x401e => string.push_str(&format!("ldc r{}, gbr", (op >> 8) & 0xf)),
        0x402e => string.push_str(&format!("ldc r{}, vbr", (op >> 8) & 0xf)),
        0x400a => string.push_str(&format!("lds r{}, mach", (op >> 8) & 0xf)),
        0x401a => string.push_str(&format!("lds r{}, macl", (op >> 8) & 0xf)),
        0x402a => string.push_str(&format!("lds r{}, pr", (op >> 8) & 0xf)),
        0x402b => string.push_str(&format!("jmp @r{}", (op >> 8) & 0xf)),
        0x400b => string.push_str(&format!("jsr @r{}", (op >> 8) & 0xf)),
        0x4007 => string.push_str(&format!("ldc.l @r{}+, sr", (op >> 8) & 0xf)),
        0x4017 => string.push_str(&format!("ldc.l @r{}+, gbr", (op >> 8) & 0xf)),
        0x4027 => string.push_str(&format!("ldc.l @r{}+, vbr", (op >> 8) & 0xf)),
        0x4006 => string.push_str(&format!("lds.l @r{}+, mach", (op >> 8) & 0xf)),
        0x4016 => string.push_str(&format!("lds.l @r{}+, macl", (op >> 8) & 0xf)),
        0x4026 => string.push_str(&format!("lds.l @r{}+, pr", (op >> 8) & 0xf)),
        0x0023 => string.push_str(&format!("braf r{}", (op >> 8) & 0xf)),
        0x0003 => string.push_str(&format!("bsrf r{}", (op >> 8) & 0xf)),
        _ => {
            match_f00f(v_addr, op, mode, string, data_labels, branch_labels);
        }
    }
}

fn sh2_disasm(
    v_addr: u32,
    op: u32,
    mode: bool,
    string: &mut String,
    data_labels: &HashMap<u32, DataLabel>,
    branch_labels: &HashMap<u32, String>,
) {
    match op & 0xffff {
        0x0008 => string.push_str("clrt"),
        0x0028 => string.push_str("clrmac"),
        0x0019 => string.push_str("div0u"),
        0x0009 => string.push_str("nop"),
        0x002b => string.push_str("rte"),
        0x000b => string.push_str("rts"),
        0x0018 => string.push_str("sett"),
        0x001b => string.push_str("sleep"),
        _ => {
            match_f0ff(v_addr, op, mode, string, data_labels, branch_labels);
        }
    }
}

fn read_file_to_vec(filename: &str) -> io::Result<Vec<u8>> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

struct FunctionRange {
    phys_start: u32,
    phys_end: u32,
    is_data: bool,
}

fn find_funcs(
    vec: &Vec<u8>,
    section_start: u64,
    section_end: u64,
    ranges: &mut Vec<FunctionRange>,
) {
    // first, find every location of an rts.
    let mut rts_pos: Vec<u32> = Vec::new();
    for i in (section_start..section_end).step_by(2) {
        let instr = (vec[i as usize] as u32) << 8 | vec[i as usize + 1] as u32;
        if instr == 0x000b {
            rts_pos.push(i as u32);
        }
    }

    for i in 0..rts_pos.len() {
        let pc = rts_pos[i] - 2;
        let prev_rts = if i > 0 { rts_pos[i - 1] } else { 0 };
        let mut func_start = 0;
        let mut preamble_found = false;

        let mut pc = pc;
        // now scan back from rts[i] to rts[i - 1] to try to find the function preamble
        while pc >= prev_rts && pc > 0 {
            let instr = (vec[pc as usize] as u32) << 8 | vec[(pc + 1) as usize] as u32;

            if !preamble_found {
                if instr & 0xFF0F == 0x2F06 {
                    preamble_found = true;
                }
            } else {
                if instr & 0xFF06 != 0x2F06 {
                    func_start = pc + 2;
                    break;
                }
            }

            pc -= 2;
        }

        if func_start != 0 {
            let range = FunctionRange {
                phys_start: func_start,
                phys_end: rts_pos[i] + 2,
                is_data: false,
            };
            ranges.push(range);
        }
    }
}

fn infunc(i: u32, ranges: &Vec<FunctionRange>) -> (bool, u32) {
    for j in 0..ranges.len() {
        let current_func = &ranges[j];
        if i >= current_func.phys_start && i <= current_func.phys_end {
            return (true, current_func.phys_start);
        }
    }
    (false, 0)
}

fn infunc_extended(i: u32, ranges: &Vec<FunctionRange>) -> (bool, u32) {
    for j in 0..ranges.len() {
        let current_func = &ranges[j];
        if i >= current_func.phys_start && i <= current_func.phys_end {
            // trivially in func
            return (true, current_func.phys_start);
        }
    }

    // check after funcs
    for j in 0..ranges.len() - 1 {
        let current_func = &ranges[j];
        let next_func = &ranges[j + 1];
        if i >= current_func.phys_start && i < next_func.phys_start {
            // in func rodata
            return (true, current_func.phys_start);
        }
    }

    (false, 0)
}

fn is_beyond_last_func(i: u32, ranges: &Vec<FunctionRange>) -> (bool, u32) {
    let last_func = &ranges[ranges.len() - 1];

    if i > last_func.phys_end {
        return (true, last_func.phys_start);
    }

    (false, 0)
}

fn add_label(addr: u32, branch_labels: &mut HashMap<u32, String>) {
    let label = format!("lab_{:08X}", addr);
    branch_labels.insert(addr, label);
}

fn add_data_label(source: u32, addr: u32, size: u32, data_labels: &mut HashMap<u32, DataLabel>) {
    let the_label = format!("dat_{:08X}", addr);
    let data_label = DataLabel {
        size,
        label: the_label,
        source: source,
    };
    data_labels.insert(addr, data_label);
}

fn find_branch_labels(v_addr: u32, op: u32, branch_labels: &mut HashMap<u32, String>) {
    let is_bf = (op & 0xff00) == 0x8b00;
    let is_bfs = (op & 0xff00) == 0x8f00;
    let is_bt = (op & 0xff00) == 0x8900;
    let is_bts = (op & 0xff00) == 0x8d00;

    let is_bra = (op & 0xf000) == 0xa000;
    let is_bsr = (op & 0xf000) == 0xb000;

    if is_bf || is_bfs || is_bt || is_bts {
        // bf
        if op & 0x80 != 0 {
            /* sign extend */
            let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                .wrapping_add(v_addr)
                .wrapping_add(4);
            add_label(addr, branch_labels);
        } else {
            let addr = ((op & 0xff) * 2) + v_addr + 4;
            add_label(addr, branch_labels);
        }
    } else if is_bra || is_bsr {
        if op & 0x800 != 0 {
            /* sign extend */
            let addr = ((op & 0xfff) + 0xfffff000)
                .wrapping_mul(2)
                .wrapping_add(v_addr)
                .wrapping_add(4);
            add_label(addr, branch_labels);
        } else {
            let addr = (op & 0xfff) * 2 + v_addr + 4;
            add_label(addr, branch_labels);
        }
    }
}

fn find_data_labels(v_addr: u32, op: u32, data_labels: &mut HashMap<u32, DataLabel>) {
    // is this already marked as data?
    if data_labels.contains_key(&v_addr.try_into().unwrap()) {
        // don't try to dissassemble as an instruction
        return;
    }

    // is this marked as the second word of long data?
    if data_labels.contains_key(&(v_addr - 2).try_into().unwrap()) {
        if let Some(value) = data_labels.get(&(v_addr - 2).try_into().unwrap()) {
            if value.size == 4 {
                // println!("skipping {:08X}", v_addr);
                return;
            }
        }
    }

    if (op & 0xf000) == 0x9000 {
        let addr = (op & 0xff) * 2 + 4 + v_addr;
        add_data_label(v_addr, addr, 2, data_labels);
    } else if (op & 0xf000) == 0xd000 {
        let target = ((op & 0xff) * 4 + 4 + v_addr) & 0xfffffffc;

        if v_addr == 0x6d94 {
            println!("problem {:08X}", target);
            // return;
        }
        // TODO fixme this shouln't be marked as data
        if target == 0x14C0 {
            println!("problem {:08X}", v_addr);
            return;
        }

        if target == 0x35c8 {
            println!("problem {:08X}", v_addr);
            return;
        }
        add_data_label(v_addr, target, 4, data_labels);
    }
}

#[derive(Debug, Deserialize)]
struct Options {
    target_path: String,
    asm_path: String,
    src_path: String,
    decomp_empty_funcs: bool,
}

#[derive(Debug, Deserialize)]
struct Subsegment {
    start: u64,
    end: u64,
    #[serde(rename = "type")]
    segment_type: Option<String>,
    file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Segment {
    name: String,
    #[serde(rename = "type")]
    segment_type: String,
    start: u64,
    subsegments: Option<Vec<Subsegment>>,
}

#[derive(Debug, Deserialize)]
struct Config {
    options: Options,
    segments: Option<Vec<Segment>>,
}

fn parse_yaml2(filename: String) -> Config {
    // Read the YAML configuration file
    let mut file =
        File::open(filename.clone()).expect(&format!("Failed to open the file {}", filename));
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Failed to read the file.");

    // Parse the YAML into a Config struct
    let config: Config = serde_yaml::from_str(&contents).expect("Failed to parse YAML.");

    return config;
}
use std::io::Write;

#[derive(Debug)]
struct FunctionPair {
    file: String,
    name: String,
    start: u32,
    end: u32,
}

fn emit_c_file(functions: &Vec<FunctionPair>, output_path: String) {
    let filename = format!("{}/output.c", output_path);
    let mut file = std::fs::File::create(filename).expect("Failed to create file.");
    writeln!(&mut file, "#include \"inc_asm.h\"").expect("Failed to write to file.");
    for pair in functions {
        writeln!(&mut file, "INCLUDE_ASM(\"{}\", {});", pair.file, pair.name)
            .expect("Failed to write to file.");
    }
}

fn emit_asm_file(filename: String, data: String) {
    let mut file = std::fs::File::create(filename).expect("Failed to create file.");
    writeln!(&mut file, "{}", data).expect("Failed to write to file.");
}

#[derive(Debug)]
struct DisassembledFunc {
    addr: u32,
    end: u32,
    text: String,
    data: bool,
}

fn handle_code_section(
    file_contents: &Vec<u8>,
    section_start: u64,
    section_end: u64,
) -> (Vec<FunctionPair>, HashMap<u32, DisassembledFunc>) {
    let len = file_contents.len();
    let mut ranges = Vec::<FunctionRange>::new();
    find_funcs(&file_contents, section_start, section_end, &mut ranges);

    let mut functions = Vec::<FunctionPair>::new();

    for r in &ranges {
        println!(
            "{:08X} {:08X} len: {}",
            r.phys_start,
            r.phys_end,
            r.phys_end - r.phys_start
        );
        let mut name = String::new();
        name.push_str(&format!("f_{:05X}", r.phys_start));
        let pair = FunctionPair {
            file: "funcs".to_string(),
            name: name,
            start: r.phys_start,
            end: r.phys_end,
        };
        functions.push(pair);
    }

    if ranges.len() == 0 {
        println!("no ranges");
    }

    let mut data_labels = HashMap::<u32, DataLabel>::new();
    let mut branch_labels = HashMap::<u32, String>::new();

    for i in (section_start..section_end).step_by(2) {
        let ii = i as usize;
        let instr: u32 = ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;

        let (is_in_func, start_address) = infunc(i as u32, &ranges);

        if !is_in_func {
            continue;
        }

        find_branch_labels(i.try_into().unwrap(), instr, &mut branch_labels);
        find_data_labels(i.try_into().unwrap(), instr, &mut data_labels);
    }

    let mut disassembled_funcs = HashMap::<u32, DisassembledFunc>::new();

    // create emtpy ones for all funcs
    for f in &ranges {
        disassembled_funcs
            .entry(f.phys_start)
            .or_insert(DisassembledFunc {
                addr: f.phys_start,
                end: f.phys_end,
                text: "".to_string(),
                data: f.is_data,
            });
    }

    let mut monolithic = String::new();

    let mut skip_next = false;

    for i in (section_start..section_end).step_by(2) {
        if skip_next {
            skip_next = false;
            continue;
        }

        let ii = i as usize;
        let instr: u32 = ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;

        let (is_in_func, start_address) = infunc(i as u32, &ranges);

        // the last function needs to emit data up until the next section

        let (is_in_func_extended, start_address_extended) = infunc_extended(i as u32, &ranges);

        let (beyond_last_func, beyond_addr) = is_beyond_last_func(i as u32, &ranges);

        if beyond_last_func {
            if let Some(func) = disassembled_funcs.get_mut(&(beyond_addr as u32)) {
                func.text
                    .push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, instr));
            }

            if ii as u64 >= section_end - 2 {
                break;
            }
            continue;
        }

        if !is_in_func && is_in_func_extended {
            // data after function, emit for individual files
            monolithic.push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, instr));

            if let Some(func) = disassembled_funcs.get_mut(&(start_address_extended as u32)) {
                func.text
                    .push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, instr));
            }
        }

        if !is_in_func {
            monolithic.push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, instr));
            continue;
        } else {
            // if this is the first instruction emit the function label
            if i as u32 == start_address {
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!("glabel func_{:08X}\n", i));
                }
            }
        }

        if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
            if func.data {
                func.text
                    .push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, instr));
                continue;
            }
        }

        if branch_labels.contains_key(&i.try_into().unwrap()) {
            if let Some(value) = branch_labels.get(&i.try_into().unwrap()) {
                // Use the label
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!("{}:\n", value));
                    monolithic.push_str(&format!("{}:\n", value));
                }
            }
        }

        if data_labels.contains_key(&i.try_into().unwrap()) {
            if let Some(value) = data_labels.get(&i.try_into().unwrap()) {
                // Use the label
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!(
                        "{}: /* source: {:08X} */\n",
                        value.label, value.source
                    ));
                    monolithic.push_str(&format!("{}:\n", value.label));
                }
                if value.size == 2 {
                    if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                        func.text.push_str(&format!(".word 0x{:04X}\n", instr));
                        monolithic.push_str(&format!(".word 0x{:04X}\n", instr));
                    }
                } else if value.size == 4 {
                    let data = ((file_contents[i as usize + 0] as u32) << 24)
                        | ((file_contents[i as usize + 1] as u32) << 16)
                        | ((file_contents[i as usize + 2] as u32) << 8)
                        | ((file_contents[i as usize + 3] as u32) << 0);
                    if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                        func.text
                            .push_str(&format!("/* {:08X} */ .long 0x{:08X}\n", i, data));
                        monolithic.push_str(&format!("/* {:08X} */ .long 0x{:08X}\n", i, data));
                    }

                    // skip next instructino since we used it

                    skip_next = true;
                }
                continue;
            }
        }
        let mut string = String::new();
        sh2_disasm(
            i as u32,
            instr,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
            func.text
                .push_str(&format!("/* 0x{:08X} 0x{:04X} */ {}\n", i, instr, string));
            monolithic.push_str(&format!("/* 0x{:08X} */ {}\n", i, string));
        }
    }

    return (functions, disassembled_funcs);
}

#[derive(Default)]
struct ProcessedSection {
    is_code: bool,
    functions: Vec<FunctionPair>,
    disassembled_funcs: HashMap<u32, DisassembledFunc>,
    data: String,
    addr: u64,
}

fn handle_segments(file_contents: &Vec<u8>, config: &Config) {
    let mut processed_sections = Vec::<ProcessedSection>::new();
    if let Some(segments) = &config.segments {
        for segment in segments {
            println!("Segment Name: {}", segment.name);
            println!("Segment Type: {}", segment.segment_type);
            println!("Segment Start: {}", segment.start);

            if let Some(subsegments) = &segment.subsegments {
                for subsegment in subsegments {
                    let subsegment_start = subsegment.start;
                    let subsegment_type = subsegment
                        .segment_type
                        .as_ref()
                        .unwrap_or(&"Unknown".to_string())
                        .clone();
                    let subsegment_file = subsegment
                        .file
                        .as_ref()
                        .unwrap_or(&"Unknown".to_string())
                        .clone();

                    let subsegment_start = subsegment.start;
                    let subsegment_end = subsegment.end;

                    println!(
                        "subsegment {:08X}-{:08X} {} {}",
                        subsegment_start, subsegment_end, subsegment_type, subsegment_file,
                    );

                    if subsegment_type == "data" {
                        // just emit words
                        let mut data_str = String::new();
                        for i in (subsegment_start..subsegment_end).step_by(2) {
                            let ii = i as usize;
                            let data: u32 =
                                ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;
                            data_str.push_str(&format!("/* 0x{:08X} */ .word 0x{:04X}\n", i, data));
                        }

                        let processed_section = ProcessedSection {
                            is_code: false,
                            functions: Vec::<FunctionPair>::new(),
                            disassembled_funcs: HashMap::<u32, DisassembledFunc>::new(),
                            data: data_str,
                            addr: subsegment_start,
                        };
                        processed_sections.push(processed_section);
                    } else {
                        // find functions and process
                        let (functions, disassembled_funcs) =
                            handle_code_section(file_contents, subsegment_start, subsegment_end);

                        let processed_section = ProcessedSection {
                            is_code: true,
                            functions: functions,
                            disassembled_funcs: disassembled_funcs,
                            data: "".to_string(),
                            addr: subsegment_start,
                        };
                        processed_sections.push(processed_section);
                    }
                }
            }
        }
    }

    // all the segments are processed, emit files

    std::fs::create_dir_all(&config.options.asm_path).expect("Failed to create directories.");

    // emit all the asm
    for processed_section in &processed_sections {
        if !processed_section.is_code {
            emit_asm_file(
                format!(
                    "{}/d_{:05X}.s",
                    config.options.asm_path, processed_section.addr
                ),
                processed_section.data.clone(),
            );
        } else {
            for (_addr, df) in &processed_section.disassembled_funcs {
                emit_asm_file(
                    format!("{}/f_{:05X}.s", config.options.asm_path, df.addr),
                    df.text.clone(),
                );
            }
        }
    }

    let path = &config.options.src_path;
    let asm_path = &config.options.asm_path;

    std::fs::create_dir_all(path).expect("Failed to create src_path directories.");

    if let Some(segs) = &config.segments {
        if !segs.is_empty() {
            let segment_name = &segs[0].name;

            let filename = format!("{}/{}.c", path, segment_name);
            let mut file = std::fs::File::create(filename).expect("Failed to create file.");
            writeln!(&mut file, "#include \"inc_asm.h\"").expect("Failed to write to file.");

            for processed_section in &processed_sections {
                if !processed_section.is_code {
                    let name = format!("d_{:05X}", processed_section.addr);
                    writeln!(
                        &mut file,
                        "INCLUDE_ASM(\"{}\", {});",
                        asm_path, // TODO fix hardcode
                        name
                    )
                    .expect("Failed to write to file.");
                } else {
                    for pair in &processed_section.functions {
                        // assume this is a empty function if the size is 8
                        if (pair.end - pair.start == 8) && config.options.decomp_empty_funcs {
                            writeln!(&mut file, "void {}() {{}}", pair.name)
                                .expect("Failed to write to file.");
                        } else {
                            writeln!(&mut file, "INCLUDE_ASM(\"{}\", {});", asm_path, pair.name)
                                .expect("Failed to write to file.");
                        }
                    }
                }
            }
        }
    }
}

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if let Some(filename) = args.get(1) {
        println!("Reading: {}", filename);
        let config = parse_yaml2(filename.to_string());

        match read_file_to_vec(&config.options.target_path) {
            Ok(file_contents) => {
                handle_segments(&file_contents, &config);
            }
            Err(error) => {
                // Error reading the file
                println!("Error: {:?} {}", error, config.options.target_path);
            }
        }
    }
}

use std::fs;
use std::process::Command;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn assemble(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Create a temporary file and write the assembly code to it
    let mut asm_file = NamedTempFile::new()?;
    asm_file.write_all(input.as_bytes())?;

    // Create a temporary file for the output
    let output_file = NamedTempFile::new()?;
   
    let cmd_str = format!(
        "sh-elf-as -o /work/{} /work/{} && sh-elf-objcopy -O binary /work/{} /work/{}",
        output_file.path().file_name().unwrap().to_string_lossy(),
        asm_file.path().file_name().unwrap().to_string_lossy(),
        output_file.path().file_name().unwrap().to_string_lossy(),
        output_file.path().file_name().unwrap().to_string_lossy(),
    );

    let output = Command::new("docker")
        .args(&[
            "run",
            "-v",
            &format!(
                "{}:/work",
                output_file.path().parent().unwrap().to_string_lossy()
            ),
            "binutils-sh-elf",
            "/bin/sh",
            "-c",
            &cmd_str,
        ])
        .output()?;

    // Print stdout
    if !output.stdout.is_empty() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    // Print stderr
    if !output.stderr.is_empty() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    // Read the output file into a byte vector
    let binary = std::fs::read(output_file.path())?;

    Ok(binary)
}

fn asm_test_case(asm: String, expected: String) {
    let output = assemble(&asm).unwrap();

    println!("output: {:?} ", output);

    let mut data_labels = HashMap::<u32, DataLabel>::new();
    let mut branch_labels = HashMap::<u32, String>::new();

    let mut output_string = String::new();

    let (functions, disassembled_funcs) =
        handle_code_section(&output, 0, output.len().try_into().unwrap());

    println!("functions {:?}", functions);
    println!("disassembled_funcs {:?}", disassembled_funcs);

    let trimmed_right: String = expected
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(disassembled_funcs[&8].text, trimmed_right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn do_asm() {
        let asm = r#"
        mov.l r8, @-r15
        mov r0, r1
        rts
        nop
    
        mov.l r8, @-r15
        mov r0, r1
        bra label
        nop
        label:
        mov r1, r0
        rts
        nop
        "#;

        let expected = r#"glabel func_00000008
        /* 0x00000008 0x2F86 */ mov.l r8, @-r15
        /* 0x0000000A 0x6103 */ mov r0, r1
        /* 0x0000000C 0xA000 */ bra lab_00000010
        /* 0x0000000E 0x0009 */ nop
        lab_00000010:
        /* 0x00000010 0x6013 */ mov r1, r0
        /* 0x00000012 0x000B */ rts
        /* 0x00000014 0x0009 */ nop
        "#;

        asm_test_case(asm.to_string(), expected.to_string());
    }

    // #[test]
    // fn test_infunc() {
    //     let ranges = vec![
    //         FunctionRange {
    //             phys_start: 100,
    //             phys_end: 200,
    //         },
    //         FunctionRange {
    //             phys_start: 300,
    //             phys_end: 400,
    //         },
    //         FunctionRange {
    //             phys_start: 500,
    //             phys_end: 600,
    //         },
    //     ];

    //     // Test with values inside the ranges
    //     assert_eq!(infunc(150, &ranges), (true, 100));
    //     assert_eq!(infunc(350, &ranges), (true, 300));
    //     assert_eq!(infunc(550, &ranges), (true, 500));

    //     // Test with values outside the ranges
    //     assert_eq!(infunc(50, &ranges), (false, 0));
    //     assert_eq!(infunc(250, &ranges), (false, 0));
    //     assert_eq!(infunc(700, &ranges), (false, 0));
    // }

    // #[test]
    // fn test_infunc_extended() {
    //     // if we are inbetween functions, include the rodata

    //     let ranges = vec![
    //         FunctionRange {
    //             phys_start: 100,
    //             phys_end: 200,
    //         },
    //         FunctionRange {
    //             phys_start: 300,
    //             phys_end: 400,
    //         },
    //         FunctionRange {
    //             phys_start: 500,
    //             phys_end: 600,
    //         },
    //     ];

    //     //0-99 hasm
    //     //100-200 func1
    //     //201-299 func1 data (include with func1)
    //     //300-400 func2
    //     //401-499 func2 data
    //     //500-600 func3

    //     // Test with values inside the ranges
    //     assert_eq!(infunc_extended(150, &ranges), (true, 100));
    //     assert_eq!(infunc_extended(350, &ranges), (true, 300));
    //     assert_eq!(infunc_extended(550, &ranges), (true, 500));

    //     // Test with values outside the ranges
    //     assert_eq!(infunc_extended(50, &ranges), (false, 0)); //hasm range, no
    //     assert_eq!(infunc_extended(250, &ranges), (true, 100)); //func1 data
    // }

    #[test]
    fn test_sts_l() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0,
            0x4f22,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "sts.l pr, @-r15");
    }

    #[test]
    fn test_mov() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0,
            0x936e,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.w @(0x0E0, pc), r3");
    }

    #[test]
    fn test_mov_l() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0xd637,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.l @(0x0DE, pc), r6");
    }

    #[test]
    fn test_sts_mach() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x010a,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "sts mach, r1");
    }

    #[test]
    fn test_sts_macl() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x031a,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "sts macl, r3");
    }

    #[test]
    fn test_mov_l_r1() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x0916,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.l r1, @(r0, r9)");
    }

    #[test]
    fn test_stc_gbr() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x0012,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "stc gbr, r0");
    }

    #[test]
    fn test_mov_w_r() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x0e15,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.w r1, @(r0, r14)");
    }
}
