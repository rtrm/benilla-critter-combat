//! Overhead "battleable" marker: a crossed pair of `Sword_1H_Short_A_01.m2` over any creature
//! Critter Combat's `pet_battle_wild` table flags (ARCHITECTURE.md Milestone 1 step 2), read off
//! the new `CREATURE_TYPEFLAGS_BATTLEABLE` bit (`0x100`) in `SMSG_CREATURE_QUERY_RESPONSE`'s
//! `type_flags` dword ([`crate::names::type_flags::BATTLEABLE`]) - vmangos's own named bits stop
//! at `0x40`, and `0x80` is reference-documented (if unimplemented here) as `CanInteractWhileDead`,
//! so this fork's own bit starts one above that; see `names.rs` for the full table.
//!
//! Unlike the questgiver markers ([`crate::quest_markers`]), there's no status to swap between and
//! no billboard geometry - the sword is a plain mesh - so this module reuses only the attach/seat
//! /scale half of that module's pattern (see its doc comment), dropping the billboard-card and
//! raise/lower pose systems entirely. The idle bob comes for free from
//! [`benilla_world::doodad_anim::spawn_anim_host`]'s own default loop (anim 0); nothing here needs
//! to arm it.
//!
//! Two copies of the model are spawned per marker. The model's own authored orientation points
//! its blade along whichever way the critter faces (its unrotated local -Z, confirmed by live
//! testing) rather than up, so each copy's rotation does two things at once (`build_markers`):
//! aims the tip from that native forward direction to a lean off vertical (local +Y), split left
//! and right (local ±X) by [`CROSS_LEAN_DEG`] so the pair reads as an X/V above the head regardless
//! of facing, computed directly as "rotate this tip direction to that one"
//! ([`Quat::from_rotation_arc`]) rather than composed axis spins, since the blade's own length,
//! width and thickness axes (measured off the raw M2's vertex bounds, not guessed) don't line up
//! with any single axis you'd want to spin a now-reoriented copy around a second time.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use benilla_assets::m2_url;
use benilla_assets::{M2Model, WorldAssets};

use crate::entities::overhead_slot;
use crate::entities::BoneAttach;
use crate::names::{type_flags, NameCache};
use crate::net::{Guid, ObjectStore};

const MODEL_DIR: &str = "Item\\ObjectComponents\\Weapon";

/// `Item\ObjectComponents\Weapon\Sword_1H_Short_A_01.m2` - verified present in the real 1.12.1
/// MPQ data (`model.MPQ`) before writing this, so a blank marker would mean a seat/attach problem
/// here, not a missing or mis-cased asset path.
const MARKER_MODEL: &str = "Item\\ObjectComponents\\Weapon\\Sword_1H_Short_A_01.m2";

/// The blade's `ItemDisplayInfo.dbc` object-skin texture (M2 type 2, `sub.texture` always `None`
/// for it - see `build_markers`). "Blue" is the most common of the model's real skin variants
/// (Rusty/Green/Blue/Black/Copper across its ~25 displays); any would do since nothing here
/// simulates a specific equipped item.
const MARKER_TEXTURE: &str = "Sword_1H_Short_A_01Blue";

/// Each blade's lean from vertical (local +Y), fanned left/right (local ±X) so the pair forms an
/// X/V above the head instead of lying along whichever way the critter happens to face. A first
/// guess for the angle itself - see the module doc.
const CROSS_LEAN_DEG: f32 = 45.0;

/// How far up the seat (local Y, nothing on X/Z) each blade sits, so the pair hangs above the
/// head rather than through it. A first guess - see the module doc.
const CROSS_LIFT_LOCAL_Y: f32 = 0.35;

/// One live marker's root; unlike [`crate::quest_markers::QuestMarkerRoot`] there is only ever one
/// model, so no `path`/status bookkeeping is needed.
#[derive(Component)]
struct BattleMarkerRoot {
    npc: u64,
    handle: Handle<M2Model>,
    /// The seat under the unit's overhead joint, once built; outside this root's hierarchy.
    seat: Option<Entity>,
    /// The overhead slot [`Self::seat`] was parented at, compared against the live pick each pass
    /// to catch a mount or dismount (same reasoning as the quest markers' own `slot` field).
    slot: Option<u16>,
    /// The body model authors neither overhead attachment: never parented, invisible. Latched.
    no_anchor: bool,
}

/// The attach seat under the unit's overhead joint. [`bake_seat_scale`] bakes the `1/L` into its
/// transform exactly as [`crate::quest_markers`]'s own seat does.
#[derive(Component)]
struct BattleMarkerSeat {
    scaled: bool,
}

