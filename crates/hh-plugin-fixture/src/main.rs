//! `hh-plugin-fixture` — the conformance-fixture binary. Launched by the
//! variant host through `hh-helper` (`argv = [bin, --socket, <path>,
//! …flags]`); connects back over the allow-listed unix socket and runs
//! the shared [`PluginRuntime`] with [`FixtureLogic`].

use std::time::Duration;

use hh_plugin_fixture::fixture::FixtureLogic;
use hh_plugin_fixture::PluginRuntime;
use hh_varhost::channel::SocketIo;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--socket" {
            socket = args.get(i + 1).cloned();
        }
        i += 1;
    }
    let Some(sock) = socket else {
        eprintln!("hh-plugin-fixture: --socket <path> required");
        std::process::exit(2);
    };
    let stream = match std::os::unix::net::UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hh-plugin-fixture: connect {sock}: {e}");
            std::process::exit(2);
        }
    };
    let logic = FixtureLogic::from_args(&args);
    match PluginRuntime::connect(
        Box::new(SocketIo::new(stream)),
        logic,
        Duration::from_secs(10),
    ) {
        Ok(mut rt) => {
            if let Err(e) = rt.run() {
                eprintln!("hh-plugin-fixture: session ended: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("hh-plugin-fixture: handshake failed: {e}");
            std::process::exit(2);
        }
    }
}
