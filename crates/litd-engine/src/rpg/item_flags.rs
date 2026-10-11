//! The Carrion Fields item-flag vocabulary with typed zap/gate semantics
//! (#1685).
//!
//! Facts are transcribed, never invented (#984/#959): the flag set is the CF
//! item-database "Flags" column (`carrionfields.net/itemsearch`, captured
//! 2026-07-12 -- e.g. `glowing humming dark magic noremove female_only`,
//! `dark evil anti_good anti_neutral`, `evil anti_good human_only`, `giant_only`)
//! plus the ROM-lineage extra-flag set the issue enumerates (anti_evil, nodrop,
//! nolocate, no_uncurse, norecall, bless, invis, burnproof, rot_death,
//! melt_drop). Each flag carries a TYPED rule ([`FlagSemantic`]) -- the
//! alignment zap, the bio gate, the stealth-signature emission, the curse, the
//! durability rule -- so downstream systems read data, not prose. Validation
//! fails LOUD (structured `LITD-RPG-STATS-INVALID-CONFIG`) on any unknown flag,
//! any duplicate, or a contradictory combination (male + female gate, or all
//! three alignment zaps -- an item nobody could ever use).

use std::collections::BTreeSet;

use crate::EngineError;

/// A stealth-signature emission channel (#1597/#1602 crossover): glowing gear
/// leaks light, a humming sword gives you away by ear, dark gear drinks light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmissionChannel {
    Light,
    Sound,
    Darkness,
}

/// A magical aura a detection spell can surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraKind {
    Magical,
    Evil,
    /// Radiates a good/holy aura (the `good` flag, parallel to `evil`; surfaced
    /// by detect-good, seen on holy/blessed gear across the itemsearch corpus).
    Good,
    Blessed,
    Invisible,
}

/// The wielder alignment an anti-alignment flag ZAPS (burn + forced drop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZapAlignment {
    Good,
    Neutral,
    Evil,
}

/// A curse / binding rule attached to an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurseEffect {
    /// Cannot be removed once worn until a remove-curse strips it (noremove).
    BindOnEquip,
    /// Cannot be dropped (nodrop).
    BindToInventory,
    /// Remove-curse will not work on it (no_uncurse).
    ResistUncurse,
    /// Blocks recall while carried (norecall).
    BlockRecall,
    /// Hidden from locate object (nolocate).
    HideFromLocate,
}

/// A durability / destruction rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityRule {
    /// Immune to fire destruction (burnproof).
    FireImmune,
    /// Crumbles when its carrier dies (rot_death).
    CrumbleOnDeath,
    /// Melts when dropped (melt_drop).
    MeltOnDrop,
}

/// A biological requirement gate: only a matching wielder may equip it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioRequirement {
    Female,
    Male,
    HumanKind,
    GiantKind,
    // Race-kind gates: only wielders of the named race lineage may equip. These
    // are the itemsearch `<race>_only` flags, exactly parallel to `human_only`
    // and `giant_only`, surfaced by the #1688 itemsearch corpus ingestion.
    DwarfKind,
    ElfKind,
    GnomeKind,
    OrcKind,
    FelarKind,
    /// Size gate: only large-frame wielders may equip (`size_large_only`).
    SizeLarge,
}

/// The typed rule an item flag encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSemantic {
    /// Leaks a stealth-signature channel while carried/worn.
    SignatureEmission(EmissionChannel),
    /// Radiates a detectable aura.
    Aura(AuraKind),
    /// Zaps a wielder of the named alignment (burn + forced drop).
    AlignmentZap(ZapAlignment),
    /// A curse / binding rule.
    Curse(CurseEffect),
    /// A durability / destruction rule.
    Durability(DurabilityRule),
    /// A bio wielder gate.
    BioGate(BioRequirement),
}

macro_rules! item_flags {
    ($($variant:ident => $id:literal, $sem:expr);+ $(;)?) => {
        /// The CF item-flag vocabulary. A closed set: an item's flags MUST each
        /// resolve to one of these (validation fails loud otherwise).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum ItemFlag {
            $($variant),+
        }

        impl ItemFlag {
            /// Every flag in the vocabulary, in declaration order.
            pub const ALL: &'static [ItemFlag] = &[$(ItemFlag::$variant),+];

            /// The canonical snake_case id stored in `ItemSpec.flags`.
            pub const fn as_id(self) -> &'static str {
                match self { $(ItemFlag::$variant => $id),+ }
            }

            /// Resolve a stored flag id, or `None` if it is not in the
            /// vocabulary (the loud-fail path for `validate_item_flags`).
            pub fn from_id(id: &str) -> Option<Self> {
                match id { $($id => Some(ItemFlag::$variant),)+ _ => None }
            }

            /// The typed rule this flag encodes.
            pub const fn semantic(self) -> FlagSemantic {
                match self { $(ItemFlag::$variant => $sem),+ }
            }
        }
    };
}