/// Despawns the root (its two blade children cascade) and the seat, which lives under the unit's
/// joint; a despawned unit has already taken the seat with it.
fn despawn_marker(commands: &mut Commands, roots: &Query<&BattleMarkerRoot>, root: Entity) {
    if let Some(seat) = roots.get(root).ok().and_then(|m| m.seat) {
        if let Ok(mut e) = commands.get_entity(seat) {
            e.despawn();
        }
    }
    commands.entity(root).despawn();
}

/// Spawns a marker for every streamed NPC [`type_flags::BATTLEABLE`] names, and despawns one the
/// moment its NPC no longer does (out of range, despawned, or - impossible today since nothing
/// unflags a wild critter at runtime, but kept for correctness - unflagged).
fn sync_markers(
    mut commands: Commands,
    names: Res<NameCache>,
    units: Query<(&Guid, &ObjectStore)>,
    asset_server: Res<AssetServer>,
    assets: Option<Res<WorldAssets>>,
    roots: Query<&BattleMarkerRoot>,
    mut live: Local<HashMap<u64, Entity>>,
) {
    if assets.is_none() {
        return; // no client data: nothing could build
    }
    let mut wanted: HashSet<u64> = HashSet::new();
    for (guid, store) in &units {
        let Some(entry) = store.0.object_entry().filter(|&e| e != 0) else {
            continue;
        };
        let Some(record) = names.creature_record(entry) else {
            continue;
        };
        if record.type_flags & type_flags::BATTLEABLE != 0 {
            wanted.insert(guid.0);
        }
    }
    let stale: Vec<u64> = live
        .keys()
        .filter(|npc| !wanted.contains(npc))
        .copied()
        .collect();
    for npc in stale {
        if let Some(root) = live.remove(&npc) {
            despawn_marker(&mut commands, &roots, root);
        }
    }
    for npc in wanted {
        if live.contains_key(&npc) {
            continue;
        }
        info!("battle_markers: crossed swords over {:#x}", npc);
        let root = commands
            .spawn((
                BattleMarkerRoot {
                    npc,
                    handle: asset_server.load::<M2Model>(m2_url(MARKER_MODEL)),
                    seat: None,
                    slot: None,
                    no_anchor: false,
                },
                Transform::IDENTITY,
                Visibility::default(),
            ))
            .id();
        live.insert(npc, root);
    }
}

