//! Drive the installer's Wi-Fi worker by hand, on a machine with iwd.
//!
//! `alpymist-install --example wifi -- scan`
//! `alpymist-install --example wifi -- connect <ssid> [passphrase]`
//!
//! The Network screen uses exactly this worker; this is how it is tried
//! against a real iwd without walking the wizard.

use alpymist_install::wifi::{self, Request};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(adapter) = wifi::adapter() else {
        eprintln!("no wireless interface");
        std::process::exit(1);
    };
    let link = wifi::spawn(adapter);
    let request = match args.first().map(String::as_str) {
        Some("connect") if args.len() >= 2 => Request::Connect {
            ssid: args[1].clone(),
            passphrase: args.get(2).cloned(),
        },
        _ => Request::Scan,
    };
    link.commands.send(request).expect("worker running");
    match link.events.recv() {
        Ok(event) => println!("{event:?}"),
        Err(_) => eprintln!("the worker stopped"),
    }
}
