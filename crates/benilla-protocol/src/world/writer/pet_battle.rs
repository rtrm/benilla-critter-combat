//! Critter Combat's one send (see the critter-combat repo's ARCHITECTURE.md): there is no
//! start-battle opcode, since Engage Critter Combat is a real spell cast (`CMSG_CAST_SPELL`)
//! against a targeted battleable critter, resolved entirely server-side.

use anyhow::Result;

use crate::messages::{self, opcode};

use super::WorldWriter;

impl WorldWriter {
    /// Use one of the active pet's three battle abilities (`CMSG_PET_BATTLE_USE_ABILITY`).
    pub fn pet_battle_use_ability(&mut self, ability_id: u32) -> Result<()> {
        self.send(
            opcode::CMSG_PET_BATTLE_USE_ABILITY,
            &messages::pet_battle_use_ability(ability_id),
        )
    }
}
