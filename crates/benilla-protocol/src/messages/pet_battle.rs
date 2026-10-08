//! Critter Combat (see the critter-combat repo's ARCHITECTURE.md): this fork's own protocol, not
//! vmangos/vanilla - the battle abilities are deliberately not real spells (no cast time/GCD/mana
//! /LOS), so there is no vanilla opcode to piggyback the turn exchange on. Field order here must
//! match the paired server fork's `Server/Packets/PetBattle.cpp` exactly.

use std::io::{self, Read};

use crate::wire::{read_cstring, read_u32_le, read_u8};

/// One ability as the client needs it to draw the battle action bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetBattleAbility {
    pub id: u32,
    pub name: String,
    /// 1 DAMAGE, 2 HIT_CHANCE_DEBUFF (enemy), 3 DAMAGE_TAKEN_SHIELD (self).
    pub effect_type: u8,
}

fn read_ability(r: &mut impl Read) -> io::Result<PetBattleAbility> {
    Ok(PetBattleAbility {
        id: read_u32_le(r)?,
        name: read_cstring(r)?,
        effect_type: read_u8(r)?,
    })
}

/// `SMSG_PET_BATTLE_START`: both sides' full combatant info, sent once when a battle begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetBattleStart {
    pub player_pet_name: String,
    pub player_pet_level: u32,
    pub player_pet_max_hp: u32,
    pub player_pet_current_hp: u32,
    pub player_abilities: [PetBattleAbility; 3],

    pub enemy_name: String,
    pub enemy_level: u32,
    pub enemy_max_hp: u32,
    pub enemy_current_hp: u32,
    pub enemy_abilities: [PetBattleAbility; 3],

    pub player_goes_first: bool,
}

/// Read `SMSG_PET_BATTLE_START`: the player's side (name, level, maxHp, currentHp, 3 abilities),
/// then the enemy's side in the same shape, then the first-actor flag.
pub fn read_pet_battle_start(r: &mut impl Read) -> io::Result<PetBattleStart> {
    let player_pet_name = read_cstring(r)?;
    let player_pet_level = read_u32_le(r)?;
    let player_pet_max_hp = read_u32_le(r)?;
    let player_pet_current_hp = read_u32_le(r)?;
    let player_abilities = [read_ability(r)?, read_ability(r)?, read_ability(r)?];

    let enemy_name = read_cstring(r)?;
    let enemy_level = read_u32_le(r)?;
    let enemy_max_hp = read_u32_le(r)?;
    let enemy_current_hp = read_u32_le(r)?;
    let enemy_abilities = [read_ability(r)?, read_ability(r)?, read_ability(r)?];

    let player_goes_first = read_u8(r)? != 0;

    Ok(PetBattleStart {
        player_pet_name,
        player_pet_level,
        player_pet_max_hp,
        player_pet_current_hp,
        player_abilities,
        enemy_name,
        enemy_level,
        enemy_max_hp,
        enemy_current_hp,
        enemy_abilities,
        player_goes_first,
    })
}

/// `SMSG_PET_BATTLE_UPDATE`: one round resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetBattleUpdate {
    /// 0 if the player's pet never got to act this round (already dead).
    pub player_ability_id: u32,
    /// 0 if the enemy never got to act this round (already dead).
    pub enemy_ability_id: u32,
    pub player_acted_first: bool,
    pub player_pet_current_hp: u32,
    pub enemy_current_hp: u32,
}

/// Read `SMSG_PET_BATTLE_UPDATE`.
pub fn read_pet_battle_update(r: &mut impl Read) -> io::Result<PetBattleUpdate> {
    Ok(PetBattleUpdate {
        player_ability_id: read_u32_le(r)?,
        enemy_ability_id: read_u32_le(r)?,
        player_acted_first: read_u8(r)? != 0,
        player_pet_current_hp: read_u32_le(r)?,
        enemy_current_hp: read_u32_le(r)?,
    })
}

/// Read `SMSG_PET_BATTLE_END`'s `playerWon` flag.
pub fn read_pet_battle_end(r: &mut impl Read) -> io::Result<bool> {
    Ok(read_u8(r)? != 0)
}

/// `CMSG_PET_BATTLE_USE_ABILITY` body: the ability id the player picked.
pub fn pet_battle_use_ability(ability_id: u32) -> Vec<u8> {
    ability_id.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ability_bytes(id: u32, name: &str, effect_type: u8) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend_from_slice(name.as_bytes());
        v.push(0);
        v.push(effect_type);
        v
    }

    #[test]
    fn pet_battle_start_is_player_side_then_enemy_side_then_first_actor_flag() {
        let mut body = Vec::new();
        body.extend_from_slice(b"Fluffy\0");
        body.extend_from_slice(&5u32.to_le_bytes()); // level
        body.extend_from_slice(&45u32.to_le_bytes()); // maxHp
        body.extend_from_slice(&45u32.to_le_bytes()); // currentHp
        body.extend(ability_bytes(63000, "Nibble", 1));
        body.extend(ability_bytes(63001, "Dust Cloud", 2));
        body.extend(ability_bytes(63002, "Burrow", 3));
        body.extend_from_slice(b"Prairie Dog\0");
        body.extend_from_slice(&3u32.to_le_bytes()); // level
        body.extend_from_slice(&35u32.to_le_bytes()); // maxHp
        body.extend_from_slice(&35u32.to_le_bytes()); // currentHp
        body.extend(ability_bytes(63000, "Nibble", 1));
        body.extend(ability_bytes(63001, "Dust Cloud", 2));
        body.extend(ability_bytes(63002, "Burrow", 3));
        body.push(1); // playerGoesFirst

        let parsed = read_pet_battle_start(&mut &body[..]).unwrap();
        assert_eq!(parsed.player_pet_name, "Fluffy");
        assert_eq!(parsed.player_pet_level, 5);
        assert_eq!(parsed.player_abilities[0].name, "Nibble");
        assert_eq!(parsed.player_abilities[1].effect_type, 2);
        assert_eq!(parsed.enemy_name, "Prairie Dog");
        assert_eq!(parsed.enemy_abilities[2].id, 63002);
        assert!(parsed.player_goes_first);
    }

    #[test]
    fn pet_battle_update_is_five_fields_in_order() {
        let mut body = Vec::new();
        body.extend_from_slice(&63000u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(1);
        body.extend_from_slice(&40u32.to_le_bytes());
        body.extend_from_slice(&30u32.to_le_bytes());

        assert_eq!(
            read_pet_battle_update(&mut &body[..]).unwrap(),
            PetBattleUpdate {
                player_ability_id: 63000,
                enemy_ability_id: 0,
                player_acted_first: true,
                player_pet_current_hp: 40,
                enemy_current_hp: 30,
            }
        );
    }

    #[test]
    fn pet_battle_end_is_one_bool_byte() {
        assert!(!read_pet_battle_end(&mut &[0u8][..]).unwrap());
        assert!(read_pet_battle_end(&mut &[1u8][..]).unwrap());
    }

    #[test]
    fn use_ability_carries_the_id() {
        assert_eq!(pet_battle_use_ability(63001), 63001u32.to_le_bytes());
    }
}