/// Builds a marker once its M2 and its unit's [`BoneAttach`] have both loaded: the seat under the
/// overhead joint, carrying two copies of the blade crossed into an X. Mirrors
/// [`crate::quest_markers`]'s own `build_markers` for the attach/seat/rig half; there is no
/// billboard half here at all, since the sword model is plain geometry.
#[allow(clippy::too_many_arguments)] // a Bevy system: each param is one resource, the app's convention
fn build_markers(
    mut commands: Commands,
    mut roots: Query<(Entity, &mut BattleMarkerRoot)>,
    asset_server: Res<AssetServer>,
    m2s: Res<Assets<M2Model>>,
    mut forms: ResMut<benilla_world::model_forms::ModelForms>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut mats: benilla_world::model_render::M2BatchMaterials,
    mut palettes: ResMut<benilla_world::rig_palette::RigPalettes>,
    index: Res<crate::net::GuidIndex>,
    anchors: Query<&BoneAttach>,
    mut poses: Query<&mut benilla_world::rig_anim::RigPose>,
    mounts: Query<(), With<crate::entities::mount::MountChild>>,
) {
    for (root, mut marker) in &mut roots {
        if marker.seat.is_some() || marker.no_anchor {
            continue;
        }
        let Some(model) = m2s.get(&marker.handle) else {
            continue; // marker M2 still loading
        };
        forms.ensure_now_rigged(&marker.handle, &model.submeshes, &mut mesh_assets);
        let Some((unit, anchor)) = index
            .0
            .get(&marker.npc)
            .and_then(|&e| Some((e, anchors.get(e).ok()?)))
        else {
            continue; // unit's body model not streamed yet
        };
        let Some((slot, joint, offset)) =
            overhead_slot(anchor, mounts.contains(unit)).and_then(|slot| {
                let &(bone, offset) = anchor.points.get(&slot)?;
                let mut pose = poses.get_mut(unit).ok()?;
                let joint = pose.anchor_for(&mut commands, unit, bone)?;
                Some((slot, joint, offset))
            })
        else {
            debug!(
                "battle_markers: {:#x} has no overhead attachment (18/29) — marker never parents",
                marker.npc
            );
            marker.no_anchor = true;
            continue;
        };

        let seat_tf = Transform::from_translation(offset);
        let host = benilla_world::doodad_anim::spawn_anim_host(&mut commands, model, seat_tf);
        let marker_slot = host.as_ref().map_or(0, |h| {
            benilla_world::rig_palette::RigSkin::allocate_bones(
                &mut palettes,
                h.bones(),
                h.inverse_bindposes.clone(),
            )
            .map_or(0, |rig| {
                let slot = rig.slot;
                commands.entity(h.root).insert(rig);
                slot
            })
        });
        let seat = match &host {
            Some(h) => h.root,
            None => commands.spawn((seat_tf, Visibility::default())).id(),
        };
        commands.entity(seat).insert(BattleMarkerSeat { scaled: false });
        commands.entity(joint).add_child(seat);
        debug!(
            "battle_markers: {:#x} attached under the slot-{slot} joint (animated: {})",
            marker.npc,
            host.is_some()
        );

        let built = forms.slices(&marker.handle);
        let (stat_forms, skin_forms) = (built.stat, built.skin.unwrap_or(&[]));
        let texture = asset_server.load::<Image>(benilla_assets::skin_url(
            MODEL_DIR,
            MARKER_TEXTURE,
        ));
        // The two blades: same mesh, each rotated from its native "points the way the critter
        // faces" orientation (local -Z, confirmed live) to lean off vertical and fan left/right -
        // see the module doc for why this is one direct tip-to-tip rotation per copy rather than a
        // composed axis spin.
        let native_tip = Vec3::NEG_Z;
        let lean = CROSS_LEAN_DEG.to_radians();
        let cross_transforms = [
            Vec3::new(lean.sin(), lean.cos(), 0.0),
            Vec3::new(-lean.sin(), lean.cos(), 0.0),
        ]
        .map(|target_tip| Quat::from_rotation_arc(native_tip, target_tip))
        .map(|rotation| Transform {
            // Straight up the seat's local Y, nothing on X/Z: simpler than pivoting the rotation
            // through the blade's own midpoint, and the seat already sits at the head.
            translation: Vec3::new(0.0, CROSS_LIFT_LOCAL_Y, 0.0),
            rotation,
            scale: Vec3::ONE,
        });
        for transform in cross_transforms {
            for (pi, sub) in model.submeshes.iter().enumerate() {
                // The blade's own batch is an `Object` skin-slot (M2 texture type 2): the raw M2
                // carries no embedded texture for it at all (`sub.texture` is always `None`) since
                // that slot is normally filled per equipped item at spawn - there's no "equipped
                // item" here, so it's loaded directly instead, the common "Blue" skin variant
                // (`ItemDisplayInfo.dbc` entries 1773/5159/10967/5147/... all pair it the same way).
                let Some(material) = mats.steady(sub, Some(texture.clone()), 0) else {
                    continue; // no shared light buffer yet
                };
                let use_rig = marker_slot != 0;
                let mesh = if use_rig {
                    skin_forms.get(pi).cloned().unwrap_or_default()
                } else {
                    stat_forms
                        .get(pi)
                        .map(|(h, _)| h.clone())
                        .unwrap_or_default()
                };
                let child = commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        bevy::mesh::MeshTag(benilla_world::mesh_tag::spawn_tag(marker_slot, 1.0)),
                        transform,
                    ))
                    .id();
                if let Some(h) = &host {
                    if use_rig {
                        commands
                            .entity(child)
                            .insert(benilla_world::rig_palette::RigPart(h.root));
                    }
                }
                commands.entity(seat).add_child(child);
            }
        }
        if let Some(h) = host {
            commands
                .entity(h.root)
                .insert(benilla_world::rig_anim::RigFrame(h.root));
            h.finish(&mut commands);
        }
        marker.seat = Some(seat);
        marker.slot = Some(slot);
        let _ = root; // kept for parity with the quest-marker shape; unused once seat is set
    }
}

/// Bakes each new seat's `1/L` once, identical to [`crate::quest_markers`]'s own
/// `bake_seat_scale` (see its doc comment for why).
fn bake_seat_scale(
    mut seats: Query<(&mut BattleMarkerSeat, &ChildOf, &mut Transform)>,
    joints: Query<&GlobalTransform, Without<BattleMarkerSeat>>,
) {
    for (mut seat, parent, mut tf) in &mut seats {
        if seat.scaled {
            continue;
        }
        let Ok(joint) = joints.get(parent.parent()) else {
            continue;
        };
        let l = joint.affine().matrix3.x_axis.length();
        if l <= 0.0 {
            continue; // not propagated yet: retry next frame
        }
        #[allow(clippy::float_cmp)] // the reference's exact identity guard (fcomp 1.0)
        if l != 1.0 {
            tf.scale = Vec3::splat(1.0 / l);
        }
        seat.scaled = true;
    }
}

pub(crate) struct BattleMarkersPlugin;

impl Plugin for BattleMarkersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_markers, build_markers, bake_seat_scale).chain(),
        );
    }
}
