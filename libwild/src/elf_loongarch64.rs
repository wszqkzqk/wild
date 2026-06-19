use crate::elf::Elf;
use crate::elf::PLT_ENTRY_SIZE;
use crate::elf::RelocationList;
use crate::error;
use crate::error::Result;
use crate::platform::Platform;
use crate::platform::RelaxSymbolInfo;
use crate::platform::Relocation;
use crate::platform::RelocationSequence;
use itertools::AllEqualValueError;
use itertools::Itertools;
use linker_utils::elf::DynamicRelocationKind;
use linker_utils::elf::PAGE_MASK_4KB;
use linker_utils::elf::RelocationKind;
use linker_utils::elf::RelocationKindInfo;
use linker_utils::elf::SIZE_2KB;
use linker_utils::elf::loongarch64_rel_type_to_string;
use linker_utils::elf::shf;
use linker_utils::loongarch64::B26_RANGE;
use linker_utils::loongarch64::RelaxationKind;
use linker_utils::loongarch64::distance_fits_b26;
use linker_utils::loongarch64::relocation_type_from_raw;
use linker_utils::relaxation::RelocationModifier;
use linker_utils::relaxation::SectionRelaxDeltas;
use linker_utils::utils::or_from_slice;

pub(crate) struct ElfLoongArch64;

const PLT_ENTRY_TEMPLATE: &[u8] = &[
    0x0f, 0x0, 0x0, 0x1a, // pcalau12i $t3, offset_high(&(.got.plt[n])
    0xef, 0x1, 0xc0, 0x28, // ld.d $t3, $t3,offset_low(&(.got.plt[n])(t3)
    0xed, 0x1, 0x0, 0x4c, // jirl $t1, $t3, 0
    0x0, 0x0, 0x2a, 0x0, // break
];

const _ASSERTS: () = {
    assert!(PLT_ENTRY_TEMPLATE.len() as u64 == PLT_ENTRY_SIZE);
};

macro_rules! rel_info_from_type {
    ($r_type:expr) => {
        const { relocation_type_from_raw($r_type).unwrap() }
    };
}

impl crate::platform::Arch for ElfLoongArch64 {
    type Relaxation = Relaxation;
    type Platform = Elf;

    fn arch_identifier() -> <Self::Platform as Platform>::ArchIdentifier {
        object::elf::EM_LOONGARCH
    }

    #[inline(always)]
    fn relocation_from_raw(r_type: u32) -> Result<RelocationKindInfo> {
        linker_utils::loongarch64::relocation_type_from_raw(r_type).ok_or_else(|| {
            error!(
                "Unsupported relocation type {}",
                Self::rel_type_to_string(r_type)
            )
        })
    }

    fn is_illegal_in_shared_object(r_type: u32) -> bool {
        matches!(r_type, object::elf::R_LARCH_32)
    }

    fn get_dynamic_relocation_type(relocation: DynamicRelocationKind) -> u32 {
        relocation.loongarch64_r_type()
    }

