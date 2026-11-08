/* Private integration input fixture, MIT. Protocol copyright is in virtual-pointer.xml.
 * Keeps a virtual pointer alive so a no-input headless seat can bind wl_pointer
 * before a click. Never used by the production shell. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wayland-client.h>
#include "virtual-pointer.h"

static struct zwlr_virtual_pointer_manager_v1 *manager;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data;
    (void)version;
    if (strcmp(interface, "zwlr_virtual_pointer_manager_v1") == 0)
        manager = wl_registry_bind(registry, name,
                    &zwlr_virtual_pointer_manager_v1_interface, 1);
}

static void removed(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener listener = {global, removed};

int main(void) {
    const char *private_bus = getenv("TOHRU_B_PRIVATE_BUS");
    const char *display_name = getenv("WAYLAND_DISPLAY");
    const char *runtime_dir = getenv("XDG_RUNTIME_DIR");
    const char *sockets = getenv("PLEAMAR_SOCKETS");
    if (!private_bus || strcmp(private_bus, "1") || !display_name || !runtime_dir || !sockets)
        return 2;
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 3;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager) {
        wl_display_disconnect(display);
        return 4;
    }
    struct zwlr_virtual_pointer_v1 *pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);
    zwlr_virtual_pointer_v1_frame(pointer);
    if (wl_display_roundtrip(display) < 0) return 5;
    puts("ready");
    fflush(stdout);
    char command[128];
    unsigned int sequence = 0;
    while (fgets(command, sizeof(command), stdin)) {
        unsigned int x, y;
        if (strcmp(command, "quit\n") == 0) break;
        if (sscanf(command, "click %u %u", &x, &y) != 2 || x >= 1280 || y >= 720)
            return 6;
        zwlr_virtual_pointer_v1_motion_absolute(pointer, 0, x, y, 1280, 720);
        zwlr_virtual_pointer_v1_frame(pointer);
        if (wl_display_roundtrip(display) < 0) return 7;
        zwlr_virtual_pointer_v1_button(pointer, 0, 0x110, WL_POINTER_BUTTON_STATE_PRESSED);
        zwlr_virtual_pointer_v1_frame(pointer);
        if (wl_display_roundtrip(display) < 0) return 8;
        zwlr_virtual_pointer_v1_button(pointer, 0, 0x110, WL_POINTER_BUTTON_STATE_RELEASED);
        zwlr_virtual_pointer_v1_frame(pointer);
        if (wl_display_roundtrip(display) < 0) return 9;
        printf("click %u\n", ++sequence);
        fflush(stdout);
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    zwlr_virtual_pointer_manager_v1_destroy(manager);
    wl_registry_destroy(registry);
    wl_display_flush(display);
    wl_display_disconnect(display);
    return 0;
}
