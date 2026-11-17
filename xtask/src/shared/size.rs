// A copy from embedded-conventions/xtask/src/shared. Edit it there, then run `cargo xtask sync` there.

//! What an ELF image takes on the device: its flash, and each RAM its sections lie in.
//!
//! A section counts in the memory with the address range that holds it, and not by its flags. An ESP32 image copies code
//! into SRAM and PSRAM, in sections that are not writable, so a rule by the write flag misses that RAM.

// A repo calls these from its own size command, and a repo without firmware does not have one yet.
#![allow(dead_code)]

use object::Endianness;
use object::elf::{SHF_ALLOC, SHT_NOBITS};
use object::read::elf::{ElfFile32, SectionHeader};

/// One section header of an ELF image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// The address the section runs at.
    pub address: u64,
    pub size: u64,
    /// `SHF_ALLOC`: the section takes space on the device.
    pub alloc: bool,
    /// `SHT_NOBITS`: the image holds no bytes for the section, so it takes no flash.
    pub nobits: bool,
}

/// What a memory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Code and constants that run from the flash, through its window.
    Flash,
    /// RAM, with sections that the footprint counts.
    Ram,
}

/// One address range of the device, from `start` up to `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// The memory's name, such as `SRAM`, in the size report.
    pub name: &'static str,
    pub start: u64,
    pub end: u64,
    pub kind: Kind,
}

/// What one image takes on the device, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footprint {
    /// Every allocated section with bytes in the image. A section copied into RAM at boot counts here as the
    /// bytes it is copied from.
    pub flash: u64,
    /// Each RAM in the order given, with its allocated sections, less the regions the runtime reserves.
    pub ram: Vec<(&'static str, u64)>,
}

/// The unwind data of the two architectures. The images abort on panic, so unwind data is dead weight, and the
/// linker scripts drop it.
const UNWIND: [&str; 4] = [".eh_frame", ".eh_frame_hdr", ".ARM.exidx", ".ARM.extab"];

/// The runtimes' reservations of the RAM that no static uses: riscv-rt's and esp-hal's `.stack`, and riscv-rt's
/// `.heap`.
const RESERVED: [&str; 2] = [".stack", ".heap"];

/// The section headers of a 32-bit ELF image.
pub fn sections(data: &[u8]) -> Result<Vec<Section>, String> {
    let file = ElfFile32::<Endianness>::parse(data)
        .map_err(|error| format!("not a 32-bit ELF image: {error}"))?;
    let endian = file.endian();
    let table = file.elf_section_table();
    table
        .iter()
        .map(|header| {
            let name = table
                .section_name(endian, header)
                .map_err(|error| format!("a section has no name: {error}"))?;
            Ok(Section {
                name: String::from_utf8_lossy(name).into_owned(),
                address: u64::from(header.sh_addr(endian)),
                size: u64::from(header.sh_size(endian)),
                alloc: (header.sh_flags(endian) & SHF_ALLOC).0 != 0,
                nobits: header.sh_type(endian) == SHT_NOBITS,
            })
        })
        .collect()
}

