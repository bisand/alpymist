//! All of the `unsafe` there is between the lock screen and libpam.
//!
//! Keep this module small enough to audit in one sitting. It declares three
//! of libpam's functions and three of the C library's, and is one callback:
//! PAM asks, a [`Conversation`] answers, and the answers are handed back in
//! memory PAM will free.
//!
//! What has to hold, and where it is seen to:
//!
//! - **PAM frees the replies with `free`**, so they are made with the C
//!   library's `calloc` and `malloc` and never by Rust's allocator.
//! - **A reply is PAM's only once the callback returns success.** Until then
//!   every one made so far is wiped and freed here, and PAM is handed nothing.
//! - **An answer is wiped on this side** as soon as PAM's copy exists, or
//!   could not be made.
//! - **A panic does not cross into C.** A conversation that panics has failed,
//!   and PAM is told so.
//! - **The conversation outlives the transaction**, because the transaction
//!   is begun and ended inside one call that borrows it.

#![allow(unsafe_code)]
// What calls the callback is Linux's; elsewhere only the tests do.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::{Conversation, Error};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

/// `PAM_SUCCESS`, `PAM_BUF_ERR` and `PAM_CONV_ERR`: what a conversation may
/// return.
const SUCCESS: c_int = 0;
const BUF_ERR: c_int = 5;
const CONV_ERR: c_int = 19;

/// The four things a module can say or ask, from `_pam_types.h`.
const PROMPT_ECHO_OFF: c_int = 1;
const PROMPT_ECHO_ON: c_int = 2;
const ERROR_MSG: c_int = 3;
const TEXT_INFO: c_int = 4;

/// `PAM_MAX_NUM_MSG`: more messages at once than this is not PAM speaking.
const MAX_MESSAGES: usize = 32;

/// `struct pam_message`.
#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

/// `struct pam_response`.
#[repr(C)]
struct Response {
    text: *mut c_char,
    /// Unused by PAM, and to be left at zero.
    retcode: c_int,
}

/// `struct pam_conv`.
#[repr(C)]
struct Conv {
    converse: Converse,
    appdata: *mut c_void,
}

/// The conversation function's signature, fixed by PAM's ABI.
type Converse =
    unsafe extern "C" fn(c_int, *const *const Message, *mut *mut Response, *mut c_void) -> c_int;

/// `pam_handle_t`, which only PAM looks inside.
#[repr(C)]
struct Handle {
    _opaque: [u8; 0],
}

// The C library's allocator: what PAM frees a reply with. std links the C
// library already, so this declares the functions and adds nothing.
unsafe extern "C" {
    fn calloc(count: usize, size: usize) -> *mut c_void;
    fn malloc(size: usize) -> *mut c_void;
    fn free(memory: *mut c_void);
}

// All of libpam that is used. Their signatures are Linux-PAM's
// `security/pam_appl.h`.
#[cfg(target_os = "linux")]
#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const Conv,
        handle: *mut *mut Handle,
    ) -> c_int;
    fn pam_authenticate(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_end(handle: *mut Handle, status: c_int) -> c_int;
}

/// Ask the PAM service `service` whether whoever answers `conversation` is
/// `user`.
///
/// One transaction: started, authenticated, ended. `Ok` is PAM's yes.
///
/// # Errors
///
/// PAM's reason for not saying yes: [`Error::Authentication`] for the wrong
/// password, and the rest for everything else that can be wrong.
#[cfg(target_os = "linux")]
pub fn authenticate<C: Conversation>(
    service: &str,
    user: &str,
    conversation: &C,
) -> Result<(), Error> {
    let service = std::ffi::CString::new(service).map_err(|_| Error::Nul)?;
    let user = std::ffi::CString::new(user).map_err(|_| Error::Nul)?;
    run(conversation, |conv, handle| {
        // SAFETY: both names are NUL-terminated and live until this returns;
        // PAM copies them, and the conversation structure, before it does.
        unsafe { pam_start(service.as_ptr(), user.as_ptr(), conv, handle) }
    })
}

