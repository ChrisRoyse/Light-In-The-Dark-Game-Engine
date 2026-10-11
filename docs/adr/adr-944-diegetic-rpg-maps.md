# ADR 944: RPG Maps Are Diegetic Only

Status: accepted

Issue: #944

## Decision

The produced RPG game has no built-in always-available map, minimap, or free
omniscient world-map HUD.

Map access is diegetic. A player can gain map information by buying a map from
a cartographer, finding or looting a map, earning one through a quest, copying
one from an archive, or learning geography from exploration and NPC knowledge.

Editor authoring views, editor minimaps, terrain previews, loading-screen
previews, and creator tooling are separate. They may exist because they help
creators build games. This ADR governs the player-runtime RPG experience.

## Requirements

- A fresh RPG play session starts with no map/minimap/world-map HUD.
- A map UI may open only from possessed/learned map sources.
- Owning or learning a map source does not create an always-on HUD minimap; the
  player must open a specific acquired source.
- Map scope is limited to the authored source: region, route, dungeon, faction
  territory, or discovered knowledge.
- Maps can be incomplete, stale, false, quality-tiered, faction-gated,
  reputation-gated, or price-gated.
- Cartographers are NPC/gameplay content, not a menu shortcut.
- Compass, waypoints, quest markers, and travel aids must be diegetic or
  item/ability based and must not recreate a free built-in GPS layer.
- Fog/visibility/minimap rendering primitives may exist for editor tooling or
  acquired map-item rendering, but not as a default player HUD.

## Verification

Before closing #944, manually Full-State-Verify through Synapse:

- Fresh RPG runtime has no free map/minimap/world-map access.
- Cartographer purchase grants only the purchased map scope and persists it as
  inventory or knowledge state.
- Found/looted map grants only its authored scope.
- Removing or losing the map source removes or degrades map access.
- Editor minimap/authoring preview still works where intended and is not
  confused with player-runtime map access.
