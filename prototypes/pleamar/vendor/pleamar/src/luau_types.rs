//! `pleamar --luau-types [scene.plm]`: a definitions file for luau-lsp, so an
//! editor knows the globals a scene's logic is given —`fact`, `on`, `sound`…—
//! instead of calling them undefined, and completes them. With a scene, `fact`,
//! `text` and `model` are typed with that scene's own names: `fact.` offers its
//! facts, each as what it is (`boolean`, `number`, `"low" | "normal"`).

use crate::scene::{FactType, FieldType, Model, Scene};

/// The API every logic has, whatever its scene.
const API: &str = r#"-- What a pleamar logic is given. Made by `pleamar --luau-types`; made again
-- when the scene changes. See docs/10-luau-logic.md for what each one does.

type PleamarRunOptions = {
	stdin: (string | "open")?,
	output: boolean?,
	input: string?,
	errors: boolean?,
	env: { [string]: string }?,
}

type PleamarPad = {
	name: string,
	lx: number, ly: number, rx: number, ry: number, lt: number, rt: number,
	a: boolean, b: boolean, x: boolean, y: boolean, lb: boolean, rb: boolean,
	back: boolean, start: boolean, guide: boolean, ls: boolean, rs: boolean,
	up: boolean, down: boolean, left: boolean, right: boolean,
}

type PleamarHow = {
	volume: number?,
	pitch: number?,
	pan: number?,
	loop: boolean?,
	-- Milliseconds to wait before it starts, kept by the mixer to the sample.
	delay: number?,
}

type PleamarTone = PleamarHow & {
	wave: ("sine" | "square" | "saw" | "triangle" | "noise")?,
	freq: number?,
	to: number?,
	ms: number?,
	attack: number?,
	release: number?,
	duty: number?,
}

-- Something that happens: a scene event the logic hears (`event x ->`), a zone
-- (`press:name`, `release:name`, `enter:name`, `leave:name`, `scroll:name`,
-- `drop:name`), a field (`text:name`, `submit:name`), `fact:name`, `frame`,
-- `key`, `key_down`, `key_up`, `focus`, `blur`, `layer:name`, `demo`.
declare function on(name: string, handler: (...any) -> ()): ()
declare function play(gesture: string): ()
declare function every(ms: number, f: () -> ()): number
declare function after(ms: number, f: () -> ()): number
declare function cancel(timer: number): ()
declare function run(command: string, args: { string }?, done: ((output: string, code: number) -> ())?, how: PleamarRunOptions?): ()
declare function spawn(command: string, args: { string }?, line: ((line: string) -> ())?, ended: ((output: string, code: number) -> ())?, how: PleamarRunOptions?): number
declare function write(id: number, text: string?): boolean
declare function kill(id: number, how: ("int" | "term")?): ()
declare function log(...: any): ()
declare function tr(text: string): string
declare function busy(ms: number): ()
declare function focus(field: string?): ()
declare function held(key: string): boolean
declare function pad(n: number): PleamarPad?
declare function require(path: string): any

declare json: {
	decode: (text: string) -> any,
	encode: (value: any) -> string,
}

declare sys: {
	-- Listens to a system service: told its state now and at every change. False if this system has none.
	watch: (name: string, f: (state: any) -> ()) -> boolean,
	call: (name: string, ...any) -> (),
	ask: (name: string, ...any) -> any,
}

declare mouse: {
	x: number?,
	y: number?,
	over: boolean,
	left: boolean,
	right: boolean,
	middle: boolean,
}

declare hit: {
	circles: (ax: number, ay: number, ar: number, bx: number, by: number, br: number) -> boolean,
	boxes: (ax: number, ay: number, aw: number, ah: number, bx: number, by: number, bw: number, bh: number) -> boolean,
	circle_box: (cx: number, cy: number, r: number, bx: number, by: number, bw: number, bh: number) -> boolean,
	point: (px: number, py: number, bx: number, by: number, bw: number, bh: number) -> boolean,
	pairs: (these: { any }, those: { any }) -> { { number } },
	first: (x: number, y: number, r: number, those: { any }) -> number?,
}

declare sound: {
	play: (file: string, how: PleamarHow?) -> number,
	tone: (tone: PleamarTone) -> number,
	stop: (id: number?) -> (),
	load: (file: string) -> (),
	volume: (v: number) -> (),
}
"#;

fn lua_name(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        && !matches!(
            s,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
                | "continue"
                | "export"
                | "type"
                | "typeof"
        )
}

