// The world is stopping: stop the sidecar so no JVM outlives the world.
fn vibe_server_stop(dedicated: bool) {
    use crate::game::vibe as m;
    eprintln!("[vibe] {} (dedicated={dedicated})", m::stop());
}