/// A transaction begun by `start`, authenticated and ended.
#[cfg(target_os = "linux")]
fn run<C: Conversation>(
    conversation: &C,
    start: impl FnOnce(*const Conv, *mut *mut Handle) -> c_int,
) -> Result<(), Error> {
    let conv = Conv {
        converse: converse::<C>,
        appdata: ptr::from_ref(conversation).cast_mut().cast(),
    };
    let mut handle: *mut Handle = ptr::null_mut();
    let started = start(&raw const conv, &raw mut handle);
    if let Some(e) = Error::from_code(started) {
        return Err(e);
    }
    if handle.is_null() {
        return Err(Error::System);
    }
    // SAFETY: `handle` is the one pam_start just made, used on this thread
    // only and ended exactly once, here. The conversation it will call is
    // `converse::<C>` with the `&C` borrowed for longer than this function
    // runs, so the pointer in `appdata` is good for every call PAM makes.
    let result = unsafe {
        let result = pam_authenticate(handle, 0);
        pam_end(handle, result);
        result
    };
    Error::from_code(result).map_or(Ok(()), Err)
}

/// PAM's conversation function, answering from a `C`.
///
/// # Safety
///
/// As PAM calls it: `messages` points to `count` pointers to messages whose
/// text, when not null, is NUL-terminated; `responses` points to somewhere a
/// pointer can be written; `appdata` is a `&C` that is still alive.
unsafe extern "C" fn converse<C: Conversation>(
    count: c_int,
    messages: *const *const Message,
    responses: *mut *mut Response,
    appdata: *mut c_void,
) -> c_int {
    let Ok(count) = usize::try_from(count) else {
        return CONV_ERR;
    };
    if !(1..=MAX_MESSAGES).contains(&count)
        || messages.is_null()
        || responses.is_null()
        || appdata.is_null()
    {
        return CONV_ERR;
    }
    // SAFETY: `appdata` is what `run` put in the conversation structure: a
    // `&C` borrowed for the whole transaction.
    let conversation = unsafe { &*appdata.cast::<C>() };
    // SAFETY: calloc with a count and a size; the result is checked. Zeroed
    // memory is `count` responses with no text and a return code of zero.
    let replies = unsafe { calloc(count, size_of::<Response>()) }.cast::<Response>();
    if replies.is_null() {
        return BUF_ERR;
    }
    for i in 0..count {
        // SAFETY: `i` is below `count`, the number of pointers PAM passed.
        // Linux-PAM's layout is an array of pointers, each to one message.
        let message = unsafe { *messages.add(i) };
        let answered = if message.is_null() {
            Err(Error::Conversation)
        } else {
            // SAFETY: not null, and PAM's to keep valid during the call.
            let (style, text) = unsafe { ((*message).style, (*message).text) };
            let text = if text.is_null() {
                String::new()
            } else {
                // SAFETY: a module's message is a NUL-terminated string.
                unsafe { CStr::from_ptr(text) }
                    .to_string_lossy()
                    .into_owned()
            };
            catch_unwind(AssertUnwindSafe(|| answer(conversation, style, &text)))
                .unwrap_or(Err(Error::Conversation))
        };
        let reply = match answered {
            Ok(None) => continue,
            Ok(Some(reply)) => hand_over(reply),
            Err(_) => ptr::null_mut(),
        };
        if reply.is_null() {
            // SAFETY: `replies` is the `count` responses allocated above, of
            // which those before `i` may hold text made by `hand_over`.
            unsafe { discard(replies, count) };
            return CONV_ERR;
        }
        // SAFETY: `i` is below `count`, the number of responses allocated.
        unsafe { (*replies.add(i)).text = reply };
    }
    // SAFETY: `responses` was checked not to be null, and is where PAM asks
    // for the array to be left. From here the replies are PAM's to free.
    unsafe { *responses = replies };
    SUCCESS
}

/// What `conversation` has to say to one message: an answer where one was
/// asked for, nothing where something was only said.
fn answer<C: Conversation>(
    conversation: &C,
    style: c_int,
    text: &str,
) -> Result<Option<String>, Error> {
    match style {
        PROMPT_ECHO_OFF => conversation.masked_prompt(text).map(Some),
        PROMPT_ECHO_ON => conversation.prompt(text).map(Some),
        ERROR_MSG => {
            conversation.error(text);
            Ok(None)
        }
        TEXT_INFO => {
            conversation.info(text);
            Ok(None)
        }
        // Linux-PAM's binary prompt and whatever else: not answered.
        _ => Err(Error::Conversation),
    }
}

