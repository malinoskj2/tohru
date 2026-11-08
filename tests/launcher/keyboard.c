/* MIT. Private fixed-input lifecycle fixture; protocol notice in virtual-keyboard.xml.
 * One keyboard lives until stdin EOF/quit, preserving headless seat focus. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "virtual-keyboard.h"

static struct zwp_virtual_keyboard_manager_v1 *manager;
static struct wl_seat *seat;
static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data; (void)version;
    if (!strcmp(interface, "zwp_virtual_keyboard_manager_v1"))
        manager = wl_registry_bind(registry, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
    if (!seat && !strcmp(interface, "wl_seat"))
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data; (void)registry; (void)name;
}
static const struct wl_registry_listener listener = {global, removed};

int main(int argc, char **argv) {
    const char *display_name = getenv("WAYLAND_DISPLAY");
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    const char *sockets = getenv("PLEAMAR_SOCKETS");
    char ipc[1024], filename[1024];
    if (argc != 2 || !runtime || strcmp(runtime, argv[1]) || !display_name ||
        strncmp(display_name, "wayland-", 8) || strchr(display_name, '/') ||
        !sockets || getenv("WAYLAND_SOCKET") || getenv("CUA_INJECT_SOCKET")) return 2;
    if (snprintf(ipc, sizeof(ipc), "%s/ipc", runtime) >= (int)sizeof(ipc) || strcmp(sockets, ipc) ||
        snprintf(filename, sizeof(filename), "%s/keyboard-XXXXXX", runtime) >= (int)sizeof(filename)) return 2;
    struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_ENVIRONMENT_NAMES);
    struct xkb_rule_names names = {.rules="evdev", .model="pc105", .layout="us", .variant="", .options=""};
    struct xkb_keymap *map = context ? xkb_keymap_new_from_names(context, &names, 0) : NULL;
    char *text = map ? xkb_keymap_get_as_string(map, XKB_KEYMAP_FORMAT_TEXT_V1) : NULL;
    if (!text) return 3;
    int fd = mkstemp(filename);
    if (fd < 0) return 4;
    unlink(filename);
    size_t length = strlen(text) + 1;
    FILE *file = fdopen(fd, "w+");
    if (!file || fwrite(text, 1, length, file) != length || fflush(file)) return 4;
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 5;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager || !seat) return 6;
    struct zwp_virtual_keyboard_v1 *keyboard = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(manager, seat);
    zwp_virtual_keyboard_v1_keymap(keyboard, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, (uint32_t)length);
    if (wl_display_roundtrip(display) < 0) return 7;
    fclose(file); free(text); xkb_keymap_unref(map); xkb_context_unref(context);
    puts("ready"); fflush(stdout);
    char command[32]; unsigned int sequence = 0;
    while (fgets(command, sizeof(command), stdin)) {
        if (!strcmp(command, "quit\n")) break;
        // Fixed evdev codes with the fixed US keymap: f,i,x,t,u,r,e / Escape.
        static const uint32_t query[] = {33, 23, 45, 20, 22, 19, 18};
        static const uint32_t escape[] = {1};
        const uint32_t *codes; size_t count;
        if (!strcmp(command, "query\n")) { codes=query; count=sizeof(query)/sizeof(query[0]); }
        else if (!strcmp(command, "escape\n")) { codes=escape; count=1; }
        else return 8;
        for (size_t i=0; i<count; ++i) {
            zwp_virtual_keyboard_v1_key(keyboard, 0, codes[i], WL_KEYBOARD_KEY_STATE_PRESSED);
            if (wl_display_roundtrip(display) < 0) return 9;
            zwp_virtual_keyboard_v1_key(keyboard, 0, codes[i], WL_KEYBOARD_KEY_STATE_RELEASED);
            if (wl_display_roundtrip(display) < 0) return 9;
        }
        printf("done %u\n", ++sequence); fflush(stdout);
    }
    zwp_virtual_keyboard_v1_destroy(keyboard);
    wl_seat_destroy(seat); wl_registry_destroy(registry);
    wl_display_flush(display); wl_display_disconnect(display);
    return 0;
}
