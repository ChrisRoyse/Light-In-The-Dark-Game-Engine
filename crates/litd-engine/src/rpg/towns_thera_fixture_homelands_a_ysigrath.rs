use super::*;

/// Ysigrath: two-tribe saurian swamp homeland in the southwest mires; sz-weh
/// villagers rank good (10-15), k-sra villagers evil (10-15). Saurian hometown (#1018).
pub fn ysigrath_area_spec() -> AreaSpec {
    let aid = YSIGRATH_AREA_ID;
    area_spec(
        aid,
        "Ysigrath",
        AreaKind::Wilderness,
        1647,
        &[985, 1018, 1601],
        "ysigrath",
        (5, 15),
        "A steaming two-tribe saurian mire in the southwest, where the sz-weh villagers \
         (good ranking 10-15) and the k-sra villagers (evil ranking 10-15) share the reed-mounds.",
        vec![
            room(
                "room.ysigrath.marsh_edge",
                "Marsh Edge",
                RoomKind::Gate,
                "The reed-choked edge where dry land gives way to mire.",
                RoomConcealment::GateChokepoint,
                &["npc.ysigrath.marsh_watcher"],
            ),
            room(
                "room.ysigrath.sz_weh_village",
                "Sz-weh Village",
                RoomKind::Wilderness,
                "Reed-mound huts of the sz-weh saurians (good ranking 10-15).",
                RoomConcealment::CrowdCover,
                &["npc.ysigrath.sz_weh_villager"],
            ),
            room(
                "room.ysigrath.k_sra_village",
                "K-sra Village",
                RoomKind::Wilderness,
                "Bone-daubed mounds of the k-sra saurians (evil ranking 10-15).",
                RoomConcealment::CrowdCover,
                &["npc.ysigrath.k_sra_villager"],
            ),
            room(
                "room.ysigrath.mound_shrine",
                "Mound Shrine",
                RoomKind::Temple,
                "A great reed-mound shrine that serves as the marsh recall point.",
                RoomConcealment::None,
                &["npc.ysigrath.mound_elder"],
            ),
            room(
                "room.ysigrath.deep_mire",
                "Deep Mire",
                RoomKind::Wilderness,
                "A concealed deep-water mire threading between the tribes.",
                RoomConcealment::HiddenLayer,
                &["npc.ysigrath.mire_lurker"],
            ),
        ],
        vec![
            internal(
                "conn.ysigrath.edge_to_sz_weh",
                "room.ysigrath.marsh_edge",
                aid,
                "room.ysigrath.sz_weh_village",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.ysigrath.edge_to_k_sra",
                "room.ysigrath.marsh_edge",
                aid,
                "room.ysigrath.k_sra_village",
                ConnectionDirection::South,
                TravelKind::Path,
            ),
            internal(
                "conn.ysigrath.sz_weh_to_shrine",
                "room.ysigrath.sz_weh_village",
                aid,
                "room.ysigrath.mound_shrine",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.ysigrath.mire_channel",
                "room.ysigrath.mound_shrine",
                aid,
                "room.ysigrath.deep_mire",
                ConnectionDirection::Down,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.ysigrath.to_galadon",
                "room.ysigrath.marsh_edge",
                GALADON_AREA_ID,
                ConnectionDirection::Northeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.ysigrath.cycle", 30),
        repop("repop.ysigrath.pop", 22, 15, 2),
        recall(
            true,
            Some("room.ysigrath.mound_shrine"),
            false,
            access_restricted(&["race.saurian"]),
        ),
        visual(
            "tileset.wild.ysigrath.reed_mire",
            "model.wild.ysigrath.swamp_villages",
            &[
                "prop.wild.reed_mound",
                "prop.wild.bone_daub_hut",
                "prop.wild.mound_shrine",
            ],
            "steaming reed-mire of two saurian mound-villages around a great shrine",
            "Original saurian mire props and reed-mound dressing; no copied iconography.",
        ),
        &[
            "Saurian hometown (access race.saurian, #1018); two tribes split good/evil ranking 10-15 (#1647).",
            "Deep mire is a Hidden channel (#944); road runs northeast toward Galadon (#1632).",
        ],
    )
}