/// A copy of `reply` for PAM to free, and `reply` itself wiped. Null when
/// there is no memory for it, or when it has a NUL in it and so cannot be a C
/// string that says what was meant.
fn hand_over(reply: String) -> *mut c_char {
    let mut bytes = reply.into_bytes();
    let copy = if bytes.contains(&0) {
        ptr::null_mut()
    } else {
        // SAFETY: malloc of the text and its terminator; checked below.
        let copy = unsafe { malloc(bytes.len() + 1) }.cast::<u8>();
        if !copy.is_null() {
            // SAFETY: `copy` is `len + 1` bytes, fresh from malloc and so
            // apart from `bytes`; `len` are copied and the last is the NUL.
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), copy, bytes.len());
                copy.add(bytes.len()).write(0);
            }
        }
        copy.cast::<c_char>()
    };
    for byte in &mut bytes {
        // SAFETY: a write through a live `&mut u8`. Volatile, so that it is
        // not dropped as a write to memory about to be freed.
        unsafe { ptr::from_mut(byte).write_volatile(0) };
    }
    copy
}

/// Wipe and free `count` responses and the array they are in.
///
/// # Safety
///
/// `replies` is an array of `count` responses from `calloc`, each text either
/// null or a NUL-terminated string from `malloc`, and none of it is used
/// again.
unsafe fn discard(replies: *mut Response, count: usize) {
    for i in 0..count {
        // SAFETY: as this function requires.
        unsafe {
            let text = (*replies.add(i)).text;
            if text.is_null() {
                continue;
            }
            for at in 0..CStr::from_ptr(text).to_bytes().len() {
                text.add(at).write_volatile(0);
            }
            free(text.cast());
        }
    }
    // SAFETY: as this function requires.
    unsafe { free(replies.cast()) };
}

#[cfg(test)]
mod tests {
    use super::{
        CONV_ERR, ERROR_MSG, Message, PROMPT_ECHO_OFF, PROMPT_ECHO_ON, Response, SUCCESS,
        TEXT_INFO, converse, discard,
    };
    use crate::{Conversation, Error};
    use std::ffi::{CStr, CString, c_int, c_void};
    use std::ptr;
    use std::sync::Mutex;

    /// A conversation with fixed answers, which keeps what it was told.
    #[derive(Default)]
    struct Fake {
        user: Option<String>,
        password: Option<String>,
        panics: bool,
        heard: Mutex<Vec<String>>,
    }

    impl Fake {
        fn note(&self, kind: &str, text: &str) {
            self.heard.lock().unwrap().push(format!("{kind}: {text}"));
        }

        fn heard(&self) -> Vec<String> {
            self.heard.lock().unwrap().clone()
        }
    }

    impl Conversation for Fake {
        fn prompt(&self, request: &str) -> Result<String, Error> {
            self.note("asked", request);
            self.user.clone().ok_or(Error::Conversation)
        }

        fn masked_prompt(&self, request: &str) -> Result<String, Error> {
            self.note("asked hidden", request);
            assert!(!self.panics, "a conversation that panics");
            self.password.clone().ok_or(Error::Conversation)
        }

        fn error(&self, message: &str) {
            self.note("error", message);
        }

        fn info(&self, message: &str) {
            self.note("info", message);
        }
    }

    /// Be PAM: put `messages` to `fake`, and give back the return value and
    /// each reply, having freed them as PAM would.
    fn ask(fake: &Fake, messages: &[(c_int, &str)]) -> (c_int, Vec<Option<String>>) {
        let texts: Vec<CString> = messages
            .iter()
            .map(|(_, text)| CString::new(*text).unwrap())
            .collect();
        let messages: Vec<Message> = messages
            .iter()
            .zip(&texts)
            .map(|((style, _), text)| Message {
                style: *style,
                text: text.as_ptr(),
            })
            .collect();
        let pointers: Vec<*const Message> = messages.iter().map(ptr::from_ref).collect();
        let mut responses: *mut Response = ptr::null_mut();
        // SAFETY: called as PAM calls it; see `converse`.
        let code = unsafe {
            converse::<Fake>(
                c_int::try_from(pointers.len()).unwrap(),
                pointers.as_ptr(),
                &raw mut responses,
                ptr::from_ref(fake).cast_mut().cast(),
            )
        };
        if code != SUCCESS {
            assert!(responses.is_null(), "a failed conversation left replies");
            return (code, Vec::new());
        }
        assert!(!responses.is_null());
        let mut replies = Vec::new();
        for i in 0..pointers.len() {
            // SAFETY: success means one response for each message.
            let response = unsafe { &*responses.add(i) };
            assert_eq!(response.retcode, 0);
            replies.push((!response.text.is_null()).then(|| {
                // SAFETY: a reply is a NUL-terminated string.
                unsafe { CStr::from_ptr(response.text) }
                    .to_str()
                    .unwrap()
                    .to_owned()
            }));
        }
        // SAFETY: the replies are ours now, as they would be PAM's.
        unsafe { discard(responses, pointers.len()) };
        (code, replies)
    }

