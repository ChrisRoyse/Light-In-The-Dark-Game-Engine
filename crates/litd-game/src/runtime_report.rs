use litd_game::replay_viewer::ReplayViewerSnapshot;

pub(crate) fn print_replay_report(snapshot: &ReplayViewerSnapshot) {
    println!(
        "litd-game replay status={} speed={} perspective={} tick={} orders={}/{} \
         state_hash={:016x} expected_hash={:016x} final_hash_matches={} \
         fog_masked={} fog_explored={} fog_visible={}",
        if snapshot.paused {
            "paused"
        } else if snapshot.complete {
            "complete"
        } else {
            "playing"
        },
        snapshot.speed.label(),
        snapshot.perspective,
        snapshot.applied_tick,
        snapshot.order_cursor,
        snapshot.order_count,
        snapshot.state_hash,
        snapshot.expected_final_hash,
        snapshot.final_hash_matches,
        snapshot.perspective_fog.0,
        snapshot.perspective_fog.1,
        snapshot.perspective_fog.2,
    );
}
