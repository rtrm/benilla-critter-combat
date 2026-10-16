//! Critter Combat's battle panel (see the critter-combat repo's ARCHITECTURE.md): tracks the
//! active pet battle's state from `SMSG_PET_BATTLE_START`/`_UPDATE`/`_END` and feeds it to the new
//! `PetBattleFrame.xml` panel, same three-stage shape as the pet bar
//! ([`crate::ui_pet::bar::feed_pet_bar`]): a net handler writes this resource, a per-frame system
//! pushes it into the Lua-exposed model and fires an event, and the (unmodified-by-us, our-own)
//! Lua script does the actual show/hide and button text.

use bevy::prelude::*;

use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{PetBattleAbilityView, PetBattleState, UiScript};

use crate::net::NetHandlerApp;

/// One ability as resolved off the wire: id (what the wire sends back on use), name, effect type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct AbilitySlot {
    id: u32,
    name: String,
    icon: String,
    effect_type: u8,
}

/// The battle's live state; `active` false means no battle, every other field stale.
#[derive(Resource, Default)]
struct PetBattle {
    active: bool,
    player_name: String,
    player_level: u32,
    player_hp: u32,
    player_max_hp: u32,
    player_abilities: [AbilitySlot; 3],
    enemy_name: String,
    enemy_level: u32,
    enemy_hp: u32,
    enemy_max_hp: u32,
    /// Raw guids cached from `PetBattleStart`, resolved through `GuidIndex` on each `PetBattleUpdate`
    /// for the lunge-on-attack animation - never anything server-authoritative.
    player_pet_guid: u64,
    enemy_guid: u64,
}

fn on_start(In(ev): In<SessionEvent>, mut battle: ResMut<PetBattle>) {
    let SessionEvent::PetBattleStart {
        player_pet_name,
        player_pet_level,
        player_pet_max_hp,
        player_pet_current_hp,
        player_abilities,
        enemy_name,
        enemy_level,
        enemy_max_hp,
        enemy_current_hp,
        enemy_abilities: _, // the enemy's kit is resolved server-side; the player never sees it
        player_goes_first: _, // turn order is server-authoritative; nothing here depends on it
        player_pet_guid,
        enemy_guid,
    } = ev
    else {
        return;
    };
    battle.active = true;
    battle.player_name = player_pet_name;
    battle.player_level = player_pet_level;
    battle.player_hp = player_pet_current_hp;
    battle.player_max_hp = player_pet_max_hp;
    for (slot, ability) in battle.player_abilities.iter_mut().zip(player_abilities) {
        *slot = AbilitySlot {
            id: ability.id,
            name: ability.name,
            icon: ability.icon,
            effect_type: ability.effect_type,
        };
    }
    battle.enemy_name = enemy_name;
    battle.enemy_level = enemy_level;
    battle.enemy_hp = enemy_current_hp;
    battle.enemy_max_hp = enemy_max_hp;
    battle.player_pet_guid = player_pet_guid;
    battle.enemy_guid = enemy_guid;
}

fn on_update(
    In(ev): In<SessionEvent>,
    mut battle: ResMut<PetBattle>,
    mut commands: Commands,
    guid_index: Res<crate::net::GuidIndex>,
    transforms: Query<&GlobalTransform>,
    time: Res<Time>,
) {
    let SessionEvent::PetBattleUpdate {
        player_ability_id,
        enemy_ability_id,
        player_pet_current_hp,
        enemy_current_hp,
        ..
    } = ev
    else {
        return;
    };
    battle.player_hp = player_pet_current_hp;
    battle.enemy_hp = enemy_current_hp;

    // Cosmetic lunge-on-attack (ARCHITECTURE.md): whichever side actually got to act this round
    // (a 0 ability id means it never did - the round already ended before its turn) briefly
    // lunges toward its live opponent. GuidIndex resolves the guids cached from BattleStart to
    // the entities the normal unit-streaming pipeline is already rendering.
    let player_entity = guid_index.0.get(&battle.player_pet_guid).copied();
    let enemy_entity = guid_index.0.get(&battle.enemy_guid).copied();
    let now = time.elapsed_secs();

    if player_ability_id != 0 {
        if let (Some(attacker), Some(defender)) = (player_entity, enemy_entity) {
            if let Ok(target) = transforms.get(defender) {
                crate::battle_lunge::lunge_towards(&mut commands, attacker, target.translation(), now);
            }
        }
    }
    if enemy_ability_id != 0 {
        if let (Some(attacker), Some(defender)) = (enemy_entity, player_entity) {
            if let Ok(target) = transforms.get(defender) {
                crate::battle_lunge::lunge_towards(&mut commands, attacker, target.translation(), now);
            }
        }
    }
}

fn on_end(In(ev): In<SessionEvent>, mut battle: ResMut<PetBattle>) {
    if matches!(ev, SessionEvent::PetBattleEnd { .. }) {
        battle.active = false;
    }
}

/// A lost socket mid-battle leaves no server to resolve further rounds; close the panel the same
/// way the pet bar clears on session end.
fn on_session_end(In(_): In<SessionEvent>, mut battle: ResMut<PetBattle>) {
    battle.active = false;
}

/// Push the battle state into the script model every frame it changed, and fire
/// `CRITTER_BATTLE_UPDATE` for `PetBattleFrame.xml`'s own `OnEvent` to show/hide and repaint from.
fn feed_pet_battle(script: Option<NonSendMut<UiScript>>, battle: Res<PetBattle>) {
    let Some(mut script) = script else {
        return;
    };
    if !battle.is_changed() {
        return;
    }
    script.set_pet_battle(PetBattleState {
        active: battle.active,
        player_name: battle.player_name.clone(),
        player_level: battle.player_level,
        player_hp: battle.player_hp,
        player_max_hp: battle.player_max_hp,
        player_abilities: std::array::from_fn(|i| {
            let slot = &battle.player_abilities[i];
            PetBattleAbilityView {
                id: slot.id,
                name: slot.name.clone(),
                icon: slot.icon.clone(),
                effect_type: slot.effect_type,
            }
        }),
        enemy_name: battle.enemy_name.clone(),
        enemy_level: battle.enemy_level,
        enemy_hp: battle.enemy_hp,
        enemy_max_hp: battle.enemy_max_hp,
    });
    script.fire_event("CRITTER_BATTLE_UPDATE", Vec::new());
}

pub(crate) struct PetBattlePlugin;

impl Plugin for PetBattlePlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<PetBattle>()
            .net_handler(K::PetBattleStart, on_start)
            .net_handler(K::PetBattleUpdate, on_update)
            .net_handler(K::PetBattleEnd, on_end)
            .net_handler(K::Disconnected, on_session_end)
            .add_systems(Update, feed_pet_battle);
    }
}
