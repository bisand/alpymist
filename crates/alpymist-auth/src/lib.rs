//! Alpymist's password prompt, for anything that needs root from the desktop.
//!
//! There is no elogind on Alpymist, so polkit sees no sessions and no desktop
//! agent can speak for one. polkit does let an agent speak for a single
//! process, though, and that is enough: a program that needs root registers
//! an [`agent`] for itself and runs its privileged helper through `pkexec`.
//! polkitd asks the agent to authenticate, the agent starts `alpymist-auth
//! prompt`, and the prompt asks for the password in a Denise dialog.
//!
//! The password is never checked here. The prompt hands it to polkit's own
//! setuid `polkit-agent-helper-1`, which runs PAM and tells polkitd the
//! answer; the program that asked never sees it, and nothing of ours runs as
//! root with it. Split so each part does one thing:
//!
//! - [`helper`]: the conversation with `polkit-agent-helper-1`.
//! - [`request`]: what is being authorised, as polkitd describes it.
//! - [`prompt`]: the dialog's state and what every key does, with no pixels.
//! - [`view`]: the dialog, laid out and painted.
//! - [`agent`]: the polkit agent a program registers for itself.
//! - [`secret`]: a password that wipes itself.

#![forbid(unsafe_code)]

pub mod agent;
pub mod helper;
pub mod prompt;
pub mod request;
pub mod secret;
pub mod view;
