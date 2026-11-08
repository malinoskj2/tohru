# Native Wi-Fi and network popup

Production: include network/state.plm + network/popup.plm in the same root as
its bar button. Initialize require("network/controller")() once; retain returned
open/close/toggle callables for root coordination (Lua emit is render-only).
Native public events: network_open/close/toggle. Permission network.*.
Private entry shell/network/entry.plm + entry.luau is test-only, with real button.
Native click grab and Escape; CLI-idle open may require Escape as accepted.

Pinned native networkmanager.rs/iwd.rs: network.{online,kind,name,strength,wifi,
error,networks}; network.wifi/scan/connect/disconnect/forget. Declarative native
service and capped64-row model, seven-row pages; Up/Down/Return select/join,
secret native input (agent:no) submits password, clear on cancel/send/close.
Native catalog marks saved/open networks so they connect without password entry;
new secured networks expose credentials. Selected observed active connection has
disconnect, saved connection has forget. An observed replacement connection is
never disconnected through a stale target. Status remains native-authoritative;
request acceptance is labelled and is never treated as completion. Detailed NM
errors may be absent (accepted native limitation). Network rates, Ethernet/IP,
VPN and external administration links are intentionally omitted by plan12/13.

Source: j2bar panel/network.rs and panel/settings/network.rs retained selection,
status and native-capable actions; accepted native panel glass approximation.
Checks: lua tests/network/controller.lua; pinned pleamar --check entry.plm.
Private native system-bus fixtures/capture remain pending coordinator allocation;
no host NetworkManager operation is part of development verification.