    #[test]
    fn the_password_is_what_a_hidden_prompt_is_answered_with() {
        let fake = Fake {
            password: Some("correct horse".into()),
            ..Fake::default()
        };
        let (code, replies) = ask(&fake, &[(PROMPT_ECHO_OFF, "Password: ")]);
        assert_eq!(code, SUCCESS);
        assert_eq!(replies, [Some("correct horse".to_owned())]);
        assert_eq!(fake.heard(), ["asked hidden: Password: "]);
    }

    #[test]
    fn each_message_gets_its_own_reply_and_what_is_only_said_gets_none() {
        let fake = Fake {
            user: Some("mist".into()),
            password: Some("pw".into()),
            ..Fake::default()
        };
        let (code, replies) = ask(
            &fake,
            &[
                (TEXT_INFO, "Place your finger"),
                (PROMPT_ECHO_ON, "login: "),
                (ERROR_MSG, "Failed to match"),
                (PROMPT_ECHO_OFF, "Password: "),
            ],
        );
        assert_eq!(code, SUCCESS);
        assert_eq!(
            replies,
            [None, Some("mist".to_owned()), None, Some("pw".to_owned())]
        );
        assert_eq!(
            fake.heard(),
            [
                "info: Place your finger",
                "asked: login: ",
                "error: Failed to match",
                "asked hidden: Password: ",
            ]
        );
    }

    #[test]
    fn a_conversation_that_refuses_ends_it_and_hands_pam_nothing() {
        // The user is given and the password is not: the first reply is made
        // and then has to be taken back.
        let fake = Fake {
            user: Some("mist".into()),
            ..Fake::default()
        };
        let (code, _) = ask(
            &fake,
            &[(PROMPT_ECHO_ON, "login: "), (PROMPT_ECHO_OFF, "Password: ")],
        );
        assert_eq!(code, CONV_ERR);
    }

    #[test]
    fn an_answer_with_a_nul_in_it_is_not_cut_short_and_sent() {
        let fake = Fake {
            password: Some("pass\0word".into()),
            ..Fake::default()
        };
        assert_eq!(ask(&fake, &[(PROMPT_ECHO_OFF, "Password: ")]).0, CONV_ERR);
    }

    #[test]
    fn a_kind_of_message_that_is_not_one_of_the_four_is_not_answered() {
        let fake = Fake {
            password: Some("pw".into()),
            ..Fake::default()
        };
        // 7 is Linux-PAM's binary prompt.
        assert_eq!(ask(&fake, &[(7, "")]).0, CONV_ERR);
        assert_eq!(ask(&fake, &[(0, "")]).0, CONV_ERR);
        assert!(fake.heard().is_empty());
    }

    #[test]
    fn a_panic_in_the_conversation_is_a_failed_conversation() {
        let fake = Fake {
            password: Some("pw".into()),
            panics: true,
            ..Fake::default()
        };
        assert_eq!(ask(&fake, &[(PROMPT_ECHO_OFF, "Password: ")]).0, CONV_ERR);
    }

