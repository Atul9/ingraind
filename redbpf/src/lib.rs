#![cfg_attr(feature = "cargo-clippy", allow(clippy))]

extern crate bpf_sys;
extern crate goblin;
extern crate libc;
extern crate zero;

mod cpus;

use bpf_sys::{bpf_insn, bpf_map_def};
use goblin::elf::{section_header as hdr, Elf, Reloc, SectionHeader, Sym};

use std::collections::HashMap;
use std::ffi::{CStr, CString, NulError};
use std::mem;
use std::os::unix::io::RawFd;
use std::str::FromStr;

pub type Result<T> = std::result::Result<T, LoadError>;

#[derive(Debug)]
pub enum LoadError {
    StringConversion,
    BPF,
    Section(String),
    Parse(goblin::error::Error),
    KernelRelease(String),
    Uname,
    Reloc,
}

impl From<goblin::error::Error> for LoadError {
    fn from(e: goblin::error::Error) -> LoadError {
        LoadError::Parse(e)
    }
}

impl From<NulError> for LoadError {
    fn from(_e: NulError) -> LoadError {
        LoadError::StringConversion
    }
}

struct Module {
    bytes: Vec<u8>,
    programs: Vec<Program>,
    maps: Vec<Map>,
    license: String,
    version: u32,
}

struct Program {
    pfd: Option<RawFd>,
    fd: Option<RawFd>,
    kind: ProgramKind,
    name: String,
    code: Vec<bpf_insn>,
}

enum ProgramKind {
    Kprobe,
    Kretprobe,
}

struct Map {
    name: String,
    kind: u32,
    fd: RawFd,
}

struct Rel {
    shndx: usize,
    target: usize,
    offset: u64,
    sym: usize,
}

struct PerfMap {
    fd: u32,
    name: String,
    page_count: u32,
    callback: Box<FnMut(&[u8])>,
}

impl ProgramKind {
    fn to_prog_type(&self) -> bpf_sys::bpf_prog_type {
        use ProgramKind::*;
        match self {
            Kprobe | Kretprobe => bpf_sys::bpf_prog_type_BPF_PROG_TYPE_KPROBE,
        }
    }

    fn to_attach_type(&self) -> bpf_sys::bpf_probe_attach_type {
        use ProgramKind::*;
        match self {
            Kprobe => bpf_sys::bpf_probe_attach_type_BPF_PROBE_ENTRY,
            Kretprobe => bpf_sys::bpf_probe_attach_type_BPF_PROBE_RETURN,
        }
    }

    fn from_section(section: &str) -> Result<ProgramKind> {
        use ProgramKind::*;
        match section {
            "kretprobe" => Ok(Kretprobe),
            "kprobe" => Ok(Kprobe),
            sec => Err(LoadError::Section(sec.to_string())),
        }
    }
}

impl Program {
    fn new(name: &str, code: &[u8]) -> Result<Program> {
        let code = zero::read_array(code).to_vec();
        let mut names = name.splitn(2, '/');

        let kind = names.next().ok_or(parse_fail("section type"))?;
        let name = names.next().ok_or(parse_fail("section name"))?.to_string();
        let kind = ProgramKind::from_section(kind)?;

        Ok(Program {
            pfd: None,
            fd: None,
            kind,
            name,
            code,
        })
    }

    fn is_loaded(&self) -> bool {
        self.fd.is_some()
    }

    fn is_attached(&self) -> bool {
        self.pfd.is_some()
    }

    fn load(&mut self, kernel_version: u32, license: String) -> Result<RawFd> {
        let clicense = CString::new(license)?;
        let cname = CString::new(self.name.clone())?;
        let mut log_buffer = [0u8; 65535];

        let fd = unsafe {
            bpf_sys::bpf_prog_load(
                self.kind.to_prog_type(),
                cname.as_ptr() as *const i8,
                self.code.as_ptr(),
                self.code.len() as i32,
                clicense.as_ptr() as *const i8,
                kernel_version as u32,
                0 as i32,
                log_buffer.as_mut_ptr() as *mut i8,
                mem::size_of_val(&log_buffer) as u32,
            )
        };

        if fd < 0 {
            Err(LoadError::BPF)
        } else {
            self.fd = Some(fd);
            Ok(fd)
        }
    }

    fn attach(&mut self) -> Result<RawFd> {
        unsafe {
            let cname = CString::new(self.name.clone()).unwrap();
            let pfd = bpf_sys::bpf_attach_kprobe(
                self.fd.unwrap(),
                self.kind.to_attach_type(),
                cname.as_ptr(),
                cname.as_ptr(),
                0,
            );

            if pfd < 0 {
                Err(LoadError::BPF)
            } else {
                self.pfd = Some(pfd);
                Ok(pfd)
            }
        }
    }
}

