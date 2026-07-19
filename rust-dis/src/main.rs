// why is 14c0 made into data?

use regex::Regex;
use serde::de::Deserializer;
use serde_derive::Deserialize;
use serde_yaml;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::io::{self, BufRead, Read};
use std::path::Path;

struct DataLabel {
    size: u32,
    label: String,
    source: u32,
    is_function: bool,
}

struct JumpTableEntry {
    table_label: String,
    target_label: String,
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
            let thing = (op & 0xff) * 2 + 4 + v_addr;

            if data_labels.contains_key(&thing) {
                if let Some(value) = data_labels.get(&thing) {
                    // Use the label
                    string.push_str(&format!(
                        "mov.w {},r{}",
                        value.label,
                        (op >> 8) & 0xf
                    ));
                }
            } else {
                // use an address
                string.push_str(&format!(
                    "mov.w @(0x{:03X},pc),r{}",
                    (op & 0xff) * 2 + 4,
                    (op >> 8) & 0xf
                ));
            }
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

                let thing = test - 2;

                if data_labels.contains_key(&thing) {
                    if let Some(value) = data_labels.get(&thing) {
                        // Use the label
                        string.push_str(&format!(
                            "mov.l {},r{}",
                            value.label,
                            (op >> 8) & 0xf
                        ));
                    }
                } else {
                    // use an address
                    string.push_str(&format!(
                        "mov.l @(0x{:03X},pc),r{}",
                        target_a,
                        (op >> 8) & 0xf
                    ));
                }
            } else {
                if data_labels.contains_key(&test) {
                    if let Some(value) = data_labels.get(&test) {
                        // Use the label
                        string.push_str(&format!(
                            "mov.l {},r{}",
                            value.label,
                            (op >> 8) & 0xf
                        ));
                    }
                } else {
                    // use an address
                    string.push_str(&format!(
                        "mov.l @(0x{:03X},pc),r{}",
                        target_a,
                        (op >> 8) & 0xf
                    ));
                }
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
        0xc000 => string.push_str(&format!("mov.b r0,@(0x{:03X},gbr)", (op & 0xff) * 1)),
        0xc100 => string.push_str(&format!("mov.w r0,@(0x{:03X},gbr)", (op & 0xff) * 2)),
        0xc200 => string.push_str(&format!("mov.l r0,@(0x{:03X},gbr)", (op & 0xff) * 4)),
        0xc400 => string.push_str(&format!("mov.b @(0x{:03X},gbr),r0", (op & 0xff) * 1)),
        0xc500 => string.push_str(&format!("mov.w @(0x{:03X},gbr),r0", (op & 0xff) * 2)),
        0xc600 => string.push_str(&format!("mov.l @(0x{:03X},gbr),r0", (op & 0xff) * 4)),

        0xc700 => {
            let addr = ((v_addr + 4) & 0xfffffffc) + ((op & 0xff) * 4);
            if let Some(label) = branch_labels.get(&addr) {
                string.push_str(&format!("mova {},r0", label));
            } else {
                string.push_str(&format!("mova 0x{:08X},r0", addr));
            }
        }

        0x8b00 => {
            if (op & 0x80) == 0x80 {
                let addr = (((op & 0xff) + 0xffffff00).wrapping_mul(2))
                    .wrapping_add(v_addr)
                    .wrapping_add(4);

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
            "mov.l r{},@(0x{:03X},r{})",
            (op >> 4) & 0xf,
            (op & 0xf) * 4,
            (op >> 8) & 0xf
        )),
        0x5000 => string.push_str(&format!(
            "mov.l @(0x{:03X},r{}),r{}",
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
                    "mov.b @(0x{:03X},r{}),r0",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.b @(0x{:03X},r{}),r0",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8500 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.w @(0x{:03X},r{}),r0",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.w @(0x{:03X},r{}),r0",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8000 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.b r0,@(0x{:03X},r{})",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.b r0,@(0x{:03X},r{})",
                    op & 0xf,
                    (op >> 4) & 0xf
                ))
            }
        }
        0x8100 => {
            if (op & 0x100) == 0x100 {
                string.push_str(&format!(
                    "mov.w r0,@(0x{:03X},r{})",
                    (op & 0xf) * 2,
                    (op >> 4) & 0xf
                ))
            } else {
                string.push_str(&format!(
                    "mov.w r0,@(0x{:03X},r{})",
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
            "mov.b r{},@r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2001 => string.push_str(&format!(
            "mov.w r{},@r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2002 => string.push_str(&format!(
            "mov.l r{},@r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6000 => string.push_str(&format!(
            "mov.b @r{},r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6001 => string.push_str(&format!(
            "mov.w @r{},r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6002 => string.push_str(&format!(
            "mov.l @r{},r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000f => string.push_str(&format!(
            "mac.l @r{}+, @r{}+",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf,
        )),
        0x400f => string.push_str(&format!(
            "mac.w @r{}+, @r{}+",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf,
        )),
        0x6004 => string.push_str(&format!(
            "mov.b @r{}+,r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6005 => string.push_str(&format!(
            "mov.w @r{}+,r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x6006 => string.push_str(&format!(
            "mov.l @r{}+,r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2004 => string.push_str(&format!(
            "mov.b r{},@-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2005 => string.push_str(&format!(
            "mov.w r{},@-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x2006 => string.push_str(&format!(
            "mov.l r{},@-r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0004 => string.push_str(&format!(
            "mov.b r{},@(r0,r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0005 => string.push_str(&format!(
            "mov.w r{},@(r0,r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x0006 => string.push_str(&format!(
            "mov.l r{},@(r0,r{})",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000c => string.push_str(&format!(
            "mov.b @(r0,r{}),r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000d => string.push_str(&format!(
            "mov.w @(r0,r{}),r{}",
            (op >> 4) & 0xf,
            (op >> 8) & 0xf
        )),
        0x000e => string.push_str(&format!(
            "mov.l @(r0,r{}),r{}",
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
    let mut literal_rts = HashSet::<u32>::new();
    for i in (section_start..section_end).step_by(2) {
        let op = (vec[i as usize] as u32) << 8 | vec[i as usize + 1] as u32;
        if op & 0xf000 != 0xd000 {
            continue;
        }
        let target = (i as u32 + 4 + (op & 0xff) * 4) & !3;
        if target >= section_start as u32 && target + 4 <= section_end as u32 {
            let high = (vec[target as usize] as u32) << 8 | vec[target as usize + 1] as u32;
            let low = (vec[target as usize + 2] as u32) << 8 | vec[target as usize + 3] as u32;
            if high & 0xf000 == 0xf000 && low == 0x000b {
                literal_rts.insert(target + 2);
            }
        }
    }

    // first, find every location of an rts.
    let mut rts_pos: Vec<u32> = Vec::new();
    for i in (section_start..section_end).step_by(2) {
        let instr = (vec[i as usize] as u32) << 8 | vec[i as usize + 1] as u32;
        if instr == 0x000b && !literal_rts.contains(&(i as u32)) {
            rts_pos.push(i as u32);
        }
    }

    for i in 0..rts_pos.len() {
        let prev_rts = if i > 0 { rts_pos[i - 1] } else { 0 };
        let has_literal_rts = literal_rts.iter().any(|&offset| {
            offset > prev_rts
                && offset < rts_pos[i]
                && ((vec[offset as usize] as u32) << 8
                    | vec[offset as usize + 1] as u32)
                    == 0x000b
        });
        let mut func_start = 0;
        let mut longest_preamble = 0;
        let mut pc = rts_pos[i] - 2;
        // Scan back to the previous return and select the longest contiguous
        // register-save sequence. A function can push a temporary value in its
        // body, so the closest push to the return is not necessarily its prologue.
        while pc >= prev_rts && pc > 0 {
            let instr = (vec[pc as usize] as u32) << 8 | vec[(pc + 1) as usize] as u32;

            if instr & 0xFF0F == 0x2F06 {
                let mut run_start = pc;
                let mut run_len = 1;
                while run_start >= prev_rts + 2 && run_start > 2 {
                    let previous = run_start - 2;
                    let previous_instr = (vec[previous as usize] as u32) << 8
                        | vec[(previous + 1) as usize] as u32;
                    if previous_instr & 0xFF06 != 0x2F06 {
                        break;
                    }
                    run_start = previous;
                    run_len += 1;
                }
                if run_len > longest_preamble {
                    longest_preamble = run_len;
                    func_start = run_start;
                }
                if !has_literal_rts {
                    break;
                }
                pc = run_start;
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

    if ranges.len() > 0 {
        // check after funcs
        for j in 0..ranges.len() - 1 {
            let current_func = &ranges[j];
            let next_func = &ranges[j + 1];
            if i >= current_func.phys_start && i < next_func.phys_start {
                // in func rodata
                return (true, current_func.phys_start);
            }
        }
    }

    if ranges.len() > 0 {
        // if this is the last func go to end
        let last_func = &ranges[ranges.len() - 1];
        if i >= last_func.phys_start {
            return (true, last_func.phys_start);
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
    let label = format!(".L{:08X}", addr);
    branch_labels.entry(addr).or_insert(label);
}

fn find_jump_tables(
    file_contents: &Vec<u8>,
    section_start: u64,
    section_end: u64,
    virtual_base_addr: u64,
    ranges: &Vec<FunctionRange>,
    branch_labels: &mut HashMap<u32, String>,
    jump_table_entries: &mut HashMap<u32, JumpTableEntry>,
) {
    for i in (section_start..section_end).step_by(2) {
        let ii = i as usize;
        let op = ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;
        let Some(function) = ranges
            .iter()
            .find(|range| i as u32 >= range.phys_start && i as u32 <= range.phys_end)
        else {
            continue;
        };
        if op & 0xff00 != 0xc700 || i < section_start + 4 || i + 8 >= section_end {
            continue;
        }

        let next = |offset: usize| -> u32 {
            ((file_contents[ii + offset] as u32) << 8) | file_contents[ii + offset + 1] as u32
        };
        if next(2) != 0x011d || next(4) != 0x301c || next(6) != 0x402b {
            continue;
        }

        let dispatch_move_pos = ii - 4;
        let dispatch_move = ((file_contents[dispatch_move_pos] as u32) << 8)
            | file_contents[dispatch_move_pos + 1] as u32;
        if dispatch_move & 0xff0f != 0x6103 {
            continue;
        }
        let dispatch_register = (dispatch_move >> 4) & 0xf;

        let mut max_index = None;
        let scan_start = i.saturating_sub(48).max(section_start);
        for address in (scan_start..i)
            .step_by(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let pos = address as usize;
            let compare = ((file_contents[pos] as u32) << 8) | file_contents[pos + 1] as u32;
            if compare & 0xf00f != 0x3006 || ((compare >> 8) & 0xf) != dispatch_register {
                continue;
            }
            let bound_register = (compare >> 4) & 0xf;
            let immediate_start = address.saturating_sub(24).max(scan_start);
            for immediate_address in (immediate_start..address)
                .step_by(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                let immediate_pos = immediate_address as usize;
                let immediate = ((file_contents[immediate_pos] as u32) << 8)
                    | file_contents[immediate_pos + 1] as u32;
                if immediate & 0xf000 == 0xe000
                    && ((immediate >> 8) & 0xf) == bound_register
                {
                    max_index = Some(immediate & 0xff);
                    break;
                }
            }
            break;
        }
        let Some(max_index) = max_index else {
            continue;
        };

        let virtual_addr = i as u32 + virtual_base_addr as u32;
        let table_addr = ((virtual_addr + 4) & 0xfffffffc) + ((op & 0xff) * 4);
        let Some(table_offset) = table_addr.checked_sub(virtual_base_addr as u32) else {
            continue;
        };
        let entry_count = max_index + 1;
        let table_end = table_offset + entry_count * 2;
        if table_offset < function.phys_start
            || table_end > function.phys_end + 1
            || table_end as usize > file_contents.len()
        {
            continue;
        }

        let table_label = format!(".Ljtbl_{:08X}", table_addr);
        let mut targets = Vec::new();
        for index in 0..entry_count {
            let entry_offset = (table_offset + index * 2) as usize;
            let raw = ((file_contents[entry_offset] as u16) << 8)
                | file_contents[entry_offset + 1] as u16;
            let target = table_addr.wrapping_add((raw as i16 as i32) as u32);
            let Some(target_offset) = target.checked_sub(virtual_base_addr as u32) else {
                targets.clear();
                break;
            };
            if target_offset < function.phys_start || target_offset > function.phys_end {
                targets.clear();
                break;
            }
            targets.push(target);
        }
        if targets.len() != entry_count as usize {
            continue;
        }

        branch_labels.insert(table_addr, table_label.clone());
        for (index, target) in targets.into_iter().enumerate() {
            add_label(target, branch_labels);
            let target_label = branch_labels.get(&target).unwrap().clone();
            jump_table_entries.insert(
                table_addr + index as u32 * 2,
                JumpTableEntry {
                    table_label: table_label.clone(),
                    target_label,
                },
            );
        }
    }
}

fn remove_jump_table_internal_labels(
    branch_labels: &mut HashMap<u32, String>,
    jump_table_entries: &HashMap<u32, JumpTableEntry>,
) {
    for (address, entry) in jump_table_entries {
        if branch_labels.get(address) != Some(&entry.table_label) {
            branch_labels.remove(address);
        }
    }
}

fn add_data_label(source: u32, addr: u32, size: u32, data_labels: &mut HashMap<u32, DataLabel>) {
    let the_label = format!(".Ldat_{:08X}", addr);
    let data_label = DataLabel {
        size,
        label: the_label,
        source: source,
        is_function: false,
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
    let is_mova = (op & 0xff00) == 0xc700;

    if is_mova {
        let addr = ((v_addr + 4) & 0xfffffffc) + ((op & 0xff) * 4);
        add_label(addr, branch_labels);
    } else if is_bf || is_bfs || is_bt || is_bts {
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

fn literal_feeds_call(
    file_contents: &Vec<u8>,
    source: u32,
    virtual_base_addr: u64,
) -> bool {
    let Some(source_offset) = source.checked_sub(virtual_base_addr as u32) else {
        return false;
    };
    let source_offset = source_offset as usize;
    if source_offset + 1 >= file_contents.len() {
        return false;
    }
    let load = ((file_contents[source_offset] as u32) << 8)
        | file_contents[source_offset + 1] as u32;
    if load & 0xf000 != 0xd000 {
        return false;
    }
    let register = (load >> 8) & 0xf;
    let scan_end = (source_offset + 34).min(file_contents.len().saturating_sub(1));
    for offset in ((source_offset + 2)..scan_end).step_by(2) {
        let op = ((file_contents[offset] as u32) << 8) | file_contents[offset + 1] as u32;
        if op & 0xf0ff == 0x400b && ((op >> 8) & 0xf) == register {
            return true;
        }
    }
    false
}

#[derive(Debug, Deserialize)]
struct Options {
    target_path: String,
    asm_path: String,
    ld_scripts_path: String,
    syms_path: String,
    src_path: String,
    #[serde(default)]
    obj_path: String,
    #[serde(default)]
    check_layout: bool,
    decomp_empty_funcs: bool,
}

#[derive(Debug)]
struct Subsegment {
    start: u64,
    end: Option<u64>,
    segment_type: Option<String>,
    file: Option<String>,
}

impl<'de> serde::Deserialize<'de> for Subsegment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum SubsegmentSyntax {
            Mapping {
                start: u64,
                end: Option<u64>,
                #[serde(rename = "type")]
                segment_type: Option<String>,
                file: Option<String>,
            },
            Compact((u64, String, String)),
        }

        match <SubsegmentSyntax as serde::Deserialize>::deserialize(deserializer)? {
            SubsegmentSyntax::Mapping {
                start,
                end,
                segment_type,
                file,
            } => Ok(Self {
                start,
                end,
                segment_type,
                file,
            }),
            SubsegmentSyntax::Compact((start, segment_type, file)) => Ok(Self {
                start,
                end: None,
                segment_type: Some(segment_type),
                file: Some(file),
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Segment {
    name: String,
    #[serde(rename = "type")]
    segment_type: String,
    start: u64,
    subsegments: Option<Vec<Subsegment>>,
    vram: u64,
    subalign: Option<u64>,
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
    let mut config: Config = serde_yaml::from_str(&contents).expect("Failed to parse YAML.");
    if let Some(ref mut segments) = config.segments {
        for segment in segments {
            if let Some(ref mut subsegments) = segment.subsegments {
                for i in 0..subsegments.len().saturating_sub(1) {
                    if subsegments[i].end.is_none() {
                        subsegments[i].end = Some(subsegments[i + 1].start)
                    }
                }
            }
        }
    }
    config
}

fn read_user_symbols(filename: &str) -> HashMap<u32, String> {
    let Ok(contents) = std::fs::read_to_string(filename) else {
        return HashMap::new();
    };
    let pattern = Regex::new(
        r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(0x[0-9A-Fa-f]+)\s*;\s*$",
    )
    .unwrap();
    let mut symbols = HashMap::new();
    for line in contents.lines() {
        let Some(captures) = pattern.captures(line) else {
            continue;
        };
        let name = captures.get(1).unwrap().as_str();
        let address = u32::from_str_radix(
            captures.get(2).unwrap().as_str().trim_start_matches("0x"),
            16,
        )
        .unwrap();
        symbols.entry(address).or_insert_with(|| name.to_string());
    }
    symbols
}

fn format_literal(
    value: u32,
    user_symbols: &HashMap<u32, String>,
    _allow_symbol: bool,
) -> String {
    if let Some(symbol) = user_symbols.get(&value) {
        return symbol.clone();
    }
    format!("0x{:08X}", value)
}
use std::io::Write;

fn emit_c_file(functions: &BTreeMap<u32, DisassembledFunc>, output_path: String) {
    let filename = format!("{}/output.c", output_path);
    let mut file = std::fs::File::create(filename).expect("Failed to create file.");
    writeln!(&mut file, "#include \"inc_asm.h\"").expect("Failed to write to file.");
    for pair in functions {
        let mut label = &format!("d_{:08X}", pair.1.addr);
        if !pair.1.data {
            let mut label = &format!("func_{:08X}", pair.1.addr);
        }
        writeln!(
            &mut file,
            "INCLUDE_ASM(\"{}\", {}, \"{}\");",
            pair.1.file, pair.1.name, label
        )
        .expect("Failed to write to file.");
    }
}

fn emit_asm_file(filename: String, data: String) {
    let mut file = std::fs::File::create(filename).expect("Failed to create file.");
    writeln!(&mut file, "{}", data).expect("Failed to write to file.");
}

use std::fmt;

struct DisassembledFunc {
    addr: u32,
    end: u32,
    text: String,
    data: bool,
    name: String,
    file: String,
}

fn check_data_labels(
    virtual_addr: u32,
    data_labels: &HashMap<u32, DataLabel>,
    skip_next: &mut bool,
    should_continue: &mut bool,
    file_contents: &Vec<u8>,
    start_address: u32,
    disassembled_funcs: &mut BTreeMap<u32, DisassembledFunc>,
    i: u32,
    instr: u16,
    user_symbols: &HashMap<u32, String>,
) {
    if data_labels.contains_key(&virtual_addr.try_into().unwrap()) {
        if let Some(value) = data_labels.get(&virtual_addr.try_into().unwrap()) {
            // Use the label

            if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                func.text.push_str(&format!(
                    "{}: /* source: {:08X} */\n",
                    value.label, value.source
                ));
            }
            if value.size == 2 {
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!(".word 0x{:04X}\n", instr));
                }
            } else if value.size == 4 {
                let data = ((file_contents[i as usize + 0] as u32) << 24)
                    | ((file_contents[i as usize + 1] as u32) << 16)
                    | ((file_contents[i as usize + 2] as u32) << 8)
                    | ((file_contents[i as usize + 3] as u32) << 0);
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!(
                        "/* {:08X} */ .long {}\n",
                        virtual_addr,
                        format_literal(data, user_symbols, !value.is_function)
                    ));
                }

                // skip next instruction since we used it
                *skip_next = true;
            }
            *should_continue = true;
        }
    }
}

fn handle_code_section(
    file_contents: &Vec<u8>,
    section_start: u64,
    section_end: u64,
    virtual_base_addr: u64,
    user_symbols: &HashMap<u32, String>,
) -> (BTreeMap<u32, DisassembledFunc>) {
    let len = file_contents.len();
    let mut ranges = Vec::<FunctionRange>::new();
    find_funcs(&file_contents, section_start, section_end, &mut ranges);

    if ranges.len() == 0 {
        println!("no ranges");
    }

    let mut data_labels = HashMap::<u32, DataLabel>::new();
    let mut branch_labels = HashMap::<u32, String>::new();
    let mut jump_table_entries = HashMap::<u32, JumpTableEntry>::new();

    find_jump_tables(
        file_contents,
        section_start,
        section_end,
        virtual_base_addr,
        &ranges,
        &mut branch_labels,
        &mut jump_table_entries,
    );

    for i in (section_start..section_end).step_by(2) {
        let ii = i as usize;
        let instr: u32 = ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;

        let (is_in_func, start_address) = infunc(i as u32, &ranges);

        if !is_in_func {
            continue;
        }

        find_branch_labels(
            TryInto::<u32>::try_into(i).unwrap() + virtual_base_addr as u32,
            instr,
            &mut branch_labels,
        );
        find_data_labels(
            TryInto::<u32>::try_into(i).unwrap() + virtual_base_addr as u32,
            instr,
            &mut data_labels,
        );
    }

    remove_jump_table_internal_labels(&mut branch_labels, &jump_table_entries);

    for label in data_labels.values_mut() {
        if label.size == 4 {
            label.is_function = literal_feeds_call(file_contents, label.source, virtual_base_addr);
        }
    }
    let mut disassembled_funcs = BTreeMap::<u32, DisassembledFunc>::new();

    // create emtpy ones for all funcs
    for f in &ranges {
        let virtual_addr =
            TryInto::<u32>::try_into(f.phys_start).unwrap() + virtual_base_addr as u32;
        disassembled_funcs
            .entry(f.phys_start)
            .or_insert(DisassembledFunc {
                addr: f.phys_start,
                end: f.phys_end,
                text: "".to_string(),
                data: f.is_data,
                name: format!("f{:07X}", virtual_addr),
                file: "_".to_string(),
            });
    }

    let mut monolithic = String::new();

    let mut skip_next = false;

    for i in (section_start..section_end).step_by(2) {
        let ii = i as usize;
        let instr: u32 = ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;
        let virtual_addr = TryInto::<u32>::try_into(i).unwrap() + virtual_base_addr as u32;
        if skip_next {
            skip_next = false;
            continue;
        }

        let (is_in_func, start_address) = infunc(i as u32, &ranges);

        // // the last function needs to emit data up until the next section
        let (is_in_func_extended, start_address_extended) = infunc_extended(i as u32, &ranges);

        if i as u32 == start_address {
            if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                func.text
                    .push_str(&format!("glabel func_{:08X}\n", virtual_addr));
            }
        }

        // check to emit data, use extended addr
        if let Some(func) = disassembled_funcs.get_mut(&(start_address_extended as u32)) {
            if func.data {
                func.text.push_str(&format!(
                    "/* 0x{:08X} */ .word 0x{:04X}\n",
                    virtual_addr, instr
                ));
                println!("\ti is {:08X} func.data continue", i);

                continue;
            }
        }

        if branch_labels.contains_key(&virtual_addr.try_into().unwrap()) {
            if let Some(value) = branch_labels.get(&virtual_addr.try_into().unwrap()) {
                // Use the label
                if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                    func.text.push_str(&format!("{}:\n", value));
                    monolithic.push_str(&format!("{}:\n", value));
                }
            }
        }

        if let Some(entry) = jump_table_entries.get(&virtual_addr) {
            if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                func.text.push_str(&format!(
                    "/* 0x{:08X} */ .word {}-{}\n",
                    virtual_addr, entry.target_label, entry.table_label
                ));
            }
            continue;
        }

        // data labels, extended addr
        let mut should_continue: bool = false;
        check_data_labels(
            virtual_addr,
            &data_labels,
            &mut skip_next,
            &mut should_continue,
            &file_contents,
            start_address_extended,
            &mut disassembled_funcs,
            i as u32,
            instr as u16,
            user_symbols,
        );

        if should_continue {
            continue;
        }

        // only disasm if we are in a func
        if is_in_func {
            let mut string = String::new();
            sh2_disasm(
                i as u32 + virtual_base_addr as u32,
                instr,
                true,
                &mut string,
                &mut data_labels,
                &mut branch_labels,
            );
            if let Some(func) = disassembled_funcs.get_mut(&(start_address as u32)) {
                func.text.push_str(&format!(
                    "/* 0x{:08X} 0x{:04X} */ {}\n",
                    virtual_addr, instr, string
                ));
            }
        } else {
            if let Some(func) = disassembled_funcs.get_mut(&(start_address_extended as u32)) {
                // emit uncaught data
                func.text.push_str(&format!(
                    "/* 0x{:08X} */ .word 0x{:04X}\n",
                    virtual_addr, instr
                ));
            }
        }
    }

    return disassembled_funcs;
}

#[derive(Default)]
struct ProcessedSection {
    is_code: bool,
    disassembled_funcs: BTreeMap<u32, DisassembledFunc>,
    data: String,
    addr: u64,
    vaddr: u64,
    vbase: u64,
}

fn write_c_file(
    config: &Config,
    path: &str,
    segment_name: &str,
    asm_path: &str,
    processed_sections: &Vec<ProcessedSection>,
) {
    let filename = format!("{}/{}.c", path, segment_name);
    let mut file = std::fs::File::create(filename).expect("Failed to create file.");
    writeln!(&mut file, "#include \"inc_asm.h\"").expect("Failed to write to file.");

    for processed_section in processed_sections {
        if !processed_section.is_code {
            let name = format!("d{:07X}", processed_section.vaddr);
            writeln!(
                &mut file,
                "INCLUDE_ASM(\"{}\", {}, d_{:08X});",
                asm_path, // TODO fix hardcode
                name,
                processed_section.vaddr
            )
            .expect("Failed to write to file.");
        } else {
            for pair in &processed_section.disassembled_funcs {
                // assume this is a empty function if the size is 8
                if (pair.1.end - pair.1.addr == 8) && config.options.decomp_empty_funcs {
                    writeln!(&mut file, "void {}() {{}}", pair.1.name)
                        .expect("Failed to write to file.");
                } else {
                    writeln!(
                        &mut file,
                        "INCLUDE_ASM(\"{}\", {}, func_{:08X});",
                        asm_path,
                        pair.1.name,
                        pair.1.addr + processed_section.vbase as u32
                    )
                    .expect("Failed to write to file.");
                }
            }
        }
    }
}

fn find_include_asm_in_c_file(filename: &str) -> io::Result<HashSet<String>> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let re = Regex::new(r#"INCLUDE_ASM(?:_NO_ALIGN)?\("(.*?)", (.*?), (.*?)\)"#).unwrap();
    let mut result = HashSet::new();

    for res in reader.lines() {
        if let Some(caps) = re.captures(&res.unwrap().to_string()) {
            let func_name = caps.get(3).unwrap().as_str().to_string();
            result.insert(func_name);
        }
    }

    Ok(result)
}

fn handle_segments(file_contents: &Vec<u8>, config: &Config) {
    let mut processed_sections = Vec::<ProcessedSection>::new();
    if let Some(segments) = &config.segments {
        for segment in segments {
            let user_symbols = read_user_symbols(
                &format!(
                    "{}/{}_user_syms.txt",
                    config.options.syms_path, segment.name
                ),
            );
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
                    let subsegment_end = subsegment.end.unwrap_or(file_contents.len() as u64);

                    println!(
                        "subsegment {:08X}-{:08X} {} {}",
                        subsegment_start, subsegment_end, subsegment_type, subsegment_file,
                    );

                    if subsegment_type == "data" || subsegment_type.starts_with('.') {
                        // just emit words
                        let mut data_str = String::new();
                        for i in (subsegment_start..subsegment_end).step_by(2) {
                            let ii = i as usize;
                            let data: u32 =
                                ((file_contents[ii] as u32) << 8) | file_contents[ii + 1] as u32;
                            data_str.push_str(&format!(
                                "/* 0x{:08X} */ .word 0x{:04X}\n",
                                i + segment.vram,
                                data
                            ));
                        }

                        let processed_section = ProcessedSection {
                            is_code: false,
                            disassembled_funcs: BTreeMap::<u32, DisassembledFunc>::new(),
                            data: data_str,
                            addr: subsegment_start,
                            vaddr: subsegment_start + segment.vram,
                            vbase: segment.vram,
                        };
                        processed_sections.push(processed_section);
                    } else {
                        // find functions and process
                        let disassembled_funcs = handle_code_section(
                            file_contents,
                            subsegment_start,
                            subsegment_end,
                            segment.vram,
                            &user_symbols,
                        );

                        let processed_section = ProcessedSection {
                            is_code: true,
                            disassembled_funcs: disassembled_funcs,
                            data: "".to_string(),
                            addr: subsegment_start,
                            vaddr: subsegment_start + segment.vram,
                            vbase: segment.vram,
                        };
                        processed_sections.push(processed_section);
                    }
                }
            }
        }
    }

    let mut includes: HashSet<String> = HashSet::new();

    // determine first what has been decompiled
    if let Some(segs) = &config.segments {
        if !segs.is_empty() {
            let segment_name = &segs[0].name;
            let base_addr = &segs[0].vram;
            let path = &config.options.src_path;
            for seg in segs
            {
                if let Some(subsegments) = &seg.subsegments 
                {
                    for subseg in subsegments
                    {
                        println!("seg {:#?}", subseg);
                        // // need to check the designated file rather than just the segment
                        // // collect all c files specified in the yaml, then check all of those
                        // // and add to includes
    
                        if let Some(subseg_file) = &subseg.file
                        {
                            let c_filename = format!("{}/{}.c", path, subseg_file);
    
                            println!("checking {}", c_filename);
                            if Path::new(&c_filename).exists() {
                                match find_include_asm_in_c_file(&c_filename) {
                                    Ok(set) => {
                                        println!("adding {:#?}", set);
                                        includes.extend(set)
                                    },
                                    Err(err) => {
                                        eprintln!("Error reading the file: {}", err);
                                    }
                                }
                            }
                        }
                    }
                }

            }
        }
    }

    // all the segments are processed, emit files

    std::fs::create_dir_all(&config.options.asm_path).expect("Failed to create directories.");
    std::fs::create_dir_all(&format!("{}/f_nonmat", config.options.asm_path))
        .expect("Failed to create directories.");
    std::fs::create_dir_all(&format!("{}/f_match", config.options.asm_path))
        .expect("Failed to create directories.");
    std::fs::create_dir_all(&format!("{}/data", config.options.asm_path))
        .expect("Failed to create directories.");

    std::fs::create_dir_all(&config.options.ld_scripts_path)
        .expect("Failed to create directories.");
    std::fs::create_dir_all(&config.options.syms_path).expect("Failed to create directories.");
    // emit all the asm
    for processed_section in &processed_sections {
        if !processed_section.is_code {
            emit_asm_file(
                format!(
                    "{}/data/d{:07X}.s",
                    config.options.asm_path, processed_section.vaddr
                ),
                processed_section.data.clone(),
            );
        } else {
            for (_addr, df) in &processed_section.disassembled_funcs {
                let func_name = format!("func_{:08X}", df.addr + processed_section.vbase as u32);

                if includes.contains(&func_name) {
                    // this has not been decompiled
                    emit_asm_file(
                        format!(
                            "{}/f_nonmat/f{:07X}.s",
                            config.options.asm_path,
                            df.addr + processed_section.vbase as u32
                        ),
                        df.text.clone(),
                    );
                } else {
                    // has been decompiled
                    emit_asm_file(
                        format!(
                            "{}/f_match/f{:07X}.s",
                            config.options.asm_path,
                            df.addr + processed_section.vbase as u32
                        ),
                        df.text.clone(),
                    );
                }
            }
        }
    }

    let path = &config.options.src_path;
    let asm_path = &config.options.asm_path;

    std::fs::create_dir_all(path).expect("Failed to create src_path directories.");

    if let Some(segs) = &config.segments {
        if !segs.is_empty() {
            let segment_name = &segs[0].name;
            let base_addr = &segs[0].vram;

            let syms_filename = format!("{}/{}_syms.txt", &config.options.syms_path, segment_name);
            let mut syms_file =
                std::fs::File::create(syms_filename).expect("Failed to create file.");

            let c_filename = format!("{}/{}.c", path, segment_name);

            // don't overwite the c file if it's already existing
            if !Path::new(&c_filename).exists() {
                write_c_file(config, &path, &segment_name, &asm_path, &processed_sections);
            }

            {
                // The linker script is generated entirely from the YAML so it can be ephemeral.
                let filename = format!("{}/{}.ld", &config.options.ld_scripts_path, segment_name);
                let inputs = linker_inputs(&segs[0]);
                let linker_script = gen_ld_script(
                    segment_name,
                    &format!("{:08X}", base_addr),
                    segs[0].subalign.unwrap_or(2),
                    &config.options.obj_path,
                    config.options.check_layout,
                    &inputs,
                );
                let contents = format!("{}\n", linker_script);
                if std::fs::read_to_string(&filename).ok().as_deref() != Some(&contents) {
                    std::fs::write(filename, contents)
                        .expect("Failed to write linker script file.");
                }
            }

            // write symbols
            for processed_section in &processed_sections {
                if !processed_section.is_code {
                } else {
                    for pair in &processed_section.disassembled_funcs {
                        // assume this is a empty function if the size is 8
                        if (pair.1.end - pair.1.addr == 8) && config.options.decomp_empty_funcs {
                        } else {
                            // need _ prefix for name mangling
                            // seems like all asm symbols need _ to be accessible
                            // from C
                            writeln!(
                                &mut syms_file,
                                "_func_{:08X} = 0x{:08X};",
                                pair.1.addr + processed_section.vbase as u32,
                                pair.1.addr + processed_section.vbase as u32
                            )
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
        if filename == "--find-funcs" {
            if let Some(funcs_filename) = args.get(2) {
                match read_file_to_vec(&funcs_filename) {
                    Ok(file_contents) => {
                        let mut ranges = Vec::<FunctionRange>::new();
                        find_funcs(&file_contents, 0, file_contents.len() as u64, &mut ranges);
                        for range in ranges {
                            // println!("{:08X}-{:08X}", range.phys_start, range.phys_end);
                            println!("0x{:08X},", range.phys_start + 0x06066000);
                        }
                    }
                    Err(error) => {
                        // Error reading the file
                        println!("Error: {:?} {}", error, funcs_filename);
                    }
                }
            }
            return;
        }
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
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;

fn assemble(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Create a temporary file and write the assembly code to it
    let mut asm_file = NamedTempFile::new()?;
    asm_file.write_all(input.as_bytes())?;

    // Create a temporary file for the output
    let output_file = NamedTempFile::new()?;

    // assemble and dump as binary
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

use similar::{ChangeTag, TextDiff};

fn print_diff(expected_lines: String, actual_lines: String) {
    let diff = TextDiff::from_lines(&expected_lines, &actual_lines);

    for diff in diff.iter_all_changes() {
        match diff.tag() {
            ChangeTag::Delete => print!("\x1b[31m{}\x1b[0m", diff),
            ChangeTag::Insert => print!("\x1b[32m{}\x1b[0m", diff),
            ChangeTag::Equal => print!("{}", diff),
        }
    }
    println!();
}

fn asm_test_case(asm: String, expected: String, virtual_base_addr: u64) {
    let output = assemble(&asm).unwrap();

    println!("output: {:?} ", output);

    let mut data_labels = HashMap::<u32, DataLabel>::new();
    let mut branch_labels = HashMap::<u32, String>::new();

    let mut output_string = String::new();

    let disassembled_funcs = handle_code_section(
        &output,
        0,
        output.len().try_into().unwrap(),
        virtual_base_addr,
        &HashMap::new(),
    );

    let trimmed_right: String = expected
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");

    if disassembled_funcs[&8].text != trimmed_right {
        let actual_lines = disassembled_funcs[&8].text.clone();
        let expected_lines = trimmed_right;

        print_diff(expected_lines, actual_lines);
        assert!(false);
    }
}

#[derive(Debug, PartialEq)]
struct LinkerInput {
    start: u64,
    object: String,
    section: String,
}

fn linker_inputs(segment: &Segment) -> Vec<LinkerInput> {
    let mut seen = HashSet::new();
    let mut inputs = Vec::new();
    let mut legacy_text_starts = HashMap::new();
    let mut legacy_c_files = HashSet::new();

    if let Some(subsegments) = &segment.subsegments {
        for subsegment in subsegments {
            if matches!(subsegment.segment_type.as_deref(), Some("data") | Some("c")) {
                if let Some(file) = &subsegment.file {
                    legacy_text_starts
                        .entry(file.clone())
                        .and_modify(|start: &mut u64| *start = (*start).min(subsegment.start))
                        .or_insert(subsegment.start);
                    if subsegment.segment_type.as_deref() == Some("c") {
                        legacy_c_files.insert(file.clone());
                    }
                }
            }
        }
        for subsegment in subsegments {
            let Some(file) = &subsegment.file else {
                continue;
            };
            let section = match subsegment.segment_type.as_deref() {
                Some("c") => ".text",
                Some("data") if !legacy_c_files.contains(file) => ".text",
                Some(".text") => ".text",
                Some(".data") => ".data",
                Some(".rodata") => ".rodata",
                Some(".bss") => ".bss",
                Some(".sbss") => ".sbss",
                _ => continue,
            };
            if seen.insert((file.clone(), section)) {
                inputs.push(LinkerInput {
                    start: if matches!(subsegment.segment_type.as_deref(), Some("c") | Some("data"))
                    {
                        legacy_text_starts[file]
                    } else {
                        subsegment.start
                    },
                    object: format!("{}.o", file),
                    section: section.to_string(),
                });
            }
        }
    }

    inputs
}

fn gen_ld_script(
    zero_prefix: &str,
    addr: &str,
    subalign: u64,
    obj_path: &str,
    check_layout: bool,
    inputs: &[LinkerInput],
) -> String {
    let mut code = String::new();

    code.push_str("SECTIONS\n{\n");
    code.push_str("    __romPos = 0;\n");
    code.push_str("    _gp = 0x0;\n");
    code.push_str(&format!("    {}_ROM_START = __romPos;\n", zero_prefix));
    code.push_str(&format!(
        "    {}_VRAM = ADDR(.{});\n",
        zero_prefix, zero_prefix
    ));
    code.push_str(&format!(
        "    .{} 0x{} : AT({}_ROM_START) SUBALIGN({})\n    {{\n",
        zero_prefix, addr, zero_prefix, subalign
    ));
    code.push_str(&format!("        {}_TEXT_START = .;\n", zero_prefix));
    for input in inputs {
        if check_layout {
            code.push_str(&format!(
                "        ASSERT(. - ADDR(.{}) == 0x{:X}, \"{} {} starts at the wrong offset\");\n",
                zero_prefix,
                input.start,
                input.object,
                input.section,
            ));
        }
        let path = if obj_path.is_empty() {
            input.object.clone()
        } else {
            format!("{}/{}", obj_path.trim_end_matches('/'), input.object)
        };
        code.push_str(&format!("        {}({});\n", path, input.section));
    }
    code.push_str(&format!("        {}_TEXT_END = .;\n", zero_prefix));
    code.push_str(&format!(
        "        {}_TEXT_SIZE = ABSOLUTE({}_TEXT_END - {}_TEXT_START);\n    }}\n",
        zero_prefix, zero_prefix, zero_prefix
    ));
    code.push_str(&format!("    __romPos += SIZEOF(.{});\n", zero_prefix));
    code.push_str("    __romPos = ALIGN(__romPos, 16);\n");
    code.push_str(&format!("    {}_ROM_END = __romPos;\n", zero_prefix));
    code.push_str(&format!("    {}_VRAM_END = .;\n", zero_prefix));
    code.push_str("\n    /DISCARD/ :\n    {\n");
    code.push_str("        *(*);\n    }\n");
    code.push_str("}");

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words_bytes(words: &[u16]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_be_bytes()).collect()
    }

    #[test]
    fn test_find_funcs_ignores_rts_inside_referenced_long_literal() {
        let bytes = words_bytes(&[
            0x0009, 0x0009, 0x2f86, 0xd102, 0x0009, 0x0009, 0x0009, 0x0009, 0xf000,
            0x000b, 0x6ef6, 0x000b, 0x68f6,
        ]);
        let mut ranges = Vec::new();

        find_funcs(&bytes, 0, bytes.len() as u64, &mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].phys_start, 4);
        assert_eq!(ranges[0].phys_end, 24);
    }

    #[test]
    fn test_find_funcs_does_not_ignore_unreferenced_rts_pattern() {
        let bytes = words_bytes(&[0x0009, 0x0009, 0x2f86, 0x0009, 0xf000, 0x000b, 0x68f6]);
        let mut ranges = Vec::new();

        find_funcs(&bytes, 0, bytes.len() as u64, &mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].phys_start, 4);
        assert_eq!(ranges[0].phys_end, 12);
    }

    #[test]
    fn test_find_funcs_keeps_rts_at_start_of_long_target() {
        let bytes = words_bytes(&[
            0x0009, 0x0009, 0x2f86, 0xd102, 0x0009, 0x0009, 0x0009, 0x0009, 0x000b,
            0x1234,
        ]);
        let mut ranges = Vec::new();

        find_funcs(&bytes, 0, bytes.len() as u64, &mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].phys_start, 4);
        assert_eq!(ranges[0].phys_end, 18);
    }

    #[test]
    fn test_find_funcs_keeps_rts_targeted_by_word_pattern() {
        let bytes = words_bytes(&[
            0x0009, 0x0009, 0x2f86, 0x9303, 0x0009, 0x0009, 0x0009, 0x0009, 0x000b,
            0x1234,
        ]);
        let mut ranges = Vec::new();

        find_funcs(&bytes, 0, bytes.len() as u64, &mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].phys_start, 4);
        assert_eq!(ranges[0].phys_end, 18);
    }

    #[test]
    fn test_find_funcs_prefers_full_prologue_over_internal_push() {
        let bytes = words_bytes(&[
            0x0009, 0x0009, 0x2f86, 0x2f96, 0x2fa6, 0xd102, 0x0009, 0x0009, 0x0009,
            0x0009, 0xf000, 0x000b, 0x2fd6, 0x0009, 0x000b, 0x68f6,
        ]);
        let mut ranges = Vec::new();

        find_funcs(&bytes, 0, bytes.len() as u64, &mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].phys_start, 4);
    }

    #[test]
    fn test_ld_script() {
        let expected = r#"SECTIONS
{
    __romPos = 0;
    _gp = 0x0;
    zero_ROM_START = __romPos;
    zero_VRAM = ADDR(.zero);
    .zero 0x06004080 : AT(zero_ROM_START) SUBALIGN(2)
    {
        zero_TEXT_START = .;
        build/saturn/zero.o(.text);
        zero_TEXT_END = .;
        zero_TEXT_SIZE = ABSOLUTE(zero_TEXT_END - zero_TEXT_START);
    }
    __romPos += SIZEOF(.zero);
    __romPos = ALIGN(__romPos, 16);
    zero_ROM_END = __romPos;
    zero_VRAM_END = .;

    /DISCARD/ :
    {
        *(*);
    }
}"#;

        let actual = gen_ld_script(
            "zero",
            "06004080",
            2,
            "build/saturn",
            false,
            &[LinkerInput {
                start: 0,
                object: "zero.o".to_string(),
                section: ".text".to_string(),
            }],
        );
        print_diff(expected.to_string(), actual.clone());
        assert!(expected == actual);
    }

    #[test]
    fn test_linker_inputs_are_ordered_and_deduplicated() {
        let segment = Segment {
            name: "zero".to_string(),
            segment_type: "code".to_string(),
            start: 0,
            vram: 0x06004080,
            subalign: Some(4),
            subsegments: Some(vec![
                Subsegment {
                    start: 8,
                    end: Some(8),
                    segment_type: Some("data".to_string()),
                    file: Some("zero".to_string()),
                },
                Subsegment {
                    start: 0,
                    end: Some(16),
                    segment_type: Some("c".to_string()),
                    file: Some("zero".to_string()),
                },
                Subsegment {
                    start: 16,
                    end: Some(24),
                    segment_type: Some("c".to_string()),
                    file: Some("lib/spr/spr_1c".to_string()),
                },
                Subsegment {
                    start: 24,
                    end: Some(32),
                    segment_type: Some("c".to_string()),
                    file: Some("zero".to_string()),
                },
            ]),
        };

        assert_eq!(
            linker_inputs(&segment),
            vec![
                LinkerInput {
                    start: 0,
                    object: "zero.o".to_string(),
                    section: ".text".to_string(),
                },
                LinkerInput {
                    start: 16,
                    object: "lib/spr/spr_1c.o".to_string(),
                    section: ".text".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_splat_style_named_sections_generate_in_yaml_order() {
        let yaml = r#"
options:
  target_path: fixture.bin
  asm_path: asm
  src_path: src
  obj_path: build
  ld_scripts_path: build
  syms_path: build
  check_layout: true
  decomp_empty_funcs: false
segments:
  - name: fixture
    type: code
    start: 0
    vram: 0x06010000
    subalign: 2
    subsegments:
      - [0x0, .data, header]
      - [0x8, c, main]
      - [0x20, .data, animations]
      - [0x28, .rodata, tables]
      - start: 0x30
        type: data
        file: raw_tail
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let segment = &config.segments.as_ref().unwrap()[0];
        let inputs = linker_inputs(segment);

        assert_eq!(
            inputs,
            vec![
                LinkerInput { start: 0, object: "header.o".to_string(), section: ".data".to_string() },
                LinkerInput { start: 8, object: "main.o".to_string(), section: ".text".to_string() },
                LinkerInput { start: 0x20, object: "animations.o".to_string(), section: ".data".to_string() },
                LinkerInput { start: 0x28, object: "tables.o".to_string(), section: ".rodata".to_string() },
                LinkerInput { start: 0x30, object: "raw_tail.o".to_string(), section: ".text".to_string() },
            ]
        );

        let script = gen_ld_script("fixture", "06010000", 2, "build", true, &inputs);
        assert!(script.contains("ASSERT(. - ADDR(.fixture) == 0x0"));
        assert!(script.contains("build/header.o(.data);"));
        assert!(script.contains("ASSERT(. - ADDR(.fixture) == 0x8"));
        assert!(script.contains("build/main.o(.text);"));
        assert!(script.contains("ASSERT(. - ADDR(.fixture) == 0x20"));
        assert!(script.contains("build/animations.o(.data);"));
        assert!(script.contains("ASSERT(. - ADDR(.fixture) == 0x28"));
        assert!(script.contains("build/tables.o(.rodata);"));
        assert!(script.contains("ASSERT(. - ADDR(.fixture) == 0x30"));
        assert!(script.contains("build/raw_tail.o(.text);"));
    }

    fn test_base_mov_l(expected: String, base: u64) {
        let asm = r#"
        mov.l r8, @-r15
        mov r0, r1
        rts
        nop
    
        mov.l r8, @-r15
        mov.l      @(dtest-.,pc),r0
        mov.w      @(wtest-.,pc),r1
        rts
        nop
        .align 2
        dtest:
        .long 0xdeadbeef
        .align 2
        wtest:
        .word 0xface
        "#;

        asm_test_case(asm.to_string(), expected.to_string(), base);
    }

    // mov.l      @(dtest-.,pc),r0
    // is apparently the new version of @(dtest,pc), r0
    // see https://github.com/bminor/binutils-gdb/commit/9691d64f9a558a599867a6528db1908e4c5bc63f
    #[test]
    fn check_mov_l_pc_0() {
        // TODO mov.w/mov.l should emit a label in addition to labeling as
        // data
        // let expected = r#"glabel func_00000008
        // /* 0x00000008 0x2F86 */ mov.l r8, @-r15
        // /* 0x0000000A 0xD002 */ mov.l @(0x00A, pc), r0
        // /* 0x0000000C 0x9104 */ mov.w @(0x00C, pc), r1
        // /* 0x0000000E 0x000B */ rts
        // /* 0x00000010 0x0009 */ nop
        // /* 0x00000012 */ .word 0x0009
        // /* 0x00000014 */ .word 0xDEAD
        // /* 0x00000016 */ .word 0xBEEF
        // /* 0x00000018 */ .word 0xFACE
        // /* 0x0000001A */ .word 0x0009
        // "#;
        let expected = r#"glabel func_00000008
                        /* 0x00000008 0x2F86 */ mov.l r8, @-r15
                        /* 0x0000000A 0xD002 */ mov.l @(dat_00000014, pc), r0
                        /* 0x0000000C 0x9104 */ mov.w @(dat_00000018, pc), r1
                        /* 0x0000000E 0x000B */ rts
                        /* 0x00000010 0x0009 */ nop
                        /* 0x00000012 */ .word 0x0009
                        dat_00000014: /* source: 0000000A */
                        /* 00000014 */ .long 0xDEADBEEF
                        dat_00000018: /* source: 0000000C */
                        .word 0xFACE
                        /* 0x0000001A */ .word 0x0009
                        "#;
        test_base_mov_l(expected.to_string(), 0);
    }

    #[test]
    fn check_mov_l_pc_1000() {
        // TODO mov.w/mov.l should emit a label in addition to labeling as
        // data
        let expected = r#"glabel func_00001008
        /* 0x00001008 0x2F86 */ mov.l r8, @-r15
        /* 0x0000100A 0xD002 */ mov.l @(dat_00001014, pc), r0
        /* 0x0000100C 0x9104 */ mov.w @(dat_00001018, pc), r1
        /* 0x0000100E 0x000B */ rts
        /* 0x00001010 0x0009 */ nop
        /* 0x00001012 */ .word 0x0009
        dat_00001014: /* source: 0000100A */
        /* 00001014 */ .long 0xDEADBEEF
        dat_00001018: /* source: 0000100C */
        .word 0xFACE
        /* 0x0000101A */ .word 0x0009
        "#;

        test_base_mov_l(expected.to_string(), 0x1000);
    }

    fn test_base(expected: String, base: u64) {
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
        asm_test_case(asm.to_string(), expected.to_string(), base);
    }

    #[test]
    fn do_asm_0() {
        let expected_0 = r#"glabel func_00000008
        /* 0x00000008 0x2F86 */ mov.l r8, @-r15
        /* 0x0000000A 0x6103 */ mov r0, r1
        /* 0x0000000C 0xA000 */ bra lab_00000010
        /* 0x0000000E 0x0009 */ nop
        lab_00000010:
        /* 0x00000010 0x6013 */ mov r1, r0
        /* 0x00000012 0x000B */ rts
        /* 0x00000014 0x0009 */ nop
        "#;
        test_base(expected_0.to_string(), 0);
    }

    #[test]
    fn do_asm_1000() {
        let expected = r#"glabel func_00001008
        /* 0x00001008 0x2F86 */ mov.l r8, @-r15
        /* 0x0000100A 0x6103 */ mov r0, r1
        /* 0x0000100C 0xA000 */ bra lab_00001010
        /* 0x0000100E 0x0009 */ nop
        lab_00001010:
        /* 0x00001010 0x6013 */ mov r1, r0
        /* 0x00001012 0x000B */ rts
        /* 0x00001014 0x0009 */ nop
        "#;
        test_base(expected.to_string(), 0x1000);
    }

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
        assert_eq!(string, "mov.w @(0x0E0,pc),r3");
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
        assert_eq!(string, "mov.l @(0x0DE,pc),r6");
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
        assert_eq!(string, "mov.l r1,@(r0,r9)");
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
        assert_eq!(string, "mov.w r1,@(r0,r14)");
    }

    #[test]
    fn test_8450() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x8450,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.b @(0x000,r5),r0");
    }

    #[test]
    fn test_8550() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x8550,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov.w @(0x000,r5),r0");
    }

    #[test]
    fn test_6143() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x6143,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mov r4, r1");
    }

    #[test]
    fn test_01ff() {
        let mut string = String::new();
        let mut data_labels = HashMap::<u32, DataLabel>::new();
        let mut branch_labels = HashMap::<u32, String>::new();
        sh2_disasm(
            0x7a,
            0x01ff,
            true,
            &mut string,
            &mut data_labels,
            &mut branch_labels,
        );
        assert_eq!(string, "mac.l @r15+, @r1+");
    }

    #[test]
    fn test_parse_tt_000_yaml() {
            let config = parse_yaml2("./config.yaml".to_string());
    
            let segments = config.segments.expect("Missing segments");
    
            assert_eq!(segments.len(), 1);
    
            let seg = &segments[0];
            assert_eq!(seg.name, "tt_000");
            assert_eq!(seg.segment_type, "code");
            assert_eq!(seg.start, 0);
            assert_eq!(seg.vram, 0x80170000);
    
            let subsegments = seg.subsegments.as_ref().unwrap();
            assert_eq!(subsegments.len(), 3);
    
            assert_eq!(subsegments[0].start, 0x0);
            assert_eq!(subsegments[0].end, Some(0x5F));
            assert_eq!(subsegments[0].segment_type.as_deref(), Some("data"));
    
            assert_eq!(subsegments[1].start, 0x60);
            assert_eq!(subsegments[1].end, Some(0x2857));
            assert_eq!(subsegments[1].segment_type.as_deref(), Some("c"));
    
            assert_eq!(subsegments[2].start, 0x2858);
            assert_eq!(subsegments[2].end, Some(0x7000));
            assert_eq!(subsegments[2].segment_type.as_deref(), Some("data"));
    }

    #[test]
    fn test_mova_jump_table() {
        let mut bytes = vec![0u8; 0x40];
        let words = [
            0xe107, 0x3216, 0x6123, 0x311c, 0xc702, 0x011d, 0x301c, 0x402b, 0x0009,
            0x0009, 0x0010, 0x0012, 0x0014, 0x0016, 0x0018, 0x001a, 0x001c, 0x001e,
        ];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 2] = (word >> 8) as u8;
            bytes[index * 2 + 1] = *word as u8;
        }

        let ranges = vec![FunctionRange {
            phys_start: 0,
            phys_end: 0x3e,
            is_data: false,
        }];
        let mut branch_labels = HashMap::new();
        let mut entries = HashMap::new();
        find_jump_tables(
            &bytes,
            0,
            bytes.len() as u64,
            0,
            &ranges,
            &mut branch_labels,
            &mut entries,
        );

        assert_eq!(branch_labels.get(&0x14).unwrap(), ".Ljtbl_00000014");
        assert_eq!(entries.len(), 8);
        branch_labels.insert(0x1a, ".L0000001A".to_string());
        remove_jump_table_internal_labels(&mut branch_labels, &entries);
        assert!(!branch_labels.contains_key(&0x1a));
        assert_eq!(
            branch_labels.get(&0x14).unwrap(),
            &entries.get(&0x14).unwrap().table_label
        );

        let mut string = String::new();
        sh2_disasm(
            8,
            0xc702,
            true,
            &mut string,
            &HashMap::new(),
            &branch_labels,
        );
        assert_eq!(string, "mova .Ljtbl_00000014,r0");
    }

    #[test]
    fn test_mova_table_outside_function_is_not_classified() {
        let mut bytes = vec![0u8; 0x100];
        let words = [
            0xe107, 0x3216, 0x6123, 0x311c, 0xc720, 0x011d, 0x301c, 0x402b,
        ];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 2] = (word >> 8) as u8;
            bytes[index * 2 + 1] = *word as u8;
        }
        for index in 0..8 {
            let entry = 0x8c + index * 2;
            let offset = (0x20i16 - 0x8ci16) as u16;
            bytes[entry] = (offset >> 8) as u8;
            bytes[entry + 1] = offset as u8;
        }

        let ranges = vec![FunctionRange {
            phys_start: 0,
            phys_end: 0x3e,
            is_data: false,
        }];
        let mut branch_labels = HashMap::new();
        let mut entries = HashMap::new();
        find_jump_tables(
            &bytes,
            0,
            bytes.len() as u64,
            0,
            &ranges,
            &mut branch_labels,
            &mut entries,
        );

        assert!(!branch_labels.contains_key(&0x8c));
        assert!(entries.is_empty());
    }

    #[test]
    fn test_mova_without_table_uses_symbolic_target() {
        let mut branch_labels = HashMap::new();
        find_branch_labels(0x060a7420, 0xc702, &mut branch_labels);

        let mut string = String::new();
        sh2_disasm(
            0x060a7420,
            0xc702,
            true,
            &mut string,
            &HashMap::new(),
            &branch_labels,
        );
        assert_eq!(string, "mova .L060A742C,r0");
    }

    #[test]
    fn test_object_symbols_in_literal_pool() {
        let mut symbols_file = NamedTempFile::new().unwrap();
        writeln!(symbols_file, "_g_Entities = 0x060997F8;").unwrap();
        writeln!(symbols_file, "_DestroyEntity = 0x0600FFB8;").unwrap();
        let symbols = read_user_symbols(symbols_file.path().to_str().unwrap());
        let call_sequence = vec![0xd100, 0x0009, 0x410b]
            .into_iter()
            .flat_map(|word: u16| word.to_be_bytes())
            .collect::<Vec<_>>();

        assert_eq!(format_literal(0x060997f8, &symbols, true), "_g_Entities");
        assert!(literal_feeds_call(&call_sequence, 0, 0));
        assert_eq!(
            format_literal(0x0600ffb8, &symbols, false),
            "_DestroyEntity"
        );
    }

    #[test]
    fn test_generated_labels_are_local() {
        let mut branch_labels = HashMap::new();
        let mut data_labels = HashMap::new();
        add_label(0x060a9200, &mut branch_labels);
        add_data_label(0x060a91e0, 0x060a9298, 4, &mut data_labels);

        assert_eq!(branch_labels.get(&0x060a9200).unwrap(), ".L060A9200");
        assert_eq!(data_labels.get(&0x060a9298).unwrap().label, ".Ldat_060A9298");
    }

    #[test]
    fn test_normalized_operand_syntax() {
        let mut data_labels = HashMap::new();
        add_data_label(0, 12, 4, &mut data_labels);
        let branch_labels = HashMap::new();

        let mut pc_relative = String::new();
        sh2_disasm(
            0,
            0xd002,
            true,
            &mut pc_relative,
            &data_labels,
            &branch_labels,
        );
        assert_eq!(pc_relative, "mov.l .Ldat_0000000C,r0");

        let mut displaced = String::new();
        sh2_disasm(
            0,
            0x181d,
            true,
            &mut displaced,
            &HashMap::new(),
            &branch_labels,
        );
        assert_eq!(displaced, "mov.l r1,@(0x034,r8)");

        let mut indexed = String::new();
        sh2_disasm(
            0,
            0x011d,
            true,
            &mut indexed,
            &HashMap::new(),
            &branch_labels,
        );
        assert_eq!(indexed, "mov.w @(r0,r1),r1");
    }
}