/// `"low" | "normal" | "critical"`.
fn names(v: &[String]) -> String {
    v.iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn row(m: &Model, depth: usize) -> String {
    let pad = "\t".repeat(depth + 1);
    let mut out = String::from("{\n");
    for f in &m.fields {
        if !lua_name(&f.name) {
            continue;
        }
        let t = match &f.kind {
            FieldType::Text | FieldType::Image(..) => "string".to_owned(),
            FieldType::Number => "number".to_owned(),
            FieldType::Bool => "boolean".to_owned(),
            FieldType::Enum(v) => format!("({})", names(v)),
            FieldType::List(inner) => format!("{{ {} }}", row(inner, depth + 1)),
        };
        out.push_str(&format!("{pad}{}: {t}?,\n", f.name));
    }
    out.push_str(&"\t".repeat(depth));
    out.push('}');
    out
}

/// The definitions: the API, and with a scene its own names.
pub fn definitions(scene: Option<&Scene>) -> String {
    let mut out = String::from(API);
    let Some(e) = scene else {
        out.push_str("\ndeclare fact: { [string]: any }\ndeclare text: { [string]: string }\ndeclare model: { [string]: { any } }\ndeclare function emit(name: string, payload: any?): ()\n");
        return out;
    };
    // Its facts, as what each one is. A name with a dot (`dog.x`, a model's
    // `rows.count`) is not a field name: it is reached as `fact["dog.x"]`.
    out.push_str("\n-- This scene's facts: `fact.open = true`, and read back as they are.\ndeclare fact: {\n");
    let mut facts: Vec<&str> = e
        .facts
        .iter()
        .map(|f| f.0)
        .filter(|n| lua_name(n))
        .collect();
    facts.sort_unstable();
    facts.dedup();
    for n in facts {
        let t = match e.types.iter().find(|t| t.0 == n).map(|t| &t.1) {
            Some(FactType::Bool) => "boolean".to_owned(),
            Some(FactType::Enum(v)) => format!("({})", names(v)),
            None => "number".to_owned(),
        };
        out.push_str(&format!("\t{n}: {t},\n"));
    }
    out.push_str("\t[string]: any,\n}\n");
    out.push_str("\n-- Its live texts: `text.title = \"…\"`.\ndeclare text: {\n");
    let mut texts: Vec<&str> = e
        .texts
        .iter()
        .map(|t| t.0)
        .filter(|n| lua_name(n))
        .collect();
    texts.sort_unstable();
    texts.dedup();
    for n in texts {
        out.push_str(&format!("\t{n}: string,\n"));
    }
    out.push_str("\t[string]: string,\n}\n");
    out.push_str(
        "\n-- Its models, handed over whole: `model.rows = { { … }, … }`.\ndeclare model: {\n",
    );
    for m in e.models.iter().filter(|m| lua_name(&m.name)) {
        out.push_str(&format!("\t{}: {{ {} }},\n", m.name, row(m, 1)));
    }
    out.push_str("\t[string]: { any },\n}\n");
    let mut events: Vec<&str> = e.signals.iter().map(|s| s.0).collect();
    events.sort_unstable();
    events.dedup();
    if events.is_empty() {
        out.push_str("\ndeclare function emit(name: string, payload: any?): ()\n");
    } else {
        out.push_str(&format!(
            "\n-- Its events.\ndeclare function emit(name: {}, payload: any?): ()\n",
            events
                .iter()
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_is_declared_and_a_scene_brings_its_names() {
        let generic = definitions(None);
        assert!(
            generic.contains("declare function on(")
                && generic.contains("declare sound:")
                && generic.contains("declare fact: { [string]: any }")
        );
        let path =
            std::env::temp_dir().join(format!("pleamar-luau-types-{}.plm", std::process::id()));
        std::fs::write(&path, "language 0.4\nscene T {\n    surface { size: 200, 60 }\n    fact mode: low | normal = low\n    fact open = false\n    event saved ->\n    model rows max 4 { label: text; on: bool }\n}\n").unwrap();
        let e = crate::scenes::from_file::read(&path.to_string_lossy()).unwrap();
        let _ = std::fs::remove_file(&path);
        let d = definitions(Some(&e));
        assert!(d.contains("\tmode: (\"low\" | \"normal\"),"), "{d}");
        assert!(d.contains("\topen: boolean,"));
        assert!(d.contains("label: string?") && d.contains("on: boolean?"));
        assert!(d.contains("\"saved\""));
    }
}