item_flags! {
    Glowing => "glowing", FlagSemantic::SignatureEmission(EmissionChannel::Light);
    Humming => "humming", FlagSemantic::SignatureEmission(EmissionChannel::Sound);
    Dark => "dark", FlagSemantic::SignatureEmission(EmissionChannel::Darkness);
    Magic => "magic", FlagSemantic::Aura(AuraKind::Magical);
    Evil => "evil", FlagSemantic::Aura(AuraKind::Evil);
    Good => "good", FlagSemantic::Aura(AuraKind::Good);
    Bless => "bless", FlagSemantic::Aura(AuraKind::Blessed);
    Invis => "invis", FlagSemantic::Aura(AuraKind::Invisible);
    AntiGood => "anti_good", FlagSemantic::AlignmentZap(ZapAlignment::Good);
    AntiNeutral => "anti_neutral", FlagSemantic::AlignmentZap(ZapAlignment::Neutral);
    AntiEvil => "anti_evil", FlagSemantic::AlignmentZap(ZapAlignment::Evil);
    NoRemove => "noremove", FlagSemantic::Curse(CurseEffect::BindOnEquip);
    NoDrop => "nodrop", FlagSemantic::Curse(CurseEffect::BindToInventory);
    NoUncurse => "no_uncurse", FlagSemantic::Curse(CurseEffect::ResistUncurse);
    NoRecall => "norecall", FlagSemantic::Curse(CurseEffect::BlockRecall);
    NoLocate => "nolocate", FlagSemantic::Curse(CurseEffect::HideFromLocate);
    BurnProof => "burnproof", FlagSemantic::Durability(DurabilityRule::FireImmune);
    RotDeath => "rot_death", FlagSemantic::Durability(DurabilityRule::CrumbleOnDeath);
    MeltDrop => "melt_drop", FlagSemantic::Durability(DurabilityRule::MeltOnDrop);
    FemaleOnly => "female_only", FlagSemantic::BioGate(BioRequirement::Female);
    MaleOnly => "male_only", FlagSemantic::BioGate(BioRequirement::Male);
    HumanOnly => "human_only", FlagSemantic::BioGate(BioRequirement::HumanKind);
    GiantOnly => "giant_only", FlagSemantic::BioGate(BioRequirement::GiantKind);
    DwarfOnly => "dwarf_only", FlagSemantic::BioGate(BioRequirement::DwarfKind);
    ElfOnly => "elf_only", FlagSemantic::BioGate(BioRequirement::ElfKind);
    GnomeOnly => "gnome_only", FlagSemantic::BioGate(BioRequirement::GnomeKind);
    OrcOnly => "orc_only", FlagSemantic::BioGate(BioRequirement::OrcKind);
    FelarOnly => "felar_only", FlagSemantic::BioGate(BioRequirement::FelarKind);
    SizeLargeOnly => "size_large_only", FlagSemantic::BioGate(BioRequirement::SizeLarge);
}

/// The stealth-signature footprint an equipped item set projects (#1597/#1602):
/// the equipment-driven stealth meta reads this, not the flag strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StealthSignature {
    pub emits_light: bool,
    pub emits_sound: bool,
    pub emits_darkness: bool,
}

impl StealthSignature {
    /// Any leak at all makes the wearer easier to detect.
    pub const fn is_noisy(self) -> bool {
        self.emits_light || self.emits_sound || self.emits_darkness
    }
}

/// Fold a resolved flag set into its stealth signature.
pub fn stealth_signature(flags: &[ItemFlag]) -> StealthSignature {
    let mut sig = StealthSignature::default();
    for flag in flags {
        if let FlagSemantic::SignatureEmission(channel) = flag.semantic() {
            match channel {
                EmissionChannel::Light => sig.emits_light = true,
                EmissionChannel::Sound => sig.emits_sound = true,
                EmissionChannel::Darkness => sig.emits_darkness = true,
            }
        }
    }
    sig
}

/// Resolve and validate an item's authored flag strings. Fails LOUD on an
/// unknown flag, a duplicate, or a contradictory combination. Returns the
/// resolved [`ItemFlag`] set on success.
pub fn validate_item_flags(field: &str, flags: &[String]) -> Result<Vec<ItemFlag>, EngineError> {
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::with_capacity(flags.len());
    for flag in flags {
        let Some(parsed) = ItemFlag::from_id(flag) else {
            return Err(invalid(
                field,
                format!("unknown item flag {flag} (not in the CF item-flag vocabulary)"),
            ));
        };
        if !seen.insert(parsed) {
            return Err(invalid(field, format!("duplicate flag {flag}")));
        }
        resolved.push(parsed);
    }
    validate_flag_combination(field, &resolved)?;
    Ok(resolved)
}

/// Cross-flag consistency: reject combinations that leave the item unusable by
/// design error (transcription slips), never valid CF data.
fn validate_flag_combination(field: &str, flags: &[ItemFlag]) -> Result<(), EngineError> {
    let has = |target: ItemFlag| flags.contains(&target);
    if has(ItemFlag::MaleOnly) && has(ItemFlag::FemaleOnly) {
        return Err(invalid(
            field,
            "male_only and female_only together gate out every wielder".to_string(),
        ));
    }
    if has(ItemFlag::AntiGood) && has(ItemFlag::AntiNeutral) && has(ItemFlag::AntiEvil) {
        return Err(invalid(
            field,
            "anti_good + anti_neutral + anti_evil zaps every alignment (unusable)".to_string(),
        ));
    }
    Ok(())
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
