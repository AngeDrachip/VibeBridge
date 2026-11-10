// `/vibe ...` typed in chat, before the server or Horizon sees it.
fn vibe_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::vibe as m;
    let mut words = command.split_whitespace();
    if words.next() != Some("vibe") {
        return;
    }
    *ran = true;
    let game_dir = game.native_instance_dir();
    let lines = match words.next().unwrap_or("status") {
        "status" => m::status(&game_dir),
        "mods" | "list" => m::mod_lines(&game_dir),
        "start" => vec![m::start(&game_dir)],
        "stop" => vec![m::stop()],
        "reload" | "restart" => vec![m::reload(&game_dir)],
        "export" => vec![m::export(&game_dir)],
        "help" => m::help(),
        other => {
            let mut lines = vec![format!("Unknown /vibe subcommand: {other}")];
            lines.extend(m::help());
            lines
        }
    };
    for line in lines {
        game.native_chat(&line);
    }
}
