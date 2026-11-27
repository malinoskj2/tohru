use std::cell::RefCell;
use std::collections::VecDeque;

use super::orca::{Entry, Hooks, Read, Wait, Waited, wait_for_reply};

fn entry(role: &str, text: &str) -> Entry {
    Entry {
        role: role.to_owned(),
        text: text.to_owned(),
        ..Entry::default()
    }
}

fn page(entries: &[&Entry], state: &str) -> Read {
    Read {
        entries: entries.iter().map(|entry| (*entry).clone()).collect(),
        state: state.to_owned(),
    }
}

struct Polled {
    waited: Waited,
    spoken: Vec<String>,
    origins: Vec<(String, String)>,
}

fn poll(wait: &Wait<'_>, pages: Vec<Read>) -> Polled {
    let pages = RefCell::new(VecDeque::from(pages));
    let mut spoken = Vec::new();
    let mut origins = Vec::new();
    let waited = {
        let mut on_delta = |text: &str| spoken.push(text.to_owned());
        let mut on_origin =
            |stable: &str, canonical: &str| origins.push((stable.to_owned(), canonical.to_owned()));
        let mut hooks = Hooks {
            on_delta: &mut on_delta,
            stop: &|| false,
            on_origin: &mut on_origin,
        };
        wait_for_reply(
            wait,
            &mut hooks,
            &mut || {
                pages
                    .borrow_mut()
                    .pop_front()
                    .ok_or_else(|| "pages exhausted".to_owned())
            },
            &mut || {},
        )
    };
    assert!(pages.borrow().is_empty(), "the poll stopped early");
    Polled {
        waited,
        spoken,
        origins,
    }
}

#[test]
fn queued_request_uses_the_native_alias_and_internal_mail_takes_no_owner_authority() {
    let owner = Entry {
        id: "orca:1133797".to_owned(),
        request_id: "orca:0887907".to_owned(),
        input_kind: Some("owner".to_owned()),
        ..entry("user", "Are you there")
    };
    let mail = Entry {
        id: "mail".to_owned(),
        input_kind: Some("internal".to_owned()),
        ..entry("user", "unrelated status")
    };
    let answer = Entry {
        input_kind: Some("answer".to_owned()),
        request_id: "orca:0887907".to_owned(),
        completion_id: "answer-final".to_owned(),
        ..entry("assistant", "Yes, I am here.")
    };
    let internal_answer = Entry {
        input_kind: Some("internal".to_owned()),
        ..entry("assistant", "PRIVATE CONTROL RESPONSE")
    };
    let wait = Wait {
        chat_id: "chat",
        before_count: 0,
        prompt_marker: "Are you there",
        message_id: "orca:0887907",
    };
    let polled = poll(
        &wait,
        vec![
            page(&[], "working"),
            page(&[&owner, &mail], "working"),
            page(&[&owner, &mail, &answer, &internal_answer], "working"),
        ],
    );
    assert_eq!(polled.waited.text, "Yes, I am here.");
    assert!(!polled.waited.stopped);
    assert_eq!(polled.waited.error, "");
    assert_eq!(
        polled.origins,
        [("orca:0887907".to_owned(), "orca:1133797".to_owned())]
    );
    assert_eq!(polled.spoken, ["Yes, I am here."]);
}

#[test]
fn replayed_commentary_and_a_full_history_window_emit_only_the_completed_final() {
    let prompt = Entry {
        id: "our-turn".to_owned(),
        ..entry("user", "same prompt")
    };
    let commentary = entry("assistant", "I will check that.");
    let last = entry("assistant", "The answer is eight.");
    let wait = Wait {
        chat_id: "chat",
        before_count: 3,
        prompt_marker: "same prompt",
        message_id: "our-turn",
    };
    // The bounded page stays as long as before sending; repeated commentary and an unknown
    // state must never become speech.
    let polled = poll(
        &wait,
        vec![
            page(&[&prompt, &commentary, &commentary], "working"),
            page(&[&prompt, &commentary, &commentary], ""),
            page(&[&prompt, &commentary, &last], "working"),
            page(&[&prompt, &commentary, &last], "done"),
        ],
    );
    assert_eq!(polled.waited.text, "The answer is eight.");
    assert!(!polled.waited.stopped);
    assert_eq!(polled.waited.error, "");
    assert_eq!(polled.spoken, ["The answer is eight."]);
}

#[test]
fn screen_captures_are_standing_knowledge_not_a_per_turn_note() {
    let soul = super::home::build_orca_soul("Tohru", "");
    for phrase in [
        "$XDG_RUNTIME_DIR/tohru-screens/",
        "`index.json`",
        "only while you are on screen",
        "only when the owner's screen is relevant",
        "Never upload, send or copy the captures anywhere.",
    ] {
        assert!(soul.contains(phrase), "{phrase}");
    }
    assert!(!soul.contains("[Screen captures"));
    assert!(
        super::quick::QUICK_VOICE_RULE
            .contains("any question about what is on the owner's screen goes home")
    );
}