    fn rel_type_to_string(r_type: u32) -> std::borrow::Cow<'static, str> {
        loongarch64_rel_type_to_string(r_type)
    }

    fn write_plt_entry(
        plt_entry: &mut [u8],
        got_address: u64,
        plt_address: u64,
    ) -> crate::error::Result {
        // TODO: For simplicity, we assume now the PLT entry precedes the GOT entry, so we can
        // make the offset calculation in the unsigned type.
        debug_assert!(plt_address < got_address);

        plt_entry.copy_from_slice(PLT_ENTRY_TEMPLATE);
        let pcala_hi20 =
            ((((got_address + SIZE_2KB) & !PAGE_MASK_4KB) - (plt_address & !PAGE_MASK_4KB)) >> 12)
                << 5;
        let pcala_lo12 = (got_address & 0xfff) << 10;
        or_from_slice(&mut plt_entry[0..4], &(pcala_hi20 as u32).to_le_bytes());
        or_from_slice(&mut plt_entry[4..8], &(pcala_lo12 as u32).to_le_bytes());
        Ok(())
    }

    fn get_dtv_offset() -> u64 {
        0
    }

    fn tp_offset_start(layout: &crate::layout::Layout<Elf>) -> u64 {
        layout.tls_start_address()
    }

    fn get_property_class(_property_type: u32) -> Option<crate::elf::PropertyClass> {
        None
    }

    fn merge_eflags(
        mut eflags: impl Iterator<Item = object::elf::FileFlags>,
    ) -> Result<object::elf::FileFlags> {
        match eflags.all_equal_value() {
            Ok(flags) => Ok(flags),
            // no items, return blank flags
            Err(AllEqualValueError(None)) => Ok(object::elf::FileFlags(0)),
            Err(AllEqualValueError(Some([a, b]))) => Err(error!("non-unique e_flags: {a}, {b}")),
        }
    }

    fn high_part_relocations() -> &'static [u32] {
        &[]
    }

    #[allow(unused_variables)]
    #[inline(always)]
    fn new_relaxation(
        relocation_kind: u32,
        section_bytes: &[u8],
        offset_in_section: u64,
        flags: crate::value_flags::ValueFlags,
        output_kind: crate::output_kind::OutputKind,
        section_flags: linker_utils::elf::SectionFlags,
        non_zero_address: bool,
        _relax_deltas: Option<&linker_utils::relaxation::SectionRelaxDeltas>,
    ) -> Option<Self::Relaxation>
    where
        Self: std::marker::Sized,
    {
        let mut relocation = ElfLoongArch64::relocation_from_raw(relocation_kind).unwrap();
        let interposable = flags.is_interposable();

        // All relaxations below only apply to executable code, so we shouldn't attempt them if a
        // relocation is in a non-executable section.
        if !section_flags.contains(shf::EXECINSTR) {
            return None;
        }

        match relocation_kind {
            object::elf::R_LARCH_CALL36 if !interposable => {
                if let Some(rd) = jirl_rd_at(section_bytes, offset_in_section)
                    && (rd == 0 || rd == 1)
                {
                    let kind = if rd == 0 {
                        RelaxationKind::Call36ToB
                    } else {
                        RelaxationKind::Call36ToBl
                    };
                    let mut b26_info = rel_info_from_type!(object::elf::R_LARCH_B26);
                    b26_info.kind = RelocationKind::Relative;
                    return Some(Relaxation {
                        kind,
                        rel_info: b26_info,
                        mandatory: false,
                    });
                }
                relocation.kind = RelocationKind::Relative;
                return Some(Relaxation {
                    kind: RelaxationKind::NoOp,
                    rel_info: relocation,
                    mandatory: true,
                });
            }

            object::elf::R_LARCH_B26 if !interposable => {
                relocation.kind = RelocationKind::Relative;
                return Some(Relaxation {
                    kind: RelaxationKind::NoOp,
                    rel_info: relocation,
                    mandatory: true,
                });
            }

            _ => (),
        }

        None
    }

    fn supports_size_reduction_relaxations() -> bool {
        true
    }

    fn collect_relaxation_deltas(
        section_output_address: u64,
        section_bytes: &[u8],
        relocations: RelocationList,
        existing_deltas: Option<&SectionRelaxDeltas>,
        mut resolve_symbol: impl FnMut(object::SymbolIndex) -> Option<RelaxSymbolInfo>,
    ) -> (Vec<(u64, u32)>, Option<u64>) {
        match relocations {
            RelocationList::Rela(rela_list) => collect_relaxation_deltas(
                section_output_address,
                section_bytes,
                rela_list.rel_iter(),
                existing_deltas,
                &mut resolve_symbol,
            ),
            RelocationList::Crel(crel_iter) => collect_relaxation_deltas(
                section_output_address,
                section_bytes,
                crel_iter.flatten(),
                existing_deltas,
                &mut resolve_symbol,
            ),
        }
    }

    fn get_source_info<'data>(
        object: &<Self::Platform as Platform>::File<'data>,
        relocations: &<Self::Platform as Platform>::RelocationSections,
        section: &<Self::Platform as Platform>::SectionHeader,
        offset_in_section: u64,
    ) -> Result<crate::platform::SourceInfo> {
        crate::dwarf_address_info::get_source_info::<Self>(
            object,
            relocations,
            section,
            offset_in_section,
        )
    }
}

