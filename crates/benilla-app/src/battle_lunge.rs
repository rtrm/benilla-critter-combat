//! Critter Combat (see the critter-combat repo's ARCHITECTURE.md): a brief, purely cosmetic
//! forward-and-back lunge when a combatant uses a battle ability. Local-only - it never touches
//! server-authoritative state, so it can't desync anything and needs no round trip. Same
//! technique as [`crate::camera_shake`]'s camera offset: it layers an offset on top of whatever
//! [`benilla_world::schedule::WorldStage::Net`] already wrote to `Transform` this frame, re-applied
//! fresh every frame rather than accumulated, so there is nothing to reconcile once it ends.

use std::f32::consts::PI;

use bevy::prelude::*;

use benilla_world::schedule::WorldStage;

/// How far (world units) a lunge travels forward before returning.
const LUNGE_DISTANCE: f32 = 0.6;
/// Total lunge lifetime: out and back.
const LUNGE_SECONDS: f32 = 0.35;

/// Added to a combatant's own entity when it acts; removed once the lunge finishes. `target` is
/// the opponent's position, snapshotted at the moment the lunge starts - fixed for its lifetime,
/// like [`crate::camera_shake::LiveShake`]'s own snapshotted `pos`, since both combatants stand
/// still for a battle's duration anyway.
#[derive(Component)]
pub(crate) struct LungeTowards {
    target: Vec3,
    start: f32,
}

/// Start (or restart) a lunge on `entity` toward `target`, snapshotted at `now`
/// (`Time::elapsed_secs`).
pub(crate) fn lunge_towards(commands: &mut Commands, entity: Entity, target: Vec3, now: f32) {
    commands.entity(entity).insert(LungeTowards { target, start: now });
}

pub(crate) struct LungePlugin;

impl Plugin for LungePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply_lunges.in_set(WorldStage::Present));
    }
}

/// Each active lunge: how far into its out-and-back arc (0 → 1 → 0 via a half sine) decides this
/// frame's offset, added onto whatever `Transform::translation` already holds - the real,
/// server-driven position for this frame, since `WorldStage::Net` already ran. Direction is
/// recomputed from the live (but, mid-battle, stationary) position each frame rather than
/// snapshotted, so a lunge still points the right way even if something nudges the attacker
/// between frames.
fn apply_lunges(
    mut commands: Commands,
    time: Res<Time>,
    mut units: Query<(Entity, &LungeTowards, &mut Transform)>,
) {
    let now = time.elapsed_secs();
    for (entity, lunge, mut transform) in &mut units {
        let t = (now - lunge.start) / LUNGE_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).remove::<LungeTowards>();
            continue;
        }
        let phase = (t * PI).sin();
        let dir = (lunge.target - transform.translation).normalize_or_zero();
        transform.translation += dir * LUNGE_DISTANCE * phase;
    }
}
