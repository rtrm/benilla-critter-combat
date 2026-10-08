//! Display entries for Critter Combat's two player-facing spells (64000 Engage Critter Combat,
//! 64001 Capture - see the critter-combat repo's ARCHITECTURE.md), a reserved custom id range
//! `Spell.dbc` was never going to carry since they only exist on our own server fork. Without an
//! entry here they're unreachable outside a GM `.cast` command: the server learns them into
//! `character_spell` just fine, but [`benilla_formats::SpellCatalog::get`] returns `None` for them
//! and the spellbook's add-gate (`ui_spellbook.rs`'s `build_book`) silently drops anything with no
//! catalog row, exactly the problem [`super::synthetic_spells`] solves for the companion range.
//!
//! Unlike the companions (self-cast, no target), both of these are real `TARGET_UNIT_ENEMY` spells
//! - the player targets the wild critter and casts normally, same click-to-cast path
//! `synthetic_spells`'s own doc comment already traced through `SpellButton_OnClick`.

use benilla_formats::{SpellCatalog, SpellDisplay};

/// `TARGET_UNIT_ENEMY`, matching the real server-side `effectImplicitTargetA1` both spells carry
/// (`server/sql/migrations/20261007155319_world.sql`) - `cast_target.rs`'s `cast_target_mask`
/// ORs in the "needs an enemy unit" bit for it, so casting prompts for (or uses) a hostile target
/// exactly as it should, with `targets` left at its own real default of 0.
const TARGET_UNIT_ENEMY: u32 = 6;

/// `SPELL_EFFECT_DUMMY`, matching the real server-side `effect1` both spells carry - what they
/// actually do server-side is a later increment (the battle engine itself, already built, reads
/// `pet_battle_ability` directly rather than branching on this), so this is just an accurate
/// mirror of the real row, not a functional client-side effect.
const EFFECT_DUMMY: u32 = 3;

pub(crate) fn install(catalog: &mut SpellCatalog) {
    catalog.insert(
        64000,
        SpellDisplay {
            id: 64000,
            name: "Engage Critter Combat".to_string(),
            icon: Some("Interface\\Icons\\Spell_Magic_PolymorphChicken".to_string()),
            effects: [EFFECT_DUMMY, 0, 0],
            implicit_target_a1: TARGET_UNIT_ENEMY,
            ..Default::default()
        },
    );
    catalog.insert(
        64001,
        SpellDisplay {
            id: 64001,
            name: "Capture Critter".to_string(),
            icon: Some("Interface\\Icons\\Ability_Ensnare".to_string()),
            effects: [EFFECT_DUMMY, 0, 0],
            implicit_target_a1: TARGET_UNIT_ENEMY,
            ..Default::default()
        },
    );
}