/// Returns the `rd` field of a `jirl` instruction at `offset` if it looks like a `jirl`.
fn jirl_rd_at(section_bytes: &[u8], offset: u64) -> Option<u32> {
    let off = offset as usize;
    if off + 4 > section_bytes.len() {
        return None;
    }
    let word = u32::from_le_bytes(section_bytes[off..off + 4].try_into().unwrap());
    if (word >> 26) & 0x3f != 0x13 {
        return None;
    }
    Some(word & 0x1f)
}

/// Scan relocations for CALL36 relaxation candidates.
fn collect_relaxation_deltas<R: Relocation>(
    section_output_address: u64,
    section_bytes: &[u8],
    relocations: impl Iterator<Item = R>,
    existing_deltas: Option<&SectionRelaxDeltas>,
    mut resolve_symbol: impl FnMut(object::SymbolIndex) -> Option<RelaxSymbolInfo>,
) -> (Vec<(u64, u32)>, Option<u64>) {
    let mut raw_deltas = Vec::new();
    let mut min_unrelaxed_margin: Option<u64> = None;
    let mut prev_call36: Option<(u64, object::SymbolIndex)> = None;

    for rel in relocations {
        match rel.raw_type() {
            object::elf::R_LARCH_CALL36 => {
                prev_call36 = rel.symbol().map(|sym_idx| (rel.offset(), sym_idx));
            }
            object::elf::R_LARCH_RELAX => {
                if let Some((call_offset, sym_idx)) = prev_call36
                    && rel.offset() == call_offset
                    && !existing_deltas.is_some_and(|d| d.has_delta_at(call_offset))
                    && let Some(info) = resolve_symbol(sym_idx)
                    && !info.is_interposable
                {
                    let distance = (info.output_address as i64 + rel.addend())
                        - (section_output_address + call_offset) as i64;
                    if distance_fits_b26(distance) {
                        if let Some(rd) = jirl_rd_at(section_bytes, call_offset + 4)
                            && (rd == 0 || rd == 1)
                        {
                            raw_deltas.push((call_offset, 4));
                        }
                    } else {
                        let b26_max = B26_RANGE.end().unsigned_abs();
                        let margin = distance.unsigned_abs() - b26_max;
                        min_unrelaxed_margin =
                            Some(min_unrelaxed_margin.map_or(margin, |m| m.min(margin)));
                    }
                }
                prev_call36 = None;
            }
            _ => {
                prev_call36 = None;
            }
        }
    }

    (raw_deltas, min_unrelaxed_margin)
}

#[derive(Debug, Clone)]
pub(crate) struct Relaxation {
    kind: RelaxationKind,
    rel_info: RelocationKindInfo,
    mandatory: bool,
}

impl crate::platform::Relaxation for Relaxation {
    fn apply(&self, section_bytes: &mut [u8], offset_in_section: &mut u64, addend: &mut i64) {
        self.kind.apply(section_bytes, offset_in_section, addend);
    }

    fn rel_info(&self) -> RelocationKindInfo {
        self.rel_info
    }

    fn debug_kind(&self) -> impl std::fmt::Debug {
        &self.kind
    }

    fn next_modifier(&self) -> RelocationModifier {
        self.kind.next_modifier()
    }

    fn is_mandatory(&self) -> bool {
        self.mandatory
    }
}
