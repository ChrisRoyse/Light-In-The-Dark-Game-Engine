<!-- GENERATED FROM litd.rpg.overworld_topology.v1 by
     render_overworld_master_map_markdown (#1632). DO NOT EDIT BY HAND;
     edit the authored OverworldTopologySpec and regenerate. -->

# Thera - Master Map

The continent of Thera organized as a region graph: the Galadon hub at the center, cardinal road spokes to the north/east/south/west reaches, the northwest keep country, the Underdark below, and the Past and island seas beyond.

- Source: https://wiki.carrionfields.net/game/world
- Canon issues: 987, 1598, 1632, 1704
- Regions: 10
- Spine edges: 12

## Regions by sector

### Center

- **Central Crossroads** (`region.thera.central_crossroads`), levels 1-30: The Galadon hub and its immediate satellites: the road-heart of Thera.
  - `area.thera.galadon`
  - `area.thera.galadon.lumber_yard`
  - `area.thera.galadon.kobold_warrens`
  - `area.thera.galadon.graveyard`
  - `area.thera.galadon.goblin_village`
  - `area.thera.galadon.copeham_inn`
  - `area.thera.inn_of_the_eternal_star`
  - `area.thera.aldevari`
  - `area.thera.open_plains`

### North

- **Northern Reach** (`region.thera.northern_reach`), levels 10-40: Udgaard and the cold northern plains, foothills, and mountains.
  - `area.thera.udgaard`
  - `area.thera.udgaardian_plains`
  - `area.thera.northern_mountains`
  - `area.thera.northern_foothills`
  - `area.thera.hillcrest`
  - `area.thera.blackclaw`
  - `area.thera.dagdan`
  - `area.thera.frigid_wasteland`

### East

- **Eastern Arkham Country** (`region.thera.eastern_arkham`), levels 15-51: Arkham and the eastern coast, reached down the Eastern Road.
  - `area.thera.arkham`
  - `area.thera.forgotten_crypts`
  - `area.thera.mansion_of_vdramir`
  - `area.thera.tower_of_dragons`

### South

- **Southern Desert** (`region.thera.southern_desert`), levels 5-51: Hamsah Mu'tazz and the twin deserts, steppes, and southern mountains.
  - `area.thera.hamsah_mu_tazz`
  - `area.thera.sands_of_sorrow`
  - `area.thera.desert_of_araile`
  - `area.thera.pyramid_of_azhan`
  - `area.thera.oryx_steppes`
  - `area.thera.tahril_mountains`
  - `area.thera.wastes_of_nonviel`
  - `area.thera.xvart_lair`

### West / Southwest

- **Western Seantryn** (`region.thera.western_seantryn`), levels 10-45: Seantryn Modan and its wetlands and marsh belt west of the hub.
  - `area.thera.seantryn_modan`
  - `area.thera.seantryn_wetlands`
  - `area.thera.marsh_belt`
- **Darsylon Wilds** (`region.thera.darsylon_wilds`), levels 15-51: The elven city of Darsylon and the deep western forests.
  - `area.thera.darsylon`
  - `area.thera.azreth_forest`
  - `area.thera.dranettie_wood`
  - `area.thera.feanwyyn_weald`
  - `area.thera.emerald_forest`

### Northwest interior

- **Northwest Keep Country** (`region.thera.northwest_keeps`), levels 20-51: Tir-Talath, Cragstone, and the ruined keeps and towers of the interior.
  - `area.thera.tir_talath`
  - `area.thera.cragstone`
  - `area.thera.keep_of_fhalaugash`
  - `area.thera.ruined_keep`
  - `area.thera.lord_tabershaw_keep`
  - `area.thera.ruins_of_maethien`
  - `area.thera.octagonal_tower`
  - `area.thera.teth_azeleth`

### Below (the Underdark)

- **The Underdark** (`region.thera.the_underdark`), levels 20-51: The deep tunnels, drow and duergar holds, and troglodyte caverns below Thera.
  - `area.thera.underdark.tunnels`
  - `area.thera.underdark.mines`
  - `area.thera.underdark.troglodyte_caverns`
  - `area.thera.underdark.svirfneblin_enclave`
  - `area.thera.underdark.arin_gird`
  - `area.thera.underdark.gol_galath`
  - `area.thera.underdark.velkyn_oloth`
  - `area.thera.kuo_toa_lair`
  - `area.thera.mines_of_zakiim`

### Beyond (planes, portals, island chains)

- **The Past** (`region.thera.the_past`), levels 1-51: The time-portal reach: Thera as it was, reached through the Vale.
  - `area.thera.past.citadel_of_ostalagiah`
  - `area.thera.past.delar_tol`
  - `area.thera.past.maethien`
  - `area.thera.past.past_grove`
  - `area.thera.past.plains_of_arendyl`
  - `area.thera.past.the_battlefield`
  - `area.thera.past.wagon_marked_road`
- **Island Seas** (`region.thera.island_seas`), levels 20-51: The ocean chains and island holds beyond Thera's coasts.
  - `area.thera.isle_of_corte`
  - `area.thera.crystal_island`
  - `area.thera.coral_head`
  - `area.thera.coral_palace`
  - `area.thera.sea_of_despair`
  - `area.thera.waters_of_the_deep`
  - `area.thera.dragon_sea`
  - `area.thera.aryth_ocean`

## Region spine (adjacency)

| From | To | Spine | Notes |
|------|----|-------|-------|
| region.thera.central_crossroads | region.thera.northern_reach | road | The North Road out of Galadon's north gate to Udgaard. |
| region.thera.central_crossroads | region.thera.eastern_arkham | road | The Eastern Road past Aldevari to Arkham. |
| region.thera.central_crossroads | region.thera.southern_desert | road | The southern road out of Galadon's south gate to Hamsah Mu'tazz. |
| region.thera.central_crossroads | region.thera.western_seantryn | road | The western road to Seantryn Modan. |
| region.thera.central_crossroads | region.thera.northwest_keeps | road | The northwest road into the keep country. |
| region.thera.northern_reach | region.thera.northwest_keeps | road | The upland road linking the Northern Reach to the northwest interior. |
| region.thera.western_seantryn | region.thera.darsylon_wilds | trail | Forest trails west from Seantryn into the Darsylon wilds. |
| region.thera.central_crossroads | region.thera.the_underdark | underdark | The Galadon sewer descent into the Underdark tunnels. |
| region.thera.northwest_keeps | region.thera.the_underdark | underdark | Keep-country delvings that break into the deep tunnels. |
| region.thera.central_crossroads | region.thera.the_past | portal | The time portal at the Vale of Arendyl into the Past. |
| region.thera.eastern_arkham | region.thera.island_seas | sea | The Dragon Sea lanes off the eastern coast. |
| region.thera.southern_desert | region.thera.island_seas | sea | The southern ocean crossing to the island holds. |

## Implementation notes

- Regions partition the authored area corpus; membership is validated against the #1704 registry so the topology cannot name an unauthored area.
- The graph is one connected component (FSV asserts connectivity); the overworld is authoritative WORLD data, never a free in-game minimap (#944).