/// Sums `sections` into flash and into each RAM of `memories`.
///
/// Returns an error naming an allocated section that lies in no memory, so a new layout cannot be miscounted
/// without notice. Returns an error naming a section of unwind data that the image allocates.
pub fn footprint(sections: &[Section], memories: &[Memory]) -> Result<Footprint, String> {
    let mut flash = 0u64;
    let mut ram: Vec<(&'static str, u64)> = memories
        .iter()
        .filter(|memory| memory.kind == Kind::Ram)
        .map(|memory| (memory.name, 0))
        .collect();
    for section in sections
        .iter()
        .filter(|section| section.alloc && section.size > 0)
    {
        if UNWIND.contains(&section.name.as_str()) {
            return Err(format!(
                "the image allocates {} bytes of {}, which a linker script should discard",
                section.size, section.name
            ));
        }
        let end = section.address.saturating_add(section.size);
        let memory = memories
            .iter()
            .find(|memory| memory.start <= section.address && end <= memory.end)
            .ok_or_else(|| {
                format!(
                    "{} at {:#x}, {} bytes, lies in no memory of the device",
                    section.name, section.address, section.size
                )
            })?;
        if !section.nobits {
            flash = flash.saturating_add(section.size);
        }
        if memory.kind == Kind::Ram
            && !RESERVED.contains(&section.name.as_str())
            && let Some((_, used)) = ram.iter_mut().find(|(name, _)| *name == memory.name)
        {
            *used = used.saturating_add(section.size);
        }
    }
    Ok(Footprint { flash, ram })
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::elf::{
        ELFOSABI_NONE, EM_RISCV, ET_EXEC, FileFlags, SHF_EXECINSTR, SHF_WRITE, SHT_PROGBITS,
    };
    use object::write::elf::{FileHeader, SectionHeader as WriteHeader, Writer};

    /// The S31's memories.
    const S31: [Memory; 4] = [
        Memory {
            name: "flash",
            start: 0x4000_0000,
            end: 0x5000_0000,
            kind: Kind::Flash,
        },
        Memory {
            name: "SRAM",
            start: 0x2F00_0000,
            end: 0x2F08_0000,
            kind: Kind::Ram,
        },
        Memory {
            name: "PSRAM",
            start: 0x5000_0000,
            end: 0x5400_0000,
            kind: Kind::Ram,
        },
        Memory {
            name: "LP SRAM",
            start: 0x2E00_0000,
            end: 0x2E00_8000,
            kind: Kind::Ram,
        },
    ];

    fn section(name: &str, address: u64, size: u64, nobits: bool) -> Section {
        Section {
            name: name.to_owned(),
            address,
            size,
            alloc: true,
            nobits,
        }
    }

    /// The sections of the S31's `console_ethernet`, in its order, at their real addresses and with sizes that
    /// are easy to add up.
    fn s31_image() -> Vec<Section> {
        vec![
            Section {
                alloc: false,
                ..section(".defmt", 0, 0xac, false)
            },
            section(".trap", 0x2F00_0000, 0x10, false),
            section(".rwtext", 0x2F00_0010, 0x20, false),
            section(".data", 0x2F00_0030, 0x40, false),
            section(".bss", 0x2F00_0070, 0x400, true),
            section(".flash.appdesc", 0x5000_0020, 0x100, false),
            section(".rodata_merge", 0x5000_0120, 0, false),
            section(".rodata", 0x5000_0120, 0x800, false),
            section(".text_gap", 0x5000_0920, 0x8, true),
            section(".text", 0x5000_0928, 0x4000, false),
            section(".rtc_fast.persistent", 0x2E00_0000, 0x4, true),
            section(".stack", 0x2F00_0470, 0x1000, true),
            section(".code_in_psram_boot", 0x2F00_1470, 0x80, false),
            Section {
                alloc: false,
                ..section(".eh_frame", 0, 0x1b4c, false)
            },
        ]
    }

    #[test]
    fn code_copied_into_ram_counts_in_flash_and_in_its_ram() {
        let footprint = footprint(&s31_image(), &S31).unwrap();
        assert_eq!(
            footprint.flash,
            0x10 + 0x20 + 0x40 + 0x100 + 0x800 + 0x4000 + 0x80
        );
        assert_eq!(
            footprint.ram,
            vec![
                ("SRAM", 0x10 + 0x20 + 0x40 + 0x400 + 0x80),
                ("PSRAM", 0x100 + 0x800 + 0x8 + 0x4000),
                ("LP SRAM", 0x4),
            ]
        );
    }

    #[test]
    fn the_heap_reservation_counts_in_no_memory() {
        let sections = [
            section(".bss", 0x2F00_0000, 0x40, true),
            section(".heap", 0x2F00_0040, 0x800, true),
        ];
        let footprint = footprint(&sections, &S31).unwrap();
        assert_eq!(footprint.ram[0], ("SRAM", 0x40));
    }

    #[test]
    fn a_section_outside_every_memory_is_an_error() {
        let mut sections = s31_image();
        sections.push(section(".lost", 0x6000_0000, 0x10, false));
        let error = footprint(&sections, &S31).unwrap_err();
        assert!(
            error.contains(".lost") && error.contains("0x60000000"),
            "{error}"
        );
    }

    #[test]
    fn a_section_that_runs_past_its_memory_is_an_error() {
        let sections = [section(".bss", 0x2F07_FFF0, 0x20, true)];
        let error = footprint(&sections, &S31).unwrap_err();
        assert!(error.contains(".bss"), "{error}");
    }

    #[test]
    fn a_section_that_fills_its_memory_to_the_end_fits() {
        let sections = [section(".bss", 0x2F07_FFF0, 0x10, true)];
        assert_eq!(footprint(&sections, &S31).unwrap().ram[0], ("SRAM", 0x10));
    }

    #[test]
    fn an_empty_section_outside_every_memory_is_left_out() {
        let sections = [section(".rtc_fast.text", 0x6000_0000, 0, false)];
        let footprint = footprint(&sections, &S31).unwrap();
        assert_eq!(footprint.flash, 0);
    }

    #[test]
    fn allocated_unwind_data_is_an_error() {
        let mut sections = s31_image();
        sections.push(section(".eh_frame", 0x5000_5000, 0x40, false));
        let error = footprint(&sections, &S31).unwrap_err();
        assert!(error.contains(".eh_frame"), "{error}");
    }

    /// An executable riscv image with `.text` of 48 bytes in SRAM and `.bss` of 16 in PSRAM.
    fn elf() -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut writer = Writer::new(Endianness::Little, false, &mut buffer);
        writer.reserve_file_header();
        let text_name = writer.add_section_name(b".text");
        let bss_name = writer.add_section_name(b".bss");
        writer.reserve_null_section_index();
        writer.reserve_section_index();
        writer.reserve_section_index();
        writer.reserve_shstrtab_section_index();
        let text_offset = writer.reserve(48, 4);
        writer.reserve_shstrtab().unwrap();
        writer.reserve_section_headers();
        writer
            .write_file_header(&FileHeader {
                os_abi: ELFOSABI_NONE,
                abi_version: 0,
                e_type: ET_EXEC,
                e_machine: EM_RISCV,
                e_entry: 0,
                e_flags: FileFlags(0),
            })
            .unwrap();
        writer.write_align(4);
        writer.write(&[0; 48]);
        writer.write_shstrtab();
        writer.write_null_section_header();
        writer.write_section_header(&WriteHeader {
            sh_name: writer.section_name_offset(Some(text_name)),
            sh_type: SHT_PROGBITS,
            sh_flags: SHF_ALLOC | SHF_EXECINSTR,
            sh_addr: 0x2F00_0100,
            sh_offset: text_offset,
            sh_size: 48,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 4,
            sh_entsize: 0,
        });
        writer.write_section_header(&WriteHeader {
            sh_name: writer.section_name_offset(Some(bss_name)),
            sh_type: SHT_NOBITS,
            sh_flags: SHF_ALLOC | SHF_WRITE,
            sh_addr: 0x5000_0200,
            sh_offset: 0,
            sh_size: 16,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 4,
            sh_entsize: 0,
        });
        writer.write_shstrtab_section_header();
        buffer
    }

    #[test]
    fn sections_reads_each_header() {
        let sections = sections(&elf()).unwrap();
        let find = |name: &str| {
            sections
                .iter()
                .find(|section| section.name == name)
                .cloned()
                .unwrap()
        };
        assert_eq!(find(".text"), section(".text", 0x2F00_0100, 48, false));
        assert_eq!(find(".bss"), section(".bss", 0x5000_0200, 16, true));
        assert!(!find(".shstrtab").alloc);
    }

    #[test]
    fn sections_refuses_bytes_that_are_not_an_elf_image() {
        let error = sections(b"not an image").unwrap_err();
        assert!(error.contains("not a 32-bit ELF image"), "{error}");
    }
}
