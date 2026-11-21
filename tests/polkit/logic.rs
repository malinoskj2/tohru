//! The polkit dialog's controller against a stand-in for the native agent.
use mlua::Lua;

fn controller() -> Lua {
    let lua = Lua::new();
    lua.load(
        r#"
        fact = {polkit_picked = 0, polkit_identities = 0, polkit_identity = -1, polkit_unfolded = false, polkit_shown = false}
        text = {polkit_answer = "", ["screen.0.name"] = "HEADLESS-1"}
        handlers, timers, calls, watches, focused = {}, {}, {}, {}, nil
        local next_id = 0
        function on(name, callback) handlers[name] = callback end
        function after(ms, callback) next_id = next_id + 1; timers[next_id] = {ms = ms, run = callback}; return next_id end
        function cancel(id) timers[id] = nil end
        function focus(name) focused = name end
        function log() end
        function every() error("the dialog never polls") end
        function run() error("the dialog runs no programs") end
        function spawn() error("the dialog runs no programs") end
        sys = {
            call = function(name, ...) calls[#calls + 1] = {name, ...} end,
            watch = function(name, callback) watches[name] = callback; return true end,
        }
        function last() return calls[#calls] end
        function fire()
            local id, t = next(timers)
            timers[id] = nil
            t.run()
            return t.ms
        end
        local ended = 0
        function agent(over)
            local v = {ready = true, pending = true, asking = true, message = "Authentication is required to run nixos-rebuild as the super user",
                action = "org.freedesktop.policykit.exec", icon = "", app = "Alacritty", command = "/run/current-system/sw/bin/nixos-rebuild switch",
                user = "jesse", uid = 1000, prompt = "Password: ", secret = true, info = "", warning = "", error = "none",
                tries = 3, attempts = 0, queued = 0, ended = ended, outcome = "none", outcome_user = "",
                identities = {{name = "jesse", uid = 1000, group = "wheel"}, {name = "root", uid = 0, group = "superuser"}, {name = "admin", uid = 1001, group = "wheel"}}}
            for k, val in pairs(over or {}) do v[k] = val end
            ended = v.ended
            watches.polkit(v)
        end
        "#,
    )
    .exec()
    .unwrap();
    let make = lua.load(include_str!("../../shell/polkit/controller.luau")).eval::<mlua::Function>().unwrap();
    lua.globals().set("polkit", make.call::<mlua::Table>(()).unwrap()).unwrap();
    lua
}

#[test]
fn off_by_default_it_never_becomes_the_agent() {
    let lua = controller();
    lua.load(r#"assert(polkit.start(false, "") == false); assert(watches.polkit == nil and watches.window == nil and not fact.polkit_enabled)"#)
        .exec()
        .unwrap();
}

#[test]
fn a_request_counts_attempts_then_characters_then_tries_left() {
    let lua = controller();
    lua.load(
        r#"
        assert(polkit.start(true, ""))
        watches.window({monitor = "DP-2"})
        agent()
        assert(fact.polkit_shown and not fact.polkit_outcome and text.polkit_output == "DP-2")
        assert(text.polkit_numeral == "03" and text.polkit_caption == "ATTEMPTS" and text.polkit_phase == "request")
        assert(text.polkit_footer == "3 admins · Tab to unfold" and not fact.polkit_alarm)
        assert(text.polkit_placeholder == "Password for jesse…" and focused == "polkit_answer")
        assert(text.polkit_app == "Alacritty" and text.polkit_id1 == "root" and text.polkit_id1_meta == "superuser · 0" and fact.polkit_identity == 0)
        assert(fact.polkit_level == 0.24 and fact.polkit_tint == 0)

        text.polkit_answer = "hunter22"; handlers["text:polkit_answer"]("hunter22")
        assert(text.polkit_numeral == "08" and text.polkit_caption == "CHARS" and fact.polkit_chars == 8 and text.polkit_phase == "typing")
        assert(math.abs(fact.polkit_level - 0.56) < 1e-9)

        handlers["submit:polkit_answer"]("hunter22")
        assert(last()[1] == "polkit.answer" and last()[2] == "hunter22")
        assert(text.polkit_answer == "" and fact.polkit_chars == 0 and text.polkit_phase == "checking")
        agent({asking = false})
        assert(text.polkit_phase == "checking" and text.polkit_footer == "Checking…")

        agent({error = "wrong", tries = 2, attempts = 1})
        assert(text.polkit_numeral == "02" and text.polkit_caption == "TRIES LEFT" and fact.polkit_tint == 1 and fact.polkit_alarm)
        assert(text.polkit_footer == "Incorrect password · 2 attempts remaining")
        text.polkit_answer = "x"; handlers["text:polkit_answer"]("x")
        assert(text.polkit_caption == "CHARS" and fact.polkit_tint == 0)
        agent({error = "wrong", tries = 1, attempts = 2}); handlers["text:polkit_answer"]("")
        assert(text.polkit_footer == "Incorrect password · 1 attempt remaining")
        "#,
    )
    .exec()
    .unwrap();
}

/// The card only ever says a request was let through when the agent reports
/// that it ended granted; submitting, or a stale report, never does.
#[test]
fn success_is_only_shown_when_the_agent_reports_it() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "DP-1")
        agent()
        assert(watches.window == nil and text.polkit_output == "DP-1")
        text.polkit_answer = "right"; handlers["text:polkit_answer"]("right")
        handlers.polkit_submit()
        assert(not fact.polkit_outcome and text.polkit_numeral ~= "✓")
        agent({asking = false, outcome = "granted", outcome_user = "jesse"})
        assert(not fact.polkit_outcome, "an outcome without a new ending is an earlier request's")
        agent({pending = false, asking = false, ended = 1, outcome = "granted", outcome_user = "jesse"})
        assert(fact.polkit_outcome and fact.polkit_shown and text.polkit_numeral == "✓" and text.polkit_caption == "UNLOCKED")
        assert(text.polkit_result == "Authenticated as jesse" and text.polkit_closing == "Continuing…" and fact.polkit_tint == -1)
        assert(focused == nil)
        handlers.polkit_submit(); handlers.polkit_cancel()
        assert(#calls == 1, "nothing is sent once the request has ended")
        assert(fire() == 1100 and not fact.polkit_shown and text.polkit_output == "", "hidden means unmapped")
        agent({ended = 1})
        assert(fact.polkit_shown and text.polkit_output == "DP-1", "the next request maps it again")
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn cancelling_clears_the_field_and_fades_to_cancelled() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "")
        agent()
        text.polkit_answer = "half"; handlers["text:polkit_answer"]("half")
        handlers.key("Escape")
        assert(last()[1] == "polkit.cancel" and text.polkit_answer == "" and fact.polkit_chars == 0)
        agent({pending = false, asking = false, ended = 1, outcome = "cancelled", outcome_user = "jesse"})
        assert(fact.polkit_outcome and text.polkit_numeral == "—" and text.polkit_caption == "DISMISSED")
        assert(text.polkit_result == "Authentication cancelled" and text.polkit_footer == "Request denied by user" and fact.polkit_tint == 0)
        assert(fire() == 900 and not fact.polkit_shown)
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn three_wrong_answers_show_the_failure() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "")
        agent()
        agent({pending = false, asking = false, ended = 1, outcome = "failed", outcome_user = "jesse", tries = 0})
        assert(fact.polkit_outcome and text.polkit_numeral == "00" and text.polkit_result == "Authentication failed" and fact.polkit_tint == 1)
        assert(fire() == 1600 and not fact.polkit_shown)
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn the_picker_unfolds_and_chooses_another_identity() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "")
        agent()
        text.polkit_answer = "abc"; handlers["text:polkit_answer"]("abc")
        handlers.key("Tab"); assert(fact.polkit_unfolded and fact.polkit_picked == 0)
        handlers.key("Down"); handlers.key("Down"); handlers.key("Down")
        assert(fact.polkit_picked == 2)
        handlers.key("Up"); assert(fact.polkit_picked == 1)
        handlers.key("Escape"); assert(not fact.polkit_unfolded and #calls == 0, "Escape in the picker only folds it")
        handlers.key("Tab"); handlers.key("Down"); handlers.key("Return")
        assert(last()[1] == "polkit.identity" and last()[2] == "root" and not fact.polkit_unfolded and text.polkit_answer == "")
        handlers.polkit_choose(7); handlers.polkit_choose(0.5); handlers.polkit_choose(0)
        assert(#calls == 1, "out of range, fractional and the current identity choose nothing")
        agent({user = "root", uid = 0})
        assert(fact.polkit_identity == 1 and text.polkit_placeholder == "Password for root…")
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn pam_messages_and_queued_requests_are_shown() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "")
        agent({info = "Place your finger on the reader"})
        assert(text.polkit_footer == "Place your finger on the reader" and not fact.polkit_alarm)
        agent({warning = "Your password expires in 2 days", info = "x"})
        assert(text.polkit_footer == "Your password expires in 2 days" and fact.polkit_alarm)
        agent({queued = 2})
        assert(text.polkit_footer == "3 admins · Tab to unfold · 2 waiting")
        agent({prompt = "Token PIN: ", secret = false})
        assert(text.polkit_placeholder == "Token PIN:")
        -- The first ends granted while the next already waits: it is shown after the success.
        agent({ended = 1, outcome = "granted", outcome_user = "jesse", queued = 0, message = "Mount a disk"})
        assert(fact.polkit_outcome)
        fire()
        assert(fact.polkit_shown and not fact.polkit_outcome and text.polkit_message == "Mount a disk" and text.polkit_numeral == "03")
        "#,
    )
    .exec()
    .unwrap();
}

/// Nothing typed survives a request: not after it ends, not while nothing is asked.
#[test]
fn typing_while_nothing_is_asked_is_dropped() {
    let lua = controller();
    lua.load(
        r#"
        polkit.start(true, "")
        agent({asking = false})
        text.polkit_answer = "early"; handlers["text:polkit_answer"]("early")
        assert(text.polkit_answer == "" and fact.polkit_chars == 0)
        handlers["submit:polkit_answer"]("early"); assert(#calls == 0)
        agent()
        text.polkit_answer = "secret"; handlers["text:polkit_answer"]("secret")
        agent({pending = false, asking = false})
        assert(text.polkit_answer == "" and not fact.polkit_shown and focused == nil)
        "#,
    )
    .exec()
    .unwrap();
}