    #[test]
    fn what_pam_would_never_pass_is_refused_and_not_followed() {
        let fake = Fake::default();
        let appdata: *mut c_void = ptr::from_ref(&fake).cast_mut().cast();
        let message = Message {
            style: TEXT_INFO,
            text: ptr::null(),
        };
        let one = [ptr::from_ref(&message)];
        let mut responses: *mut Response = ptr::null_mut();
        // SAFETY: each call breaks one thing PAM promises, and that thing is
        // what `converse` checks before it touches anything.
        unsafe {
            for count in [0, -1, 33] {
                let code = converse::<Fake>(count, one.as_ptr(), &raw mut responses, appdata);
                assert_eq!(code, CONV_ERR);
            }
            let code = converse::<Fake>(1, ptr::null(), &raw mut responses, appdata);
            assert_eq!(code, CONV_ERR);
            let code = converse::<Fake>(1, one.as_ptr(), ptr::null_mut(), appdata);
            assert_eq!(code, CONV_ERR);
            let code = converse::<Fake>(1, one.as_ptr(), &raw mut responses, ptr::null_mut());
            assert_eq!(code, CONV_ERR);
            let none = [ptr::null::<Message>()];
            let code = converse::<Fake>(1, none.as_ptr(), &raw mut responses, appdata);
            assert_eq!(code, CONV_ERR);
        }
        assert!(responses.is_null());
        assert!(fake.heard().is_empty());

        // A message with no text is one that says nothing, not a fault.
        // SAFETY: called as PAM calls it.
        let code = unsafe { converse::<Fake>(1, one.as_ptr(), &raw mut responses, appdata) };
        assert_eq!(code, SUCCESS);
        assert_eq!(fake.heard(), ["info: "]);
        // SAFETY: one response, ours to free.
        unsafe { discard(responses, 1) };
    }

    /// libpam itself, with services of the tests' own in a directory of their
    /// own, which `pam_start_confdir` (Linux-PAM 1.4) allows without root.
    #[cfg(target_os = "linux")]
    mod with_libpam {
        use super::super::{Conv, Handle, run};
        use super::Fake;
        use crate::Error;
        use std::ffi::{CString, c_char, c_int};
        use std::path::PathBuf;

        #[link(name = "pam")]
        unsafe extern "C" {
            fn pam_start_confdir(
                service: *const c_char,
                user: *const c_char,
                conv: *const Conv,
                confdir: *const c_char,
                handle: *mut *mut Handle,
            ) -> c_int;
        }

        /// Authenticate `user` with a service whose whole configuration is
        /// `stack`.
        fn authenticate(name: &str, stack: &str, user: &str, fake: &Fake) -> Result<(), Error> {
            let dir: PathBuf =
                std::env::temp_dir().join(format!("alpymist-pam-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(name), stack).unwrap();
            let service = CString::new(name).unwrap();
            let user = CString::new(user).unwrap();
            let confdir = CString::new(dir.to_str().unwrap()).unwrap();
            let result = run(fake, |conv, handle| {
                // SAFETY: as `pam_start`, with a directory's name as well.
                unsafe {
                    pam_start_confdir(
                        service.as_ptr(),
                        user.as_ptr(),
                        conv,
                        confdir.as_ptr(),
                        handle,
                    )
                }
            });
            std::fs::remove_dir_all(&dir).unwrap();
            result
        }

        #[test]
        fn a_stack_that_permits_says_yes() {
            let fake = Fake::default();
            let result = authenticate("permit", "auth required pam_permit.so\n", "mist", &fake);
            assert_eq!(result, Ok(()));
        }

        #[test]
        fn a_stack_that_denies_is_a_failed_authentication() {
            let fake = Fake::default();
            let result = authenticate("deny", "auth required pam_deny.so\n", "mist", &fake);
            assert_eq!(result, Err(Error::Authentication));
        }

        #[test]
        fn what_a_module_says_reaches_the_conversation() {
            let fake = Fake::default();
            let stack = "auth optional pam_echo.so Touch the reader\n\
                         auth required pam_permit.so\n";
            assert_eq!(authenticate("says", stack, "mist", &fake), Ok(()));
            assert_eq!(fake.heard(), ["info: Touch the reader"]);
        }

        /// `pam_unix` asks for a password before it finds there is no such
        /// account, so the reply goes through PAM and is freed by it.
        #[test]
        fn a_password_is_asked_for_taken_and_freed_by_pam() {
            let fake = Fake {
                password: Some("not it".into()),
                ..Fake::default()
            };
            let stack = "auth required pam_unix.so nodelay\n";
            let result = authenticate("asks", stack, "nobody-by-this-name", &fake);
            assert!(result.is_err(), "{result:?}");
            assert_eq!(fake.heard().len(), 1, "{:?}", fake.heard());
            assert!(fake.heard()[0].starts_with("asked hidden: "));
        }

        #[test]
        fn a_conversation_that_refuses_fails_the_attempt() {
            // No password to give.
            let fake = Fake::default();
            let stack = "auth required pam_unix.so nodelay\n";
            let result = authenticate("refuses", stack, "nobody-by-this-name", &fake);
            assert!(result.is_err(), "{result:?}");
            assert_ne!(result, Err(Error::Nul));
        }
    }
}