impl Module {
    fn parse(bytes: Vec<u8>) -> Result<Module> {
        let object = Elf::parse(&bytes[..])?;
        let strings = object.shdr_strtab.to_vec()?;
        let symtab = object.syms.to_vec();
        let shdr_relocs = &object.shdr_relocs;

        let mut rels = vec![];
        let mut programs = HashMap::new();
        let mut maps = HashMap::new();

        let mut license = String::new();
        let mut version = 0u32;

        for (shndx, shdr) in object.section_headers.iter().enumerate() {
            let name = strings[shdr.sh_name];
            let kind = shdr.sh_type;
            let content = data(&bytes, &shdr);

            match (kind, name) {
                (hdr::SHT_REL, _) => add_rel(&mut rels, shndx, &shdr, &shdr_relocs),
                (hdr::SHT_PROGBITS, "license") => license.insert_str(0, zero::read_str(content)),
                (hdr::SHT_PROGBITS, "version") => version = get_version(&content),
                (hdr::SHT_PROGBITS, "maps") => {
                    // Maps are immediately bpf_create_map'd
                    maps.insert(shndx, Map::load(&name, &content)?);
                }
                (hdr::SHT_PROGBITS, name) => {
                    programs.insert(shndx, Program::new(&name, &content)?);
                }
                _ => {}
            }
        }

        // Rewrite programs with relocation data
        for rel in rels.iter() {
            rel.apply(&mut programs, &maps, &symtab)?;
        }

        let programs = programs.drain().map(|(_, v)| v).collect();
        let maps = maps.drain().map(|(_, v)| v).collect();
        Ok(Module {
            bytes: bytes.clone(),
            programs,
            maps,
            license,
            version,
        })
    }
}

impl Rel {
    #[inline]
    fn apply(
        &self,
        programs: &mut HashMap<usize, Program>,
        maps: &HashMap<usize, Map>,
        symtab: &Vec<Sym>,
    ) -> Result<()> {
        let prog = programs.get_mut(&self.target).ok_or(LoadError::Reloc)?;
        let map = maps
            .get(&symtab[self.sym].st_shndx)
            .ok_or(LoadError::Reloc)?;
        let insn_idx = (self.offset / std::mem::size_of::<bpf_insn>() as u64) as usize;

        prog.code[insn_idx].set_src_reg(bpf_sys::BPF_PSEUDO_MAP_FD as u8);
        prog.code[insn_idx].imm = map.fd;

        Ok(())
    }
}

impl Map {
    fn load(name: &str, code: &[u8]) -> Result<Map> {
        let config: &bpf_map_def = zero::read(code);
        let cname = CString::new(name.clone())?;
        let fd = unsafe {
            bpf_sys::bpf_create_map(
                config.kind,
                cname.as_ptr(),
                config.key_size as i32,
                config.value_size as i32,
                config.max_entries as i32,
                config.map_flags as i32,
            )
        };
        if fd < 0 {
            return Err(LoadError::BPF);
        }

        Ok(Map {
            name: name.to_string(),
            kind: config.kind,
            fd,
        })
    }
}

#[inline]
fn add_rel(
    rels: &mut Vec<Rel>,
    shndx: usize,
    shdr: &SectionHeader,
    shdr_relocs: &Vec<(usize, Vec<Reloc>)>,
) {
    // if unwrap blows up, something's really bad
    let section_rels = &shdr_relocs.iter().find(|(idx, _)| idx == &shndx).unwrap().1;
    rels.extend(section_rels.iter().map(|rel| Rel {
        shndx,
        target: shdr.sh_info as usize,
        sym: rel.r_sym,
        offset: rel.r_offset,
    }));
}

#[inline]
fn get_version(bytes: &[u8]) -> u32 {
    let version = zero::read::<u32>(bytes);
    match version {
        0xFFFFFFFE => get_kernel_version().unwrap(),
        _ => version.clone(),
    }
}

#[inline]
fn get_kernel_version() -> Result<u32> {
    let mut uname = libc::utsname {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };

    let res = unsafe { libc::uname(&mut uname) };
    if res < 0 {
        return Err(LoadError::Uname);
    }

    let urelease = to_str(&uname.release);
    let err = || LoadError::KernelRelease(urelease.to_string());
    let err_ = |_| LoadError::KernelRelease(urelease.to_string());

    let mut release_package = urelease.splitn(2, '-');
    let mut release = release_package.next().ok_or(err())?.splitn(3, '.');

    let major = u32::from_str(release.next().ok_or(err())?).map_err(err_)?;
    let minor = u32::from_str(release.next().ok_or(err())?).map_err(err_)?;
    let patch = u32::from_str(release.next().ok_or(err())?).map_err(err_)?;

    Ok(major << 16 | minor << 8 | patch)
}

#[inline]
fn to_str(bytes: &[i8]) -> &str {
    unsafe {
        let cstr = CStr::from_ptr(bytes.as_ptr());
        std::str::from_utf8_unchecked(cstr.to_bytes())
    }
}

#[inline]
fn data<'d>(bytes: &'d [u8], shdr: &SectionHeader) -> &'d [u8] {
    let offset = shdr.sh_offset as usize;
    let end = (shdr.sh_offset + shdr.sh_size) as usize;

    &bytes[offset..end]
}

#[inline]
fn parse_fail(reason: &str) -> goblin::error::Error {
    goblin::error::Error::Malformed(format!("Failed to parse: {}", reason))
}
