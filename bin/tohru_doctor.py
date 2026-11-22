"""Read-only bounded diagnostics. Never imports models or reports config/credential values."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
from tohru_portability import BIN, DATA, config, json_probe, outputs, probe, protocols, shortcuts, xdg


def required_checks(values):
    core = {"config", "runtime", "systemd-user", "logind", "protocol.layer-shell",
            "environment.WAYLAND_DISPLAY", "environment.XDG_RUNTIME_DIR",
            "environment.DBUS_SESSION_BUS_ADDRESS", "environment.XDG_SESSION_ID",
            "environment.runtime-directory"}
    core.update("executable."+name for name in ("bash","sh","python3","readlink","dirname","sha256sum","cut","rm",
                "setsid","flock","systemctl","loginctl","busctl","timeout","systemd-notify"))
    if values.get("shortcuts.mode")=="portal":
        core.update(("shortcuts.dependencies","shortcuts.portal","shortcuts.registration"))
    if values.get("wallpaper.enabled"):
        core.update(("wallpaper","wallpaper.owner","outputs","executable.wayland-info"))
    if values.get("idle.owner") == "tohru":
        core.update(("executable.swayidle","protocol.idle-notify","protocol.session-lock","locker.pam","service.lock"))
    if values.get("assistant.voice"):
        core.update(("voice.dependencies","voice.vad_model","voice.asr_model"))
        core.update("executable."+name for name in ("parec","pacat","pactl","espeak-ng"))
        if values.get("assistant.voice_wake") == "kws":core.add("voice.kws_model")
    return core


def summarize(checks, values):
    required = required_checks(values)
    healthy = {"ok","configured","present","present-unverified","observed"}
    unknown = {"unknown","incompatible-or-unknown","missing-or-unknown"}
    failures,unobserved,warnings = [],[],[]
    for check in checks:
        check["required"] = check["name"] in required
        if check["required"] and check["state"] not in healthy:
            is_unknown = check["state"] in unknown
            (unobserved if is_unknown else failures).append(check["name"])
            check["severity"] = "warning" if is_unknown else "error"
        else:
            check["severity"] = "info" if check["state"] in healthy|{"disabled","optional-unset","default","selected","loaded","absent"} else "warning"
            if check["severity"] == "warning":warnings.append(check["name"])
    if "config" in failures:code,status=2,"configuration-error"
    elif failures:code,status=1,"required-failures"
    elif unobserved:code,status=3,"required-unknown"
    else:code,status=0,"healthy"
    return {"exit_code":code,"status":status,"required_failures":failures,"required_unknown":unobserved,"warnings":warnings}


def doctor(as_json=False):
    checks=[]
    def add(name,state,detail,action=""):
        checks.append({"name":name,"state":state,"detail":detail,"action":action})
    try:
        values,managed=config()
        add("config","ok","XDG native config; managed overlay" if managed else "XDG native config",
            "Managed values are read-only for standalone lifecycle helpers")
    except (OSError,ValueError):
        add("config","invalid","Configuration cannot be read or validated","Check TOML types, absolute paths and managed-file readability; contents withheld")
        values={};managed=set()
    for name in ("WAYLAND_DISPLAY","XDG_RUNTIME_DIR","DBUS_SESSION_BUS_ADDRESS","XDG_SESSION_ID"):
        add("environment."+name,"ok" if os.environ.get(name) else "missing","set" if os.environ.get(name) else "unset",
            "Import session environment into systemd --user before starting graphical-session.target")
    runtime_dir=os.environ.get("XDG_RUNTIME_DIR", "")
    try:
        info=Path(runtime_dir).stat() if runtime_dir else None
        valid=info is not None and Path(runtime_dir).is_dir() and info.st_uid==os.getuid() and info.st_mode & 0o077 == 0
    except OSError:
        valid=False
    add("environment.runtime-directory","ok" if valid else "invalid","Existing private directory owned by the current user" if valid else "XDG_RUNTIME_DIR is missing, inaccessible or not private/owned")
    tools={"bash":"required shell", "sh":"application launch shell", "readlink":"required coreutils", "dirname":"required coreutils",
           "sha256sum":"required coreutils", "cut":"required coreutils", "rm":"required coreutils", "python3":"required (Python 3.11+)","setsid":"required util-linux","flock":"required util-linux",
           "systemctl":"required systemd","loginctl":"required logind","busctl":"required systemd",
           "timeout":"required coreutils","systemd-notify":"required locker readiness",
           "wayland-info":"protocol/output diagnostics and wallpaper discovery",
           "xdg-terminal-exec":"terminal application launch","wl-copy":"assistant copy",
           "pactl":"audio diagnostics/voice","parec":"voice capture","pacat":"voice playback",
           "espeak-ng":"voice fallback TTS","ffmpeg":"wallpaper previews","ffprobe":"wallpaper metadata",
           "jq":"wallpaper preview metadata","swayidle":"optional idle owner"}
    for tool,purpose in tools.items():
        add("executable."+tool,"ok" if shutil.which(tool) else "missing",purpose,"Install through your distribution package manager")
    runtime=os.environ.get("TOHRU_RUNTIME","pleamar")
    version,state=json_probe([runtime,"--tohru-capabilities"])
    compatible=isinstance(version,dict) and version.get("tohru_portability")==1 and version.get("lock_state")=="native-cycle"
    add("runtime","ok" if compatible else "missing" if state=="missing" else "incompatible" if isinstance(version,dict) else "incompatible-or-unknown",
        "Patched runtime capability schema 1" if compatible else "Patch compatibility was not established",
        "Use the runtime shipped with this checkout/release; stock Pleamar is insufficient")
    caps,state=protocols()
    for name,interface in (("layer-shell","zwlr_layer_shell_v1"),("session-lock","ext_session_lock_manager_v1"),
                           ("idle-notify","ext_idle_notifier_v1"),("window","zwlr_foreign_toplevel_manager_v1"),
                           ("workspace","ext_workspace_manager_v1")):
        available=caps is not None and interface in caps
        add("protocol."+name,"ok" if available else "unknown" if caps is None else "unsupported",
            interface+" advertised" if available else interface+": "+state if caps is None else interface+" not advertised",
            "Choose a layer-shell compositor; workspace/window support degrades independently" if not available else "")
    monitors,discovery=outputs()
    add("outputs","ok" if monitors else "unknown" if discovery!="ok" else "missing",
        f"{len(monitors)} named wl_output outputs; discovery {discovery}","wl_output/xdg-output names are required by wallpaper discovery; no workspace metadata is used")
    hypr=bool(os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")) and not os.environ.get("PLEAMAR_GENERIC")
    add("compositor.backend","ok" if hypr or caps is not None else "unknown",
        "Hyprland IPC (window/workspace), Wayland outputs" if hypr else "Standard protocol backend; only advertised protocols are supported")
    for label,args in (("systemd-user",["systemctl","--user","show-environment"]),
                       ("logind",["loginctl","show-session",os.environ.get("XDG_SESSION_ID", "self"),"-p","Type","--value"])):
        out,state=probe(args)
        # Never include show-environment in the report: it can contain secrets.
        add(label,"ok" if out is not None else state,"reachable" if out is not None else "unavailable","Check systemd user manager, logind session and D-Bus permissions")
    names,state=probe(["busctl","--user","list","--no-pager","--no-legend"])
    notification=None
    if names is not None:
        notification=next((line for line in names.splitlines() if line.startswith("org.freedesktop.Notifications ")),None)
    known_daemons = {"dunst", "mako", "swaync", "xfce4-notifyd"}
    other_notification = notification and any(part in known_daemons for part in notification.split())
    add("notifications.owner","possible-conflict" if other_notification else "unknown" if names is None else "present-unverified" if notification else "missing",
        "A notification owner exists; verify it is Tohru before enabling another" if notification else "No owner observed" if names is not None else state)
    procs,_=probe(["ps","-u",str(os.getuid()),"-o","comm="])
    agents=sorted(set(procs.split()) & {"hyprpolkitagent","polkit-gnome-au","lxqt-policykit-","mate-polkit","polkit-kde-auth"}) if procs else []
    add("polkit.conflicts","possible-conflict" if agents else "unknown",
        "Another known polkit agent process was observed" if agents else "No known competing process observed; registration ownership is not universally enumerable",
        "Run only one polkit agent per session")
    for component in ("bar","session","launcher","notifications","polkit","idle","wallpaper","lock"):
        out,state=probe(["systemctl","--user","show",f"tohru-{component}.service","-p","LoadState","-p","ActiveState","--value"],timeout=1)
        unit_states=set(out.split()) if out else set()
        unit_state="present" if "loaded" in unit_states else "missing" if "not-found" in unit_states else "unavailable" if "masked" in unit_states else "unknown"
        add("service."+component,unit_state,out.strip().replace("\n",", ") if out else state,
            "Generate/install units and inspect journalctl --user -u tohru-"+component)
    pam=next((name for name in ("tohru-lock","pleamar","hyprlock","login") if Path("/etc/pam.d",name).is_file()),None)
    add("locker.pam","present-unverified" if pam else "missing",pam or "No fallback PAM policy found",
        "Have your distro administrator review tohru-lock PAM policy; no authentication attempted")
    add("locker.recovery","requires-validation","Recovery after a locker crash is compositor-dependent",
        "Hyprland needs allow_session_lock_restore; verify nested-session recovery and keep TTY access")
    wallpaper=values.get("wallpaper.enabled",False)
    executable=values.get("wallpaper.executable","mpvpaper")
    add("wallpaper","disabled" if not wallpaper else "configured" if shutil.which(executable) else "missing",
        "Explicit ownership "+("enabled" if wallpaper else "disabled"),"Configure wallpaper.directory, install mpvpaper, and start only Tohru's owner if desired")
    wallpaper_status,wallpaper_state=json_probe([str(BIN/"tohru-wallpaper"),"status"]) if wallpaper else (None,"disabled")
    backend_errors=isinstance(wallpaper_status,dict) and bool(wallpaper_status.get("errors"))
    add("wallpaper.owner","disabled" if not wallpaper else "backend-errors" if backend_errors else "ok" if isinstance(wallpaper_status,dict) and wallpaper_status.get("owner")=="tohru" else "missing" if isinstance(wallpaper_status,dict) else "unknown",
        "Owned supervisor reports output backend failures" if backend_errors else "Owned supervisor reachable" if isinstance(wallpaper_status,dict) and wallpaper_status.get("owner")=="tohru" else "No owned supervisor observed",
        "Explicitly start the configured wallpaper owner; doctor never starts one")
    for key in ("wallpaper.directory","wallpaper.assignments"):
        value=values.get(key,"")
        add(key,"default" if not value else "present" if Path(value).exists() else "missing","XDG/Pictures default" if not value else "Configured path exists" if Path(value).exists() else "Configured path missing")
    add("idle.owner","configured",str(values.get("idle.owner","unknown")),"Use tohru or an external idle manager, never both; suspend requires confirmed lock")
    idle_names=sorted(set(procs.split()) & {"hypridle","swayidle","wayidle","xidlehook"}) if procs else []
    add("idle.processes", "present-unverified" if idle_names else "unknown",
        "Known idle-manager process observed; check ownership before enabling Tohru" if idle_names else "No known idle process observed; unknown managers cannot be excluded")
    delay,delay_state=probe(["busctl","--system","--timeout=1","get-property","org.freedesktop.login1","/org/freedesktop/login1","org.freedesktop.login1.Manager","InhibitDelayMaxUSec"],timeout=2)
    add("idle.inhibit-delay", "observed" if delay else "unknown", "logind finite delay limit is readable; validate cold lock startup against it" if delay else delay_state,
        "External/forced suspend may proceed after logind's delay deadline; Tohru's own suspend requires confirmed lock")
    for key in ("appearance.font_text","appearance.font_display","appearance.font_mono"):
        out,state=probe(["fc-match","--format=%{family}",str(values.get(key,"Noto Sans"))])
        add(key,"ok" if out else "unknown","Resolved font: "+out[:160] if out else state,"Install Noto Sans or another freely distributable system font; restart components after changes")
    audio,state=json_probe(["pactl","-f","json","info"])
    add("audio","ok" if isinstance(audio,dict) else "unknown","PulseAudio-compatible server reachable" if audio else state,"Use PipeWire-Pulse or PulseAudio")
    for name,bus in (("networkmanager","org.freedesktop.NetworkManager"),("iwd","net.connman.iwd"),("bluetooth","org.bluez")):
        out,state=probe(["busctl","--system","--timeout=1","status",bus],timeout=2)
        add(name,"ok" if out is not None else "unknown", "Service reachable" if out is not None else state,"Install/enable the service and review system-bus policy; kernel network fallback is read-only")
    sources,state=json_probe(["pactl","-f","json","list","sources"])
    available=isinstance(sources,list) and any(x.get("name")=="tohru-echo-cancel-source" for x in sources if isinstance(x,dict))
    aecfile=xdg("CONFIG")/"pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf"
    selected=values.get("assistant.voice_source")=="tohru-echo-cancel-source"
    modules,module_state=probe(["pw-cli","ls","Module"])
    module_present=modules is not None and "libpipewire-module-echo-cancel" in modules
    add("echo-cancel.module", "loaded" if module_present else "unknown" if modules is None else "not-observed",
        "PipeWire echo-cancel module observed" if module_present else module_state if modules is None else "Module not in PipeWire registry")
    libdirs=[Path(root) for root in ("/usr/lib/pipewire-0.3","/usr/lib64/pipewire-0.3","/usr/lib/x86_64-linux-gnu/pipewire-0.3")]
    if os.environ.get("PIPEWIRE_MODULE_DIR"):libdirs.insert(0,Path(os.environ["PIPEWIRE_MODULE_DIR"]))
    installed=any((directory/"libpipewire-module-echo-cancel.so").is_file() for directory in libdirs)
    add("echo-cancel.installed", "present" if installed else "unknown", "Module file observed" if installed else "Module file not found in common module directories; custom/Nix layout may differ")
    add("echo-cancel.config","configured" if aecfile.is_file() else "absent","Drop-in present" if aecfile.is_file() else "No Tohru user drop-in found")
    add("echo-cancel.nodes","present" if available else "unknown" if sources is None else "missing","Virtual source observed" if available else "Virtual source not observed")
    add("echo-cancel.selection","selected" if selected else "automatic-or-other","Explicit voice source selection" if selected else "Voice source not explicitly selected")
    add("echo-cancel.effectiveness","unknown","No physical acoustic validation performed","Select both the virtual source and playback sink; module presence alone proves no cancellation")
    enabled=values.get("assistant.voice",False)
    interpreter=values.get("assistant.voice_python","python3")
    # find_spec in the selected interpreter only; never imports native speech runtimes.
    code="import importlib.util,json; print(json.dumps({n:importlib.util.find_spec(n) is not None for n in ['numpy','onnxruntime','sherpa_onnx','misaki','en_core_web_sm']}))"
    envfile=values.get("assistant.voice_environment", "")
    deps,state=json_probe([interpreter,"-c",code]) if enabled and not envfile else (None,"disabled" if not enabled else "environment-specific inspection requires manual check")
    add("voice.dependencies","disabled" if not enabled else "ok" if isinstance(deps,dict) and all(deps.get(name) for name in ("numpy","onnxruntime","sherpa_onnx")) else "missing" if isinstance(deps,dict) else "unknown",
        "Required modules discoverable (not loaded)" if isinstance(deps,dict) and all(deps.get(name) for name in ("numpy","onnxruntime","sherpa_onnx")) else state,
        "Follow docs/standalone.md; activate the configured environment before checking dependencies")
    kokoro = values.get("assistant.tts_engine") == "kokoro" and bool(values.get("assistant.tts_model"))
    add("voice.tts.dependencies", "optional-unset" if not kokoro else "present-unverified" if isinstance(deps,dict) and deps.get("misaki") and deps.get("en_core_web_sm") else "missing-or-unknown",
        "Kokoro needs misaki and en_core_web_sm; espeak-ng is the fallback", "Install the explicit environment from docs/standalone.md; no implicit downloader runs")
    for key,required in (("vad_model",True),("asr_model",True),("turn_model",False),("kws_model",values.get("assistant.voice_wake")=="kws"),("tts_model",False)):
        path=values.get("assistant."+key,"")
        present=bool(path) and Path(path).exists()
        add("voice."+key,"disabled" if not enabled else "present-unverified" if present else "missing" if required else "optional-unset",
            "Configured artifact exists; compatibility not tested" if present else "Required artifact missing" if required else "Optional; fallback applies",
            "Use pinned artifacts and hashes in docs/configuration.md; nothing is downloaded")
    try:shortcuts_report=shortcuts()
    except (OSError,ValueError):shortcuts_report={"state":"config-invalid","owner":"unknown","bindings":[]}
    add("shortcuts",shortcuts_report["state"],shortcuts_report["owner"]+f"; {len(shortcuts_report['bindings'])} read-only runtime entries",
        "Configure shortcuts in your compositor; Tohru has no global registration backend")
    portal,portal_state=probe(["busctl","--user","--auto-start=no","--timeout=1","get-property","org.freedesktop.portal.Desktop","/org/freedesktop/portal/desktop","org.freedesktop.portal.GlobalShortcuts","version"],timeout=2)
    add("shortcuts.portal","present-unverified" if portal else "unknown", "GlobalShortcuts interface version readable; no binding session requested" if portal else portal_state)
    portal_mode=values.get("shortcuts.mode")=="portal"
    dependency,dep_state=json_probe([str(BIN/"tohru-shortcuts"),"--check-dependencies"]) if portal_mode else (None,"disabled")
    add("shortcuts.dependencies","disabled" if not portal_mode else "ok" if isinstance(dependency,dict) and dependency.get("dbus_fast") else "missing" if isinstance(dependency,dict) else "unknown",
        "Optional dbus-fast client" if portal_mode else "Portal ownership disabled", "Install dbus-fast for the portal service; Nix packaging includes it")
    active_grants=shortcuts_report.get("active_registration") and bool(shortcuts_report.get("bindings"))
    add("shortcuts.registration","disabled" if not portal_mode else "ok" if active_grants else "unknown" if shortcuts_report.get("state")=="request-pending" else "configured-ungranted",
        "Current portal session has backend-confirmed action grants" if active_grants else str(shortcuts_report.get("state","unavailable")),
        "Use Settings → Shortcuts setup/configure after explicitly enabling the owner service; doctor never requests grants")
    for name,path in (("greeter.greetd","/etc/greetd/config.toml"),("greeter.pam","/etc/pam.d/greetd")):
        add(name,"present-unverified" if Path(path).is_file() else "optional-unset","File present" if Path(path).is_file() else "Standalone shell does not require greetd","See docs/standalone.md; no system changes or authentication attempted")
    summary=summarize(checks,values)
    result={"schema":1,"read_only":True,"summary":summary,"checks":checks}
    if as_json:print(json.dumps(result))
    else:
        for check in checks:
            print(f"{check['severity']:7} {check['state']:24} {check['name']}: {check['detail']}")
            if check['action']:print('  '+check['action'])
    if not as_json:print(f"Doctor: {summary['status']} (exit {summary['exit_code']})")
    return summary["exit_code"]
