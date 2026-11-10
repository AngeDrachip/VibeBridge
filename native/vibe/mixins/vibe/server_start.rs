// A world is opening: start Vibe's JVM sidecar on the jars in <game>/vibe/mods.
fn vibe_server_start(game: &mut crate::game::Game<'_>, dedicated: bool) {
    use crate::game::vibe as m;
    let message = m::start(&game.native_instance_dir());
    eprintln!("[vibe] {message} (dedicated={dedicated})");
}
