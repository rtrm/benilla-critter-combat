//! Critter Combat's battle panel globals (see the critter-combat repo's ARCHITECTURE.md): a
//! wholly new, non-stock Lua/XML frame (`assets/ui/PetBattleFrame.xml`), not a stock-WoW
//! reproduction, so unlike most of this crate these bindings have no reference address to cite.
//! Mirrors [`super::pet`]'s per-slot getter shape (`GetPetActionInfo(slot)`) rather than one giant
//! flat tuple, since that is this codebase's own established idiom for "N small widgets fed from
//! one Rust-side array."

use mlua::Lua;

use super::Model;

/// One ability as the panel needs it to label a button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PetBattleAbilityView {
    /// The real ability id `CritterBattleUseAbility` sends on the wire - not the 1-based slot.
    pub id: u32,
    pub name: String,
    /// 1 DAMAGE, 2 HIT_CHANCE_DEBUFF (enemy), 3 DAMAGE_TAKEN_SHIELD (self); 0 for an empty slot.
    pub effect_type: u8,
}

/// The pushed battle state: both sides' name/level/HP and the active pet's three abilities (the
/// enemy's own three are resolved server-side but never shown - the player only ever picks their
/// own pet's move).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PetBattleState {
    pub active: bool,
    pub player_name: String,
    pub player_level: u32,
    pub player_hp: u32,
    pub player_max_hp: u32,
    pub player_abilities: [PetBattleAbilityView; 3],
    pub enemy_name: String,
    pub enemy_level: u32,
    pub enemy_hp: u32,
    pub enemy_max_hp: u32,
}

impl super::UiScript {
    /// Push the whole battle state; the app fires `CRITTER_BATTLE_UPDATE` after, same shape as
    /// `set_pet_actions` + `PET_BAR_UPDATE`.
    pub fn set_pet_battle(&mut self, state: PetBattleState) {
        self.model_mut().pet_battle = state;
    }
}

/// Register the battle panel's globals.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "CritterBattleIsActive",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.pet_battle.active)
        })?,
    )?;

    g.set(
        "CritterBattleGetCombatants",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let b = &model.pet_battle;
            Ok((
                b.player_name.clone(),
                b.player_level,
                b.player_hp,
                b.player_max_hp,
                b.enemy_name.clone(),
                b.enemy_level,
                b.enemy_hp,
                b.enemy_max_hp,
            ))
        })?,
    )?;

    // `CritterBattleGetAbility(slot)`, a 1-based ability slot: name, then effect type (0 empty).
    g.set(
        "CritterBattleGetAbility",
        lua.create_function(|lua, i: u32| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let ability = usize::try_from(i.wrapping_sub(1))
                .ok()
                .and_then(|n| model.pet_battle.player_abilities.get(n));
            Ok(match ability {
                Some(a) => (a.name.clone(), a.effect_type),
                None => (String::new(), 0),
            })
        })?,
    )?;

    // `CritterBattleUseAbility(slot)`, a 1-based ability slot: resolves it to the real ability id
    // (the wire needs the id, not the slot) and queues the use, same shape as `CastPetAction`.
    g.set(
        "CritterBattleUseAbility",
        lua.create_function(|lua, i: u32| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let ability_id = usize::try_from(i.wrapping_sub(1))
                .ok()
                .and_then(|n| model.pet_battle.player_abilities.get(n))
                .map(|a| a.id);
            if let Some(ability_id) = ability_id {
                model
                    .script_calls
                    .push(super::ScriptCall::CritterBattleUseAbility(ability_id));
            }
            Ok(())
        })?,
    )?;

    Ok(())
}
